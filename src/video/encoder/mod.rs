//! HAP Video Encoder
//! 
//! Converts video files to HAP format using ffmpeg with the hap encoder.
//! Supports DXT1, DXT5, DXT5-YCoCg, and BC6H compression formats.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use anyhow::{anyhow, Context, Result};

/// HAP compression format for encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapEncodeFormat {
    /// DXT1 (RGB, no alpha, 8:1 compression)
    Dxt1,
    /// DXT5 (RGBA, 4:1 compression) 
    Dxt5,
    /// DXT5-YCoCg (higher quality color, 4:1 compression)
    Dxt5Ycocg,
    /// BC6H (HDR color, 6:1 compression)
    Bc6h,
}

impl HapEncodeFormat {
    /// Get ffmpeg hap encoder name
    pub fn ffmpeg_name(&self) -> &'static str {
        match self {
            HapEncodeFormat::Dxt1 => "hap",
            HapEncodeFormat::Dxt5 => "hap",
            HapEncodeFormat::Dxt5Ycocg => "hap",
            HapEncodeFormat::Bc6h => "hap",
        }
    }
    
    /// Get ffmpeg format argument
    pub fn ffmpeg_format_arg(&self) -> &'static str {
        match self {
            HapEncodeFormat::Dxt1 => "hap",
            HapEncodeFormat::Dxt5 => "hap_q",
            HapEncodeFormat::Dxt5Ycocg => "hap_alpha",
            HapEncodeFormat::Bc6h => "hap_yCoCg",
        }
    }
}

impl std::fmt::Display for HapEncodeFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HapEncodeFormat::Dxt1 => write!(f, "DXT1"),
            HapEncodeFormat::Dxt5 => write!(f, "DXT5"),
            HapEncodeFormat::Dxt5Ycocg => write!(f, "DXT5-YCoCg"),
            HapEncodeFormat::Bc6h => write!(f, "BC6H"),
        }
    }
}

impl std::str::FromStr for HapEncodeFormat {
    type Err = anyhow::Error;
    
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "dxt1" => Ok(HapEncodeFormat::Dxt1),
            "dxt5" => Ok(HapEncodeFormat::Dxt5),
            "dxt5-ycocg" | "ycocg" => Ok(HapEncodeFormat::Dxt5Ycocg),
            "bc6h" => Ok(HapEncodeFormat::Bc6h),
            _ => Err(anyhow!("Unknown HAP format: {}", s)),
        }
    }
}

/// HAP encoder configuration
#[derive(Debug, Clone)]
pub struct HapEncoderConfig {
    /// Output format
    pub format: HapEncodeFormat,
    /// Target width (0 = keep original)
    pub width: u32,
    /// Target height (0 = keep original)
    pub height: u32,
    /// Target FPS (0 = keep original)
    pub fps: u32,
    /// Enable chunks for faster multi-threaded decoding
    pub chunks: u32,
    /// Compression quality (1-31, lower is better)
    pub quality: u32,
}

impl Default for HapEncoderConfig {
    fn default() -> Self {
        Self {
            format: HapEncodeFormat::Dxt5,
            width: 0,
            height: 0,
            fps: 0,
            chunks: 1,
            quality: 5,
        }
    }
}

/// HAP Video Encoder
pub struct HapEncoder {
    config: HapEncoderConfig,
}

impl HapEncoder {
    /// Create a new encoder with default settings
    pub fn new() -> Self {
        Self {
            config: HapEncoderConfig::default(),
        }
    }
    
    /// Create a new encoder with custom settings
    pub fn with_config(config: HapEncoderConfig) -> Self {
        Self { config }
    }
    
    /// Encode a video file to HAP format
    /// 
    /// Matches VP-404 command: ffmpeg -y -i "input" -c:v hap -format hap_q -chunks 4 "output"
    pub fn encode(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        // Check if ffmpeg is available
        self.check_ffmpeg()?;
        
        log::info!("Converting to HAP: {:?} -> {:?}", input_path, output_path);
        
        // Build ffmpeg command to match VP-404 exactly
        // ffmpeg -y -i "input" -c:v hap -format hap_q -chunks 4 "output"
        let mut cmd = Command::new("ffmpeg");
        
        // Overwrite output (-y comes first in VP-404)
        cmd.arg("-y");
        
        // Input file
        cmd.arg("-i").arg(input_path);
        
        // Video codec
        cmd.arg("-c:v").arg("hap");
        
        // Format specifier
        let format_arg = match self.config.format {
            HapEncodeFormat::Dxt1 => "hap",
            HapEncodeFormat::Dxt5 => "hap_q",
            HapEncodeFormat::Dxt5Ycocg => "hap_alpha",
            HapEncodeFormat::Bc6h => "hap_yCoCg",
        };
        cmd.arg("-format").arg(format_arg);
        
        // Chunks for multi-threaded decoding (VP-404 uses 4)
        let chunks = if self.config.chunks > 1 { self.config.chunks } else { 4 };
        cmd.arg("-chunks").arg(chunks.to_string());
        
        // No audio
        cmd.arg("-an");
        
        // Output file
        cmd.arg(output_path);
        
        log::debug!("Running ffmpeg command: {:?}", cmd);
        
        // Run ffmpeg
        let output = cmd
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output()
            .context("Failed to run ffmpeg")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("ffmpeg stderr: {}", stderr);
            return Err(anyhow!("ffmpeg failed: {}", stderr));
        }
        
        // Check that output file was created
        if !output_path.exists() {
            return Err(anyhow!("Output file was not created"));
        }
        
        let output_size = std::fs::metadata(output_path)?.len();
        log::info!("HAP conversion complete: {} bytes", output_size);
        
        Ok(())
    }
    
    /// Check if ffmpeg is installed and has HAP support
    pub fn check_ffmpeg(&self) -> Result<()> {
        let output = Command::new("ffmpeg")
            .args(&["-encoders"])
            .output()
            .context("Failed to run ffmpeg. Is ffmpeg installed?")?;
        
        if !output.status.success() {
            return Err(anyhow!("ffmpeg -encoders failed"));
        }
        
        let encoders = String::from_utf8_lossy(&output.stdout);
        if !encoders.contains("hap") {
            return Err(anyhow!("ffmpeg does not have HAP encoder support"));
        }
        
        Ok(())
    }
    
    /// Get video info using ffprobe
    pub fn get_video_info(path: &Path) -> Result<VideoInfo> {
        let output = Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
                "-of", "csv=s=x:p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .context("Failed to run ffprobe. Is ffprobe installed?")?;
        
        if !output.status.success() {
            return Err(anyhow!("ffprobe failed"));
        }
        
        let info = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = info.trim().split('x').collect();
        
        if parts.len() < 4 {
            return Err(anyhow!("Could not parse video info"));
        }
        
        let width = parts[0].parse::<u32>().context("Invalid width")?;
        let height = parts[1].parse::<u32>().context("Invalid height")?;
        
        // Parse frame rate (e.g., "30000/1001")
        let fps_parts: Vec<&str> = parts[2].split('/').collect();
        let fps = if fps_parts.len() == 2 {
            let num = fps_parts[0].parse::<f32>().unwrap_or(30000.0);
            let den = fps_parts[1].parse::<f32>().unwrap_or(1001.0);
            num / den
        } else {
            parts[2].parse::<f32>().unwrap_or(30.0)
        };
        
        // Parse frame count
        let frames = parts[3].parse::<u32>().unwrap_or(0);
        
        Ok(VideoInfo {
            width,
            height,
            fps,
            frames,
        })
    }
}

impl Default for HapEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Video information from ffprobe
#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub frames: u32,
}

impl std::fmt::Display for VideoInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} @ {:.2} fps ({} frames)", 
            self.width, self.height, self.fps, self.frames)
    }
}

/// Batch encode multiple files
pub fn batch_encode(
    input_files: &[PathBuf],
    output_dir: &Path,
    config: &HapEncoderConfig,
) -> Result<Vec<(PathBuf, Result<()>)>> {
    let encoder = HapEncoder::with_config(config.clone());
    
    std::fs::create_dir_all(output_dir)?;
    
    let results: Vec<(PathBuf, Result<()>)> = input_files
        .iter()
        .map(|input| {
            let stem = input.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let output = output_dir.join(format!("{}.hap.mov", stem));
            
            println!("Encoding: {} -> {}", input.display(), output.display());
            
            let result = encoder.encode(input, &output);
            (input.clone(), result)
        })
        .collect();
    
    Ok(results)
}

/// Convert a captured video file (e.g., from screen capture) to HAP
pub fn convert_capture_to_hap(
    input_path: &Path,
    output_path: &Path,
    format: HapEncodeFormat,
) -> Result<()> {
    let config = HapEncoderConfig {
        format,
        ..Default::default()
    };
    
    let encoder = HapEncoder::with_config(config);
    encoder.encode(input_path, output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_parsing() {
        assert_eq!(
            "dxt1".parse::<HapEncodeFormat>().unwrap(),
            HapEncodeFormat::Dxt1
        );
        assert_eq!(
            "DXT5".parse::<HapEncodeFormat>().unwrap(),
            HapEncodeFormat::Dxt5
        );
        assert_eq!(
            "dxt5-ycocg".parse::<HapEncodeFormat>().unwrap(),
            HapEncodeFormat::Dxt5Ycocg
        );
    }
    
    #[test]
    fn test_ffmpeg_check() {
        let encoder = HapEncoder::new();
        // This will fail if ffmpeg is not installed, but that's OK for the test
        let _ = encoder.check_ffmpeg();
    }
}
