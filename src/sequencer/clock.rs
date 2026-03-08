use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Clock for sequencer timing
pub struct SequencerClock {
    /// BPM (beats per minute)
    bpm: f32,
    
    /// Sample rate for timing calculations
    sample_rate: f64,
    
    /// Current position in ticks (96 PPQN - parts per quarter note)
    ticks: u64,
    
    /// Last update time
    last_update: Instant,
    
    /// Accumulated time
    accum: Duration,
    
    /// Whether clock is running
    running: bool,
    
    /// Swing amount (0.0 - 1.0)
    swing: f32,
    
    /// External sync mode
    sync_mode: SyncMode,
    
    /// Tap tempo times (last 8 taps)
    tap_times: Vec<f64>,
    /// Last tap timestamp
    last_tap_time: f64,
    /// Tap tempo flash for UI
    tap_flash: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Internal,
    MidiClock,
    // AbletonLink, // Future
}

/// Clock events that can be generated
#[derive(Debug, Clone)]
pub enum ClockEvent {
    Step { track: usize, step: usize },
    Beat(u32),  // Beat number within bar
    Bar(u32),   // Bar number
}

impl SequencerClock {
    pub fn new() -> Self {
        Self {
            bpm: 120.0,
            sample_rate: 44100.0,
            ticks: 0,
            last_update: Instant::now(),
            accum: Duration::ZERO,
            running: false,
            swing: 0.0,
            sync_mode: SyncMode::Internal,
            tap_times: Vec::new(),
            last_tap_time: 0.0,
            tap_flash: 0.0,
        }
    }

    /// Update clock based on elapsed time
    /// Returns number of ticks advanced
    pub fn update(&mut self) -> u32 {
        if !self.running {
            return 0;
        }

        let now = Instant::now();
        let elapsed = now - self.last_update;
        self.last_update = now;

        self.accum += elapsed;

        // Calculate tick duration
        let tick_duration = self.tick_duration();

        let mut ticks_advanced = 0;
        while self.accum >= tick_duration {
            self.accum -= tick_duration;
            self.ticks += 1;
            ticks_advanced += 1;
        }

        ticks_advanced
    }

    /// Get duration of one tick
    fn tick_duration(&self) -> Duration {
        // 96 PPQN (pulses per quarter note)
        let ticks_per_beat = 96.0;
        let beats_per_second = self.bpm / 60.0;
        let ticks_per_second = ticks_per_beat * beats_per_second;
        let seconds_per_tick = 1.0 / ticks_per_second;
        
        Duration::from_secs_f64(seconds_per_tick as f64)
    }

    /// Start the clock
    pub fn start(&mut self) {
        self.running = true;
        self.last_update = Instant::now();
    }

    /// Stop the clock
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Reset to beginning
    pub fn reset(&mut self) {
        self.ticks = 0;
        self.accum = Duration::ZERO;
    }

    /// Set BPM
    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(20.0, 999.0);
    }

    /// Get current BPM
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get current position
    pub fn position(&self) -> (u32, u32, u32) {
        // (bar, beat, tick)
        let ticks_per_beat = 96;
        let beats_per_bar = 4;
        
        let total_beats = self.ticks / ticks_per_beat as u64;
        let bar = (total_beats / beats_per_bar) as u32;
        let beat = (total_beats % beats_per_bar) as u32;
        let tick = (self.ticks % ticks_per_beat as u64) as u32;
        
        (bar, beat, tick)
    }

    /// Get swing amount
    pub fn swing(&self) -> f32 {
        self.swing
    }

    /// Set swing amount (0.0 - 1.0)
    pub fn set_swing(&mut self, swing: f32) {
        self.swing = swing.clamp(0.0, 1.0);
    }

    /// Calculate swing offset for a step
    /// Returns offset in ticks (positive = delayed)
    pub fn swing_offset(&self, step: usize) -> i32 {
        if self.swing <= 0.0 {
            return 0;
        }
        
        // Apply swing to even-numbered 16th notes (steps 1, 3, 5, 7...)
        if step % 2 == 1 {
            // Max swing = 66% (shuffle feel)
            let max_offset = 48; // Half a beat in ticks
            ((self.swing * max_offset as f32) as i32)
        } else {
            0
        }
    }

    /// Tap tempo - call this on each tap
    /// Returns calculated BPM after 4 taps, resets phase every tap
    pub fn tap(&mut self) -> Option<f32> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        
        // Clear taps if it's been too long since last tap (2 seconds)
        if now - self.last_tap_time > 2.0 {
            self.tap_times.clear();
        }
        
        // Add tap time
        self.tap_times.push(now);
        
        // Keep only last 8 taps
        if self.tap_times.len() > 8 {
            self.tap_times.remove(0);
        }
        
        // Update last tap time
        self.last_tap_time = now;
        
        // Set flash for UI feedback
        self.tap_flash = 1.0;
        
        // Reset phase on every tap (sync to beat)
        self.accum = Duration::ZERO;
        
        // Calculate BPM from tap intervals (need at least 4 taps)
        if self.tap_times.len() >= 4 {
            let mut intervals = Vec::new();
            for i in 1..self.tap_times.len() {
                intervals.push(self.tap_times[i] - self.tap_times[i-1]);
            }
            
            // Average interval
            let avg_interval: f64 = intervals.iter().sum::<f64>() / intervals.len() as f64;
            
            if avg_interval > 0.1 && avg_interval < 3.0 { // Reasonable range (20-600 BPM)
                let new_bpm = (60.0 / avg_interval) as f32;
                self.bpm = new_bpm.clamp(20.0, 999.0);
                return Some(self.bpm);
            }
        }
        
        None
    }
    
    /// Get tap tempo flash value (0.0 - 1.0, decays over time)
    pub fn tap_flash(&self) -> f32 {
        self.tap_flash
    }
    
    /// Update tap flash (call in update loop)
    pub fn update_tap_flash(&mut self, dt: Duration) {
        self.tap_flash = (self.tap_flash - dt.as_secs_f32() * 3.0).max(0.0);
    }
    
    /// Get number of taps recorded (capped at 4 for display purposes)
    pub fn tap_count(&self) -> usize {
        self.tap_times.len().min(4)
    }

    /// Get current tick count
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Convert step to tick position
    pub fn step_to_tick(step: usize, steps_per_beat: usize) -> u64 {
        let ticks_per_beat = 96u64;
        let step_ticks = ticks_per_beat / steps_per_beat as u64;
        step as u64 * step_ticks
    }
}

impl Default for SequencerClock {
    fn default() -> Self {
        Self::new()
    }
}
