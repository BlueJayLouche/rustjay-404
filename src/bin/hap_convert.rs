// Convert QuickTime HAP to raw HAP format
// Usage: cargo run --bin hap_convert -- <input.mov> <output.hap>

use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Input QuickTime HAP file
    input: PathBuf,
    /// Output raw HAP file
    output: PathBuf,
}

/// Raw HAP file header (64 bytes)
#[repr(C)]
struct HapHeader {
    magic: [u8; 4],      // "hap "
    version: u32,        // 1
    width: u32,
    height: u32,
    frame_count: u32,
    fps: f32,
    format_id: u32,      // 0xAB=DXT1, 0xBB=DXT5, 0xEB=YCoCg
    _padding: [u8; 40],  // Reserved
}

/// Frame index entry (16 bytes)
#[repr(C)]
struct FrameEntry {
    chunk_count: u32,    // Always 1 for now
    pts: f64,           // Presentation timestamp
    data_offset: u64,   // Offset in file where frame data starts
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    println!("Converting {:?} -> {:?}", args.input, args.output);
    
    // Probe input file
    let (width, height, fps, frame_count, format_tag) = probe_hap_video(&args.input)?;
    println!("Input: {}x{} @ {:.2}fps, {} frames, format: {}", 
        width, height, fps, frame_count, format_tag);
    
    // Create output file
    let mut output = File::create(&args.output)?;
    
    // Map QuickTime format tag to HAP format ID
    let format_id = match format_tag.as_str() {
        "Hap1" => 0xABu32,  // DXT1
        "Hap5" => 0xBBu32,  // DXT5
        "HapY" => 0xEBu32,  // YCoCg
        _ => return Err(anyhow::anyhow!("Unknown HAP format: {}", format_tag)),
    };
    
    // Write header (64 bytes)
    let header = HapHeader {
        magic: *b"hap ",
        version: 1,
        width,
        height,
        frame_count,
        fps,
        format_id,
        _padding: [0; 40],
    };
    write_header(&mut output, &header)?;
    
    // Reserve space for frame index
    let index_offset = 64u64;
    let data_start = index_offset + (frame_count as u64 * 16); // 16 bytes per frame entry
    output.seek(SeekFrom::Start(data_start))?;
    
    // Extract frames using ffmpeg and write to output
    println!("Extracting frames...");
    let frame_offsets = extract_frames(&args.input, &mut output, frame_count)?;
    
    // Write frame index
    println!("Writing frame index...");
    output.seek(SeekFrom::Start(index_offset))?;
    
    for (i, offset) in frame_offsets.iter().enumerate() {
        let entry = FrameEntry {
            chunk_count: 1,
            pts: i as f64 / fps as f64,
            data_offset: *offset,
        };
        write_frame_entry(&mut output, &entry)?;
    }
    
    // Flush and finalize
    output.flush()?;
    
    let output_size = std::fs::metadata(&args.output)?.len();
    println!();
    println!("Conversion complete!");
    println!("Output: {:?} ({} bytes)", args.output, output_size);
    println!("Format: Raw HAP ({})", format_tag);
    
    Ok(())
}

fn probe_hap_video(path: &PathBuf) -> anyhow::Result<(u32, u32, f32, u32, String)> {
    // Get video stream info
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
            "-of", "csv=p=0",
            path.to_str().unwrap(),
        ])
        .output()?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = output_str.trim().split(',').collect();
    
    let width = parts[0].parse::<u32>()?;
    let height = parts[1].parse::<u32>()?;
    
    let fps = if let Some(fps_str) = parts.get(2) {
        if fps_str.contains('/') {
            let fps_parts: Vec<&str> = fps_str.split('/').collect();
            fps_parts[0].parse::<f32>()? / fps_parts[1].parse::<f32>()?
        } else {
            fps_str.parse::<f32>()?
        }
    } else {
        30.0
    };
    
    let frame_count = if let Some(fc_str) = parts.get(3) {
        fc_str.parse::<u32>()?
    } else {
        // Estimate from duration
        let dur_output = std::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()?;
        
        let duration = String::from_utf8_lossy(&dur_output.stdout)
            .trim()
            .parse::<f32>()?;
        (duration * fps) as u32
    };
    
    // Get codec tag (Hap1, Hap5, HapY, etc)
    let tag_output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=codec_tag_string",
            "-of", "csv=p=0",
            path.to_str().unwrap(),
        ])
        .output()?;
    
    let format_tag = String::from_utf8_lossy(&tag_output.stdout).trim().to_string();
    
    Ok((width, height, fps, frame_count, format_tag))
}

fn extract_frames(input: &PathBuf, output: &mut File, frame_count: u32) -> anyhow::Result<Vec<u64>> {
    let mut offsets = Vec::with_capacity(frame_count as usize);
    
    // Use ffmpeg to output raw HAP frames
    let mut child = std::process::Command::new("ffmpeg")
        .args(&[
            "-i", input.to_str().unwrap(),
            "-c:v", "copy",        // Copy HAP data without re-encoding
            "-f", "rawvideo",
            "-",                    // Output to stdout
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    
    let mut stdout = child.stdout.take().unwrap();
    
    // Read frames one by one
    // HAP frames have a 4-byte header: frame_size (big-endian)
    let mut frame_buf = Vec::new();
    stdout.read_to_end(&mut frame_buf)?;
    
    // Parse HAP frames from the buffer
    // Each HAP frame starts with: size (4 bytes BE), type (4 bytes BE), ...
    let mut pos = 0usize;
    let mut frame_idx = 0u32;
    
    while pos < frame_buf.len() && frame_idx < frame_count {
        if pos + 8 > frame_buf.len() {
            break;
        }
        
        // Read frame size (big-endian)
        let frame_size = u32::from_be_bytes([
            frame_buf[pos], frame_buf[pos+1], 
            frame_buf[pos+2], frame_buf[pos+3]
        ]) as usize;
        
        if frame_size == 0 || pos + frame_size > frame_buf.len() {
            break;
        }
        
        // Record offset
        let current_offset = output.seek(SeekFrom::Current(0))?;
        offsets.push(current_offset);
        
        // Write frame data
        output.write_all(&frame_buf[pos..pos + frame_size])?;
        
        pos += frame_size;
        frame_idx += 1;
        
        if frame_idx % 30 == 0 {
            print!("\r  Frame {}/{}", frame_idx, frame_count);
            std::io::stdout().flush()?;
        }
    }
    
    println!("\r  Frame {}/{}", frame_idx, frame_count);
    
    // Wait for ffmpeg to finish
    let status = child.wait()?;
    if !status.success() {
        eprintln!("Warning: ffmpeg exited with status {}", status);
    }
    
    Ok(offsets)
}

fn write_header(file: &mut File, header: &HapHeader) -> anyhow::Result<()> {
    file.write_all(&header.magic)?;
    file.write_all(&header.version.to_le_bytes())?;
    file.write_all(&header.width.to_le_bytes())?;
    file.write_all(&header.height.to_le_bytes())?;
    file.write_all(&header.frame_count.to_le_bytes())?;
    file.write_all(&header.fps.to_le_bytes())?;
    file.write_all(&header.format_id.to_le_bytes())?;
    file.write_all(&header._padding)?;
    Ok(())
}

fn write_frame_entry(file: &mut File, entry: &FrameEntry) -> anyhow::Result<()> {
    file.write_all(&entry.chunk_count.to_le_bytes())?;
    file.write_all(&entry.pts.to_le_bytes())?;
    file.write_all(&entry.data_offset.to_le_bytes())?;
    Ok(())
}
