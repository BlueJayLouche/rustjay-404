use super::step::Step;
use serde::{Deserialize, Serialize};

/// Active gate tracking for release events
#[derive(Debug, Clone, Copy)]
pub struct ActiveGate {
    /// When this gate should close (in ticks from start)
    pub end_tick: u64,
    /// Which step created this gate
    pub step_index: usize,
}

/// A sequencer track - controls one pad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencerTrack {
    /// Which pad this track controls (0-15)
    pub pad_index: usize,
    
    /// Steps in this track
    pub steps: Vec<Step>,
    
    /// Number of active steps (can be less than steps.len())
    pub length: usize,
    
    /// Current playback step
    #[serde(skip)]
    pub current_step: usize,
    
    /// Whether this track is playing
    #[serde(skip)]
    pub is_playing: bool,
    
    /// Mute state
    pub muted: bool,
    
    /// Solo state
    pub solo: bool,
    
    /// Override all step probabilities
    pub probability_override: Option<f32>,
    
    /// Track name (defaults to pad name)
    pub name: Option<String>,
    
    /// Accumulated ticks for step timing (not serialized)
    #[serde(skip)]
    pub tick_accumulator: u32,
    
    /// Currently active gates that need to be released (not serialized)
    #[serde(skip)]
    pub active_gates: Vec<ActiveGate>,
}

impl SequencerTrack {
    pub fn new(pad_index: usize) -> Self {
        Self {
            pad_index,
            steps: vec![Step::new(); 64], // Max 64 steps
            length: 16,                    // Default 16 steps
            current_step: 0,
            is_playing: false,
            muted: false,
            solo: false,
            probability_override: None,
            name: None,
            tick_accumulator: 0,
            active_gates: Vec::new(),
        }
    }

    /// Get the current step
    pub fn current(&self) -> &Step {
        &self.steps[self.current_step]
    }

    /// Get mutable current step
    pub fn current_mut(&mut self) -> &mut Step {
        &mut self.steps[self.current_step]
    }

    /// Get a specific step
    pub fn get_step(&self, index: usize) -> Option<&Step> {
        self.steps.get(index)
    }

    /// Get mutable step
    pub fn get_step_mut(&mut self, index: usize) -> Option<&mut Step> {
        self.steps.get_mut(index)
    }

    /// Advance to next step
    /// Returns true if wrapped around
    pub fn advance(&mut self) -> bool {
        self.current_step += 1;
        
        if self.current_step >= self.length {
            self.current_step = 0;
            true
        } else {
            false
        }
    }

    /// Reset to first step
    pub fn reset(&mut self) {
        self.current_step = 0;
        self.tick_accumulator = 0;
        self.active_gates.clear();
    }
    
    /// Update active gates and return true if a release should be emitted
    /// Call this every tick with the current global tick count
    pub fn update_gates(&mut self, current_tick: u64) -> bool {
        let mut should_release = false;
        self.active_gates.retain(|gate| {
            if current_tick >= gate.end_tick {
                should_release = true;
                false // Remove this gate
            } else {
                true // Keep it
            }
        });
        should_release
    }
    
    /// Add a new active gate
    pub fn add_gate(&mut self, start_tick: u64, gate_length_ticks: u32) {
        let end_tick = start_tick + gate_length_ticks as u64;
        self.active_gates.push(ActiveGate {
            end_tick,
            step_index: self.current_step,
        });
    }
    
    /// Check if there are any active gates
    pub fn has_active_gates(&self) -> bool {
        !self.active_gates.is_empty()
    }

    /// Set the sequence length
    pub fn set_length(&mut self, length: usize) {
        self.length = length.clamp(1, 64);
        
        // Ensure current step is valid
        if self.current_step >= self.length {
            self.current_step = 0;
        }
    }

    /// Toggle a step on/off
    pub fn toggle_step(&mut self, step: usize) {
        if let Some(s) = self.steps.get_mut(step) {
            s.toggle();
        }
    }

    /// Clear all steps
    pub fn clear(&mut self) {
        for step in &mut self.steps {
            *step = Step::new();
        }
        self.current_step = 0;
    }

    /// Randomize steps
    pub fn randomize(&mut self, density: f32) {
        for i in 0..self.length {
            self.steps[i].active = super::step::rand::random::<f32>() < density;
            self.steps[i].velocity = 0.5 + super::step::rand::random::<f32>() * 0.5;
        }
    }

    /// Check if track should trigger current step
    pub fn should_trigger(&self) -> bool {
        if self.muted {
            return false;
        }
        
        let step = self.current();
        
        // Check probability override
        if let Some(prob) = self.probability_override {
            if prob <= 0.0 {
                return false;
            }
            if prob < 1.0 && super::step::rand::random::<f32>() > prob {
                return false;
            }
        }
        
        step.should_trigger()
    }

    /// Get display name
    pub fn display_name(&self) -> String {
        self.name.clone()
            .unwrap_or_else(|| format!("Pad {}", self.pad_index + 1))
    }

    /// Get step data for a range (for UI)
    pub fn get_step_range(&self, start: usize, count: usize) -> &[Step] {
        let end = (start + count).min(self.steps.len());
        &self.steps[start..end]
    }
}

impl Default for SequencerTrack {
    fn default() -> Self {
        Self::new(0)
    }
}
