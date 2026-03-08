//! Pre-cached HAP Video Decoder
//! 
//! Loads all frames into memory for smooth seeking and reverse playback.
//! Best for short clips (up to a few seconds).

use crate::sampler::sample::VideoDecoder;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::io::Read;
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

/// Pre-cached decoder - all frames stored in memory
pub struct CachedDecoder {
    width: u32,
    height: u32,
    frame_count: u32,
    fps: f32,
    /// Frame cache - frame_num -> texture
    frames: HashMap<u32, Arc<wgpu::Texture>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl CachedDecoder {
    /// Create a new cached decoder - loads all frames into GPU memory
    pub fn new(
        path: &Path,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Result<Self> {
        let info = probe_video_info(path)?;
        
        println!("CachedDecoder: Loading {} frames into memory...", info.frame_count);
        
        // Load all frames
        let frames = Self::load_all_frames(path, &device, &queue, &info)?;
        
        println!("CachedDecoder: Loaded {} frames", frames.len());
        
        Ok(Self {
            width: info.width,
            height: info.height,
            frame_count: info.frame_count,
            fps: info.fps,
            frames,
            device,
            queue,
        })
    }
    
    /// Load all frames from video file
    fn load_all_frames(
        path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        info: &VideoInfo,
    ) -> Result<HashMap<u32, Arc<wgpu::Texture>>> {
        let frame_size = (info.width * info.height * 4) as usize;
        let mut frames = HashMap::new();
        
        // Start ffmpeg process
        let mut child = Command::new("ffmpeg")
            .args(&[
                "-i", path.to_str().unwrap(),
                "-f", "rawvideo",
                "-pix_fmt", "rgba",
                "-s", &format!("{}x{}", info.width, info.height),
                "-threads", "4",
                "-an", "-sn",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn ffmpeg")?;
        
        let mut stdout = child.stdout.take().unwrap();
        let mut frame_data = vec![0u8; frame_size];
        let mut frame_num = 0u32;
        
        // Read frames one by one
        while frame_num < info.frame_count {
            match stdout.read_exact(&mut frame_data) {
                Ok(_) => {
                    // Create texture
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(&format!("Cached Frame {}", frame_num)),
                        size: wgpu::Extent3d {
                            width: info.width,
                            height: info.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &frame_data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(info.width * 4),
                            rows_per_image: Some(info.height),
                        },
                        wgpu::Extent3d {
                            width: info.width,
                            height: info.height,
                            depth_or_array_layers: 1,
                        },
                    );
                    
                    frames.insert(frame_num, Arc::new(texture));
                    frame_num += 1;
                    
                    if frame_num % 30 == 0 {
                        print!("\r  Loading frame {}/{}", frame_num, info.frame_count);
                    }
                }
                Err(_) => {
                    // End of stream
                    break;
                }
            }
        }
        
        println!("\r  Loading frame {}/{}", frame_num, info.frame_count);
        
        // Clean up ffmpeg
        let _ = child.kill();
        
        Ok(frames)
    }
    
    /// Get memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        let frame_size = (self.width * self.height * 4) as usize;
        self.frames.len() * frame_size
    }
    

}

impl VideoDecoder for CachedDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        let frame = frame.min(self.frame_count - 1);
        self.frames.get(&frame).cloned()
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

/// Hybrid decoder - uses cached decoder for small files, streaming for large
pub struct HybridDecoder {
    inner: Box<dyn VideoDecoder>,
}

impl HybridDecoder {
    pub fn new(
        path: &Path,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Result<Self> {
        // Probe file to decide which decoder to use
        let info = probe_video_info(path)?;
        let frame_size = (info.width * info.height * 4) as usize;
        let total_size = frame_size * info.frame_count as usize;
        
        // Use cached decoder for files under 100MB decoded
        const MAX_CACHED_SIZE: usize = 100 * 1024 * 1024; // 100MB
        
        if total_size < MAX_CACHED_SIZE {
            println!("Using cached decoder ({} MB)", total_size / (1024 * 1024));
            let decoder = CachedDecoder::new(path, device, queue)?;
            Ok(Self {
                inner: Box::new(decoder),
            })
        } else {
            println!("Using streaming decoder ({} MB - too large for cache)", 
                total_size / (1024 * 1024));
            // Fall back to streaming decoder
            use crate::video::decoder::streaming::StreamingDecoder;
            let decoder = StreamingDecoder::new(path, device, queue, 1.0)?;
            Ok(Self {
                inner: Box::new(decoder),
            })
        }
    }
}

impl VideoDecoder for HybridDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        self.inner.get_frame(frame)
    }
    
    fn resolution(&self) -> (u32, u32) {
        self.inner.resolution()
    }
    
    fn frame_count(&self) -> u32 {
        self.inner.frame_count()
    }
    
    fn fps(&self) -> f32 {
        self.inner.fps()
    }
}

fn probe_video_info(path: &Path) -> Result<VideoInfo> {
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
        let output = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to get duration")?;
        
        if output.status.success() {
            let duration = String::from_utf8_lossy(&output.stdout)
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
