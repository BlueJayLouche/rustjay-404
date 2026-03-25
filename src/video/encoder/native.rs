//! Native HAP Video Encoder
//!
//! Uses hap-rs crates for pure Rust HAP encoding.
//! Decodes MP4/H.264 input natively via `mp4` + `openh264`.
//! Falls back to ffmpeg for non-H.264 input.
//!
//! GPU acceleration: When wgpu device/queue are provided, uses GPU compute shaders
//! for DXT compression (10-50x faster than CPU). Falls back to CPU otherwise.

use std::path::Path;
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};
use hap_qt::{HapFormat, HapFrameEncoder, QtHapWriter, VideoConfig, CompressionMode, DxtQuality};

use super::{HapEncodeFormat, HapEncoderConfig};

/// Native HAP encoder using hap-rs
/// 
/// Supports GPU-accelerated encoding when wgpu device/queue are provided.
/// Falls back to CPU encoding (parallelized with rayon) when GPU is unavailable.
pub struct NativeHapEncoder {
    config: HapEncoderConfig,
    /// Optional GPU encoder for hardware-accelerated DXT compression
    gpu_encoder: Option<hap_wgpu::HapVideoEncoder>,
    /// Stored device for creating new encoders (needed for init_gpu)
    device: Option<Arc<wgpu::Device>>,
    /// Stored queue for creating new encoders
    queue: Option<Arc<wgpu::Queue>>,
}

impl NativeHapEncoder {
    /// Map the config quality (1-31) to a DXT quality tier.
    /// 1-10 = Best, 11-20 = Balanced, 21-31 = Fast
    fn dxt_quality(&self) -> DxtQuality {
        match self.config.quality {
            1..=10 => DxtQuality::Best,
            11..=20 => DxtQuality::Balanced,
            _ => DxtQuality::Fast,
        }
    }

    /// Create a new native encoder with default settings (CPU only)
    pub fn new() -> Self {
        Self {
            config: HapEncoderConfig::default(),
            gpu_encoder: None,
            device: None,
            queue: None,
        }
    }
    
    /// Create a new encoder with custom settings (CPU only)
    pub fn with_config(config: HapEncoderConfig) -> Self {
        Self { 
            config,
            gpu_encoder: None,
            device: None,
            queue: None,
        }
    }
    
    /// Create a new encoder with GPU acceleration
    /// 
    /// # Arguments
    /// * `config` - Encoder configuration
    /// * `device` - wgpu device for GPU compute
    /// * `queue` - wgpu command queue
    /// 
    /// # Example
    /// ```no_run
    /// # use std::sync::Arc;
    /// # fn example(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
    /// use rustjay_404::video::encoder::{NativeHapEncoder, HapEncoderConfig};
    /// let config = HapEncoderConfig::default();
    /// let encoder = NativeHapEncoder::with_gpu(config, device, queue);
    /// if encoder.is_gpu_available() {
    ///     println!("Using GPU acceleration!");
    /// }
    /// # }
    /// ```
    pub fn with_gpu(
        config: HapEncoderConfig,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> Self {
        let gpu_encoder = hap_wgpu::HapVideoEncoder::new(
            Arc::clone(&device), 
            Arc::clone(&queue)
        );
        Self {
            config,
            gpu_encoder: Some(gpu_encoder),
            device: Some(device),
            queue: Some(queue),
        }
    }
    
    /// Initialize GPU encoding after creation
    /// 
    /// Call this before encoding if you want GPU acceleration.
    /// Safe to call even if GPU is already initialized.
    pub fn init_gpu(&mut self, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) {
        self.gpu_encoder = Some(hap_wgpu::HapVideoEncoder::new(
            Arc::clone(&device), 
            Arc::clone(&queue)
        ));
        self.device = Some(device);
        self.queue = Some(queue);
    }
    
    /// Check if GPU encoding is available
    pub fn is_gpu_available(&self) -> bool {
        self.gpu_encoder.is_some()
    }
    
    /// Convert HapEncodeFormat to hap_qt::HapFormat
    pub fn to_hap_format(format: HapEncodeFormat) -> HapFormat {
        match format {
            HapEncodeFormat::Dxt1 => HapFormat::Hap1,
            HapEncodeFormat::Dxt5 => HapFormat::Hap5,
            HapEncodeFormat::Dxt5Ycocg => HapFormat::HapY,
            HapEncodeFormat::Bc6h => HapFormat::HapH,
        }
    }
    
    /// Convert HapEncodeFormat to hap_wgpu::HapFormat
    fn to_wgpu_format(format: HapEncodeFormat) -> hap_wgpu::HapFormat {
        match format {
            HapEncodeFormat::Dxt1 => hap_wgpu::HapFormat::Hap1,
            HapEncodeFormat::Dxt5 => hap_wgpu::HapFormat::Hap5,
            HapEncodeFormat::Dxt5Ycocg => hap_wgpu::HapFormat::HapY,
            HapEncodeFormat::Bc6h => hap_wgpu::HapFormat::HapH,
        }
    }
    
    /// Encode a video file to HAP format using native encoding
    ///
    /// Automatically uses GPU acceleration if available, otherwise falls back to CPU.
    /// Tries native MP4/H.264 decoding first, then falls back to ffmpeg for other codecs.
    pub fn encode(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        // If GPU is available and format is supported, use GPU encoding
        if self.should_use_gpu() {
            match self.encode_with_gpu(input_path, output_path) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    log::warn!("GPU encoding failed ({}), falling back to CPU", e);
                    let _ = std::fs::remove_file(output_path);
                }
            }
        }
        
        // Fall back to CPU encoding
        self.encode_cpu(input_path, output_path)
    }
    
    /// Check if we should use GPU encoding
    fn should_use_gpu(&self) -> bool {
        if self.gpu_encoder.is_none() {
            return false;
        }
        
        // GPU supports Hap1, Hap5, HapY, HapA formats
        // BC6H and BC7 are not supported for GPU encoding
        matches!(self.config.format, 
            HapEncodeFormat::Dxt1 | 
            HapEncodeFormat::Dxt5 | 
            HapEncodeFormat::Dxt5Ycocg
        )
    }
    
    /// Encode using GPU acceleration
    ///
    /// Uses GpuDxtCompressor directly with our own EOF-aware frame loop.
    /// This avoids the fixed-frame-count loop in HapVideoEncoder::encode()
    /// which hangs when ffmpeg produces fewer frames than probe reports.
    fn encode_with_gpu(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        use std::process::{Command, Stdio};
        use std::io::{BufReader, Read};

        log::info!("Converting to HAP (GPU accelerated): {:?} -> {:?}", input_path, output_path);

        // Get video info
        let info = super::HapEncoder::get_video_info(input_path)?;
        log::info!("Input: {}x{} @ {:.2} fps, {} frames",
            info.width, info.height, info.fps, info.frames);

        // Determine output dimensions
        let width = if self.config.width > 0 { self.config.width } else { info.width };
        let height = if self.config.height > 0 { self.config.height } else { info.height };
        let fps = if self.config.fps > 0 { self.config.fps as f32 } else { info.fps };

        // Initialize GPU compressor directly (not through HapVideoEncoder)
        let device = self.device.as_ref()
            .ok_or_else(|| anyhow!("GPU device not available"))?
            .clone();
        let queue = self.queue.as_ref()
            .ok_or_else(|| anyhow!("GPU queue not available"))?
            .clone();

        let gpu = hap_wgpu::GpuDxtCompressor::try_new(device, queue, width, height)
            .ok_or_else(|| anyhow!("GPU compression not available for {}x{}", width, height))?;

        log::info!("GPU compression initialized for {}x{}", width, height);

        // Spawn ffmpeg decode process
        let fps_str = format!("{}", fps);
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-i").arg(input_path)
            .arg("-vf").arg(format!("scale={}:{}", width, height))
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg("rgba")
            .arg("-r").arg(&fps_str)
            .arg("-an")
            .arg("-")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        log::debug!("Running ffmpeg decode: {:?}", cmd);

        let mut child = cmd.spawn().context("Failed to spawn ffmpeg for decoding")?;
        let stdout = child.stdout.take().context("Failed to capture stdout")?;

        // Drain stderr in background to prevent pipe deadlock
        let stderr = child.stderr.take();
        let stderr_handle = std::thread::spawn(move || -> String {
            let Some(stderr) = stderr else { return String::new() };
            let mut buf = String::new();
            let mut reader = BufReader::new(stderr);
            let _ = reader.read_to_string(&mut buf);
            buf
        });

        // Set up HAP frame encoder (for Snappy + header wrapping of GPU-compressed DXT)
        let hap_format = Self::to_hap_format(self.config.format);
        let wgpu_format = Self::to_wgpu_format(self.config.format);
        let mut frame_encoder = HapFrameEncoder::new(hap_format, width, height)
            .context("Failed to create HAP frame encoder")?;
        frame_encoder.set_compression(CompressionMode::Snappy);

        let video_config = VideoConfig::new(width, height, fps, hap_format);
        let mut writer = QtHapWriter::create(output_path, video_config)
            .context("Failed to create HAP video writer")?;

        let frame_size = (width * height * 4) as usize;
        let (padded_w, padded_h) = gpu.dimensions();
        let needs_padding = width != padded_w || height != padded_h;

        let mut reader = BufReader::with_capacity(frame_size, stdout);
        let start = std::time::Instant::now();
        let mut frame_count = 0u32;

        log::info!("Streaming GPU encode (~{} expected frames)...", info.frames);

        // EOF-aware loop: read until ffmpeg closes stdout
        loop {
            let mut frame_buffer = vec![0u8; frame_size];
            match reader.read_exact(&mut frame_buffer) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            // Pad to GPU dimensions if needed
            let gpu_input = if needs_padding {
                hap_wgpu::pad_rgba(&frame_buffer, width, height, padded_w, padded_h)
            } else {
                frame_buffer
            };

            // GPU DXT compression → Snappy + header → write
            let dxt_data = gpu.compress(&gpu_input, wgpu_format)
                .context(format!("GPU compression failed at frame {}", frame_count))?;
            let hap_frame = frame_encoder.encode_from_dxt(&dxt_data)
                .context(format!("HAP frame encoding failed at frame {}", frame_count))?;
            writer.write_frame(&hap_frame)
                .context(format!("Failed to write frame {}", frame_count))?;

            frame_count += 1;
            if frame_count % 30 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let fps_rate = frame_count as f64 / elapsed;
                log::debug!("GPU encoded {} frames ({:.1} fps)...", frame_count, fps_rate);
            }
        }

        writer.finalize().context("Failed to finalize HAP file")?;

        // Clean up ffmpeg
        let _ = child.wait();
        if let Ok(stderr_output) = stderr_handle.join() {
            if !stderr_output.is_empty() {
                log::debug!("ffmpeg stderr: {}", stderr_output.lines().last().unwrap_or(""));
            }
        }

        let duration = start.elapsed();
        let encode_fps = frame_count as f64 / duration.as_secs_f64();
        log::info!("GPU encoding complete: {} frames in {:?} ({:.1} fps)",
            frame_count, duration, encode_fps);

        if !output_path.exists() {
            return Err(anyhow!("Output file was not created"));
        }

        let output_size = std::fs::metadata(output_path)?.len();
        log::info!("Output file size: {} bytes", output_size);

        Ok(())
    }
    
    /// Encode using CPU (original implementation)
    fn encode_cpu(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        // Try fully native path first (MP4 + H.264)
        match self.encode_native(input_path, output_path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                log::info!("Native decode unavailable ({}), falling back to ffmpeg", e);
                let _ = std::fs::remove_file(output_path);
            }
        }

        self.encode_with_ffmpeg_decode(input_path, output_path)
    }

    /// Fully native encode: MP4/H.264 decode → HAP encode (no ffmpeg)
    fn encode_native(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        log::info!(
            "Converting to HAP (fully native): {:?} -> {:?}",
            input_path,
            output_path
        );

        let mut decoder = super::decode::Mp4Decoder::open(input_path)?;

        let width = if self.config.width > 0 {
            self.config.width
        } else {
            decoder.width()
        };
        let height = if self.config.height > 0 {
            self.config.height
        } else {
            decoder.height()
        };

        // Native decoder doesn't support scaling — fall through to ffmpeg if needed
        if width != decoder.width() || height != decoder.height() {
            return Err(anyhow!(
                "Native decoder cannot scale {}x{} -> {}x{}, need ffmpeg",
                decoder.width(), decoder.height(), width, height
            ));
        }
        let fps = if self.config.fps > 0 {
            self.config.fps as f32
        } else {
            decoder.fps()
        };

        log::info!(
            "Input: {}x{} @ {:.2} fps, {} samples",
            decoder.width(),
            decoder.height(),
            decoder.fps(),
            decoder.sample_count()
        );

        // Set up HAP encoder
        let hap_format = Self::to_hap_format(self.config.format);
        let quality = self.dxt_quality();
        let mut frame_encoder = HapFrameEncoder::new(hap_format, width, height)
            .context("Failed to create HAP frame encoder")?;
        frame_encoder.set_compression(CompressionMode::Snappy);
        frame_encoder.set_quality(quality);

        log::info!("DXT quality: {:?} (config quality={})", quality, self.config.quality);

        let video_config = VideoConfig::new(width, height, fps, hap_format);
        let mut writer = QtHapWriter::create(output_path, video_config)
            .context("Failed to create HAP video writer")?;

        let mut frame_count = 0u32;
        let start = std::time::Instant::now();

        // Batched parallel encode: decode BATCH_SIZE frames, compress in parallel,
        // write sequentially. Keeps memory bounded while saturating all cores on DXT.
        const BATCH_SIZE: usize = 32;
        let mut batch: Vec<Vec<u8>> = Vec::with_capacity(BATCH_SIZE);

        loop {
            // Fill the batch
            batch.clear();
            for _ in 0..BATCH_SIZE {
                match decoder.next_frame()? {
                    Some(rgba_data) => batch.push(rgba_data),
                    None => break,
                }
            }
            if batch.is_empty() {
                break;
            }

            // Parallel DXT compress the batch
            use rayon::prelude::*;
            let encoded: Vec<Vec<u8>> = batch
                .par_iter()
                .enumerate()
                .map(|(i, frame_data)| {
                    let mut enc = HapFrameEncoder::new(hap_format, width, height)
                        .map_err(|e| anyhow!("Frame encoder init failed: {}", e))?;
                    enc.set_compression(CompressionMode::Snappy);
                    enc.set_quality(quality);
                    enc.encode(frame_data)
                        .map_err(|e| anyhow!("Failed to encode frame {}: {}", frame_count as usize + i, e).into())
                })
                .collect::<Result<Vec<_>>>()?;

            // Sequential write
            for hap_frame in &encoded {
                writer.write_frame(hap_frame)?;
                frame_count += 1;
            }

            if frame_count % 30 < BATCH_SIZE as u32 {
                let elapsed = start.elapsed().as_secs_f64();
                let fps_rate = frame_count as f64 / elapsed;
                log::debug!("Encoded {} frames ({:.1} fps)...", frame_count, fps_rate);
            }
        }

        writer.finalize().context("Failed to finalize HAP file")?;

        let duration = start.elapsed();
        let encode_fps = frame_count as f64 / duration.as_secs_f64();
        log::info!(
            "Native HAP encoding complete: {} frames in {:?} ({:.1} fps)",
            frame_count, duration, encode_fps
        );

        let output_size = std::fs::metadata(output_path)?.len();
        log::info!("Output file size: {} bytes", output_size);

        Ok(())
    }
    
    /// Encode using ffmpeg for decode, native for encode
    fn encode_with_ffmpeg_decode(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        use std::process::{Command, Stdio};
        use std::io::{BufReader, Read};
        
        log::info!("Converting to HAP (native encoder): {:?} -> {:?}", input_path, output_path);
        
        // Get video info first
        let info = super::HapEncoder::get_video_info(input_path)?;
        log::info!("Input: {}x{} @ {:.2} fps, {} frames", 
            info.width, info.height, info.fps, info.frames);
        
        // Determine output dimensions
        let width = if self.config.width > 0 { self.config.width } else { info.width };
        let height = if self.config.height > 0 { self.config.height } else { info.height };
        let fps = if self.config.fps > 0 { self.config.fps as f32 } else { info.fps };
        
        // Use ffmpeg to decode to raw RGBA frames
        let fps_str = format!("{}", fps);
        
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-i").arg(input_path)
            .arg("-vf").arg(format!("scale={}:{}", width, height))
            .arg("-f").arg("rawvideo")
            .arg("-pix_fmt").arg("rgba")
            .arg("-r").arg(&fps_str)
            .arg("-an")
            .arg("-") // Output to stdout
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        log::debug!("Running ffmpeg decode: {:?}", cmd);
        
        let mut child = cmd.spawn().context("Failed to spawn ffmpeg for decoding")?;
        let stdout = child.stdout.take().context("Failed to capture stdout")?;

        // Drain stderr in background thread to prevent pipe deadlock
        let stderr = child.stderr.take();
        let stderr_handle = std::thread::spawn(move || -> String {
            let Some(stderr) = stderr else { return String::new() };
            let mut buf = String::new();
            let mut reader = BufReader::new(stderr);
            let _ = reader.read_to_string(&mut buf);
            buf
        });

        // Set up native HAP encoder
        let hap_format = Self::to_hap_format(self.config.format);
        let quality = self.dxt_quality();
        let mut frame_encoder = HapFrameEncoder::new(hap_format, width, height)
            .context("Failed to create HAP frame encoder")?;

        frame_encoder.set_compression(CompressionMode::Snappy);
        frame_encoder.set_quality(quality);

        log::info!("DXT quality: {:?} (config quality={})", quality, self.config.quality);
        
        // Create video writer
        let video_config = VideoConfig::new(width, height, fps, hap_format);
        let mut writer = QtHapWriter::create(output_path, video_config)
            .context("Failed to create HAP video writer")?;
        
        // Batched parallel encode from ffmpeg stdout
        let frame_size = (width * height * 4) as usize;
        let mut reader = BufReader::with_capacity(frame_size * 2, stdout);
        let mut frame_count = 0u32;
        let start = std::time::Instant::now();

        const BATCH_SIZE: usize = 32;
        let mut batch: Vec<Vec<u8>> = Vec::with_capacity(BATCH_SIZE);

        loop {
            // Fill batch from ffmpeg stdout
            batch.clear();
            for _ in 0..BATCH_SIZE {
                let mut frame_buffer = vec![0u8; frame_size];
                match reader.read_exact(&mut frame_buffer) {
                    Ok(()) => batch.push(frame_buffer),
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(e.into()),
                }
            }
            if batch.is_empty() {
                break;
            }

            // Parallel DXT compress the batch
            use rayon::prelude::*;
            let encoded: Vec<Vec<u8>> = batch
                .par_iter()
                .enumerate()
                .map(|(i, frame_data)| {
                    let mut enc = HapFrameEncoder::new(hap_format, width, height)
                        .map_err(|e| anyhow!("Frame encoder init failed: {}", e))?;
                    enc.set_compression(CompressionMode::Snappy);
                    enc.set_quality(quality);
                    enc.encode(frame_data)
                        .map_err(|e| anyhow!("Failed to encode frame {}: {}", frame_count as usize + i, e).into())
                })
                .collect::<Result<Vec<_>>>()?;

            // Sequential write
            for hap_frame in &encoded {
                writer.write_frame(hap_frame)?;
                frame_count += 1;
            }

            if frame_count % 30 < BATCH_SIZE as u32 {
                let elapsed = start.elapsed().as_secs_f64();
                let fps_rate = frame_count as f64 / elapsed;
                log::debug!("Encoded {} frames ({:.1} fps)...", frame_count, fps_rate);
            }
        }

        // Wait for ffmpeg to finish
        let _ = child.wait();
        if let Ok(stderr_output) = stderr_handle.join() {
            if !stderr_output.is_empty() {
                log::debug!("ffmpeg stderr: {}", stderr_output.lines().last().unwrap_or(""));
            }
        }

        // Finalize the HAP file
        writer.finalize().context("Failed to finalize HAP file")?;

        let duration = start.elapsed();
        let encode_fps = frame_count as f64 / duration.as_secs_f64();
        log::info!(
            "Native HAP encoding complete: {} frames in {:?} ({:.1} fps)",
            frame_count, duration, encode_fps
        );

        if !output_path.exists() {
            return Err(anyhow!("Output file was not created"));
        }

        let output_size = std::fs::metadata(output_path)?.len();
        log::info!("Output file size: {} bytes", output_size);
        
        Ok(())
    }
    
    /// Check if native encoding is available (always true now)
    pub fn check_native(&self) -> Result<()> {
        // Native encoding is always available
        Ok(())
    }
}

impl Default for NativeHapEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Encode frames directly to HAP (for live recording)
///
/// Uses rayon to parallelize DXT compression across all CPU cores,
/// then writes the encoded frames sequentially.
pub fn encode_frames_to_hap(
    frames: &[Vec<u8>],
    width: u32,
    height: u32,
    fps: f32,
    format: HapEncodeFormat,
    output_path: &Path,
) -> Result<()> {
    use rayon::prelude::*;

    if frames.is_empty() {
        return Err(anyhow!("No frames to encode"));
    }

    let hap_format = NativeHapEncoder::to_hap_format(format);

    log::info!(
        "Encoding {} frames ({}x{} {:?}) with parallel DXT compression...",
        frames.len(), width, height, format
    );

    // Batched parallel encode + sequential write
    // Processes BATCH_SIZE frames at a time to bound peak memory
    // Recording path always uses Fast quality — this is live performance
    const BATCH_SIZE: usize = 64;

    let video_config = VideoConfig::new(width, height, fps, hap_format);
    let mut writer = QtHapWriter::create(output_path, video_config)
        .context("Failed to create HAP video writer")?;

    let start = std::time::Instant::now();

    for chunk in frames.chunks(BATCH_SIZE) {
        let encoded: Vec<Vec<u8>> = chunk
            .par_iter()
            .enumerate()
            .map(|(i, frame_data)| {
                let mut encoder = HapFrameEncoder::new(hap_format, width, height)
                    .map_err(|e| anyhow!("Failed to create encoder for frame {}: {}", i, e))?;
                encoder.set_compression(CompressionMode::Snappy);
                encoder.set_quality(DxtQuality::Fast);
                encoder
                    .encode(frame_data)
                    .map_err(|e| anyhow!("Failed to encode frame {}: {}", i, e).into())
            })
            .collect::<Result<Vec<_>>>()?;

        for hap_frame in &encoded {
            writer.write_frame(hap_frame)?;
        }
    }

    writer.finalize().context("Failed to finalize HAP file")?;

    let duration = start.elapsed();
    let encode_fps = frames.len() as f64 / duration.as_secs_f64();
    log::info!(
        "Encoded {} frames to HAP in {:?} ({:.1} fps): {:?}",
        frames.len(), duration, encode_fps, output_path
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_native_encoder_creation() {
        let encoder = NativeHapEncoder::new();
        assert!(encoder.check_native().is_ok());
        assert!(!encoder.is_gpu_available());
    }
    
    #[test]
    fn test_format_conversion() {
        assert!(matches!(
            NativeHapEncoder::to_hap_format(HapEncodeFormat::Dxt1),
            HapFormat::Hap1
        ));
        assert!(matches!(
            NativeHapEncoder::to_hap_format(HapEncodeFormat::Dxt5),
            HapFormat::Hap5
        ));
        assert!(matches!(
            NativeHapEncoder::to_hap_format(HapEncodeFormat::Dxt5Ycocg),
            HapFormat::HapY
        ));
    }
}
