use super::sample::VideoSample;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::fs::OpenOptions;
use std::io::Write;

fn debug_log(msg: &str) {
    use std::sync::OnceLock;
    static DEBUG_LOG: OnceLock<std::sync::Mutex<std::fs::File>> = OnceLock::new();
    
    let mutex = DEBUG_LOG.get_or_init(|| {
        Mutex::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/rustjay404_debug.log")
                .unwrap()
        )
    });
    
    if let Ok(mut file) = mutex.lock() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let _ = writeln!(file, "[{:.3}] {}", timestamp, msg);
        let _ = file.flush();
    }
}

/// Trigger modes - SP-404 style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    /// Play while held, stop on release
    Gate,
    /// Toggle on/off with each trigger
    Latch,
    /// Play once and stop at end
    OneShot,
}

impl Default for TriggerMode {
    fn default() -> Self {
        TriggerMode::Gate
    }
}

/// Blend modes for mixing (legacy - mapped to MixMode in engine)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Replace,
    Add,
    Multiply,
    Screen,
    Alpha,
}

impl Default for BlendMode {
    fn default() -> Self {
        BlendMode::Replace
    }
}

/// Mix mode including keying effects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PadMixMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Lighten,
    Darken,
    ChromaKey,
    LumaKey,
}

impl Default for PadMixMode {
    fn default() -> Self {
        PadMixMode::Normal
    }
}

/// Keying parameters for chroma/luma key
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PadKeyParams {
    /// Key color for chroma key [R, G, B] (0-1)
    pub key_color: [f32; 3],
    /// Distance/brightness threshold (0-1)
    pub threshold: f32,
    /// Edge smoothness (0-1)
    pub smoothness: f32,
    /// Invert the key (for luma key)
    pub invert: bool,
}

impl Default for PadKeyParams {
    fn default() -> Self {
        Self {
            key_color: [0.0, 1.0, 0.0], // Default green screen
            threshold: 0.3,
            smoothness: 0.1,
            invert: false,
        }
    }
}

/// A sample pad - wraps a VideoSample with SP-404 style triggering
pub struct SamplePad {
    pub index: usize,           // 0-15 within bank
    pub name: String,
    pub color: [u8; 3],

    // Sample
    pub sample: Option<Arc<Mutex<VideoSample>>>,

    // Trigger settings
    pub trigger_mode: TriggerMode,
    pub loop_enabled: bool,
    pub speed: f32,             // Playback speed (-2.0 to 2.0)

    // Playback state
    pub is_playing: bool,
    pub is_triggered: bool,     // Currently held (for GATE mode)
    pub current_frame: f32,     // Sub-frame precision for smooth speed changes
    pub direction: i8,          // 1 or -1 for forward/reverse

    // MIDI
    pub midi_note: Option<u8>,

    // Mixing
    pub volume: f32,            // 0.0 - 1.0
    pub blend_mode: BlendMode,  // Legacy blend mode
    pub mix_mode: PadMixMode,   // New mix mode with keying support
    pub key_params: PadKeyParams, // Keying parameters
    
    // Visual feedback
    pub trigger_level: f32,     // For UI animation (0.0 - 1.0)
}

impl SamplePad {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            name: format!("Pad {}", index + 1),
            color: Self::default_color(index),
            sample: None,
            trigger_mode: TriggerMode::default(),
            loop_enabled: false,
            speed: 1.0,
            is_playing: false,
            is_triggered: false,
            current_frame: 0.0,
            direction: 1,
            midi_note: None,
            volume: 1.0,
            blend_mode: BlendMode::default(),
            mix_mode: PadMixMode::default(),
            key_params: PadKeyParams::default(),
            trigger_level: 0.0,
        }
    }

    /// Default color based on pad index (SP-404 style)
    fn default_color(index: usize) -> [u8; 3] {
        let colors = [
            [255, 100, 100],  // Red
            [255, 150, 50],   // Orange
            [255, 220, 50],   // Yellow
            [150, 255, 100],  // Green
            [100, 255, 150],  // Teal
            [100, 200, 255],  // Blue
            [150, 100, 255],  // Purple
            [255, 100, 200],  // Pink
            [255, 80, 80],
            [255, 180, 80],
            [220, 255, 80],
            [80, 255, 120],
            [80, 220, 255],
            [120, 100, 255],
            [220, 80, 255],
            [255, 80, 180],
        ];
        colors[index % colors.len()]
    }

    /// Assign a sample to this pad
    pub fn assign_sample(&mut self, sample: VideoSample) {
        self.sample = Some(Arc::new(Mutex::new(sample)));
        self.current_frame = 0.0;
    }

    /// Clear the sample from this pad
    pub fn clear_sample(&mut self) {
        self.sample = None;
        self.stop();
        self.name = format!("Pad {}", self.index + 1);
    }
    
    /// Clear the pad (alias for clear_sample for UI)
    pub fn clear(&mut self) {
        self.clear_sample();
    }

    /// Trigger the pad (start playback)
    pub fn trigger(&mut self) {
        self.is_triggered = true;
        self.trigger_level = 1.0;

        match self.trigger_mode {
            TriggerMode::Gate => {
                self.is_playing = true;
                self.current_frame = self.get_in_point() as f32;
            }
            TriggerMode::Latch => {
                self.is_playing = !self.is_playing;
                if self.is_playing {
                    self.current_frame = self.get_in_point() as f32;
                }
            }
            TriggerMode::OneShot => {
                self.is_playing = true;
                self.current_frame = self.get_in_point() as f32;
                self.direction = if self.speed >= 0.0 { 1 } else { -1 };
            }
        }
        
        debug_log(&format!("[PAD TRIGGER] Pad {}: SET playing=true, frame={:.1} (in_point={})", 
            self.index, self.current_frame, self.get_in_point()));
    }

    /// Release the pad (for GATE mode)
    pub fn release(&mut self) {
        self.is_triggered = false;

        if self.trigger_mode == TriggerMode::Gate {
            self.is_playing = false;
        }
    }

    /// Stop playback
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.is_triggered = false;
        self.current_frame = self.get_in_point() as f32;
    }

    /// Update playback position (call every frame)
    pub fn update(&mut self, dt: Duration) {
        // Decay trigger level for UI animation
        self.trigger_level = (self.trigger_level - dt.as_secs_f32() * 5.0).max(0.0);

        if !self.is_playing {
            return;
        }

        // Check if sample exists
        if self.sample.is_none() {
            debug_log(&format!("[PAD UPDATE] Pad {} stopped - no sample", self.index));
            self.is_playing = false;
            return;
        }
        
        let sample_arc = self.sample.as_ref().unwrap();

        // Use try_lock to avoid blocking - skip update if sample is busy
        let sample = match sample_arc.try_lock() {
            Ok(guard) => guard,
            Err(_) => return, // Skip this update cycle if sample is locked
        };
        
        let fps = sample.fps.max(1.0);
        // Use the already-locked sample to get in/out points
        let in_point = sample.in_point as f32;
        let out_point = sample.out_point as f32;

        // Advance frame based on speed and time
        let clamped_speed = self.speed.clamp(-5.0, 5.0);
        
        // For reverse playback, use slightly smaller time steps for smoother seeking
        let dt_factor = if clamped_speed < 0.0 { 0.6 } else { 1.0 };
        let frame_delta = clamped_speed * fps * dt.as_secs_f32() * dt_factor;
        
        self.current_frame += frame_delta;
        
        // Clamp to valid range
        let effective_in = in_point.max(0.0);
        let effective_out = out_point.min(sample.frame_count as f32);

        // Handle loop/reach end with seamless wrapping
        if clamped_speed >= 0.0 {
            if self.current_frame >= effective_out {
                if self.loop_enabled || self.trigger_mode == TriggerMode::Latch || self.trigger_mode == TriggerMode::Gate {
                    // For Gate mode: stay at end while key held (is_triggered=true)
                    // For Latch/Loop: wrap to beginning
                    if self.trigger_mode == TriggerMode::Gate && !self.loop_enabled {
                        // Stay at the end frame while key is held
                        self.current_frame = effective_out;
                    } else {
                        // Wrap to beginning seamlessly
                        self.current_frame = effective_in + (self.current_frame - effective_out);
                    }
                } else {
                    self.current_frame = effective_out;
                    self.is_playing = false;
                }
            }
        } else {
            // Reverse playback
            if self.current_frame <= effective_in {
                if self.loop_enabled || self.trigger_mode == TriggerMode::Gate {
                    // For Gate mode: stay at start while key held
                    if self.trigger_mode == TriggerMode::Gate && !self.loop_enabled {
                        self.current_frame = effective_in;
                    } else {
                        // Wrap to end seamlessly
                        let overflow = effective_in - self.current_frame;
                        self.current_frame = effective_out - overflow;
                    }
                } else {
                    log::info!("[PAD {}] Stopped at start (reverse playback)", self.index);
                    self.current_frame = effective_in;
                    self.is_playing = false;
                }
            }
        }
    }

    /// Get current texture for rendering
    pub fn get_current_frame(&mut self) -> Option<Arc<wgpu::Texture>> {
        if !self.is_playing {
            return None;
        }
        
        let sample_arc = self.sample.as_ref()?;
        
        // Lock the sample - in single-threaded context this should always succeed
        let mut sample = sample_arc.try_lock().ok()?;
        
        let frame = self.current_frame as u32;
        sample.get_frame(frame)
    }
    
    /// Get color space for this pad's sample
    pub fn color_space(&self) -> super::sample::ColorSpace {
        if let Some(ref sample_arc) = self.sample {
            if let Ok(sample) = sample_arc.try_lock() {
                return sample.color_space();
            }
        }
        super::sample::ColorSpace::Rgb
    }

    /// Get the sample's in point
    fn get_in_point(&self) -> u32 {
        self.sample
            .as_ref()
            .map(|s| s.try_lock().map(|g| g.in_point).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Get the sample's out point
    fn get_out_point(&self) -> u32 {
        self.sample
            .as_ref()
            .map(|s| s.try_lock().map(|g| g.out_point).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Check if pad has a loaded sample
    pub fn has_sample(&self) -> bool {
        self.sample
            .as_ref()
            .map(|s| s.try_lock().map(|g| g.is_loaded()).unwrap_or(false))
            .unwrap_or(false)
    }

    /// Get playback progress (0.0 - 1.0)
    pub fn progress(&self) -> f32 {
        let Some(ref sample) = self.sample else {
            return 0.0;
        };
        
        let sample = match sample.try_lock() {
            Ok(guard) => guard,
            Err(_) => return 0.0,
        };
        
        let range = sample.out_point.saturating_sub(sample.in_point) as f32;
        
        if range <= 0.0 {
            return 0.0;
        }
        
        let current = self.current_frame - sample.in_point as f32;
        (current / range).clamp(0.0, 1.0)
    }

    /// Set playback speed with direction handling
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(-2.0, 2.0);
        self.direction = if self.speed >= 0.0 { 1 } else { -1 };
    }
}

impl Clone for SamplePad {
    fn clone(&self) -> Self {
        Self {
            index: self.index,
            name: self.name.clone(),
            color: self.color,
            sample: self.sample.clone(),
            trigger_mode: self.trigger_mode,
            loop_enabled: self.loop_enabled,
            speed: self.speed,
            is_playing: false, // Cloned pads start stopped
            is_triggered: false,
            current_frame: 0.0,
            direction: self.direction,
            midi_note: self.midi_note,
            volume: self.volume,
            blend_mode: self.blend_mode,
            mix_mode: self.mix_mode,
            key_params: self.key_params,
            trigger_level: 0.0,
        }
    }
}
