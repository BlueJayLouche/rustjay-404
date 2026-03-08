//! Application state machine

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal performance mode - pad grid, mixer
    Perform,
    /// Sample editing - in/out points, waveform
    Edit,
    /// Live capture mode
    Record,
    /// Sequencer editing
    Sequence,
}

pub struct AppState {
    pub mode: AppMode,
    pub selected_pad: Option<usize>,
    pub show_settings: bool,
    pub show_browser: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Perform,
            selected_pad: None,
            show_settings: false,
            show_browser: false,
        }
    }
    
    pub fn next_mode(&mut self) {
        self.mode = match self.mode {
            AppMode::Perform => AppMode::Edit,
            AppMode::Edit => AppMode::Sequence,
            AppMode::Sequence => AppMode::Record,
            AppMode::Record => AppMode::Perform,
        };
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
