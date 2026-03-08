//! MIDI Mapping System for Rusty-404
//!
//! Provides MIDI learn functionality and configurable mappings for:
//! - Pad triggers (notes)
//! - Pad parameters (CC: volume, speed, etc.)
//! - Global controls (BPM, stop all)

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use crate::input::events::{InputEvent, MidiEvent};

/// Type of MIDI message for mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiMappingType {
    /// Note On/Off for pad triggering
    #[serde(rename = "note")]
    Note,
    /// Control Change for parameters
    #[serde(rename = "cc")]
    ControlChange,
}

/// A single MIDI mapping entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMappingEntry {
    /// What this controls: "pad.trigger.0", "pad.volume.3", "global.bpm", etc.
    pub control_id: String,
    /// Type of MIDI message
    pub msg_type: MidiMappingType,
    /// MIDI channel (0-15, where 0 = any channel)
    #[serde(default)]
    pub channel: u8,
    /// Note number or CC number
    pub number: u8,
    /// For CC: minimum parameter value (default 0.0)
    #[serde(default = "default_min")]
    pub min_value: f32,
    /// For CC: maximum parameter value (default 1.0)
    #[serde(default = "default_max")]
    pub max_value: f32,
    /// For CC: invert the value (127-value)
    #[serde(default)]
    pub invert: bool,
    /// For CC: curve type ("linear", "log", "exp")
    #[serde(default = "default_curve")]
    pub curve: String,
}

fn default_min() -> f32 { 0.0 }
fn default_max() -> f32 { 1.0 }
fn default_curve() -> String { "linear".to_string() }

impl MidiMappingEntry {
    /// Create a new note mapping for pad trigger
    pub fn pad_note(pad_index: usize, note: u8, channel: u8) -> Self {
        Self {
            control_id: format!("pad.trigger.{}", pad_index),
            msg_type: MidiMappingType::Note,
            channel,
            number: note,
            min_value: 0.0,
            max_value: 1.0,
            invert: false,
            curve: "linear".to_string(),
        }
    }
    
    /// Create a new CC mapping for pad parameter
    pub fn pad_cc(pad_index: usize, param: &str, cc: u8, channel: u8, min: f32, max: f32) -> Self {
        Self {
            control_id: format!("pad.{}.{}", param, pad_index),
            msg_type: MidiMappingType::ControlChange,
            channel,
            number: cc,
            min_value: min,
            max_value: max,
            invert: false,
            curve: "linear".to_string(),
        }
    }
    
    /// Create global BPM mapping
    pub fn global_bpm(cc: u8, channel: u8) -> Self {
        Self {
            control_id: "global.bpm".to_string(),
            msg_type: MidiMappingType::ControlChange,
            channel,
            number: cc,
            min_value: 60.0,
            max_value: 240.0,
            invert: false,
            curve: "linear".to_string(),
        }
    }
    
    /// Check if a MIDI event matches this mapping
    pub fn matches(&self, event: &MidiEvent) -> bool {
        match (self.msg_type, event) {
            (MidiMappingType::Note, MidiEvent::NoteOn { channel, note, .. }) |
            (MidiMappingType::Note, MidiEvent::NoteOff { channel, note, .. }) => {
                (self.channel == 0 || self.channel == *channel) && self.number == *note
            }
            (MidiMappingType::ControlChange, MidiEvent::ControlChange { channel, cc, .. }) => {
                (self.channel == 0 || self.channel == *channel) && self.number == *cc
            }
            _ => false,
        }
    }
    
    /// Scale a MIDI value to the parameter range
    pub fn scale_value(&self, midi_value: u8) -> f32 {
        let mut normalized = midi_value as f32 / 127.0;
        
        if self.invert {
            normalized = 1.0 - normalized;
        }
        
        // Apply curve
        normalized = match self.curve.as_str() {
            "log" => normalized.powf(2.0),
            "exp" => normalized.sqrt(),
            _ => normalized, // linear
        };
        
        self.min_value + normalized * (self.max_value - self.min_value)
    }
    
    /// Convert MIDI event to input event
    pub fn to_input_event(&self, midi_event: &MidiEvent) -> Option<InputEvent> {
        if !self.matches(midi_event) {
            return None;
        }
        
        let parts: Vec<&str> = self.control_id.split('.').collect();
        match parts.as_slice() {
            ["pad", "trigger", pad_idx] => {
                let pad = pad_idx.parse::<usize>().ok()?;
                match midi_event {
                    MidiEvent::NoteOn { velocity, .. } => {
                        Some(InputEvent::PadTrigger {
                            pad,
                            velocity: *velocity as f32 / 127.0,
                        })
                    }
                    MidiEvent::NoteOff { .. } => {
                        Some(InputEvent::PadRelease { pad })
                    }
                    _ => None,
                }
            }
            ["pad", param, pad_idx] => {
                let pad = pad_idx.parse::<usize>().ok()?;
                if let MidiEvent::ControlChange { value, .. } = midi_event {
                    let scaled = self.scale_value(*value);
                    match *param {
                        "volume" => Some(InputEvent::PadVolume { pad, volume: scaled }),
                        "speed" => Some(InputEvent::PadSpeed { pad, speed: scaled }),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            ["global", "bpm"] => {
                if let MidiEvent::ControlChange { value, .. } = midi_event {
                    Some(InputEvent::SetBpm(self.scale_value(*value)))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Complete MIDI mapping configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MidiMappingConfig {
    /// All mappings
    #[serde(default)]
    pub mappings: Vec<MidiMappingEntry>,
    /// Whether to use default note mapping (notes 36-51 → pads 0-15)
    #[serde(default = "default_true")]
    pub use_default_note_mapping: bool,
}

fn default_true() -> bool { true }

impl MidiMappingConfig {
    /// Create with default mappings
    pub fn default_mapping() -> Self {
        let mut mappings = Vec::new();
        
        // Default: MIDI notes 36-51 → pads 0-15
        for i in 0..16 {
            mappings.push(MidiMappingEntry::pad_note(i, 36 + i as u8, 0));
        }
        
        // CC 1 → Global BPM (60-240)
        mappings.push(MidiMappingEntry::global_bpm(1, 0));
        
        Self {
            mappings,
            use_default_note_mapping: true,
        }
    }
    
    /// Load from file or create default
    pub fn load_or_default(path: &std::path::Path) -> Self {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str(&data) {
                return config;
            }
        }
        Self::default_mapping()
    }
    
    /// Save to file
    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }
    
    /// Find mapping for a MIDI event
    pub fn find_mapping(&self, event: &MidiEvent) -> Option<&MidiMappingEntry> {
        self.mappings.iter().find(|m| m.matches(event))
    }
    
    /// Add a new mapping
    pub fn add_mapping(&mut self, mapping: MidiMappingEntry) {
        // Remove any existing mapping for the same control
        self.mappings.retain(|m| m.control_id != mapping.control_id);
        self.mappings.push(mapping);
    }
    
    /// Remove mapping by control ID
    pub fn remove_mapping(&mut self, control_id: &str) {
        self.mappings.retain(|m| m.control_id != control_id);
    }
}

/// MIDI Learn state machine
#[derive(Debug, Clone)]
pub struct MidiLearnState {
    /// Whether learn mode is active
    pub active: bool,
    /// What we're learning for (e.g., "pad.volume.3")
    pub target_control: String,
    /// Parameter min value
    pub min_value: f32,
    /// Parameter max value
    pub max_value: f32,
    /// When learning started
    pub start_time: Option<Instant>,
    /// Timeout duration
    pub timeout: Duration,
}

impl MidiLearnState {
    pub fn new() -> Self {
        Self {
            active: false,
            target_control: String::new(),
            min_value: 0.0,
            max_value: 1.0,
            start_time: None,
            timeout: Duration::from_secs(10),
        }
    }
    
    /// Start learning for a control
    pub fn start(&mut self, control_id: &str, min: f32, max: f32) {
        self.active = true;
        self.target_control = control_id.to_string();
        self.min_value = min;
        self.max_value = max;
        self.start_time = Some(Instant::now());
        log::info!("MIDI learn started for '{}' [{}-{}]", control_id, min, max);
    }
    
    /// Cancel learning
    pub fn cancel(&mut self) {
        self.active = false;
        self.target_control.clear();
        self.start_time = None;
        log::info!("MIDI learn cancelled");
    }
    
    /// Check if learning is active (and not timed out)
    pub fn is_active(&self) -> bool {
        if !self.active {
            return false;
        }
        if let Some(start) = self.start_time {
            if start.elapsed() > self.timeout {
                return false;
            }
        }
        true
    }
    
    /// Check if learning a specific control
    pub fn is_learning(&self, control_id: &str) -> bool {
        self.is_active() && self.target_control == control_id
    }
    
    /// Handle a MIDI message during learn mode
    /// Returns Some(mapping) if a mapping was created
    pub fn handle_message(&mut self, event: &MidiEvent) -> Option<MidiMappingEntry> {
        if !self.is_active() {
            return None;
        }
        
        // Determine message type and number
        let (msg_type, channel, number) = match event {
            MidiEvent::NoteOn { channel, note, .. } => {
                (MidiMappingType::Note, *channel, *note)
            }
            MidiEvent::ControlChange { channel, cc, .. } => {
                (MidiMappingType::ControlChange, *channel, *cc)
            }
            _ => return None, // Only learn notes and CCs
        };
        
        self.active = false;
        self.start_time = None;
        
        let mapping = MidiMappingEntry {
            control_id: self.target_control.clone(),
            msg_type,
            channel,
            number,
            min_value: self.min_value,
            max_value: self.max_value,
            invert: false,
            curve: "linear".to_string(),
        };
        
        log::info!("MIDI learn completed: {} mapped to {:?} ch={} num={}",
            self.target_control, msg_type, channel, number);
        
        Some(mapping)
    }
    
    /// Get visual flash intensity (0.0-1.0) for UI feedback
    pub fn flash_intensity(&self) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        
        let elapsed = self.start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);
        
        // Flash at 2Hz
        ((elapsed * 4.0 * std::f32::consts::PI).sin() + 1.0) / 2.0
    }
    
    /// Get the current target control ID being learned
    pub fn target(&self) -> Option<&str> {
        if self.is_active() {
            Some(&self.target_control)
        } else {
            None
        }
    }
}

impl Default for MidiLearnState {
    fn default() -> Self {
        Self::new()
    }
}
