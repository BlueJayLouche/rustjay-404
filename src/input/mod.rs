//! Input handling - MIDI, keyboard, OSC

pub mod events;
pub mod keyboard;
pub mod midi;
pub mod midi_mapping;
pub mod osc;
pub mod router;

pub use events::*;
pub use midi_mapping::{MidiMappingConfig, MidiMappingEntry, MidiLearnState};
pub use router::InputRouter;
