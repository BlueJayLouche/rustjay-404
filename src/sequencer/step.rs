use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step in a sequence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Whether this step is active (contains a trigger)
    pub active: bool,
    
    /// Velocity/intensity (0.0 - 1.0)
    pub velocity: f32,
    
    /// Probability of trigger (0.0 - 1.0)
    /// 1.0 = always trigger, 0.5 = 50% chance, etc.
    pub probability: f32,
    
    /// Number of ratchet repeats (1-8)
    /// 1 = single trigger, 2 = double, etc.
    pub ratchet: u8,
    
    /// Time between ratchets as fraction of step duration
    /// 0.5 = half step, 0.25 = quarter step, etc.
    pub ratchet_spacing: f32,
    
    /// Gate length as fraction of step duration (0.0 - 1.0)
    /// 1.0 = full step, 0.5 = half step, etc.
    /// For one-shot triggers, use a small value like 0.1
    #[serde(default = "default_gate_length")]
    pub gate_length: f32,
    
    /// Per-step parameter locks (effect overrides)
    /// Key: parameter name, Value: override value
    #[serde(default)]
    pub parameter_locks: HashMap<String, f32>,
}

fn default_gate_length() -> f32 {
    0.25 // Default to 1/4 of a step for one-shot behavior
}

impl Step {
    pub fn new() -> Self {
        Self {
            active: false,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
            ratchet_spacing: 0.5,
            gate_length: default_gate_length(),
            parameter_locks: HashMap::new(),
        }
    }

    /// Create an active step with default settings
    pub fn active() -> Self {
        Self {
            active: true,
            ..Self::new()
        }
    }

    /// Toggle active state
    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    /// Check if this step should trigger based on probability
    pub fn should_trigger(&self) -> bool {
        if !self.active {
            return false;
        }
        
        if self.probability >= 1.0 {
            return true;
        }
        
        // Use random check
        rand::random::<f32>() < self.probability
    }

    /// Get the ratchet times for this step
    /// Returns a list of offsets (0.0 to 1.0) for each ratchet
    pub fn ratchet_times(&self) -> Vec<f32> {
        if self.ratchet <= 1 {
            return vec![0.0];
        }
        
        let count = self.ratchet.min(8) as usize;
        let spacing = self.ratchet_spacing;
        
        (0..count)
            .map(|i| i as f32 * spacing)
            .collect()
    }
}

impl Default for Step {
    fn default() -> Self {
        Self::new()
    }
}

// Simple random implementation for probability checks
pub mod rand {
    use std::cell::Cell;
    
    thread_local! {
        static RNG: Cell<u64> = Cell::new(0x123456789abcdef0);
    }
    
    pub fn random<T>() -> T
    where
        T: Random,
    {
        T::random()
    }
    
    pub trait Random {
        fn random() -> Self;
    }
    
    impl Random for f32 {
        fn random() -> Self {
            RNG.with(|rng| {
                let old = rng.get();
                // Simple xorshift
                let mut x = old;
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                rng.set(x);
                
                // Convert to f32 in [0, 1)
                (x as f64 / u64::MAX as f64) as f32
            })
        }
    }
}
