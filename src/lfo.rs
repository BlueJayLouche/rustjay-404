//! LFO (Low Frequency Oscillator) System
//!
//! 4 LFOs that can modulate pad opacity, speed, or master opacity.
//! Tempo-syncable with phase offset support.

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Beat division multipliers for tempo sync (cycle duration in beats)
pub const BEAT_DIVISIONS: [f32; 8] = [
    0.0625, // 1/16
    0.125,  // 1/8
    0.25,   // 1/4
    0.5,    // 1/2
    1.0,    // 1 beat
    2.0,    // 2 beats
    4.0,    // 4 beats
    8.0,    // 8 beats
];

pub const BEAT_DIVISION_NAMES: [&str; 8] = [
    "1/16", "1/8", "1/4", "1/2", "1", "2", "4", "8",
];

pub fn beat_division_to_hz(division: usize, bpm: f32) -> f32 {
    let division = division.min(BEAT_DIVISIONS.len() - 1);
    let beats_per_cycle = BEAT_DIVISIONS[division];
    let beat_duration = 60.0 / bpm.max(1.0);
    let cycle_duration = beat_duration * beats_per_cycle;
    1.0 / cycle_duration
}

/// LFO Waveforms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Waveform {
    Sine = 0,
    Triangle = 1,
    Ramp = 2,
    Saw = 3,
    Square = 4,
}

impl Waveform {
    pub fn name(&self) -> &'static str {
        match self {
            Waveform::Sine => "Sine",
            Waveform::Triangle => "Triangle",
            Waveform::Ramp => "Ramp",
            Waveform::Saw => "Saw",
            Waveform::Square => "Square",
        }
    }

    pub fn all() -> &'static [Waveform] {
        &[
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Ramp,
            Waveform::Saw,
            Waveform::Square,
        ]
    }
}

impl Default for Waveform {
    fn default() -> Self {
        Waveform::Sine
    }
}

/// Target parameter for LFO modulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LfoTarget {
    None,
    /// Modulate a specific pad's opacity (index 0-15)
    PadOpacity(usize),
    /// Modulate a specific pad's speed (index 0-15)
    PadSpeed(usize),
    /// Modulate master output opacity
    MasterOpacity,
}

impl LfoTarget {
    pub fn name(&self) -> String {
        match self {
            LfoTarget::None => "None".to_string(),
            LfoTarget::PadOpacity(i) => format!("Pad {} Opacity", i + 1),
            LfoTarget::PadSpeed(i) => format!("Pad {} Speed", i + 1),
            LfoTarget::MasterOpacity => "Master Opacity".to_string(),
        }
    }
}

impl Default for LfoTarget {
    fn default() -> Self {
        LfoTarget::None
    }
}

/// Single LFO configuration and state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lfo {
    pub index: usize,
    pub enabled: bool,
    pub target: LfoTarget,
    pub waveform: Waveform,
    pub amplitude: f32,
    pub tempo_sync: bool,
    pub division: usize,
    pub rate: f32,
    pub phase_offset: f32,
    #[serde(skip)]
    pub phase: f32,
    #[serde(skip)]
    pub output: f32,
}

impl Lfo {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            enabled: false,
            target: LfoTarget::None,
            waveform: Waveform::Sine,
            amplitude: 0.5,
            tempo_sync: true,
            division: 2, // 1/4 note
            rate: 1.0,
            phase_offset: 0.0,
            phase: 0.0,
            output: 0.0,
        }
    }

    pub fn calculate_value(phase: f32, waveform: Waveform) -> f32 {
        let phase = phase % 1.0;
        match waveform {
            Waveform::Sine => (phase * 2.0 * PI).sin(),
            Waveform::Triangle => {
                if phase < 0.25 {
                    4.0 * phase
                } else if phase < 0.75 {
                    2.0 - 4.0 * phase
                } else {
                    4.0 * phase - 4.0
                }
            }
            Waveform::Ramp => 2.0 * phase - 1.0,
            Waveform::Saw => 1.0 - 2.0 * phase,
            Waveform::Square => {
                if phase < 0.5 { 1.0 } else { -1.0 }
            }
        }
    }

    pub fn update(&mut self, bpm: f32, delta_time: f32, beat_phase: f32) {
        if !self.enabled || self.target == LfoTarget::None {
            self.output = 0.0;
            return;
        }

        let rate_hz = if self.tempo_sync {
            let division = self.division.clamp(0, BEAT_DIVISIONS.len() - 1);
            let beat_duration = 60.0 / bpm.max(1.0);
            let cycle_duration = beat_duration * BEAT_DIVISIONS[division];
            1.0 / cycle_duration
        } else {
            self.rate.clamp(0.01, 20.0)
        };

        self.phase += rate_hz * delta_time;
        self.phase %= 1.0;

        let offset_normalized = self.phase_offset / 360.0;
        let effective_phase = (self.phase + offset_normalized) % 1.0;
        let synced_phase = (effective_phase + beat_phase) % 1.0;

        let raw_value = Self::calculate_value(synced_phase, self.waveform);
        self.output = raw_value * self.amplitude;
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.output = 0.0;
    }
}

impl Default for Lfo {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Collection of 4 LFOs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfoBank {
    pub lfos: [Lfo; 4],
    /// Whether LFO window is shown
    #[serde(skip)]
    pub show_window: bool,
}

impl LfoBank {
    pub fn new() -> Self {
        Self {
            lfos: [Lfo::new(0), Lfo::new(1), Lfo::new(2), Lfo::new(3)],
            show_window: false,
        }
    }

    pub fn update(&mut self, bpm: f32, delta_time: f32, beat_phase: f32) {
        for lfo in &mut self.lfos {
            lfo.update(bpm, delta_time, beat_phase);
        }
    }

    /// Get all active modulations as (target, value) pairs
    pub fn get_modulations(&self) -> Vec<(LfoTarget, f32)> {
        self.lfos
            .iter()
            .filter(|lfo| lfo.enabled && lfo.target != LfoTarget::None)
            .map(|lfo| (lfo.target, lfo.output))
            .collect()
    }

    pub fn reset_all(&mut self) {
        for lfo in &mut self.lfos {
            lfo.reset();
        }
    }
}

impl Default for LfoBank {
    fn default() -> Self {
        Self::new()
    }
}
