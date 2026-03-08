//! Streaming HAP Video Decoder using FFmpeg
//! 
//! Decodes HAP files by maintaining a continuous ffmpeg stream.
//! Uses a ring buffer for streaming and LRU cache for decoded frames.

use crate::sampler::sample::VideoDecoder;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::io::{Read, BufReader};
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

/// Maximum number of decoded frames to cache
/// Smaller cache = less memory pressure, more frequent decoding
const MAX_FRAME_CACHE_SIZE: usize = 16;
/// Number of frames to read ahead in the streaming thread
const STREAM_BUFFER_FRAMES: usize = 8;

/// Streaming FFmpeg-based HAP decoder
pub struct HapFfmpegDecoder {
    width: u32,
    height: u32,
    frame_count: u32,
    fps: f32,
    filepath: std::path::PathBuf,
    device: *const wgpu::Device,
    queue: *const wgpu::Queue,
    /// Decoded frame cache
    frame_cache: HashMap<u32, Arc<wgpu::Texture>>,
    /// Track cache access order for LRU eviction
    cache_order: Vec<u32>,
    /// Streaming decoder process
    stream_decoder: Option<StreamDecoder>,
    /// Next frame to read from stream
    stream_position: u32,
    /// Frame size in bytes
    frame_size: usize,
}

/// Streaming decoder state
struct StreamDecoder {
    /// The ffmpeg child process
    child: Child,
    /// Buffered reader for stdout
    reader: BufReader<std::process::ChildStdout>,
}

/// Video information from probing
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f32,
    frame_count: u32,
}

impl HapFfmpegDecoder {
    /// Create a new decoder for a HAP video file
    pub unsafe fn new(path: &Path, device: *const wgpu::Device, queue: *const wgpu::Queue) -> Result<Self> {
        let info = Self::probe_file(path)?;
        let frame_size = (info.width * info.height * 4) as usize;
        
        Ok(Self {
            width: info.width,
            height: info.height,
            frame_count: info.frame_count,
            fps: info.fps,
            filepath: path.to_path_buf(),
            device,
            queue,
            frame_cache: HashMap::new(),
            cache_order: Vec::new(),
            stream_decoder: None,
            stream_position: 0,
            frame_size,
        })
    }
    
    /// Start streaming decoder at specific frame
    fn start_stream_at(&mut self, start_frame: u32) -> Result<()> {
        // Kill existing stream if any
        if let Some(mut old) = self.stream_decoder.take() {
            let _ = old.child.kill();
        }
        
        let timestamp = start_frame as f32 / self.fps;
        
        log::debug!("Starting ffmpeg stream at frame {} ({:.3}s)", start_frame, timestamp);
        
        let mut child = Command::new("ffmpeg")
            .args(&[
                "-ss", &format!("{:.3}", timestamp),
                "-i", self.filepath.to_str().unwrap(),
                "-f", "rawvideo",
                "-pix_fmt", "rgba",
                "-s", &format!("{}x{}", self.width, self.height),
                "-threads", "4",
                "-bufsize", "100M",         // Larger buffer
                "-max_delay", "0",          // Minimize latency
                "-an",                      // No audio
                "-sn",                      // No subtitles
                "-", 
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn ffmpeg")?;
        
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
        // Use larger buffer to prevent underruns
        let reader = BufReader::with_capacity(self.frame_size * STREAM_BUFFER_FRAMES * 2, stdout);
        
        self.stream_decoder = Some(StreamDecoder {
            child,
            reader,
        });
        self.stream_position = start_frame;
        
        Ok(())
    }
    
    /// Read next frame from active stream
    fn read_next_frame(&mut self) -> Result<(u32, Arc<wgpu::Texture>)> {
        let decoder = self.stream_decoder.as_mut()
            .ok_or_else(|| anyhow!("No active stream"))?;
        
        let mut frame_data = vec![0u8; self.frame_size];
        
        decoder.reader.read_exact(&mut frame_data)
            .context("Failed to read frame from stream")?;
        
        let frame_num = self.stream_position;
        self.stream_position += 1;
        
        let texture = self.create_texture_from_rgba(&frame_data)?;
        
        Ok((frame_num, texture))
    }
    
    /// Insert a frame into the cache with LRU eviction
    fn insert_into_cache(&mut self, frame: u32, texture: Arc<wgpu::Texture>) {
        // If cache is full, remove oldest entry
        if self.frame_cache.len() >= MAX_FRAME_CACHE_SIZE && !self.frame_cache.contains_key(&frame) {
            if let Some(oldest) = self.cache_order.first().copied() {
                self.frame_cache.remove(&oldest);
                self.cache_order.remove(0);
            }
        }
        
        // Add new frame to cache
        self.frame_cache.insert(frame, texture);
        
        // Update access order
        self.cache_order.retain(|&f| f != frame);
        self.cache_order.push(frame);
    }
    
    /// Get a frame from cache and update access order
    fn get_from_cache(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        if let Some(texture) = self.frame_cache.get(&frame).cloned() {
            // Update LRU order
            self.cache_order.retain(|&f| f != frame);
            self.cache_order.push(frame);
            Some(texture)
        } else {
            None
        }
    }
    
    /// Probe video file info using ffprobe
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
            return Err(anyhow!("ffprobe failed: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = output_str.trim().split(',').collect();
        
        if parts.len() < 3 {
            return Err(anyhow!("Unexpected ffprobe output format"));
        }
        
        let width = parts[0].parse::<u32>().unwrap_or(1920);
        let height = parts[1].parse::<u32>().unwrap_or(1080);
        
        let fps = if let Some(fps_str) = parts.get(2) {
            if fps_str.contains('/') {
                let fps_parts: Vec<&str> = fps_str.split('/').collect();
                if fps_parts.len() == 2 {
                    let num = fps_parts[0].parse::<f32>().unwrap_or(30.0);
                    let den = fps_parts[1].parse::<f32>().unwrap_or(1.0);
                    num / den
                } else {
                    30.0
                }
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
            Self::estimate_frame_count(path, fps).unwrap_or(30)
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
    
    /// Estimate frame count from duration
    fn estimate_frame_count(path: &Path, fps: f32) -> Result<u32> {
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
            Ok((duration * fps) as u32)
        } else {
            Ok(30)
        }
    }
    
    /// Create GPU texture from RGBA data
    fn create_texture_from_rgba(&self, data: &[u8]) -> Result<Arc<wgpu::Texture>> {
        unsafe {
            let texture = (*self.device).create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("HAP Frame {}x{}", self.width, self.height)),
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
            
            (*self.queue).write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
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
            
            Ok(Arc::new(texture))
        }
    }
}

impl VideoDecoder for HapFfmpegDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        let frame = frame.min(self.frame_count.saturating_sub(1));
        
        // Check cache first - this is the fast path
        if let Some(texture) = self.get_from_cache(frame) {
            return Some(texture);
        }
        
        // Frame not in cache - need to decode it
        // Check if current stream can provide this frame
        let needs_restart = match self.stream_decoder {
            None => true,
            _ => {
                // Restart if frame is behind stream position or too far ahead
                frame < self.stream_position || 
                frame > self.stream_position + STREAM_BUFFER_FRAMES as u32
            }
        };
        
        if needs_restart {
            if let Err(e) = self.start_stream_at(frame) {
                log::error!("Failed to start stream at frame {}: {}", frame, e);
                return None;
            }
        }
        
        // Read frames until we get the one we want, caching as we go
        loop {
            match self.read_next_frame() {
                Ok((frame_num, texture)) => {
                    let is_target = frame_num == frame;
                    self.insert_into_cache(frame_num, texture.clone());
                    
                    if is_target {
                        return Some(texture);
                    }
                    
                    // Safety check - don't loop forever
                    if frame_num >= self.frame_count - 1 {
                        return self.get_from_cache(frame);
                    }
                }
                Err(e) => {
                    log::error!("Failed to read frame {}: {}", frame, e);
                    return self.get_from_cache(frame);
                }
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

impl Drop for HapFfmpegDecoder {
    fn drop(&mut self) {
        if let Some(mut decoder) = self.stream_decoder.take() {
            let _ = decoder.child.kill();
        }
    }
}

// SAFETY: The device and queue pointers are only accessed from the thread that created the decoder
unsafe impl Send for HapFfmpegDecoder {}
