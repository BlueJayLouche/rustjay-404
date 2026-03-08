//! MIDI input handling using midir
//!
//! Note: midir has complex ownership requirements. This implementation
//! focuses on basic connectivity. Full event routing requires a different
//! architecture (e.g., using a thread-local callback with a channel).

use midir::{MidiInput, MidiInputPort};
use std::sync::mpsc;
use crate::input::events::{InputEvent, InputSource, MidiEvent};

/// MIDI controller manager
pub struct MidiController {
    _connection: Option<MidiInputConnection>,
    event_sender: mpsc::Sender<(MidiEvent, InputSource)>,
    port_name: Option<String>,
}

/// Wrapper to handle midir's type complexities
struct MidiInputConnection {
    #[allow(dead_code)]
    conn: midir::MidiInputConnection<()>,
    port_name: String,
}

impl MidiController {
    /// Create new MIDI controller
    pub fn new(event_sender: mpsc::Sender<(MidiEvent, InputSource)>) -> anyhow::Result<Self> {
        Ok(Self {
            _connection: None,
            event_sender,
            port_name: None,
        })
    }
    
    /// List available MIDI input ports
    pub fn list_ports() -> anyhow::Result<Vec<(usize, String)>> {
        let input = MidiInput::new("Rusty-404 MIDI")?;
        let ports = input.ports();
        
        let mut result = Vec::new();
        for (i, port) in ports.iter().enumerate() {
            let name = input.port_name(port).unwrap_or_else(|_| format!("Port {}", i));
            result.push((i, name));
        }
        
        Ok(result)
    }
    
    /// Connect to a specific MIDI port
    pub fn connect(&mut self, port_index: usize) -> anyhow::Result<()> {
        // Drop any existing connection first
        self.disconnect();
        
        let input = MidiInput::new("Rusty-404 MIDI")?;
        let ports = input.ports();
        
        if port_index >= ports.len() {
            return Err(anyhow::anyhow!("Invalid MIDI port index: {}", port_index));
        }
        
        let port = &ports[port_index];
        let port_name = input.port_name(port).unwrap_or_else(|_| "Unknown".to_string());
        
        log::info!("Connecting to MIDI port {}: {}", port_index, port_name);
        
        let sender = self.event_sender.clone();
        
        let conn = input.connect(
            port,
            "Rusty-404",
            move |_stamp, message, _| {
                // Parse and send MIDI event
                if let Some(event) = parse_midi_message(message) {
                    let source = InputSource::Midi { port: port_index };
                    let _ = sender.send((event, source));
                }
            },
            (),
        ).map_err(|e| anyhow::anyhow!("MIDI connection failed: {:?}", e))?;
        
        self._connection = Some(MidiInputConnection { conn, port_name: port_name.clone() });
        self.port_name = Some(port_name);
        
        log::info!("MIDI connected");
        Ok(())
    }
    
    /// Auto-connect to first available port
    pub fn auto_connect(&mut self) -> anyhow::Result<()> {
        let ports = Self::list_ports()?;
        
        if ports.is_empty() {
            log::warn!("No MIDI input ports available");
            return Err(anyhow::anyhow!("No MIDI ports"));
        }
        
        log::info!("Available MIDI ports:");
        for (i, name) in &ports {
            log::info!("  [{}] {}", i, name);
        }
        
        self.connect(0)
    }
    
    /// Disconnect from current port
    pub fn disconnect(&mut self) {
        if self._connection.take().is_some() {
            log::info!("MIDI disconnected");
        }
        self.port_name = None;
    }
    
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self._connection.is_some()
    }
    
    /// Get current port name
    pub fn current_port(&self) -> Option<&str> {
        self.port_name.as_deref()
    }
}

impl Drop for MidiController {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Parse MIDI message bytes into event
fn parse_midi_message(msg: &[u8]) -> Option<MidiEvent> {
    if msg.len() < 2 {
        return None;
    }
    
    let status = msg[0];
    let channel = status & 0x0F;
    let msg_type = status & 0xF0;
    
    match msg_type {
        0x80 => {
            // Note Off
            if msg.len() >= 3 {
                Some(MidiEvent::NoteOff { 
                    channel, 
                    note: msg[1] 
                })
            } else {
                None
            }
        }
        0x90 => {
            // Note On
            if msg.len() >= 3 {
                let velocity = msg[2];
                if velocity == 0 {
                    Some(MidiEvent::NoteOff { channel, note: msg[1] })
                } else {
                    Some(MidiEvent::NoteOn { 
                        channel, 
                        note: msg[1], 
                        velocity 
                    })
                }
            } else {
                None
            }
        }
        0xB0 => {
            // Control Change
            if msg.len() >= 3 {
                Some(MidiEvent::ControlChange { 
                    channel, 
                    cc: msg[1], 
                    value: msg[2] 
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// MIDI mapping configuration
#[derive(Debug, Clone)]
pub struct MidiMapping {
    /// MIDI note number → Pad index (0-15)
    pub note_to_pad: [Option<usize>; 128],
    /// CC number → control type
    pub cc_map: std::collections::HashMap<u8, CcControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcControl {
    PadVolume(usize),
    PadSpeed(usize),
    GlobalVolume,
    Bpm,
}

impl Default for MidiMapping {
    fn default() -> Self {
        let mut note_to_pad = [None; 128];
        
        // Default: MIDI notes 36-51 (C2 to D#3) → pads 0-15
        for i in 0..16 {
            note_to_pad[36 + i] = Some(i);
        }
        
        let mut cc_map = std::collections::HashMap::new();
        cc_map.insert(1, CcControl::Bpm);
        
        Self {
            note_to_pad,
            cc_map,
        }
    }
}

impl MidiMapping {
    /// Convert MIDI event to input event
    pub fn map_event(&self, midi: &MidiEvent) -> Option<InputEvent> {
        match midi {
            MidiEvent::NoteOn { note, velocity, .. } => {
                self.note_to_pad.get(*note as usize)
                    .and_then(|&pad| pad)
                    .map(|pad| InputEvent::PadTrigger {
                        pad,
                        velocity: *velocity as f32 / 127.0,
                    })
            }
            MidiEvent::NoteOff { note, .. } => {
                self.note_to_pad.get(*note as usize)
                    .and_then(|&pad| pad)
                    .map(|pad| InputEvent::PadRelease { pad })
            }
            MidiEvent::ControlChange { cc, value, .. } => {
                self.cc_map.get(cc).and_then(|control| {
                    let normalized = *value as f32 / 127.0;
                    match control {
                        CcControl::PadVolume(pad) => Some(InputEvent::PadVolume {
                            pad: *pad,
                            volume: normalized,
                        }),
                        CcControl::PadSpeed(pad) => Some(InputEvent::PadSpeed {
                            pad: *pad,
                            speed: normalized * 2.0,
                        }),
                        CcControl::Bpm => Some(InputEvent::SetBpm(
                            60.0 + normalized * 180.0
                        )),
                        _ => None,
                    }
                })
            }
        }
    }
}
