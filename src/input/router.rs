//! Input event router - connects MIDI/OSC to app actions

use crate::input::events::{InputEvent, MidiEvent, OscEvent, InputSource};
use crate::input::midi::{MidiController, MidiMapping};
use crate::input::midi_mapping::{MidiMappingConfig, MidiLearnState};
use crate::input::osc::{OscServer, OscMapping};
use crate::sampler::BankManager;
use crate::sequencer::SequencerEngine;
use std::path::PathBuf;
use std::sync::mpsc;

/// Channels for receiving events from MIDI/OSC
pub struct InputChannels {
    pub midi_rx: mpsc::Receiver<(MidiEvent, InputSource)>,
    pub osc_rx: mpsc::Receiver<(OscEvent, InputSource)>,
    midi_tx: mpsc::Sender<(MidiEvent, InputSource)>,
    osc_tx: mpsc::Sender<(OscEvent, InputSource)>,
}

impl InputChannels {
    pub fn new() -> Self {
        let (midi_tx, midi_rx) = mpsc::channel();
        let (osc_tx, osc_rx) = mpsc::channel();
        
        Self { midi_rx, osc_rx, midi_tx, osc_tx }
    }
    
    pub fn midi_sender(&self) -> mpsc::Sender<(MidiEvent, InputSource)> {
        self.midi_tx.clone()
    }
    
    pub fn osc_sender(&self) -> mpsc::Sender<(OscEvent, InputSource)> {
        self.osc_tx.clone()
    }
}

/// Routes all input events to application actions
pub struct InputRouter {
    /// MIDI controller
    midi: Option<MidiController>,
    /// OSC server
    osc: Option<OscServer>,
    /// MIDI mapping configuration
    midi_mapping: MidiMappingConfig,
    /// OSC event mapping
    osc_mapping: OscMapping,
    /// MIDI learn state
    pub midi_learn: MidiLearnState,
    /// Channels for receiving events
    channels: InputChannels,
    /// Config file path
    config_path: PathBuf,
}

impl InputRouter {
    pub fn new() -> Self {
        let channels = InputChannels::new();
        
        // Load or create default MIDI mapping
        let config_path = Self::default_config_path();
        let midi_mapping = MidiMappingConfig::load_or_default(&config_path);
        
        Self {
            midi: None,
            osc: None,
            midi_mapping,
            osc_mapping: OscMapping::default(),
            midi_learn: MidiLearnState::new(),
            channels,
            config_path,
        }
    }
    
    fn default_config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("rusty404");
        
        std::fs::create_dir_all(&config_dir).ok();
        config_dir.join("midi_mapping.json")
    }
    
    /// Save current MIDI mapping
    pub fn save_mapping(&self) -> anyhow::Result<()> {
        self.midi_mapping.save(&self.config_path)?;
        log::info!("MIDI mapping saved to {:?}", self.config_path);
        Ok(())
    }
    
    /// Initialize MIDI
    pub fn init_midi(&mut self) -> anyhow::Result<()> {
        let controller = MidiController::new(self.channels.midi_sender())?;
        self.midi = Some(controller);
        log::info!("MIDI initialized");
        Ok(())
    }
    
    /// Initialize OSC
    pub fn init_osc(&mut self) {
        let server = OscServer::new(self.channels.osc_sender());
        self.osc = Some(server);
        log::info!("OSC initialized");
    }
    
    /// Auto-connect to first available MIDI port
    pub fn auto_connect_midi(&mut self) -> anyhow::Result<()> {
        if self.midi.is_none() {
            self.init_midi()?;
        }
        self.midi.as_mut().unwrap().auto_connect()
    }
    
    /// Auto-start OSC on default port
    pub fn auto_start_osc(&mut self) -> anyhow::Result<u16> {
        if self.osc.is_none() {
            self.init_osc();
        }
        self.osc.as_mut().unwrap().auto_start()?;
        Ok(self.osc.as_ref().unwrap().port())
    }
    
    /// List MIDI ports
    pub fn list_midi_ports(&mut self) -> Vec<(usize, String)> {
        MidiController::list_ports().unwrap_or_default()
    }
    
    /// Connect to specific MIDI port
    pub fn connect_midi(&mut self, port: usize) -> anyhow::Result<()> {
        if self.midi.is_none() {
            self.init_midi()?;
        }
        self.midi.as_mut().unwrap().connect(port)
    }
    
    /// Disconnect MIDI
    pub fn disconnect_midi(&mut self) {
        if let Some(ref mut midi) = self.midi {
            midi.disconnect();
        }
    }
    
    /// Start OSC on specific port
    pub fn start_osc(&mut self, port: u16) -> anyhow::Result<()> {
        if self.osc.is_none() {
            self.init_osc();
        }
        self.osc.as_mut().unwrap().start(port)
    }
    
    /// Stop OSC
    pub fn stop_osc(&mut self) {
        if let Some(ref mut osc) = self.osc {
            osc.stop();
        }
    }
    
    /// Get MIDI connection status
    pub fn midi_status(&self) -> Option<&str> {
        self.midi.as_ref().and_then(|m| {
            if m.is_connected() {
                m.current_port()
            } else {
                None
            }
        })
    }
    
    /// Get OSC status
    pub fn osc_status(&self) -> Option<u16> {
        self.osc.as_ref().and_then(|o| {
            if o.is_running() {
                Some(o.port())
            } else {
                None
            }
        })
    }
    
    /// Start MIDI learn for a control
    pub fn start_learn(&mut self, control_id: &str, min: f32, max: f32) {
        self.midi_learn.start(control_id, min, max);
    }
    
    /// Cancel MIDI learn
    pub fn cancel_learn(&mut self) {
        self.midi_learn.cancel();
    }
    
    /// Check if learning a specific control
    pub fn is_learning(&self, control_id: &str) -> bool {
        self.midi_learn.is_learning(control_id)
    }
    
    /// Get learn flash intensity for UI
    pub fn learn_flash(&self) -> f32 {
        self.midi_learn.flash_intensity()
    }
    
    /// Get the current learn target control ID
    pub fn learn_target(&self) -> Option<&str> {
        self.midi_learn.target()
    }
    
    /// Process all pending events
    pub fn process_events(
        &mut self,
        bank_manager: &mut BankManager,
        sequencer: &mut SequencerEngine,
    ) {
        // Process MIDI events
        while let Ok((midi_event, _source)) = self.channels.midi_rx.try_recv() {
            // First check if we're in learn mode
            if self.midi_learn.is_active() {
                if let Some(mapping) = self.midi_learn.handle_message(&midi_event) {
                    self.midi_mapping.add_mapping(mapping);
                    if let Err(e) = self.save_mapping() {
                        log::error!("Failed to save MIDI mapping: {}", e);
                    }
                }
                continue;
            }
            
            // Try to find a mapping
            if let Some(mapping) = self.midi_mapping.find_mapping(&midi_event) {
                if let Some(input_event) = mapping.to_input_event(&midi_event) {
                    self.handle_event(input_event, bank_manager, sequencer);
                }
            } else {
                // Fall back to default mapping
                if let Some(input_event) = MidiMapping::default().map_event(&midi_event) {
                    self.handle_event(input_event, bank_manager, sequencer);
                }
            }
        }
        
        // Process OSC events
        while let Ok((osc_event, source)) = self.channels.osc_rx.try_recv() {
            log::debug!("OSC: {:?} from {:?}", osc_event, source);
            
            if let Some(input_event) = self.osc_mapping.map_event(&osc_event) {
                self.handle_event(input_event, bank_manager, sequencer);
            }
        }
    }
    
    /// Handle a single input event
    fn handle_event(
        &self,
        event: InputEvent,
        bank_manager: &mut BankManager,
        sequencer: &mut SequencerEngine,
    ) {
        let bank = bank_manager.current_bank_mut();
        
        match event {
            InputEvent::PadTrigger { pad, velocity } => {
                if pad < 16 {
                    if let Some(p) = bank.get_pad_mut(pad) {
                        p.volume = velocity;
                        p.trigger();
                    }
                }
            }
            InputEvent::PadRelease { pad } => {
                if pad < 16 {
                    if let Some(p) = bank.get_pad_mut(pad) {
                        if p.trigger_mode == crate::sampler::TriggerMode::Gate {
                            p.release();
                        }
                    }
                }
            }
            InputEvent::PadVolume { pad, volume } => {
                if pad < 16 {
                    if let Some(p) = bank.get_pad_mut(pad) {
                        p.volume = volume;
                    }
                }
            }
            InputEvent::PadSpeed { pad, speed } => {
                if pad < 16 {
                    if let Some(p) = bank.get_pad_mut(pad) {
                        p.speed = speed.clamp(-5.0, 5.0);
                        p.direction = if speed >= 0.0 { 1 } else { -1 };
                    }
                }
            }
            InputEvent::StopAll => {
                for i in 0..16 {
                    if let Some(p) = bank.get_pad_mut(i) {
                        p.stop();
                    }
                }
            }
            InputEvent::SetBpm(bpm) => {
                sequencer.set_bpm(bpm.clamp(20.0, 999.0));
            }
            InputEvent::ToggleSequencer => {
                if sequencer.is_playing() {
                    sequencer.stop();
                } else {
                    sequencer.play();
                }
            }
        }
    }
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}
