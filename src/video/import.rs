//! Video import with automatic HAP conversion
//!
//! Handles importing any video format and converting to HAP on-the-fly.
//! 
//! GPU acceleration: When wgpu device/queue are provided, uses GPU compute shaders
//! for DXT compression (10-50x faster than CPU). See `VideoImporter::with_gpu()`.

use crate::video::encoder::{HapEncoder, HapEncoderConfig, HapEncodeFormat};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};

/// Import status for UI feedback
#[derive(Debug, Clone)]
pub enum ImportStatus {
    /// Ready to import
    Idle,
    /// Checking file format
    Analyzing,
    /// Converting to HAP
    Converting { progress: f32 },
    /// Loading into pad
    Loading,
    /// Complete
    Complete(PathBuf),
    /// Error occurred
    Error(String),
}

/// Video file information
#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_secs: f32,
    pub is_hap: bool,
    pub codec: String,
}

/// Import any video file, converting to HAP if necessary
///
/// Supports GPU-accelerated encoding when initialized with `with_gpu()`.
/// Falls back to CPU encoding when GPU is not available.
pub struct VideoImporter {
    config: ImportConfig,
    status: ImportStatus,
    /// Optional wgpu device for GPU encoding
    device: Option<Arc<wgpu::Device>>,
    /// Optional wgpu queue for GPU encoding  
    queue: Option<Arc<wgpu::Queue>>,
}

/// Import configuration
#[derive(Debug, Clone)]
pub struct ImportConfig {
    /// HAP format for conversion (default: Dxt1 for speed)
    pub hap_format: HapEncodeFormat,
    /// Target width (0 = auto from max_dimension)
    pub width: u32,
    /// Target height (0 = auto from max_dimension)
    pub height: u32,
    /// Target FPS (0 = keep original)
    pub fps: u32,
    /// Output directory for converted files
    pub output_dir: PathBuf,
    /// Whether to delete original after conversion
    pub delete_original: bool,
    /// Maximum pixel dimension (width or height). Videos larger than this
    /// are scaled down preserving aspect ratio. 0 = no limit.
    pub max_dimension: u32,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            hap_format: HapEncodeFormat::Dxt1,
            width: 0,
            height: 0,
            fps: 0,
            output_dir: PathBuf::from("./samples"),
            delete_original: false,
            max_dimension: 1920,
        }
    }
}

impl VideoImporter {
    /// Create a new importer with default settings (CPU only)
    pub fn new() -> Self {
        Self {
            config: ImportConfig::default(),
            status: ImportStatus::Idle,
            device: None,
            queue: None,
        }
    }
    
    /// Create a new importer with custom settings (CPU only)
    pub fn with_config(config: ImportConfig) -> Self {
        Self {
            config,
            status: ImportStatus::Idle,
            device: None,
            queue: None,
        }
    }
    
    /// Create a new importer with GPU acceleration
    ///
    /// # Arguments
    /// * `config` - Import configuration
    /// * `device` - wgpu device for GPU compute
    /// * `queue` - wgpu command queue
    ///
    /// # Example
    /// ```no_run
    /// # use std::sync::Arc;
    /// # fn example(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
    /// use rustjay_404::video::import::{VideoImporter, ImportConfig};
    /// let config = ImportConfig::default();
    /// let importer = VideoImporter::with_gpu(config, device, queue);
    /// if importer.is_gpu_available() {
    ///     println!("Using GPU acceleration for import!");
    /// }
    /// # }
    /// ```
    pub fn with_gpu(
        config: ImportConfig,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        Self {
            config,
            status: ImportStatus::Idle,
            device: Some(device),
            queue: Some(queue),
        }
    }
    
    /// Initialize GPU support after creation
    pub fn init_gpu(&mut self, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
        self.device = Some(device);
        self.queue = Some(queue);
    }
    
    /// Check if GPU encoding is available
    pub fn is_gpu_available(&self) -> bool {
        self.device.is_some() && self.queue.is_some()
    }
    
    /// Get current status
    pub fn status(&self) -> &ImportStatus {
        &self.status
    }
    
    /// Reset status to idle
    pub fn reset(&mut self) {
        self.status = ImportStatus::Idle;
    }
    
    /// Check if a file is already HAP format
    pub fn is_hap_file(path: &Path) -> bool {
        // First try the native HAP parser from hap-wgpu crate
        if crate::video::decoder::is_hap_file(path) {
            log::debug!("Detected HAP via native parser: {:?}", path);
            return true;
        }
        
        // Check file extension - .hap, .hap.mov, etc.
        let filename = path.to_string_lossy().to_lowercase();
        if filename.ends_with(".hap") || filename.contains(".hap.") {
            log::debug!("Detected HAP by filename: {:?}", path);
            return true;
        }
        
        // Also check codec via ffprobe if available
        match Self::probe_video(path) {
            Ok(info) => {
                if info.is_hap {
                    log::debug!("Detected HAP by codec: {:?} -> {}", path, info.codec);
                    return true;
                }
                log::debug!("Not HAP codec: {:?} -> {}", path, info.codec);
            }
            Err(e) => {
                log::debug!("Failed to probe file: {:?} -> {}", path, e);
            }
        }
        
        false
    }
    
    /// Probe video file for information (tries native MP4 parsing first, falls back to ffprobe)
    pub fn probe_video(path: &Path) -> Result<VideoInfo> {
        // Try native MP4 probe first
        if let Ok(info) = crate::video::encoder::decode::probe_mp4_extended(path) {
            log::debug!("Got video info via native MP4 parser");
            return Ok(VideoInfo {
                path: path.to_path_buf(),
                width: info.width,
                height: info.height,
                fps: info.fps,
                duration_secs: info.duration_secs,
                is_hap: info.is_hap,
                codec: info.codec,
            });
        }

        // Fall back to ffprobe
        log::debug!("Native probe failed, falling back to ffprobe");
        Self::probe_video_ffprobe(path)
    }

    /// Probe video file using ffprobe (fallback)
    fn probe_video_ffprobe(path: &Path) -> Result<VideoInfo> {
        let output = std::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate,codec_name,duration",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to run ffprobe. Is ffmpeg installed?")?;

        if !output.status.success() {
            return Err(anyhow!("ffprobe failed"));
        }

        let info = String::from_utf8_lossy(&output.stdout);
        log::debug!("ffprobe output: {}", info);

        let parts: Vec<&str> = info.trim().split(',').collect();

        if parts.len() < 4 {
            log::error!("ffprobe returned insufficient data: {:?}", parts);
            return Err(anyhow!("Could not parse video info: insufficient data"));
        }

        // Parse dimensions (handle "N/A" and empty strings)
        let width = if parts[0].is_empty() || parts[0] == "N/A" {
            0u32
        } else {
            parts[0].parse::<u32>().unwrap_or(0)
        };

        let height = if parts[1].is_empty() || parts[1] == "N/A" {
            0u32
        } else {
            parts[1].parse::<u32>().unwrap_or(0)
        };

        // Parse frame rate
        let fps = if parts[2].contains('/') {
            let fps_parts: Vec<&str> = parts[2].split('/').collect();
            let num = fps_parts[0].parse::<f32>().unwrap_or(30000.0);
            let den = fps_parts[1].parse::<f32>().unwrap_or(1001.0);
            num / den
        } else {
            parts[2].parse::<f32>().unwrap_or(30.0)
        };

        let codec = parts[3].to_string();
        let is_hap = codec.to_lowercase().contains("hap");

        // Parse duration if available
        let duration_secs = parts.get(4)
            .and_then(|s| if s.is_empty() || *s == "N/A" { None } else { s.parse::<f32>().ok() })
            .unwrap_or(0.0);

        Ok(VideoInfo {
            path: path.to_path_buf(),
            width,
            height,
            fps,
            duration_secs,
            is_hap,
            codec,
        })
    }
    
    /// Import a video file, converting to HAP if necessary
    /// 
    /// Automatically uses GPU acceleration if available.
    /// Returns the path to the HAP file (either original or converted)
    pub fn import(&mut self, input_path: &Path) -> Result<PathBuf> {
        self.status = ImportStatus::Analyzing;
        
        // Ensure output directory exists
        std::fs::create_dir_all(&self.config.output_dir)?;
        
        // Check if already HAP
        if Self::is_hap_file(input_path) {
            log::info!("✓ File is already HAP format, skipping conversion: {:?}", input_path);
            self.status = ImportStatus::Complete(input_path.to_path_buf());
            return Ok(input_path.to_path_buf());
        }
        
        log::info!("→ File needs conversion to HAP: {:?}", input_path);
        
        // Probe video info
        let info = Self::probe_video(input_path)?;
        log::info!("Importing video: {}x{} @ {:.2} fps, codec: {}",
            info.width, info.height, info.fps, info.codec);

        // Compute target dimensions from max_dimension (if explicit width/height not set)
        let (target_w, target_h) = if self.config.width > 0 && self.config.height > 0 {
            (self.config.width, self.config.height)
        } else if self.config.max_dimension > 0 && info.width > 0 && info.height > 0 {
            compute_scaled_dimensions(info.width, info.height, self.config.max_dimension)
        } else {
            (0, 0) // keep original
        };

        if target_w > 0 && (target_w != info.width || target_h != info.height) {
            log::info!("Scaling: {}x{} -> {}x{} (max_dimension={})",
                info.width, info.height, target_w, target_h, self.config.max_dimension);
        }

        // Generate output path
        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let output_path = self.config.output_dir.join(format!("{}_converted.hap.mov", stem));

        // Convert to HAP
        self.status = ImportStatus::Converting { progress: 0.0 };

        let encoder_config = HapEncoderConfig {
            format: self.config.hap_format,
            width: target_w,
            height: target_h,
            fps: self.config.fps,
            chunks: 1,
            quality: 5,
            gpu_mode: crate::video::encoder::GpuMode::Auto,
        };
        
        // Create encoder with GPU if available
        let encoder = if let (Some(device), Some(queue)) = (&self.device, &self.queue) {
            log::info!("Using GPU-accelerated HAP encoding");
            HapEncoder::with_gpu(
                encoder_config,
                Arc::clone(device),
                Arc::clone(queue),
            )
        } else {
            log::info!("Using CPU HAP encoding (consider using with_gpu() for faster encoding)");
            HapEncoder::with_config(encoder_config)
        };
        
        log::info!("Converting to HAP: {:?} -> {:?}", input_path, output_path);
        encoder.encode(input_path, &output_path)?;
        
        // Verify output
        if !output_path.exists() {
            return Err(anyhow!("Conversion failed - output file not created"));
        }
        
        // Get file sizes for logging
        let input_size = std::fs::metadata(input_path)?.len();
        let output_size = std::fs::metadata(&output_path)?.len();
        log::info!("Conversion complete: {} -> {} bytes ({:.1}%)",
            input_size, output_size,
            (output_size as f64 / input_size as f64) * 100.0);
        
        self.status = ImportStatus::Complete(output_path.clone());
        Ok(output_path)
    }
    
    /// Import async version for use with tokio
    /// 
    /// Note: This creates a new importer in the blocking thread, so GPU context
    /// must be passed explicitly if needed.
    pub async fn import_async(&mut self, input_path: PathBuf) -> Result<PathBuf> {
        let config = self.config.clone();
        let device = self.device.clone();
        let queue = self.queue.clone();
        
        // Run conversion in blocking thread
        let result = tokio::task::spawn_blocking(move || {
            let mut importer = if let (Some(d), Some(q)) = (device, queue) {
                VideoImporter::with_gpu(config, d, q)
            } else {
                VideoImporter::with_config(config)
            };
            importer.import(&input_path)
        }).await?;
        
        result
    }
}

impl Default for VideoImporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Quick import function for simple use cases (CPU only)
/// 
/// For GPU-accelerated import, use `VideoImporter::with_gpu()` instead.
pub fn import_video(input_path: &Path, output_dir: Option<&Path>) -> Result<PathBuf> {
    let config = ImportConfig {
        output_dir: output_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("./samples")),
        ..Default::default()
    };
    
    let mut importer = VideoImporter::with_config(config);
    importer.import(input_path)
}

/// Import video with GPU acceleration
/// 
/// Convenience function for GPU-accelerated import.
/// 
/// # Example
/// ```no_run
/// # use std::path::Path;
/// # async fn example(device: &wgpu::Device, queue: &wgpu::Queue) -> anyhow::Result<()> {
/// use rustjay_404::video::import::import_video_with_gpu;
/// let path = Path::new("video.mp4");
/// let result = import_video_with_gpu(&path, None, device, queue).await?;
/// # Ok(())
/// # }
/// ```
pub async fn import_video_with_gpu(
    input_path: &Path,
    output_dir: Option<&Path>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<PathBuf> {
    let config = ImportConfig {
        output_dir: output_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("./samples")),
        ..Default::default()
    };
    
    let mut importer = VideoImporter::with_gpu(
        config,
        Arc::new(device.clone()),
        Arc::new(queue.clone()),
    );
    importer.import(input_path)
}

/// Compute scaled dimensions that fit within `max_dim` while preserving aspect ratio.
/// Rounds down to nearest multiple of 4 (required for DXT block alignment).
/// Returns original dimensions if already within the limit.
pub fn compute_scaled_dimensions(src_w: u32, src_h: u32, max_dim: u32) -> (u32, u32) {
    if max_dim == 0 || (src_w <= max_dim && src_h <= max_dim) {
        return (src_w, src_h);
    }

    let (w, h) = if src_w >= src_h {
        // Landscape or square: constrain width
        let scale = max_dim as f64 / src_w as f64;
        (max_dim, (src_h as f64 * scale) as u32)
    } else {
        // Portrait: constrain height
        let scale = max_dim as f64 / src_h as f64;
        ((src_w as f64 * scale) as u32, max_dim)
    };

    // Round down to multiple of 4 for DXT block alignment
    (w & !3, h & !3)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_hap_file() {
        assert!(VideoImporter::is_hap_file(Path::new("test.hap")));
        assert!(VideoImporter::is_hap_file(Path::new("test.HAP")));
        assert!(!VideoImporter::is_hap_file(Path::new("test.mp4")));
        assert!(!VideoImporter::is_hap_file(Path::new("test.mov")));
    }
    
    #[test]
    fn test_default_format_is_dxt1() {
        let config = ImportConfig::default();
        assert_eq!(config.hap_format, HapEncodeFormat::Dxt1);
    }

    #[test]
    fn test_compute_scaled_dimensions() {
        // Retina screen recording: 3016x1876 -> max 1920
        let (w, h) = compute_scaled_dimensions(3016, 1876, 1920);
        assert!(w <= 1920);
        assert!(w % 4 == 0);
        assert!(h % 4 == 0);
        assert_eq!(w, 1920);
        assert_eq!(h, 1192); // 1876 * (1920/3016) ≈ 1194.7, rounded to mult of 4

        // Already within limit
        assert_eq!(compute_scaled_dimensions(1280, 720, 1920), (1280, 720));

        // Portrait video
        let (w, h) = compute_scaled_dimensions(1080, 1920, 1080);
        assert!(h <= 1080);
        assert!(w % 4 == 0);
        assert!(h % 4 == 0);

        // No limit
        assert_eq!(compute_scaled_dimensions(4000, 3000, 0), (4000, 3000));
    }
}
