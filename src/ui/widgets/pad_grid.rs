//! 4x4 pad grid widget

use crate::sampler::SamplePad;
use imgui::Ui;

pub struct PadGrid;

impl PadGrid {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&mut self, _ui: &Ui, _pads: &mut [SamplePad; 16]) {
        // Stub
    }
}

impl Default for PadGrid {
    fn default() -> Self {
        Self::new()
    }
}
