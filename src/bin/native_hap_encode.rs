//! Native HAP Encoder Example
//!
//! Demonstrates encoding video files to HAP using native Rust encoding (no FFmpeg HAP encoder needed).
//! 
//! Usage:
//!   cargo run --bin native_hap_encode -- <input.mp4> [output.mov] [format]
//!
//! Formats: dxt1, dxt5, dxt5-ycocg (default: dxt5)

use std::path::PathBuf;
use clap::Parser;
use rustjay_404::video::encoder::{HapEncoder, HapEncoderConfig, HapEncodeFormat};

#[derive(Parser)]
struct Args {
    /// Input video file
    input: PathBuf,
    /// Output HAP file (default: input_name.hap.mov)
    output: Option<PathBuf>,
    /// HAP format: dxt1 (fastest), dxt5, dxt5-ycocg
    #[arg(short, long, default_value = "dxt1")]
    format: String,
    /// Target width (0 = keep original)
    #[arg(short, long, default_value = "0")]
    width: u32,
    /// Target height (0 = keep original)
    #[arg(long, default_value = "0")]
    height: u32,
    /// Use legacy ffmpeg encoder instead of native
    #[arg(long)]
    ffmpeg: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    // Determine output path
    let output = args.output.unwrap_or_else(|| {
        let stem = args.input.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        PathBuf::from(format!("{}.hap.mov", stem))
    });
    
    // Parse format
    let format: HapEncodeFormat = args.format.parse()?;
    
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     Native HAP Encoder (FFmpeg-free encoding)            ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("Input:  {:?}", args.input);
    println!("Output: {:?}", output);
    println!("Format: {} (native Rust encoding)", format);
    if args.width > 0 || args.height > 0 {
        println!("Resize: {}x{}", args.width, args.height);
    }
    println!();
    
    // Get video info
    println!("Probing input video...");
    let info = HapEncoder::get_video_info(&args.input)?;
    println!("  Resolution: {}x{}", info.width, info.height);
    println!("  FPS: {:.2}", info.fps);
    println!("  Frames: {}", info.frames);
    println!();
    
    // Create encoder config
    let config = HapEncoderConfig {
        format,
        width: args.width,
        height: args.height,
        fps: 0, // Keep original
        chunks: 4,
        quality: 5,
        gpu_mode: rustjay_404::video::encoder::GpuMode::Auto,
    };
    
    let encoder = HapEncoder::with_config(config);
    
    // Encode
    let start = std::time::Instant::now();
    
    if args.ffmpeg {
        println!("Using LEGACY FFmpeg HAP encoder...");
        encoder.encode_ffmpeg(&args.input, &output)?;
    } else {
        println!("Using NATIVE Rust HAP encoder...");
        encoder.encode(&args.input, &output)?;
    }
    
    let duration = start.elapsed();
    
    println!();
    println!("✓ Encoding complete!");
    println!("  Time: {:?}", duration);
    println!("  Output: {:?}", output);
    
    let output_size = std::fs::metadata(&output)?.len();
    let input_size = std::fs::metadata(&args.input)?.len();
    println!("  Input size:  {:.2} MB", input_size as f64 / 1_048_576.0);
    println!("  Output size: {:.2} MB", output_size as f64 / 1_048_576.0);
    println!("  Ratio: {:.1}%", (output_size as f64 / input_size as f64) * 100.0);
    
    // Verify the output is valid HAP
    println!();
    println!("Verifying output file...");
    if rustjay_404::video::decoder::is_hap_file(&output) {
        println!("✓ Output is valid HAP format!");
    } else {
        println!("✗ Warning: Output may not be valid HAP");
    }
    
    Ok(())
}
