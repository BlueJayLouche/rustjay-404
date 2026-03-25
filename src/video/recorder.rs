//! Video recorder for live sampling
//!
//! Records from webcam/NDI/Syphon, buffers frames, and encodes to HAP format.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::video::capture::webcam::{CapturedFrame, WebcamCapture};

#[cfg(target_os = "macos")]
use crate::video::interapp::syphon_input::{SyphonInputReceiver, SyphonFrame};

/// Recording state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Encoding,
    Error,
}

/// A single recording session
pub struct Recording {
    /// Recording start time
    pub start_time: Instant,
    /// Captured frames
    pub frames: Vec<CapturedFrame>,
    /// Target FPS
    fps: f32,
    /// Resolution
    resolution: (u32, u32),
}

impl Recording {
    pub fn new(resolution: (u32, u32), fps: f32) -> Self {
        Self {
            start_time: Instant::now(),
            frames: Vec::new(),
            fps,
            resolution,
        }
    }
    
    pub fn add_frame(&mut self, frame: CapturedFrame) {
        self.frames.push(frame);
    }
    
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    
    pub fn duration(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// Save recording as HAP file using native encoding
    ///
    /// # Arguments
    /// * `output_path` - Where to write the .mov file
    /// * `format` - HAP format to encode to (DXT1 is fastest, DXT5-YCoCg is highest quality)
    pub fn save_to_hap(
        &self,
        output_path: &Path,
        format: crate::video::encoder::HapEncodeFormat,
    ) -> anyhow::Result<()> {
        use rayon::prelude::*;
        use crate::video::encoder::encode_frames_to_hap;

        if self.frames.is_empty() {
            return Err(anyhow::anyhow!("No frames to save"));
        }

        let (width, height) = self.resolution;

        log::info!(
            "Saving {} frames ({}x{} @ {}fps) as {:?} using native encoder...",
            self.frames.len(),
            width,
            height,
            self.fps,
            format
        );

        // Parallel RGB24 → RGBA32 conversion
        let rgba_frames: Vec<Vec<u8>> = self.frames.par_iter()
            .map(|frame| {
                if frame.data.len() == (frame.width * frame.height * 3) as usize {
                    let pixel_count = (frame.width * frame.height) as usize;
                    let mut rgba = vec![0u8; pixel_count * 4];
                    for (i, rgb) in frame.data.chunks_exact(3).enumerate() {
                        let off = i * 4;
                        rgba[off] = rgb[0];
                        rgba[off + 1] = rgb[1];
                        rgba[off + 2] = rgb[2];
                        rgba[off + 3] = 255;
                    }
                    rgba
                } else {
                    frame.data.clone()
                }
            })
            .collect();

        encode_frames_to_hap(
            &rgba_frames,
            width,
            height,
            self.fps,
            format,
            output_path,
        )?;

        log::info!("Recording saved to: {:?}", output_path);
        Ok(())
    }
}

/// Live sampler that records from input
pub struct LiveSampler {
    /// Current recording state
    state: Arc<Mutex<RecordingState>>,
    /// Active webcam capture
    webcam: Option<WebcamCapture>,
    /// Syphon input receiver (macOS only)
    #[cfg(target_os = "macos")]
    syphon: Option<SyphonInputReceiver>,
    /// Current recording
    current_recording: Option<Recording>,
    /// Recording buffer (for when we're recording)
    buffer: Arc<Mutex<Vec<CapturedFrame>>>,
    /// Target FPS
    fps: f32,
}

impl LiveSampler {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            webcam: None,
            #[cfg(target_os = "macos")]
            syphon: None,
            current_recording: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            fps: 30.0,
        }
    }
    
    /// Initialize webcam with warm-up
    pub fn init_webcam(&mut self, device_index: u32) -> anyhow::Result<()> {
        // Stop any existing capture
        self.shutdown();
        
        let mut webcam = WebcamCapture::new();
        webcam.start(device_index)?;
        
        // Warm up: capture and discard first few frames
        // Cameras often output black frames while initializing
        log::info!("Warming up camera...");
        std::thread::sleep(std::time::Duration::from_millis(500));
        for _ in 0..5 {
            let _ = webcam.get_frame();
        }
        
        self.webcam = Some(webcam);
        log::info!("Camera ready");
        Ok(())
    }
    
    /// Initialize Syphon input (macOS only)
    #[cfg(target_os = "macos")]
    pub fn init_syphon(&mut self, server_name: &str) -> anyhow::Result<()> {
        use crate::video::interapp::syphon_input::SyphonInputReceiver;
        
        // Stop any existing capture
        self.shutdown();
        
        log::info!("Initializing Syphon input from: {}", server_name);
        
        let mut syphon = SyphonInputReceiver::new();
        syphon.connect(server_name)
            .map_err(|e| anyhow::anyhow!("Failed to connect to Syphon server '{}': {}", server_name, e))?;
        
        // Warm up: capture and discard first few frames
        log::info!("Warming up Syphon connection...");
        std::thread::sleep(std::time::Duration::from_millis(100));
        for _ in 0..3 {
            let _ = syphon.try_receive();
        }
        
        self.syphon = Some(syphon);
        log::info!("Syphon input ready");
        Ok(())
    }
    
    /// Stub for non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    pub fn init_syphon(&mut self, _server_name: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("Syphon input is only available on macOS"))
    }
    
    /// Get current state
    pub fn state(&self) -> RecordingState {
        *self.state.lock().unwrap()
    }
    
    /// Start recording (uses internal capture source resolution)
    pub fn start_recording(&mut self) -> anyhow::Result<()> {
        // Determine resolution from whichever capture source is active
        let resolution = if let Some(ref webcam) = self.webcam {
            webcam.resolution()
        } else {
            #[cfg(target_os = "macos")]
            {
                if let Some(ref syphon) = self.syphon {
                    syphon.resolution()
                } else {
                    return Err(anyhow::anyhow!("No capture source initialized"));
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Err(anyhow::anyhow!("No capture source initialized"));
            }
        };

        self.start_recording_with_resolution(resolution)
    }

    /// Start recording with externally provided resolution (from VideoInputManager)
    pub fn start_recording_with_resolution(&mut self, resolution: (u32, u32)) -> anyhow::Result<()> {
        *self.state.lock().unwrap() = RecordingState::Recording;
        self.current_recording = Some(Recording::new(resolution, self.fps));

        log::info!("Started recording at {:?}", resolution);
        Ok(())
    }

    /// Add a frame from an external source (VideoInputManager)
    pub fn add_frame(&mut self, frame: CapturedFrame) {
        if *self.state.lock().unwrap() == RecordingState::Recording {
            if let Some(ref mut recording) = self.current_recording {
                recording.add_frame(frame);
            }
        }
    }
    
    /// Stop recording and save to file
    pub fn stop_recording(
        &mut self,
        output_path: &Path,
        format: crate::video::encoder::HapEncodeFormat,
    ) -> anyhow::Result<PathBuf> {
        *self.state.lock().unwrap() = RecordingState::Encoding;
        
        if let Some(mut recording) = self.current_recording.take() {
            // Drop the last frame as it might be partial/corrupted
            // This commonly happens when stopping the recording
            if recording.frame_count() > 5 {
                log::debug!("Dropping last frame to avoid partial frame at end");
                recording.frames.pop();
            }
            
            // Also check if first frame is mostly black and skip it
            if recording.frame_count() > 1 {
                if let Some(first_frame) = recording.frames.first() {
                    let avg_brightness: u32 = first_frame.data.iter().map(|&b| b as u32).sum::<u32>() 
                        / first_frame.data.len() as u32;
                    if avg_brightness < 20 { // Very dark
                        log::debug!("Dropping black first frame (avg brightness: {})", avg_brightness);
                        recording.frames.remove(0);
                    }
                }
            }
            
            recording.save_to_hap(output_path, format)?;
            *self.state.lock().unwrap() = RecordingState::Idle;
            Ok(output_path.to_path_buf())
        } else {
            *self.state.lock().unwrap() = RecordingState::Error;
            Err(anyhow::anyhow!("No active recording"))
        }
    }
    
    /// Stop recording without saving (discard)
    pub fn cancel_recording(&mut self) {
        self.current_recording = None;
        *self.state.lock().unwrap() = RecordingState::Idle;
        log::info!("Recording cancelled");
    }
    
    /// Update - should be called every frame to capture
    pub fn update(&mut self) {
        if *self.state.lock().unwrap() == RecordingState::Recording {
            // Try webcam first
            if let Some(ref mut webcam) = self.webcam {
                if let Some(frame) = webcam.get_frame() {
                    if let Some(ref mut recording) = self.current_recording {
                        recording.add_frame(frame);
                    }
                }
            }
            
            // Try Syphon (macOS only)
            #[cfg(target_os = "macos")]
            {
                if let Some(ref mut syphon) = self.syphon {
                    if let Some(syphon_frame) = syphon.try_receive() {
                        // Convert SyphonFrame to CapturedFrame
                        let frame = CapturedFrame {
                            width: syphon_frame.width,
                            height: syphon_frame.height,
                            data: syphon_frame.data, // Now RGBA after conversion
                            timestamp: syphon_frame.timestamp,
                        };
                        if let Some(ref mut recording) = self.current_recording {
                            recording.add_frame(frame);
                        }
                    }
                }
            }
        }
    }
    
    /// Get preview frame for UI
    pub fn get_preview(&mut self) -> Option<CapturedFrame> {
        // Try webcam first
        if let Some(frame) = self.webcam.as_mut()?.get_frame() {
            return Some(frame);
        }
        
        // Try Syphon (macOS only)
        #[cfg(target_os = "macos")]
        {
            if let Some(ref mut syphon) = self.syphon {
                if let Some(syphon_frame) = syphon.try_receive() {
                    return Some(CapturedFrame {
                        width: syphon_frame.width,
                        height: syphon_frame.height,
                        data: syphon_frame.data,
                        timestamp: syphon_frame.timestamp,
                    });
                }
            }
        }
        
        None
    }
    
    /// Get recording duration
    pub fn recording_duration(&self) -> Option<Duration> {
        self.current_recording.as_ref().map(|r| r.duration())
    }
    
    /// Get recording frame count
    pub fn recording_frame_count(&self) -> usize {
        self.current_recording.as_ref().map(|r| r.frame_count()).unwrap_or(0)
    }
    
    /// Shutdown
    pub fn shutdown(&mut self) {
        if let Some(mut webcam) = self.webcam.take() {
            webcam.stop();
        }
        
        #[cfg(target_os = "macos")]
        {
            self.syphon = None; // Drop the Syphon receiver
        }
    }
}

impl Drop for LiveSampler {
    fn drop(&mut self) {
        self.shutdown();
    }
}
