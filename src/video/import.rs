//! Video import with automatic HAP conversion
//!
//! Handles importing any video format and converting to HAP on-the-fly.

use crate::video::encoder::{HapEncoder, HapEncoderConfig, HapEncodeFormat};
use std::path::{Path, PathBuf};
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
pub struct VideoImporter {
    config: ImportConfig,
    status: ImportStatus,
}

/// Import configuration
#[derive(Debug, Clone)]
pub struct ImportConfig {
    /// HAP format for conversion
    pub hap_format: HapEncodeFormat,
    /// Target width (0 = keep original)
    pub width: u32,
    /// Target height (0 = keep original)
    pub height: u32,
    /// Target FPS (0 = keep original)
    pub fps: u32,
    /// Output directory for converted files
    pub output_dir: PathBuf,
    /// Whether to delete original after conversion
    pub delete_original: bool,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            hap_format: HapEncodeFormat::Dxt5,
            width: 0,
            height: 0,
            fps: 0,
            output_dir: PathBuf::from("./samples"),
            delete_original: false,
        }
    }
}

impl VideoImporter {
    pub fn new() -> Self {
        Self {
            config: ImportConfig::default(),
            status: ImportStatus::Idle,
        }
    }
    
    pub fn with_config(config: ImportConfig) -> Self {
        Self {
            config,
            status: ImportStatus::Idle,
        }
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
    
    /// Probe video file for information
    pub fn probe_video(path: &Path) -> Result<VideoInfo> {
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
            width: self.config.width,
            height: self.config.height,
            fps: self.config.fps,
            chunks: 1,
            quality: 5,
        };
        
        let encoder = HapEncoder::with_config(encoder_config);
        
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
    pub async fn import_async(&mut self, input_path: PathBuf) -> Result<PathBuf> {
        // Run conversion in blocking thread
        let config = self.config.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut importer = VideoImporter::with_config(config);
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

/// Quick import function for simple use cases
pub fn import_video(input_path: &Path, output_dir: Option<&Path>) -> Result<PathBuf> {
    let config = ImportConfig {
        output_dir: output_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("./samples")),
        ..Default::default()
    };
    
    let mut importer = VideoImporter::with_config(config);
    importer.import(input_path)
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
}
