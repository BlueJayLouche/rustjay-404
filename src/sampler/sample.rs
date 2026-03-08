use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// A video sample that can be loaded and played back
/// Supports HAP codec for GPU-efficient playback
pub struct VideoSample {
    pub id: Uuid,
    pub name: String,
    pub filepath: PathBuf,

    // Metadata
    pub duration: Duration,
    pub frame_count: u32,
    pub fps: f32,
    pub resolution: (u32, u32),

    // Playback range (in frames, frame-accurate)
    pub in_point: u32,
    pub out_point: u32,

    // Playback settings
    pub loop_enabled: bool,

    // GPU resources
    pub gpu_texture: Option<Arc<wgpu::Texture>>,
    
    // Thumbnail
    pub thumbnail: Option<Arc<wgpu::Texture>>,
    
    // Decoder state (HAP or fallback)
    decoder: Option<Box<dyn VideoDecoder>>,
}

/// Color space for video frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// Standard RGB (no conversion needed)
    Rgb,
    /// YCoCg (requires shader conversion to RGB)
    YcoCg,
}

/// Trait for video decoders (HAP, FFmpeg, etc.)
pub trait VideoDecoder: Send {
    /// Get a specific frame as a GPU texture
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>>;
    
    /// Get frame dimensions
    fn resolution(&self) -> (u32, u32);
    
    /// Get total frame count
    fn frame_count(&self) -> u32;
    
    /// Get frame rate
    fn fps(&self) -> f32;
    
    /// Get color space (default to RGB)
    fn color_space(&self) -> ColorSpace {
        ColorSpace::Rgb
    }
}

impl VideoSample {
    /// Create a new empty sample
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            filepath: PathBuf::new(),
            duration: Duration::ZERO,
            frame_count: 0,
            fps: 30.0,
            resolution: (0, 0),
            in_point: 0,
            out_point: 0,
            gpu_texture: None,
            thumbnail: None,
            decoder: None,
            loop_enabled: false,
        }
    }

    /// Load a HAP video file using the hap-wgpu crate
    /// 
    /// Supports all HAP formats in QuickTime container:
    /// - Hap1/DXT1 (RGB, 4:1 compression)
    /// - Hap5/DXT5 (RGBA, 4:1 compression)  
    /// - HapY/YCoCg-DXT5 (High quality RGB)
    /// - HapA/BC4 (Alpha only)
    /// - Hap7/BC7 (High quality RGBA)
    /// - HapH/BC6H (HDR RGB)
    pub fn from_hap(
        path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Self> {
        use crate::video::decoder::HapWgpuDecoder;
        use std::sync::Arc;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Use the hap-wgpu crate decoder
        let decoder = HapWgpuDecoder::new(
            path, 
            Arc::new(device.clone()), 
            Arc::new(queue.clone())
        )?;
        
        let resolution = decoder.resolution();
        let frame_count = decoder.frame_count();
        let fps = decoder.fps();

        let mut sample = Self::new(name);
        sample.filepath = path.to_path_buf();
        sample.resolution = resolution;
        sample.frame_count = frame_count;
        sample.fps = fps;
        sample.out_point = sample.frame_count.saturating_sub(1);
        sample.duration = Duration::from_secs_f32(sample.frame_count as f32 / sample.fps);
        
        // Store decoder for frame access
        sample.decoder = Some(Box::new(decoder));
        
        log::info!(
            "Loaded HAP sample '{}' ({}x{} @ {}fps, {} frames)",
            sample.name, resolution.0, resolution.1, fps, frame_count
        );
        
        Ok(sample)
    }

    /// Load from a live capture (webcam/NDI)
    pub fn from_capture(name: impl Into<String>, resolution: (u32, u32)) -> Self {
        let mut sample = Self::new(name);
        sample.resolution = resolution;
        sample.loop_enabled = true; // Captured samples loop by default
        sample
    }
    
    /// Check if this sample should loop
    pub fn is_looping(&self) -> bool {
        self.loop_enabled
    }

    /// Get a frame for playback
    pub fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        let clamped = frame.clamp(self.in_point, self.out_point);
        
        if let Some(ref mut decoder) = self.decoder {
            decoder.get_frame(clamped)
        } else {
            self.gpu_texture.clone()
        }
    }
    
    /// Get color space for this sample
    pub fn color_space(&self) -> ColorSpace {
        if let Some(ref decoder) = self.decoder {
            decoder.color_space()
        } else {
            ColorSpace::Rgb
        }
    }

    /// Set the playback range
    pub fn set_range(&mut self, in_point: u32, out_point: u32) {
        self.in_point = in_point.min(self.frame_count.saturating_sub(1));
        self.out_point = out_point.min(self.frame_count.saturating_sub(1));
        
        // Ensure in < out
        if self.in_point > self.out_point {
            std::mem::swap(&mut self.in_point, &mut self.out_point);
        }
    }

    /// Get the effective duration within in/out points
    pub fn effective_duration(&self) -> Duration {
        let frames = self.out_point.saturating_sub(self.in_point);
        Duration::from_secs_f32(frames as f32 / self.fps)
    }

    /// Generate a thumbnail from the first frame
    pub fn generate_thumbnail(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> anyhow::Result<()> {
        // TODO: Implement thumbnail generation
        Ok(())
    }

    /// Check if this is a valid loaded sample
    pub fn is_loaded(&self) -> bool {
        self.frame_count > 0 && self.resolution.0 > 0
    }
}

impl Clone for VideoSample {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            filepath: self.filepath.clone(),
            duration: self.duration,
            frame_count: self.frame_count,
            fps: self.fps,
            resolution: self.resolution,
            in_point: self.in_point,
            out_point: self.out_point,
            loop_enabled: self.loop_enabled,
            gpu_texture: self.gpu_texture.clone(),
            thumbnail: self.thumbnail.clone(),
            decoder: None, // Decoder is not cloned (each pad has its own playback)
        }
    }
}
