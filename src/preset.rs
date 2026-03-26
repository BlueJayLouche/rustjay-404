//! # Preset Management
//!
//! Handles saving and loading of preset files for the entire application state.
//! Inspired by rustjay_waaaves preset system.

use crate::sampler::bank::{BankManager, SampleBank};
use crate::sampler::pad::{BlendMode, PadKeyParams, PadMixMode, SamplePad, TriggerMode};
use crate::sequencer::SequencerEngine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Preset version for migration support
/// 1.0: Initial format with basic pad and sequencer data
/// 1.1: Added mix_mode, key_params, base_volume, opacity, and has_sample fields
const PRESET_VERSION: &str = "1.1";

/// Serializable trigger mode
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// Serializable blend mode (legacy - kept for backward compatibility)
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// Serializable mix mode with keying support
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetMixMode {
    #[default]
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

impl From<PadMixMode> for PresetMixMode {
    fn from(mode: PadMixMode) -> Self {
        match mode {
            PadMixMode::Normal => PresetMixMode::Normal,
            PadMixMode::Add => PresetMixMode::Add,
            PadMixMode::Multiply => PresetMixMode::Multiply,
            PadMixMode::Screen => PresetMixMode::Screen,
            PadMixMode::Overlay => PresetMixMode::Overlay,
            PadMixMode::SoftLight => PresetMixMode::SoftLight,
            PadMixMode::HardLight => PresetMixMode::HardLight,
            PadMixMode::Difference => PresetMixMode::Difference,
            PadMixMode::Lighten => PresetMixMode::Lighten,
            PadMixMode::Darken => PresetMixMode::Darken,
            PadMixMode::ChromaKey => PresetMixMode::ChromaKey,
            PadMixMode::LumaKey => PresetMixMode::LumaKey,
        }
    }
}

impl From<PresetMixMode> for PadMixMode {
    fn from(mode: PresetMixMode) -> Self {
        match mode {
            PresetMixMode::Normal => PadMixMode::Normal,
            PresetMixMode::Add => PadMixMode::Add,
            PresetMixMode::Multiply => PadMixMode::Multiply,
            PresetMixMode::Screen => PadMixMode::Screen,
            PresetMixMode::Overlay => PadMixMode::Overlay,
            PresetMixMode::SoftLight => PadMixMode::SoftLight,
            PresetMixMode::HardLight => PadMixMode::HardLight,
            PresetMixMode::Difference => PadMixMode::Difference,
            PresetMixMode::Lighten => PadMixMode::Lighten,
            PresetMixMode::Darken => PadMixMode::Darken,
            PresetMixMode::ChromaKey => PadMixMode::ChromaKey,
            PresetMixMode::LumaKey => PadMixMode::LumaKey,
        }
    }
}

/// Serializable keying parameters
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct PresetKeyParams {
    /// Key color for chroma key [R, G, B] (0-1)
    pub key_color: [f32; 3],
    /// Distance/brightness threshold (0-1)
    pub threshold: f32,
    /// Edge smoothness (0-1)
    pub smoothness: f32,
    /// Invert the key (for luma key)
    pub invert: bool,
}

impl From<PadKeyParams> for PresetKeyParams {
    fn from(params: PadKeyParams) -> Self {
        Self {
            key_color: params.key_color,
            threshold: params.threshold,
            smoothness: params.smoothness,
            invert: params.invert,
        }
    }
}

impl From<PresetKeyParams> for PadKeyParams {
    fn from(params: PresetKeyParams) -> Self {
        Self {
            key_color: params.key_color,
            threshold: params.threshold,
            smoothness: params.smoothness,
            invert: params.invert,
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
    /// User-set base speed (before LFO/audio modulation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_speed: Option<f32>,
    /// Current volume (effective, after modulation)
    pub volume: f32,
    /// User-set base volume (before LFO/audio modulation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_volume: Option<f32>,
    /// Legacy blend mode (kept for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<PresetBlendMode>,
    /// Mix mode with keying support (preferred over blend_mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mix_mode: Option<PresetMixMode>,
    /// Keying parameters for chroma/luma key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_params: Option<PresetKeyParams>,
    /// Opacity (0.0 - 1.0), alternative to volume for mixing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    pub midi_note: Option<u8>,
    pub sample_path: Option<String>,
    /// Whether this pad should have a sample loaded (explicit "empty" vs "missing file")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_sample: Option<bool>,
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
            base_speed: Some(pad.base_speed),
            volume: pad.volume,
            base_volume: Some(pad.base_volume),
            blend_mode: Some(pad.blend_mode.into()),
            mix_mode: Some(pad.mix_mode.into()),
            key_params: Some(pad.key_params.into()),
            opacity: Some(pad.volume), // Use volume as opacity for mixing
            midi_note: pad.midi_note,
            sample_path: sample_info.as_ref().map(|(p, _, _)| p.clone()),
            has_sample: Some(pad.sample.is_some()),
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
                
                // Speed: prefer base_speed if available, fall back to speed
                let speed = pad_data.base_speed.unwrap_or(pad_data.speed);
                pad.speed = speed.clamp(-5.0, 5.0);
                pad.base_speed = speed.clamp(-5.0, 5.0);
                
                // Volume: prefer base_volume if available, fall back to volume
                let volume = pad_data.base_volume.unwrap_or(pad_data.volume);
                pad.volume = volume.clamp(0.0, 1.0);
                pad.base_volume = volume.clamp(0.0, 1.0);
                
                // Mix mode: prefer mix_mode if available, fall back to blend_mode mapping
                if let Some(mix_mode) = pad_data.mix_mode {
                    pad.mix_mode = mix_mode.into();
                } else if let Some(blend_mode) = pad_data.blend_mode {
                    // Map legacy blend_mode to mix_mode
                    pad.mix_mode = match blend_mode {
                        PresetBlendMode::Replace => PadMixMode::Normal,
                        PresetBlendMode::Add => PadMixMode::Add,
                        PresetBlendMode::Multiply => PadMixMode::Multiply,
                        PresetBlendMode::Screen => PadMixMode::Screen,
                        PresetBlendMode::Alpha => PadMixMode::Normal,
                    };
                }
                
                // Legacy blend_mode is kept in sync
                pad.blend_mode = pad_data.blend_mode.map(|b| b.into()).unwrap_or_default();
                
                // Keying parameters
                if let Some(key_params) = pad_data.key_params {
                    pad.key_params = key_params.into();
                }
                
                pad.midi_note = pad_data.midi_note;
                
                // Clear sample if preset explicitly says this pad should be empty
                if pad_data.has_sample == Some(false) {
                    pad.clear_sample();
                }
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
    
    #[test]
    fn test_mix_mode_roundtrip() {
        // Test all mix modes convert correctly
        let modes = vec![
            PadMixMode::Normal,
            PadMixMode::Add,
            PadMixMode::Multiply,
            PadMixMode::Screen,
            PadMixMode::Overlay,
            PadMixMode::SoftLight,
            PadMixMode::HardLight,
            PadMixMode::Difference,
            PadMixMode::Lighten,
            PadMixMode::Darken,
            PadMixMode::ChromaKey,
            PadMixMode::LumaKey,
        ];
        
        for mode in modes {
            let preset_mode: PresetMixMode = mode.into();
            let back: PadMixMode = preset_mode.into();
            assert_eq!(mode, back, "Mix mode {:?} failed roundtrip", mode);
        }
    }
    
    #[test]
    fn test_key_params_roundtrip() {
        let original = PadKeyParams {
            key_color: [0.1, 0.2, 0.3],
            threshold: 0.5,
            smoothness: 0.25,
            invert: true,
        };
        
        let preset: PresetKeyParams = original.into();
        let back: PadKeyParams = preset.into();
        
        assert_eq!(original.key_color, back.key_color);
        assert_eq!(original.threshold, back.threshold);
        assert_eq!(original.smoothness, back.smoothness);
        assert_eq!(original.invert, back.invert);
    }
    
    #[test]
    fn test_preset_pad_data_serialization() {
        // Create a pad with all settings
        let mut pad = SamplePad::new(0);
        pad.name = "Test Pad".to_string();
        pad.color = [100, 150, 200];
        pad.trigger_mode = TriggerMode::Latch;
        pad.loop_enabled = true;
        pad.speed = 1.5;
        pad.base_speed = 1.5;
        pad.volume = 0.75;
        pad.base_volume = 0.8;
        pad.mix_mode = PadMixMode::ChromaKey;
        pad.key_params = PadKeyParams {
            key_color: [0.0, 1.0, 0.0],
            threshold: 0.3,
            smoothness: 0.1,
            invert: false,
        };
        pad.midi_note = Some(60);
        
        // Convert to preset data
        let preset_data: PresetPadData = (&pad).into();
        
        // Verify all fields
        assert_eq!(preset_data.index, 0);
        assert_eq!(preset_data.name, "Test Pad");
        assert_eq!(preset_data.color, [100, 150, 200]);
        assert_eq!(preset_data.trigger_mode, PresetTriggerMode::Latch);
        assert_eq!(preset_data.loop_enabled, true);
        assert_eq!(preset_data.speed, 1.5);
        assert_eq!(preset_data.base_speed, Some(1.5));
        assert_eq!(preset_data.volume, 0.75);
        assert_eq!(preset_data.base_volume, Some(0.8));
        assert_eq!(preset_data.opacity, Some(0.75));
        assert_eq!(preset_data.mix_mode, Some(PresetMixMode::ChromaKey));
        assert!(preset_data.key_params.is_some());
        assert_eq!(preset_data.has_sample, Some(false)); // No sample assigned
        
        // Test JSON serialization
        let json = serde_json::to_string_pretty(&preset_data).unwrap();
        assert!(json.contains("mix_mode"));
        assert!(json.contains("chroma_key"));
        assert!(json.contains("key_params"));
        assert!(json.contains("base_volume"));
        assert!(json.contains("base_speed"));
        
        // Test deserialization
        let deserialized: PresetPadData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mix_mode, Some(PresetMixMode::ChromaKey));
        assert_eq!(deserialized.base_volume, Some(0.8));
        assert_eq!(deserialized.base_speed, Some(1.5));
    }
    
    #[test]
    fn test_backward_compatibility_blend_mode() {
        // Test that old presets with only blend_mode still work
        let old_preset_json = r#"
        {
            "index": 0,
            "name": "Old Pad",
            "color": [255, 100, 100],
            "trigger_mode": "gate",
            "loop_enabled": false,
            "speed": 1.0,
            "volume": 0.5,
            "blend_mode": "add",
            "midi_note": null,
            "sample_path": null,
            "in_point": 0,
            "out_point": 0
        }
        "#;
        
        let data: PresetPadData = serde_json::from_str(old_preset_json).unwrap();
        assert_eq!(data.blend_mode, Some(PresetBlendMode::Add));
        assert!(data.mix_mode.is_none()); // New field not present in old preset
    }
}
