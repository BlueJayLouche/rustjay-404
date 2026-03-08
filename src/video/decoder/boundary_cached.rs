//! Boundary-Cached Video Decoder
//! 
//! Caches just the loop boundary frames (start and end) for seamless looping.
//! Much less memory than full caching, smooth loop transitions.

use crate::sampler::sample::VideoDecoder;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::io::Read;
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

/// Number of frames to cache at each boundary
const BOUNDARY_CACHE_SIZE: u32 = 8;

/// Decoder that caches loop boundaries for seamless transitions
pub struct BoundaryCachedDecoder {
    width: u32,
    height: u32,
    frame_count: u32,
    fps: f32,
    filepath: std::path::PathBuf,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    /// Start boundary cache (frame 0 to N)
    start_cache: HashMap<u32, Arc<wgpu::Texture>>,
    /// End boundary cache (last N frames)
    end_cache: HashMap<u32, Arc<wgpu::Texture>>,
    /// LRU cache for recent frames (small)
    recent_cache: HashMap<u32, Arc<wgpu::Texture>>,
    recent_order: Vec<u32>,
}

impl BoundaryCachedDecoder {
    pub fn new(
        path: &Path,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Result<Self> {
        let info = Self::probe_file(path)?;
        
        println!("BoundaryCachedDecoder: {}x{} @ {}fps, {} frames", 
            info.width, info.height, info.fps, info.frame_count);
        
        let mut decoder = Self {
            width: info.width,
            height: info.height,
            frame_count: info.frame_count,
            fps: info.fps,
            filepath: path.to_path_buf(),
            device,
            queue,
            start_cache: HashMap::new(),
            end_cache: HashMap::new(),
            recent_cache: HashMap::new(),
            recent_order: Vec::new(),
        };
        
        // Preload boundary frames
        decoder.preload_boundaries()?;
        
        Ok(decoder)
    }
    
    /// Preload start and end boundary frames
    fn preload_boundaries(&mut self) -> Result<()> {
        let frame_size = (self.width * self.height * 4) as usize;
        
        // Load start boundary (frames 0 to BOUNDARY_CACHE_SIZE-1)
        let start_frames = self.frame_count.min(BOUNDARY_CACHE_SIZE);
        println!("  Caching start frames 0-{}...", start_frames - 1);
        self.start_cache = self.decode_frame_range(0, start_frames)?;
        
        // Load end boundary (last BOUNDARY_CACHE_SIZE frames)
        if self.frame_count > BOUNDARY_CACHE_SIZE {
            let end_start = self.frame_count - BOUNDARY_CACHE_SIZE;
            println!("  Caching end frames {}-{}...", end_start, self.frame_count - 1);
            self.end_cache = self.decode_frame_range(end_start, self.frame_count)?;
        }
        
        let total_cached = self.start_cache.len() + self.end_cache.len();
        let memory_mb = (total_cached * frame_size) / (1024 * 1024);
        println!("  Cached {} frames (~{} MB)", total_cached, memory_mb);
        
        Ok(())
    }
    
    /// Decode a specific range of frames using ffmpeg
    fn decode_frame_range(&self, start: u32, end: u32) -> Result<HashMap<u32, Arc<wgpu::Texture>>> {
        let frame_size = (self.width * self.height * 4) as usize;
        let mut frames = HashMap::new();
        
        let timestamp = start as f32 / self.fps;
        let count = end - start;
        
        let mut child = Command::new("ffmpeg")
            .args(&[
                "-ss", &format!("{:.3}", timestamp),
                "-i", self.filepath.to_str().unwrap(),
                "-f", "rawvideo",
                "-pix_fmt", "rgba",
                "-s", &format!("{}x{}", self.width, self.height),
                "-vframes", &count.to_string(),
                "-an", "-sn",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn ffmpeg")?;
        
        let mut stdout = child.stdout.take().unwrap();
        let mut frame_data = vec![0u8; frame_size];
        let mut frame_num = start;
        
        while frame_num < end {
            match stdout.read_exact(&mut frame_data) {
                Ok(_) => {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(&format!("Boundary Frame {}", frame_num)),
                        size: wgpu::Extent3d {
                            width: self.width,
                            height: self.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(self.width * 4),
                            rows_per_image: Some(self.height),
                        },
                        wgpu::Extent3d {
                            width: self.width,
                            height: self.height,
                            depth_or_array_layers: 1,
                        },
                    );
                    
                    frames.insert(frame_num, Arc::new(texture));
                    frame_num += 1;
                }
                Err(_) => break,
            }
        }
        
        let _ = child.kill();
        
        Ok(frames)
    }
    
    /// Decode a single frame on demand
    fn decode_single_frame(&self, frame: u32) -> Result<Arc<wgpu::Texture>> {
        let mut frames = self.decode_frame_range(frame, frame + 1)?;
        frames.remove(&frame)
            .ok_or_else(|| anyhow!("Failed to decode frame {}", frame))
    }
    
    fn probe_file(path: &Path) -> Result<VideoInfo> {
        let output = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to run ffprobe")?;
        
        if !output.status.success() {
            return Err(anyhow!("ffprobe failed"));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = output_str.trim().split(',').collect();
        
        let width = parts[0].parse::<u32>().unwrap_or(1920);
        let height = parts[1].parse::<u32>().unwrap_or(1080);
        
        let fps = if let Some(fps_str) = parts.get(2) {
            if fps_str.contains('/') {
                let fps_parts: Vec<&str> = fps_str.split('/').collect();
                fps_parts[0].parse::<f32>().unwrap_or(30.0) / 
                fps_parts[1].parse::<f32>().unwrap_or(1.0)
            } else {
                fps_str.parse::<f32>().unwrap_or(30.0)
            }
        } else {
            30.0
        };
        
        let frame_count = if let Some(fc_str) = parts.get(3) {
            fc_str.parse::<u32>().unwrap_or(0)
        } else {
            0
        };
        
        let frame_count = if frame_count == 0 {
            let dur_output = Command::new("ffprobe")
                .args(&[
                    "-v", "error",
                    "-show_entries", "format=duration",
                    "-of", "csv=p=0",
                    path.to_str().unwrap(),
                ])
                .output()?;
            
            if dur_output.status.success() {
                let duration = String::from_utf8_lossy(&dur_output.stdout)
                    .trim()
                    .parse::<f32>()
                    .unwrap_or(0.0);
                (duration * fps) as u32
            } else {
                30
            }
        } else {
            frame_count
        };
        
        Ok(VideoInfo {
            width,
            height,
            fps,
            frame_count,
        })
    }
    
    fn add_to_recent_cache(&mut self, frame: u32, texture: Arc<wgpu::Texture>) {
        // Evict oldest if at capacity
        const MAX_RECENT: usize = 4;
        
        if self.recent_cache.len() >= MAX_RECENT && !self.recent_cache.contains_key(&frame) {
            if let Some(oldest) = self.recent_order.first().copied() {
                self.recent_cache.remove(&oldest);
                self.recent_order.remove(0);
            }
        }
        
        self.recent_cache.insert(frame, texture);
        self.recent_order.retain(|&f| f != frame);
        self.recent_order.push(frame);
    }
}

impl VideoDecoder for BoundaryCachedDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        let frame = frame.min(self.frame_count - 1);
        
        // Check start boundary cache
        if let Some(texture) = self.start_cache.get(&frame) {
            return Some(texture.clone());
        }
        
        // Check end boundary cache
        if let Some(texture) = self.end_cache.get(&frame) {
            return Some(texture.clone());
        }
        
        // Check recent cache
        if let Some(texture) = self.recent_cache.get(&frame) {
            return Some(texture.clone());
        }
        
        // Decode on demand (will be slow, but loop boundaries are cached)
        match self.decode_single_frame(frame) {
            Ok(texture) => {
                self.add_to_recent_cache(frame, texture.clone());
                Some(texture)
            }
            Err(e) => {
                log::error!("Failed to decode frame {}: {}", frame, e);
                None
            }
        }
    }
    
    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    
    fn frame_count(&self) -> u32 {
        self.frame_count
    }
    
    fn fps(&self) -> f32 {
        self.fps
    }
}

struct VideoInfo {
    width: u32,
    height: u32,
    fps: f32,
    frame_count: u32,
}
