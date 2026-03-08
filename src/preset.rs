//! # Preset Management
//!
//! Handles saving and loading of preset files for the entire application state.
//! Inspired by rustjay_waaaves preset system.

use crate::sampler::bank::{BankManager, SampleBank};
use crate::sampler::pad::{BlendMode, SamplePad, TriggerMode};
use crate::sequencer::SequencerEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Preset version for migration support
const PRESET_VERSION: &str = "1.0";

/// Serializable trigger mode
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum PresetTriggerMode {
    #[default]
    Gate,
    Latch,
    OneShot,
}

impl From<TriggerMode> for PresetTriggerMode {
    fn from(mode: TriggerMode) -> Self {
        match mode {
            TriggerMode::Gate => PresetTriggerMode::Gate,
            TriggerMode::Latch => PresetTriggerMode::Latch,
            TriggerMode::OneShot => PresetTriggerMode::OneShot,
        }
    }
}

impl From<PresetTriggerMode> for TriggerMode {
    fn from(mode: PresetTriggerMode) -> Self {
        match mode {
            PresetTriggerMode::Gate => TriggerMode::Gate,
            PresetTriggerMode::Latch => TriggerMode::Latch,
            PresetTriggerMode::OneShot => TriggerMode::OneShot,
        }
    }
}

/// Serializable blend mode
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum PresetBlendMode {
    #[default]
    Replace,
    Add,
    Multiply,
    Screen,
    Alpha,
}

impl From<BlendMode> for PresetBlendMode {
    fn from(mode: BlendMode) -> Self {
        match mode {
            BlendMode::Replace => PresetBlendMode::Replace,
            BlendMode::Add => PresetBlendMode::Add,
            BlendMode::Multiply => PresetBlendMode::Multiply,
            BlendMode::Screen => PresetBlendMode::Screen,
            BlendMode::Alpha => PresetBlendMode::Alpha,
        }
    }
}

impl From<PresetBlendMode> for BlendMode {
    fn from(mode: PresetBlendMode) -> Self {
        match mode {
            PresetBlendMode::Replace => BlendMode::Replace,
            PresetBlendMode::Add => BlendMode::Add,
            PresetBlendMode::Multiply => BlendMode::Multiply,
            PresetBlendMode::Screen => BlendMode::Screen,
            PresetBlendMode::Alpha => BlendMode::Alpha,
        }
    }
}

/// Pad data for serialization
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PresetPadData {
    pub index: usize,
    pub name: String,
    pub color: [u8; 3],
    pub trigger_mode: PresetTriggerMode,
    pub loop_enabled: bool,
    pub speed: f32,
    pub volume: f32,
    pub blend_mode: PresetBlendMode,
    pub midi_note: Option<u8>,
    pub sample_path: Option<String>,
    pub in_point: u32,
    pub out_point: u32,
}

impl From<&SamplePad> for PresetPadData {
    fn from(pad: &SamplePad) -> Self {
        let sample_info = pad.sample.as_ref().and_then(|s| {
            s.try_lock().ok().map(|guard| {
                (
                    guard.filepath.to_string_lossy().to_string(),
                    guard.in_point,
                    guard.out_point,
                )
            })
        });

        Self {
            index: pad.index,
            name: pad.name.clone(),
            color: pad.color,
            trigger_mode: pad.trigger_mode.into(),
            loop_enabled: pad.loop_enabled,
            speed: pad.speed,
            volume: pad.volume,
            blend_mode: pad.blend_mode.into(),
            midi_note: pad.midi_note,
            sample_path: sample_info.as_ref().map(|(p, _, _)| p.clone()),
            in_point: sample_info.as_ref().map(|(_, i, _)| *i).unwrap_or(0),
            out_point: sample_info.map(|(_, _, o)| o).unwrap_or(0),
        }
    }
}

/// Sequencer pattern step
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct PresetStep {
    pub active: bool,
    pub velocity: f32,
}

/// Sequencer track data
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PresetTrack {
    pub pad_index: usize,
    pub steps: Vec<PresetStep>,
    pub muted: bool,
}

/// Sequencer data for serialization
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PresetSequencerData {
    pub bpm: f32,
    pub playing: bool,
    pub current_step: usize,
    pub tracks: Vec<PresetTrack>,
}

/// Complete preset data structure
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PresetData {
    /// Preset format version
    pub version: String,
    /// Preset name
    pub name: String,
    /// Pad settings for all 16 pads
    pub pads: Vec<PresetPadData>,
    /// Sequencer settings
    pub sequencer: PresetSequencerData,
    /// Global BPM (also in sequencer, but for compatibility)
    pub bpm: f32,
    /// Creation/modification timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl PresetData {
    /// Create preset data from current app state
    pub fn from_app(bank_manager: &BankManager, sequencer: &SequencerEngine, name: &str) -> Self {
        let bank = bank_manager.current_bank();
        
        // Collect pad data
        let pads: Vec<PresetPadData> = bank.pads.iter().map(|pad| pad.into()).collect();
        
        // Collect sequencer data
        let tracks = sequencer
            .tracks()
            .iter()
            .map(|track| PresetTrack {
                pad_index: track.pad_index,
                steps: track
                    .steps
                    .iter()
                    .map(|step| PresetStep {
                        active: step.active,
                        velocity: step.velocity,
                    })
                    .collect(),
                muted: track.muted,
            })
            .collect();
        
        let sequencer_data = PresetSequencerData {
            bpm: sequencer.bpm(),
            playing: sequencer.is_playing(),
            current_step: sequencer.current_step(),
            tracks,
        };
        
        Self {
            version: PRESET_VERSION.to_string(),
            name: name.to_string(),
            pads,
            sequencer: sequencer_data,
            bpm: sequencer.bpm(),
            timestamp: Some(chrono::Local::now().to_rfc3339()),
        }
    }
    
    /// Get list of samples that need to be loaded for this preset
    /// Returns Vec of (pad_index, sample_path)
    pub fn get_samples_to_load(&self) -> Vec<(usize, String)> {
        self.pads
            .iter()
            .filter_map(|pad| {
                pad.sample_path.as_ref().map(|path| (pad.index, path.clone()))
            })
            .collect()
    }
    
    /// Apply preset to app state (without loading samples - use get_samples_to_load for that)
    pub fn apply_to_app(
        &self,
        bank_manager: &mut BankManager,
        sequencer: &mut SequencerEngine,
    ) -> anyhow::Result<()> {
        let bank = bank_manager.current_bank_mut();
        
        // Apply pad settings
        for pad_data in &self.pads {
            if let Some(pad) = bank.pads.get_mut(pad_data.index) {
                pad.name = pad_data.name.clone();
                pad.color = pad_data.color;
                pad.trigger_mode = pad_data.trigger_mode.into();
                pad.loop_enabled = pad_data.loop_enabled;
                pad.speed = pad_data.speed.clamp(-5.0, 5.0);
                pad.volume = pad_data.volume.clamp(0.0, 1.0);
                pad.blend_mode = pad_data.blend_mode.into();
                pad.midi_note = pad_data.midi_note;
                // Note: Samples are loaded separately via load_preset_samples
            }
        }
        
        // Apply sequencer settings
        sequencer.set_bpm(self.sequencer.bpm.clamp(20.0, 300.0));
        
        // Apply track patterns
        for (i, track_data) in self.sequencer.tracks.iter().enumerate() {
            if i < 16 {
                for (j, step_data) in track_data.steps.iter().enumerate() {
                    if j < 16 {
                        sequencer.set_step(i, j, step_data.active);
                        sequencer.set_step_velocity(i, j, step_data.velocity);
                    }
                }
                if track_data.muted {
                    sequencer.mute_track(i);
                } else {
                    sequencer.unmute_track(i);
                }
            }
        }
        
        log::info!("Applied preset '{}'", self.name);
        Ok(())
    }
}

impl Default for PresetData {
    fn default() -> Self {
        Self {
            version: PRESET_VERSION.to_string(),
            name: "Untitled".to_string(),
            pads: Vec::new(),
            sequencer: PresetSequencerData::default(),
            bpm: 120.0,
            timestamp: None,
        }
    }
}

/// Preset bank information
#[derive(Debug, Clone)]
pub struct PresetBank {
    pub name: String,
    pub path: PathBuf,
    pub preset_files: Vec<String>,
    pub preset_display_names: Vec<String>,
}

/// Centralized preset management
pub struct PresetManager {
    banks: HashMap<String, PresetBank>,
    current_bank: String,
    base_path: PathBuf,
}

impl PresetManager {
    /// Create a new preset manager with local presets folder
    pub fn new() -> Self {
        // Use local "presets" folder in current directory
        let base_path = PathBuf::from("presets");
        
        // Ensure presets directory exists
        if !base_path.exists() {
            let _ = std::fs::create_dir_all(&base_path);
        }
        
        let mut manager = Self {
            banks: HashMap::new(),
            current_bank: "Default".to_string(),
            base_path,
        };
        
        // Scan for banks and ensure Default exists
        manager.scan_banks();
        
        manager
    }
    
    /// Create with custom base path (for testing)
    pub fn with_path(base_path: PathBuf) -> Self {
        if !base_path.exists() {
            let _ = std::fs::create_dir_all(&base_path);
        }
        
        let mut manager = Self {
            banks: HashMap::new(),
            current_bank: "Default".to_string(),
            base_path,
        };
        
        manager.scan_banks();
        manager
    }
    
    /// Scan for available preset banks
    pub fn scan_banks(&mut self) {
        self.banks.clear();
        
        if let Ok(entries) = std::fs::read_dir(&self.base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    
                    let mut bank = PresetBank {
                        name: name.clone(),
                        path: path.clone(),
                        preset_files: Vec::new(),
                        preset_display_names: Vec::new(),
                    };
                    
                    // Index presets in this bank
                    Self::index_presets_in_bank(&mut bank);
                    
                    self.banks.insert(name, bank);
                }
            }
        }
        
        // Ensure Default bank exists
        if !self.banks.contains_key("Default") {
            let default_path = self.base_path.join("Default");
            let _ = std::fs::create_dir_all(&default_path);
            
            let bank = PresetBank {
                name: "Default".to_string(),
                path: default_path,
                preset_files: Vec::new(),
                preset_display_names: Vec::new(),
            };
            
            self.banks.insert("Default".to_string(), bank);
        }
        
        log::info!("Scanned {} preset banks", self.banks.len());
    }
    
    /// Get list of bank names
    pub fn get_bank_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.banks.keys().cloned().collect();
        names.sort();
        names
    }
    
    /// Switch to a different bank
    pub fn switch_bank(&mut self, bank_name: &str) -> bool {
        if self.banks.contains_key(bank_name) {
            self.current_bank = bank_name.to_string();
            
            // Re-index presets
            if let Some(bank) = self.banks.get_mut(bank_name) {
                Self::index_presets_in_bank(bank);
            }
            
            log::info!("Switched to preset bank: {}", bank_name);
            true
        } else {
            log::warn!("Preset bank not found: {}", bank_name);
            false
        }
    }
    
    /// Create a new bank
    pub fn create_bank(&mut self, name: &str) -> bool {
        let path = self.base_path.join(name);
        
        if path.exists() {
            log::warn!("Bank '{}' already exists", name);
            return false;
        }
        
        match std::fs::create_dir_all(&path) {
            Ok(_) => {
                let bank = PresetBank {
                    name: name.to_string(),
                    path,
                    preset_files: Vec::new(),
                    preset_display_names: Vec::new(),
                };
                
                self.banks.insert(name.to_string(), bank);
                log::info!("Created preset bank: {}", name);
                true
            }
            Err(e) => {
                log::error!("Failed to create bank '{}': {:?}", name, e);
                false
            }
        }
    }
    
    /// Get current bank name
    pub fn get_current_bank(&self) -> &str {
        &self.current_bank
    }
    
    /// Get preset names in current bank
    pub fn get_preset_names(&self) -> Vec<String> {
        if let Some(bank) = self.banks.get(&self.current_bank) {
            bank.preset_display_names.clone()
        } else {
            Vec::new()
        }
    }
    
    /// Save a preset
    pub fn save_preset(
        &mut self,
        name: &str,
        data: &PresetData,
    ) -> anyhow::Result<PathBuf> {
        let bank = self
            .banks
            .get(&self.current_bank)
            .ok_or_else(|| anyhow::anyhow!("Current bank not found"))?;
        
        let filename = Self::generate_preset_filename(name);
        let full_path = bank.path.join(&filename);
        
        // Serialize to JSON with pretty printing
        let json = serde_json::to_string_pretty(data)?;
        std::fs::write(&full_path, json)?;
        
        // Re-index
        if let Some(bank) = self.banks.get_mut(&self.current_bank) {
            Self::index_presets_in_bank(bank);
        }
        
        log::info!("Saved preset '{}' to {:?}", name, full_path);
        Ok(full_path)
    }
    
    /// Load a preset
    pub fn load_preset(&self, name: &str) -> anyhow::Result<PresetData> {
        let bank = self
            .banks
            .get(&self.current_bank)
            .ok_or_else(|| anyhow::anyhow!("Current bank not found"))?;
        
        // Find preset file by display name
        let index = bank
            .preset_display_names
            .iter()
            .position(|n| n == name)
            .or_else(|| {
                bank.preset_display_names
                    .iter()
                    .position(|n| n == &Self::clean_display_name(name))
            })
            .ok_or_else(|| anyhow::anyhow!("Preset '{}' not found", name))?;
        
        let filename = &bank.preset_files[index];
        let full_path = bank.path.join(filename);
        
        let json = std::fs::read_to_string(&full_path)?;
        let data: PresetData = serde_json::from_str(&json)?;
        
        log::info!("Loaded preset '{}' from {:?}", name, full_path);
        Ok(data)
    }
    
    /// Load preset by index
    pub fn load_preset_by_index(&self, index: usize) -> anyhow::Result<PresetData> {
        let bank = self
            .banks
            .get(&self.current_bank)
            .ok_or_else(|| anyhow::anyhow!("Current bank not found"))?;
        
        if index >= bank.preset_files.len() {
            anyhow::bail!("Invalid preset index: {}", index);
        }
        
        let filename = &bank.preset_files[index];
        let full_path = bank.path.join(filename);
        
        let json = std::fs::read_to_string(&full_path)?;
        let data: PresetData = serde_json::from_str(&json)?;
        
        log::info!("Loaded preset '{}' from {:?}", data.name, full_path);
        Ok(data)
    }
    
    /// Delete a preset
    pub fn delete_preset(&mut self, index: usize) -> anyhow::Result<()> {
        let bank = self
            .banks
            .get(&self.current_bank)
            .ok_or_else(|| anyhow::anyhow!("Current bank not found"))?;
        
        if index >= bank.preset_files.len() {
            return Err(anyhow::anyhow!("Invalid preset index"));
        }
        
        let filename = &bank.preset_files[index];
        let full_path = bank.path.join(filename);
        
        std::fs::remove_file(&full_path)?;
        
        // Re-index
        if let Some(bank) = self.banks.get_mut(&self.current_bank) {
            Self::index_presets_in_bank(bank);
        }
        
        log::info!("Deleted preset at index {}", index);
        Ok(())
    }
    
    /// Get the path for a preset
    pub fn get_preset_path(&self, name: &str) -> Option<PathBuf> {
        let bank = self.banks.get(&self.current_bank)?;
        let index = bank.preset_display_names.iter().position(|n| n == name)?;
        let filename = &bank.preset_files[index];
        Some(bank.path.join(filename))
    }
    
    /// Index presets in a bank
    fn index_presets_in_bank(bank: &mut PresetBank) {
        bank.preset_files.clear();
        bank.preset_display_names.clear();
        
        if let Ok(entries) = std::fs::read_dir(&bank.path) {
            let mut files: Vec<_> = entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e == "json")
                        .unwrap_or(false)
                })
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            
            files.sort();
            
            for filename in files {
                bank.preset_display_names
                    .push(Self::clean_display_name(&filename));
                bank.preset_files.push(filename);
            }
        }
    }
    
    /// Generate a filename from a display name
    fn generate_preset_filename(display_name: &str) -> String {
        // Sanitize name
        let sanitized: String = display_name
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect();
        
        format!("{}.json", sanitized)
    }
    
    /// Clean a filename to get display name
    fn clean_display_name(filename: &str) -> String {
        // Remove .json extension
        let name = filename.strip_suffix(".json").unwrap_or(filename);
        
        // Remove numeric prefix (###_)
        let name = if let Some(pos) = name.find('_') {
            if name[..pos].chars().all(|c| c.is_ascii_digit()) {
                &name[pos + 1..]
            } else {
                name
            }
        } else {
            name
        };
        
        if name.is_empty() {
            "Preset".to_string()
        } else {
            name.to_string()
        }
    }
}

impl Default for PresetManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_preset_filename_generation() {
        assert_eq!(
            PresetManager::generate_preset_filename("Test Preset"),
            "Test Preset.json"
        );
        assert_eq!(
            PresetManager::generate_preset_filename("Test/Invalid"),
            "Test_Invalid.json"
        );
    }
    
    #[test]
    fn test_clean_display_name() {
        assert_eq!(PresetManager::clean_display_name("test.json"), "test");
        assert_eq!(PresetManager::clean_display_name("001_test.json"), "test");
        assert_eq!(PresetManager::clean_display_name("my_preset.json"), "my_preset");
    }
}
