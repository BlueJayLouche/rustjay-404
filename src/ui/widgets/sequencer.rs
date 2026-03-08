//! Sequencer grid widget

use crate::sequencer::SequencerEngine;
use imgui::Ui;

pub struct SequencerWidget;

impl SequencerWidget {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&mut self, _ui: &Ui, _sequencer: &mut SequencerEngine) {
        // Stub
    }
}

impl Default for SequencerWidget {
    fn default() -> Self {
        Self::new()
    }
}
