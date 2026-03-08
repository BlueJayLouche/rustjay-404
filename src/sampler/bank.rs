use super::pad::SamplePad;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A bank of 16 sample pads
#[derive(Clone)]
pub struct SampleBank {
    pub name: String,
    pub pads: [SamplePad; 16],
    
    // File path for saving/loading
    pub filepath: Option<std::path::PathBuf>,
}

/// Serializable bank data for JSON save/load
#[derive(Serialize, Deserialize)]
struct BankData {
    version: String,
    name: String,
    pads: Vec<PadData>,
}

#[derive(Serialize, Deserialize)]
struct PadData {
    index: usize,
    name: String,
    color: [u8; 3],
    trigger_mode: String,
    loop_enabled: bool,
    speed: f32,
    volume: f32,
    blend_mode: String,
    midi_note: Option<u8>,
    sample: Option<SampleData>,
}

#[derive(Serialize, Deserialize)]
struct SampleData {
    filepath: String,
    in_point: u32,
    out_point: u32,
}

impl SampleBank {
    pub fn new(name: impl Into<String>) -> Self {
        let pads = std::array::from_fn(|i| SamplePad::new(i));
        
        Self {
            name: name.into(),
            pads,
            filepath: None,
        }
    }

    /// Get a pad by index
    pub fn get_pad(&self, index: usize) -> Option<&SamplePad> {
        self.pads.get(index)
    }

    /// Get a mutable pad by index
    pub fn get_pad_mut(&mut self, index: usize) -> Option<&mut SamplePad> {
        self.pads.get_mut(index)
    }

    /// Get all currently playing pads
    pub fn get_active_pads(&self) -> Vec<&SamplePad> {
        self.pads.iter().filter(|p| p.is_playing).collect()
    }

    /// Stop all pads
    pub fn stop_all(&mut self) {
        for pad in &mut self.pads {
            pad.stop();
        }
    }

    /// Trigger a pad by index
    pub fn trigger_pad(&mut self, index: usize) {
        if let Some(pad) = self.pads.get_mut(index) {
            pad.trigger();
        }
    }

    /// Release a pad by index (for GATE mode)
    pub fn release_pad(&mut self, index: usize) {
        if let Some(pad) = self.pads.get_mut(index) {
            pad.release();
        }
    }

    /// Update all pads (call every frame)
    pub fn update(&mut self, dt: std::time::Duration) {
        for pad in &mut self.pads {
            pad.update(dt);
        }
    }

    /// Load bank from JSON file
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let bank_data: BankData = serde_json::from_str(&data)?;
        
        let mut bank = Self::new(&bank_data.name);
        bank.filepath = Some(path.to_path_buf());
        
        // Restore pad settings
        for pad_data in bank_data.pads {
            if let Some(pad) = bank.pads.get_mut(pad_data.index) {
                pad.name = pad_data.name;
                pad.color = pad_data.color;
                pad.trigger_mode = match pad_data.trigger_mode.as_str() {
                    "Gate" => super::pad::TriggerMode::Gate,
                    "Latch" => super::pad::TriggerMode::Latch,
                    "OneShot" => super::pad::TriggerMode::OneShot,
                    _ => super::pad::TriggerMode::default(),
                };
                pad.loop_enabled = pad_data.loop_enabled;
                pad.speed = pad_data.speed;
                pad.volume = pad_data.volume;
                pad.blend_mode = match pad_data.blend_mode.as_str() {
                    "Replace" => super::pad::BlendMode::Replace,
                    "Add" => super::pad::BlendMode::Add,
                    "Multiply" => super::pad::BlendMode::Multiply,
                    "Screen" => super::pad::BlendMode::Screen,
                    "Alpha" => super::pad::BlendMode::Alpha,
                    _ => super::pad::BlendMode::default(),
                };
                pad.midi_note = pad_data.midi_note;
                
                // TODO: Load sample files
                // This requires async loading and GPU resources
            }
        }
        
        log::info!("Loaded bank '{}' from {:?}", bank.name, path);
        Ok(bank)
    }

    /// Save bank to JSON file
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let bank_data = BankData {
            version: "1.0".to_string(),
            name: self.name.clone(),
            pads: self.pads.iter().map(|pad| PadData {
                index: pad.index,
                name: pad.name.clone(),
                color: pad.color,
                trigger_mode: format!("{:?}", pad.trigger_mode),
                loop_enabled: pad.loop_enabled,
                speed: pad.speed,
                volume: pad.volume,
                blend_mode: format!("{:?}", pad.blend_mode),
                midi_note: pad.midi_note,
                sample: pad.sample.as_ref().and_then(|s| {
                    s.try_lock().ok().map(|guard| SampleData {
                        filepath: guard.filepath.to_string_lossy().to_string(),
                        in_point: guard.in_point,
                        out_point: guard.out_point,
                    })
                }),
            }).collect(),
        };
        
        let json = serde_json::to_string_pretty(&bank_data)?;
        std::fs::write(path, json)?;
        
        log::info!("Saved bank '{}' to {:?}", self.name, path);
        Ok(())
    }

    /// Get number of loaded samples in this bank
    pub fn loaded_sample_count(&self) -> usize {
        self.pads.iter().filter(|p| p.has_sample()).count()
    }

    /// Clear all samples from the bank
    pub fn clear_all(&mut self) {
        for pad in &mut self.pads {
            pad.clear_sample();
        }
    }
}

/// Bank manager for multiple banks
pub struct BankManager {
    pub banks: Vec<SampleBank>,
    pub current_index: usize,
}

impl BankManager {
    pub fn new() -> Self {
        Self {
            banks: vec![SampleBank::new("Bank A")],
            current_index: 0,
        }
    }

    pub fn current_bank(&self) -> &SampleBank {
        &self.banks[self.current_index]
    }

    pub fn current_bank_mut(&mut self) -> &mut SampleBank {
        &mut self.banks[self.current_index]
    }

    pub fn switch_bank(&mut self, index: usize) {
        if index < self.banks.len() {
            // Stop all pads in current bank before switching
            self.current_bank_mut().stop_all();
            self.current_index = index;
        }
    }

    pub fn add_bank(&mut self, name: impl Into<String>) -> usize {
        let bank = SampleBank::new(name);
        self.banks.push(bank);
        self.banks.len() - 1
    }
}

impl Default for BankManager {
    fn default() -> Self {
        Self::new()
    }
}
