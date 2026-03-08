//! Unified input events from MIDI, OSC, and keyboard

/// Input event types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// Trigger a pad (press)
    PadTrigger { pad: usize, velocity: f32 },
    /// Release a pad (for gate mode)
    PadRelease { pad: usize },
    /// Set pad volume
    PadVolume { pad: usize, volume: f32 },
    /// Set pad speed
    PadSpeed { pad: usize, speed: f32 },
    /// Stop all pads
    StopAll,
    /// BPM change
    SetBpm(f32),
    /// Start/stop sequencer
    ToggleSequencer,
}

/// MIDI event types (raw)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ControlChange { channel: u8, cc: u8, value: u8 },
}

/// OSC message types
#[derive(Debug, Clone, PartialEq)]
pub enum OscEvent {
    Trigger { pad: usize },
    Release { pad: usize },
    Volume { pad: usize, value: f32 },
    Speed { pad: usize, value: f32 },
    Bpm(f32),
    Command(String),
}

/// Input source identification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputSource {
    Midi { port: usize },
    Osc { addr: std::net::SocketAddr },
    Keyboard,
    Ui,
}

/// Timestamped input event
#[derive(Debug, Clone)]
pub struct TimedInputEvent {
    pub event: InputEvent,
    pub source: InputSource,
    pub timestamp: std::time::Instant,
}

impl TimedInputEvent {
    pub fn new(event: InputEvent, source: InputSource) -> Self {
        Self {
            event,
            source,
            timestamp: std::time::Instant::now(),
        }
    }
}
