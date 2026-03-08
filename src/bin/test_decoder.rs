// Headless test for decoder thread architecture
// Usage: cargo run --bin test_decoder -- <video_path> [--speed <speed>]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use clap::Parser;

/// Decoder test
#[derive(Parser, Debug)]
#[command(name = "test_decoder")]
struct Args {
    /// Video file path
    video: PathBuf,
    
    /// Playback speed (negative for reverse)
    #[arg(short, long, default_value = "1.0")]
    speed: f32,
}

/// Frame cache - stores decoded frames for playback
struct FrameCache {
    frames: HashMap<u32, Vec<u8>>,
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
    
    /// Get the next frame to display and advance playback position
    fn get_display_frame(&mut self) -> Option<(u32, Vec<u8>)> {
        let target = self.current_frame;
        
        if let Some(data) = self.frames.remove(&target) {
            // Advance position
            let next = self.current_frame as f32 + self.speed;
            if self.speed >= 0.0 {
                self.current_frame = if next >= self.frame_count as f32 { 0 } else { next as u32 };
            } else {
                self.current_frame = if next < 0.0 { self.frame_count - 1 } else { next as u32 };
            }
            
            return Some((target, data));
        }
        
        None
    }
    
    fn add_frame(&mut self, frame_num: u32, data: Vec<u8>) {
        self.frames.insert(frame_num, data);
    }
    
    /// Get the range of frames we need to decode
    fn get_needed_range(&self) -> (u32, u32) {
        const WINDOW_SIZE: u32 = 32; // Larger window for smoother playback
        
        if self.speed >= 0.0 {
            // Forward: decode current and ahead
            let start = self.current_frame;
            let end = (start + WINDOW_SIZE).min(self.frame_count - 1);
            (start, end)
        } else {
            // Reverse: decode window ending at current
            let end = self.current_frame;
            let start = if end >= WINDOW_SIZE { end - WINDOW_SIZE } else { 0 };
            (start, end)
        }
    }
    
    fn has_frame(&self, frame: u32) -> bool {
        self.frames.contains_key(&frame)
    }
    
    fn len(&self) -> usize {
        self.frames.len()
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    
    println!("Decoder Thread Test");
    println!("Video: {:?}", args.video);
    println!("Speed: {:.1}x", args.speed);
    println!();
    
    // Probe video
    let (width, height, fps, frame_count) = probe_video(&args.video)?;
    println!("Video: {}x{} @ {:.2}fps, {} frames", width, height, fps, frame_count);
    println!();
    
    // Create shared frame cache
    let frame_cache = Arc::new(Mutex::new(FrameCache::new(frame_count, args.speed)));
    
    // Spawn decoder thread
    let video_path = args.video.clone();
    let cache_clone = frame_cache.clone();
    
    let decoder_handle = std::thread::spawn(move || {
        decoder_thread(video_path, cache_clone, width, height, fps);
    });
    
    // Wait for initial frame to be ready
    println!("Waiting for frame {} to be ready...", 
        if args.speed >= 0.0 { 0 } else { frame_count - 1 });
    let target_frame = if args.speed >= 0.0 { 0 } else { frame_count - 1 };
    loop {
        let cache = frame_cache.lock().unwrap();
        if cache.has_frame(target_frame) {
            println!("Frame {} ready! (cache: {} frames)", target_frame, cache.len());
            break;
        }
        drop(cache);
        std::thread::sleep(Duration::from_millis(10));
    }
    
    // Simulate playback
    let frame_duration = Duration::from_secs_f32(1.0 / fps.abs());
    let test_duration = Duration::from_secs(5);
    let start_time = Instant::now();
    let mut frame_count_played = 0;
    let mut underruns = 0;
    
    println!("Starting playback simulation for 5 seconds...");
    println!();
    
    while start_time.elapsed() < test_duration {
        let frame_start = Instant::now();
        
        let mut cache = frame_cache.lock().unwrap();
        
        if let Some((frame_num, _data)) = cache.get_display_frame() {
            if frame_count_played % 10 == 0 {
                println!("Frame {} (cache: {})", frame_num, cache.len());
            }
            frame_count_played += 1;
        } else {
            underruns += 1;
            if underruns % 30 == 0 {
                let current = cache.current_frame;
                println!("Underrun at frame {}! (total: {})", current, underruns);
            }
        }
        
        drop(cache);
        
        // Sleep to maintain frame rate
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
    
    println!();
    println!("=== Results ===");
    println!("Frames played: {}", frame_count_played);
    println!("Underruns: {}", underruns);
    println!("Target FPS: {:.1}", fps.abs());
    println!("Avg FPS: {:.1}", frame_count_played as f32 / test_duration.as_secs_f32());
    
    drop(decoder_handle);
    
    Ok(())
}

fn probe_video(path: &PathBuf) -> anyhow::Result<(u32, u32, f32, u32)> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
            "-of", "csv=p=0",
            path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run ffprobe: {}", e))?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!("ffprobe failed"));
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
        let dur_output = std::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output();
        
        if let Ok(dur) = dur_output {
            let duration = String::from_utf8_lossy(&dur.stdout)
                .trim()
                .parse::<f32>()
                .unwrap_or(0.0);
            (duration * fps) as u32
        } else {
            30
        }
    } else {
        frame_count
    };
    
    Ok((width, height, fps, frame_count))
}

fn decoder_thread(
    video_path: PathBuf,
    frame_cache: Arc<Mutex<FrameCache>>,
    width: u32,
    height: u32,
    fps: f32,
) {
    use std::io::Read;
    
    let frame_size = (width * height * 4) as usize;
    let mut ffmpeg: Option<std::process::Child> = None;
    let mut reader: Option<std::io::BufReader<std::process::ChildStdout>> = None;
    let mut stream_position: u32 = 0;
    
    loop {
        // Get the range we need to decode
        let (needed_start, needed_end) = {
            let cache = frame_cache.lock().unwrap();
            cache.get_needed_range()
        };
        
        // Check if we need to restart ffmpeg
        let needs_restart = match &ffmpeg {
            None => true,
            Some(_) => {
                // Restart if our current stream doesn't cover the needed range
                stream_position > needed_end || 
                stream_position + 16 < needed_start
            }
        };
        
        if needs_restart {
            if let Some(mut child) = ffmpeg.take() {
                let _ = child.kill();
            }
            reader = None;
            
            let timestamp = needed_start as f32 / fps;
            println!("[Decoder] Decoding range [{}-{}] at {:.3}s", needed_start, needed_end, timestamp);
            
            match std::process::Command::new("ffmpeg")
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
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    stream_position = needed_start;
                    reader = Some(std::io::BufReader::with_capacity(
                        frame_size * 16,
                        child.stdout.take().unwrap()
                    ));
                    ffmpeg = Some(child);
                }
                Err(e) => {
                    eprintln!("[Decoder] Failed to start ffmpeg: {}", e);
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        }
        
        // Check if we've decoded enough for current window
        let have_all_needed = {
            let cache = frame_cache.lock().unwrap();
            (needed_start..=needed_end).all(|f| cache.has_frame(f))
        };
        
        if have_all_needed {
            // We have all frames for this window, slow down decoding
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        
        // Read and decode frames
        if let Some(ref mut r) = reader {
            let mut frame_data = vec![0u8; frame_size];
            
            match r.read_exact(&mut frame_data) {
                Ok(_) => {
                    let mut cache = frame_cache.lock().unwrap();
                    
                    // Only add frame if it's in our needed range
                    if stream_position >= needed_start && stream_position <= needed_end {
                        cache.add_frame(stream_position, frame_data);
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
