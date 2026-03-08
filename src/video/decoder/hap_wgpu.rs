//! HAP Decoder using the hap-wgpu crate
//!
//! This wraps the hap-wgpu crate's HapPlayer to provide GPU-accelerated
//! HAP playback with direct DXT texture upload.

use crate::sampler::sample::VideoDecoder;
use hap_wgpu::{HapPlayer, LoopMode};
use std::path::Path;
use std::sync::Arc;

/// HAP decoder using the hap-wgpu crate
/// 
/// Supports all HAP formats via QuickTime container:
/// - Hap1/DXT1 (RGB, 4:1 compression)
/// - Hap5/DXT5 (RGBA, 4:1 compression)
/// - HapY/YCoCg-DXT5 (High quality RGB, 4:1 compression)
/// - HapA/BC4 (Alpha only)
/// - Hap7/BC7 (High quality RGBA)
/// - HapH/BC6H (HDR RGB)
pub struct HapWgpuDecoder {
    player: HapPlayer,
}

impl HapWgpuDecoder {
    /// Open a HAP video file (QuickTime .mov format)
    /// 
    /// # Arguments
    /// * `path` - Path to the HAP video file
    /// * `device` - wgpu device for texture creation
    /// * `queue` - wgpu queue for texture upload
    pub fn new(
        path: &Path,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<Self> {
        let player = HapPlayer::open(path, device, queue)?;
        
        log::info!(
            "HapWgpuDecoder: {}x{} @ {:.2}fps, {} frames, codec: {}",
            player.dimensions().0,
            player.dimensions().1,
            player.fps(),
            player.frame_count(),
            player.codec_type()
        );
        
        Ok(Self { player })
    }
    
    /// Get the codec type (e.g., "HapY", "Hap1", "Hap5")
    pub fn codec_type(&self) -> &str {
        self.player.codec_type()
    }
    
    /// Get padded dimensions (dimensions rounded to multiples of 4 for DXT)
    pub fn padded_dimensions(&self) -> (u32, u32) {
        self.player.padded_dimensions()
    }
    
    /// Get the underlying HapPlayer for advanced control
    pub fn player(&mut self) -> &mut HapPlayer {
        &mut self.player
    }
}

impl VideoDecoder for HapWgpuDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        // Seek to the requested frame
        self.player.seek_to_frame(frame);
        
        // Get the frame texture
        self.player.update()
            .map(|hap_texture| Arc::clone(&hap_texture.texture))
    }

    fn resolution(&self) -> (u32, u32) {
        self.player.dimensions()
    }

    fn frame_count(&self) -> u32 {
        self.player.frame_count()
    }

    fn fps(&self) -> f32 {
        self.player.fps()
    }
    
    fn color_space(&self) -> crate::sampler::sample::ColorSpace {
        use hap_wgpu::TextureFormat;
        use crate::sampler::sample::ColorSpace;
        
        match self.player.texture_format() {
            TextureFormat::YcoCgDxt5 => ColorSpace::YcoCg,
            _ => ColorSpace::Rgb,
        }
    }
}

/// Check if a file is a valid HAP QuickTime file
pub fn is_hap_file(path: &Path) -> bool {
    use hap_qt::QtHapReader;
    
    QtHapReader::open(path).is_ok()
}

/// Get information about a HAP file without loading it fully
pub fn probe_hap_file(path: &Path) -> anyhow::Result<HapFileInfo> {
    use hap_qt::QtHapReader;
    
    let reader = QtHapReader::open(path)?;
    
    Ok(HapFileInfo {
        width: reader.resolution().0,
        height: reader.resolution().1,
        frame_count: reader.frame_count(),
        fps: reader.fps(),
        duration_secs: reader.duration() as f32,
        codec: reader.codec_type().to_string(),
    })
}

/// Information about a HAP file
#[derive(Debug, Clone)]
pub struct HapFileInfo {
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
    pub fps: f32,
    pub duration_secs: f32,
    pub codec: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_probe_hap_file() {
        // This test requires a HAP sample file
        let sample_path = std::path::PathBuf::from("samples/output_converted.hap.mov");
        if sample_path.exists() {
            let info = probe_hap_file(&sample_path).unwrap();
            assert!(info.width > 0);
            assert!(info.height > 0);
            assert!(info.frame_count > 0);
            println!("HAP file info: {:?}", info);
        }
    }
}
