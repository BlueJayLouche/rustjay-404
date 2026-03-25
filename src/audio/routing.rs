//! Audio routing system.
//!
//! Routes FFT frequency bands to video sampler parameters for audio-reactive visuals.

use serde::{Deserialize, Serialize};

/// FFT frequency bands (8-band spectrum)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FftBand {
    SubBass = 0,  // 20-60 Hz
    Bass = 1,     // 60-120 Hz
    LowMid = 2,  // 120-250 Hz
    Mid = 3,      // 250-500 Hz
    HighMid = 4,  // 500-2000 Hz
    High = 5,     // 2000-4000 Hz
    VeryHigh = 6, // 4000-8000 Hz
    Presence = 7, // 8000-16000 Hz
}

impl FftBand {
    pub fn name(&self) -> &'static str {
        match self {
            FftBand::SubBass => "Sub Bass",
            FftBand::Bass => "Bass",
            FftBand::LowMid => "Low Mid",
            FftBand::Mid => "Mid",
            FftBand::HighMid => "High Mid",
            FftBand::High => "High",
            FftBand::VeryHigh => "Very High",
            FftBand::Presence => "Presence",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            FftBand::SubBass => "Sub",
            FftBand::Bass => "Bass",
            FftBand::LowMid => "LoMid",
            FftBand::Mid => "Mid",
            FftBand::HighMid => "HiMid",
            FftBand::High => "High",
            FftBand::VeryHigh => "VHigh",
            FftBand::Presence => "Pres",
        }
    }

    pub fn all() -> &'static [FftBand] {
        &[
            FftBand::SubBass,
            FftBand::Bass,
            FftBand::LowMid,
            FftBand::Mid,
            FftBand::HighMid,
            FftBand::High,
            FftBand::VeryHigh,
            FftBand::Presence,
        ]
    }

    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(FftBand::SubBass),
            1 => Some(FftBand::Bass),
            2 => Some(FftBand::LowMid),
            3 => Some(FftBand::Mid),
            4 => Some(FftBand::HighMid),
            5 => Some(FftBand::High),
            6 => Some(FftBand::VeryHigh),
            7 => Some(FftBand::Presence),
            _ => None,
        }
    }
}

/// Video parameters that can be modulated by audio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModulationTarget {
    PadOpacity(usize),
    PadSpeed(usize),
    MasterOpacity,
}

impl ModulationTarget {
    pub fn name(&self) -> String {
        match self {
            ModulationTarget::PadOpacity(i) => format!("Pad {} Opacity", i + 1),
            ModulationTarget::PadSpeed(i) => format!("Pad {} Speed", i + 1),
            ModulationTarget::MasterOpacity => "Master Opacity".to_string(),
        }
    }

    /// All common targets for UI selection
    pub fn all_options() -> Vec<ModulationTarget> {
        let mut targets = Vec::new();
        for i in 0..16 {
            targets.push(ModulationTarget::PadOpacity(i));
        }
        for i in 0..16 {
            targets.push(ModulationTarget::PadSpeed(i));
        }
        targets.push(ModulationTarget::MasterOpacity);
        targets
    }
}

/// A single audio-to-parameter routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRoute {
    pub id: usize,
    pub band: FftBand,
    pub target: ModulationTarget,
    /// Modulation depth (-1.0 to 1.0)
    pub amount: f32,
    /// Attack smoothing (0.0 = instant, 1.0 = very slow)
    pub attack: f32,
    /// Release smoothing
    pub release: f32,
    pub enabled: bool,
    #[serde(skip)]
    pub current_value: f32,
    #[serde(skip)]
    smoothed_fft: f32,
}

impl AudioRoute {
    pub fn new(id: usize, band: FftBand, target: ModulationTarget) -> Self {
        Self {
            id,
            band,
            target,
            amount: 0.5,
            attack: 0.1,
            release: 0.3,
            enabled: true,
            current_value: 0.0,
            smoothed_fft: 0.0,
        }
    }

    pub fn process(&mut self, fft_bands: &[f32; 8], delta_time: f32) {
        if !self.enabled {
            self.current_value = 0.0;
            self.smoothed_fft *= 0.9;
            return;
        }

        let target_value = fft_bands[self.band as usize];
        let diff = target_value - self.smoothed_fft;
        let smoothing = if diff > 0.0 { self.attack } else { self.release };
        let smoothing_factor = (-delta_time / smoothing.max(0.001)).exp();
        self.smoothed_fft =
            self.smoothed_fft * smoothing_factor + target_value * (1.0 - smoothing_factor);
        self.current_value = self.smoothed_fft * self.amount;
    }

    pub fn reset(&mut self) {
        self.current_value = 0.0;
        self.smoothed_fft = 0.0;
    }
}

/// Manages all audio-to-parameter routings
#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingMatrix {
    routes: Vec<AudioRoute>,
    #[serde(skip)]
    next_id: usize,
    max_routes: usize,
}

impl RoutingMatrix {
    pub fn new(max_routes: usize) -> Self {
        Self {
            routes: Vec::new(),
            next_id: 0,
            max_routes,
        }
    }

    pub fn add_route(&mut self, band: FftBand, target: ModulationTarget) -> Option<usize> {
        if self.routes.len() >= self.max_routes {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.routes.push(AudioRoute::new(id, band, target));
        Some(id)
    }

    pub fn remove_route(&mut self, id: usize) {
        self.routes.retain(|r| r.id != id);
    }

    pub fn get_route_mut(&mut self, id: usize) -> Option<&mut AudioRoute> {
        self.routes.iter_mut().find(|r| r.id == id)
    }

    pub fn routes(&self) -> &[AudioRoute] {
        &self.routes
    }

    pub fn routes_mut(&mut self) -> &mut [AudioRoute] {
        &mut self.routes
    }

    pub fn process(&mut self, fft_bands: &[f32; 8], delta_time: f32) {
        for route in &mut self.routes {
            route.process(fft_bands, delta_time);
        }
    }

    pub fn get_modulation(&self, target: ModulationTarget) -> f32 {
        self.routes
            .iter()
            .filter(|r| r.target == target && r.enabled)
            .map(|r| r.current_value)
            .sum::<f32>()
            .clamp(-2.0, 2.0)
    }

    pub fn clear(&mut self) {
        self.routes.clear();
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn max_routes(&self) -> usize {
        self.max_routes
    }

    pub fn can_add_route(&self) -> bool {
        self.routes.len() < self.max_routes
    }

    pub fn reset(&mut self) {
        for route in &mut self.routes {
            route.reset();
        }
    }
}

impl Default for RoutingMatrix {
    fn default() -> Self {
        Self::new(8)
    }
}
