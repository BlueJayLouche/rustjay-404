//! Streaming HAP Video Decoder with background thread
//! 
//! Uses a dedicated decoder thread to fill a frame cache,
//! allowing smooth playback even with ffmpeg seek overhead.

use crate::sampler::sample::VideoDecoder;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::io::{Read, BufReader};
use std::sync::Arc;
use std::time::Duration;
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;

/// Size of decode window (frames to decode ahead/behind)
const DECODE_WINDOW: u32 = 24;

/// Frame cache shared between decoder thread and playback
struct FrameCache {
    frames: HashMap<u32, Arc<wgpu::Texture>>,
    current_frame: u32,
    speed: f32,
    frame_count: u32,
}

impl FrameCache {
    fn new(frame_count: u32, speed: f32) -> Self {
        Self {
            frames: HashMap::new(),
            current_frame: if speed >= 0.0 { 0 } else { frame_count - 1 },
            speed,
            frame_count,
        }
    }
    
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        self.frames.remove(&frame)
    }
    
    fn add_frame(&mut self, frame_num: u32, texture: Arc<wgpu::Texture>) {
        // Limit cache size to prevent memory bloat
        if self.frames.len() > 64 {
            // Remove oldest frames (simple eviction)
            let to_remove: Vec<u32> = self.frames.keys()
                .filter(|&&f| f < self.current_frame.saturating_sub(32))
                .copied()
                .collect();
            for f in to_remove {
                self.frames.remove(&f);
            }
        }
        self.frames.insert(frame_num, texture);
    }
    
    fn get_needed_range(&self) -> (u32, u32) {
        if self.speed >= 0.0 {
            let start = self.current_frame;
            let end = (start + DECODE_WINDOW).min(self.frame_count - 1);
            (start, end)
        } else {
            let end = self.current_frame;
            let start = if end >= DECODE_WINDOW { end - DECODE_WINDOW } else { 0 };
            (start, end)
        }
    }
    
    fn has_frame(&self, frame: u32) -> bool {
        self.frames.contains_key(&frame)
    }
    
    fn needs_frame(&self, frame: u32) -> bool {
        // Check if frame is in our upcoming window and not cached
        let (start, end) = self.get_needed_range();
        frame >= start && frame <= end && !self.has_frame(frame)
    }
}

/// Streaming decoder with background thread
pub struct StreamingDecoder {
    width: u32,
    height: u32,
    frame_count: u32,
    fps: f32,
    filepath: std::path::PathBuf,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    frame_cache: Arc<Mutex<FrameCache>>,
}

impl StreamingDecoder {
    pub fn new(
        path: &Path,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        initial_speed: f32,
    ) -> Result<Self> {
        let info = Self::probe_file(path)?;
        
        let frame_cache = Arc::new(Mutex::new(FrameCache::new(
            info.frame_count,
            initial_speed,
        )));
        
        // Spawn decoder thread
        let video_path = path.to_path_buf();
        let cache_clone = frame_cache.clone();
        let device_arc = device.clone();
        let queue_arc = queue.clone();
        
        std::thread::spawn(move || {
            decoder_thread(
                video_path,
                cache_clone,
                device_arc,
                queue_arc,
                info.width,
                info.height,
                info.fps,
            );
        });
        
        Ok(Self {
            width: info.width,
            height: info.height,
            frame_count: info.frame_count,
            fps: info.fps,
            filepath: path.to_path_buf(),
            device,
            queue,
            frame_cache,
        })
    }
    
    /// Update playback speed (affects decode direction)
    pub fn set_speed(&mut self, speed: f32) {
        let mut cache = self.frame_cache.lock();
        cache.speed = speed;
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
}

impl VideoDecoder for StreamingDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        let frame = frame.min(self.frame_count.saturating_sub(1));
        
        let mut cache = self.frame_cache.lock();
        cache.current_frame = frame;
        
        // Try to get from cache
        if let Some(texture) = cache.get_frame(frame) {
            return Some(texture);
        }
        
        // Not in cache - decoder thread will fetch it
        // Return None for now (caller should handle stall)
        None
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

fn decoder_thread(
    video_path: std::path::PathBuf,
    frame_cache: Arc<Mutex<FrameCache>>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    width: u32,
    height: u32,
    fps: f32,
) {
    let frame_size = (width * height * 4) as usize;
    let mut ffmpeg: Option<Child> = None;
    let mut reader: Option<BufReader<std::process::ChildStdout>> = None;
    let mut stream_position: u32 = 0;
    
    loop {
        // Get range we need to decode
        let (needed_start, needed_end) = {
            let cache = frame_cache.lock();
            cache.get_needed_range()
        };
        
        // Check if we need to restart ffmpeg
        let needs_restart = match &ffmpeg {
            None => true,
            Some(_) => {
                stream_position > needed_end || stream_position + DECODE_WINDOW < needed_start
            }
        };
        
        if needs_restart {
            if let Some(mut child) = ffmpeg.take() {
                let _ = child.kill();
            }
            reader = None;
            
            let timestamp = needed_start as f32 / fps;
            
            match Command::new("ffmpeg")
                .args(&[
                    "-ss", &format!("{:.3}", timestamp),
                    "-i", video_path.to_str().unwrap(),
                    "-f", "rawvideo",
                    "-pix_fmt", "rgba",
                    "-s", &format!("{}x{}", width, height),
                    "-threads", "4",
                    "-an", "-sn",
                    "-",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    stream_position = needed_start;
                    reader = Some(BufReader::with_capacity(
                        frame_size * DECODE_WINDOW as usize,
                        child.stdout.take().unwrap()
                    ));
                    ffmpeg = Some(child);
                }
                Err(e) => {
                    log::error!("Failed to start ffmpeg: {}", e);
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        }
        
        // Check if we have all needed frames
        let have_all = {
            let cache = frame_cache.lock();
            (needed_start..=needed_end).all(|f| cache.has_frame(f))
        };
        
        if have_all {
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        
        // Read frame
        if let Some(ref mut r) = reader {
            let mut frame_data = vec![0u8; frame_size];
            
            match r.read_exact(&mut frame_data) {
                Ok(_) => {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(&format!("Frame {}", stream_position)),
                        size: wgpu::Extent3d {
                            width,
                            height,
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
                            bytes_per_row: Some(width * 4),
                            rows_per_image: Some(height),
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                    
                    let mut cache = frame_cache.lock();
                    if stream_position >= needed_start && stream_position <= needed_end {
                        cache.add_frame(stream_position, Arc::new(texture));
                    }
                    
                    stream_position += 1;
                }
                Err(_) => {
                    ffmpeg = None;
                    reader = None;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
