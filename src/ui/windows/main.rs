//! Main application window layout

use crate::sampler::{BankManager, VideoSample};
use crate::sequencer::SequencerEngine;
use imgui::Ui;
use std::path::PathBuf;

/// Commands from UI to App
#[derive(Debug, Clone)]
pub enum UICommand {
    StartRecording(usize), // pad_index
    StopRecording,
    CancelRecording,
    StartMidiLearn { control_id: String, min: f32, max: f32 },
    CancelMidiLearn,
    SavePreset(String), // preset name
    LoadPreset(String), // preset name
    DeletePreset(usize), // preset index
    RefreshVideoDevices,
    SelectVideoDevice(u32), // device index
    RefreshSyphonServers,
    SelectSyphonServer(String), // server name
}

pub struct MainWindow {
    /// Currently selected pad for loading
    selected_pad: Option<usize>,
    /// Async file dialog channel
    file_dialog_receiver: Option<flume::Receiver<Option<PathBuf>>>,
    /// Import result channel
    import_receiver: Option<flume::Receiver<Result<PathBuf, String>>>,
    /// Currently importing path (for display)
    importing_path: Option<String>,
    /// Command sender to App
    command_sender: Option<flume::Sender<UICommand>>,
    /// Recording state from App
    is_recording: bool,
    recording_pad: Option<usize>,
    /// MIDI learn state
    midi_learn_target: Option<String>, // Which control is being learned
    midi_learn_flash: f32, // Visual flash intensity (0-1)
    /// Preset management
    preset_name_input: String,
    selected_preset_index: i32,
    preset_names: Vec<String>,
    current_bank_name: String,
    /// Track which pads were triggered by mouse (for Gate mode)
    mouse_triggered: [bool; 16],
    /// Tap tempo info text
    tap_tempo_info: String,
    /// Control window for file dialogs (fixes macOS focus)
    control_window: Option<std::sync::Arc<winit::window::Window>>,
}

impl MainWindow {
    pub fn new() -> Self {
        Self {
            selected_pad: None,
            file_dialog_receiver: None,
            import_receiver: None,
            importing_path: None,
            command_sender: None,
            is_recording: false,
            recording_pad: None,
            midi_learn_target: None,
            midi_learn_flash: 0.0,
            preset_name_input: String::new(),
            selected_preset_index: -1,
            preset_names: Vec::new(),
            current_bank_name: "Default".to_string(),
            mouse_triggered: [false; 16],
            tap_tempo_info: "Tap 4+ times to set tempo".to_string(),
            control_window: None,
        }
    }
    
    /// Set up command channel (call once after creating)
    pub fn setup_command_channel(&mut self) -> flume::Receiver<UICommand> {
        let (tx, rx) = flume::unbounded();
        self.command_sender = Some(tx);
        rx
    }
    
    /// Get command sender (for external windows)
    pub fn command_sender(&self) -> Option<flume::Sender<UICommand>> {
        self.command_sender.clone()
    }
    
    /// Set control window reference for file dialogs (fixes macOS focus)
    pub fn set_control_window(&mut self, window: std::sync::Arc<winit::window::Window>) {
        self.control_window = Some(window);
    }
    
    /// Update recording state from App
    pub fn set_recording_state(&mut self, is_recording: bool, pad: Option<usize>) {
        self.is_recording = is_recording;
        self.recording_pad = pad;
    }
    
    /// Update MIDI learn state from App
    pub fn set_midi_learn_state(&mut self, target: Option<String>, flash: f32) {
        self.midi_learn_target = target;
        self.midi_learn_flash = flash;
    }
    
    /// Update preset list from App
    pub fn set_preset_list(&mut self, names: Vec<String>, bank_name: String) {
        self.preset_names = names;
        self.current_bank_name = bank_name;
        // Reset selection if out of bounds
        if self.selected_preset_index >= self.preset_names.len() as i32 {
            self.selected_preset_index = -1;
        }
    }
    
    fn send_command(&self, cmd: UICommand) {
        if let Some(ref sender) = self.command_sender {
            let _ = sender.send(cmd);
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        bank_manager: &mut BankManager,
        sequencer: &mut SequencerEngine,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        video_settings: &mut super::video_settings::VideoSettingsWindow,
    ) {
        self.handle_file_dialog_result(bank_manager, device, queue);
        self.handle_import_result(bank_manager, device, queue);
        
        // Main menu bar
        if let Some(_main_menu) = ui.begin_main_menu_bar() {
            if let Some(_view_menu) = ui.begin_menu("View") {
                if ui.menu_item("Video Settings") {
                    video_settings.toggle();
                }
            }
        }
        
        ui.text("Rusty-404 - Video Sampler");
        ui.separator();
        
        // Show import progress if active
        self.draw_import_status(ui);
        
        // Transport controls
        self.draw_transport(ui, sequencer);
        ui.separator();
        
        // Pad grid
        self.draw_pad_grid(ui, bank_manager);
        ui.separator();
        
        // Sequencer view
        self.draw_sequencer(ui, sequencer);
        ui.separator();
        
        // Mixer panel
        self.draw_mixer_panel(ui, bank_manager);
        ui.separator();
        
        // Preset panel
        self.draw_preset_panel(ui);
    }
    
    fn draw_import_status(&self, ui: &Ui) {
        if let Some(ref path) = self.importing_path {
            ui.separator();
            ui.text("Importing:");
            ui.text(path);
            
            // Show a progress bar (indeterminate since we don't have real progress)
            ui.text("Converting to HAP format...");
            
            // Spinner animation
            let spinner = ["|", "/", "-", "\\"];
            let frame = (ui.time() * 10.0) as usize % spinner.len();
            ui.text(spinner[frame]);
        }
    }
    
    fn draw_transport(&mut self, ui: &Ui, sequencer: &mut SequencerEngine) {
        let play_label = if sequencer.is_playing() { "STOP" } else { "PLAY" };
        if ui.button(play_label) {
            sequencer.toggle_playback();
        }
        ui.same_line();
        
        // BPM control
        let mut bpm = sequencer.bpm();
        ui.set_next_item_width(80.0);
        if ui.input_float("BPM", &mut bpm).build() {
            sequencer.set_bpm(bpm.clamp(20.0, 300.0));
        }
        ui.same_line();
        
        // Tap Tempo button with flash effect
        let flash = sequencer.tap_flash();
        let tap_color = [0.5 + flash * 0.5, 0.5 + flash * 0.5, 0.5, 1.0f32];
        let _color_token = ui.push_style_color(imgui::StyleColor::Button, tap_color);
        if ui.button("TAP") {
            if let Some(new_bpm) = sequencer.tap_tempo() {
                self.tap_tempo_info = format!("Tempo: {:.1} BPM", new_bpm);
            } else {
                let tap_count = sequencer.tap_count();
                if tap_count == 0 {
                    self.tap_tempo_info = "Reset: new tempo sequence".to_string();
                } else {
                    self.tap_tempo_info = format!("Tap {} more...", 4 - tap_count.min(4));
                }
            }
        }
        drop(_color_token);
        
        ui.same_line();
        ui.text(&self.tap_tempo_info);
        
        ui.same_line();
        ui.text(format!("Pos: {}", sequencer.position_display()));
        
        // Record button
        ui.same_line();
        let rec_color = if sequencer.is_recording() {
            [1.0f32, 0.0, 0.0, 1.0]
        } else {
            [0.5f32, 0.5, 0.5, 1.0]
        };
        ui.text_colored(rec_color, "● REC");
    }
    
    fn draw_pad_grid(&mut self, ui: &Ui, bank_manager: &mut BankManager) {
        ui.text("Pad Grid (Click to trigger, Right-click to load)");
        
        let bank = bank_manager.current_bank_mut();
        let button_size = [80.0f32, 60.0];
        
        for row in 0..4 {
            for col in 0..4 {
                let idx = row * 4 + col;
                let pad = &mut bank.pads[idx];
                
                // Determine button color based on state
                let (base_color, hover_color) = if pad.is_playing {
                    ([0.0f32, 0.8, 0.0, 1.0], [0.0f32, 1.0, 0.0, 1.0]) // Green when playing
                } else if pad.sample.is_some() {
                    ([0.8f32, 0.6, 0.0, 1.0], [1.0f32, 0.8, 0.0, 1.0]) // Orange when loaded
                } else {
                    ([0.3f32, 0.3, 0.3, 1.0], [0.4f32, 0.4, 0.4, 1.0]) // Gray when empty
                };
                
                // Style the button
                let _token1 = ui.push_style_color(imgui::StyleColor::Button, base_color);
                let _token2 = ui.push_style_color(imgui::StyleColor::ButtonHovered, hover_color);
                let _token3 = ui.push_style_color(imgui::StyleColor::ButtonActive, [0.0, 0.6, 0.0, 1.0]);
                
                let label = format!("{}\n{}", idx + 1, pad.name);
                
                // For gate mode, we need to detect mouse down on the button
                // For latch/one-shot, we use button click
                let button_size_with_pos = button_size; // Already positioned by imgui
                let item_pos = ui.cursor_screen_pos();
                
                // Draw the button and get click state
                let button_clicked = ui.button_with_size(&label, button_size);
                let _button_hovered = ui.is_item_hovered();
                let button_active = ui.is_item_active(); // True when held
                
                // Handle trigger modes
                match pad.trigger_mode {
                    crate::sampler::TriggerMode::Gate => {
                        // Gate: play while mouse is held down on the button
                        if button_active && !pad.is_triggered {
                            // Mouse pressed on button
                            pad.trigger();
                            self.mouse_triggered[idx] = true;
                        } else if !button_active && pad.is_triggered && self.mouse_triggered[idx] {
                            // Mouse released (only release if WE triggered it)
                            pad.release();
                            self.mouse_triggered[idx] = false;
                        }
                    }
                    _ => {
                        // Latch and OneShot: trigger on click (press + release)
                        if button_clicked {
                            pad.trigger();
                        }
                    }
                }
                
                // Right-click context menu
                if let Some(_token) = ui.begin_popup_context_item() {
                    ui.text(format!("Pad {}", idx + 1));
                    ui.separator();
                    
                    if ui.button("Load HAP Video...") {
                        self.open_file_dialog(idx);
                        ui.close_current_popup();
                    }
                    
                    if pad.sample.is_some() {
                        if ui.button("Clear") {
                            pad.clear();
                            ui.close_current_popup();
                        }
                    }
                    
                    ui.separator();
                    
                    // Recording options
                    if self.is_recording && self.recording_pad == Some(idx) {
                        // Currently recording to this pad
                        ui.text_colored([1.0, 0.0, 0.0, 1.0], "● REC");
                        if ui.button("Stop Recording") {
                            self.send_command(UICommand::StopRecording);
                            ui.close_current_popup();
                        }
                        if ui.button("Cancel") {
                            self.send_command(UICommand::CancelRecording);
                            ui.close_current_popup();
                        }
                    } else if self.is_recording {
                        // Recording to different pad
                        ui.text_disabled("Recording to another pad...");
                    } else {
                        // Not recording - show record option
                        if ui.button("Record from Webcam...") {
                            self.send_command(UICommand::StartRecording(idx));
                            ui.close_current_popup();
                        }
                    }
                    
                    // MIDI Learn section
                    ui.separator();
                    ui.text("MIDI Mapping");
                    
                    let trigger_control_id = format!("pad.trigger.{}", idx);
                    let volume_control_id = format!("pad.volume.{}", idx);
                    let speed_control_id = format!("pad.speed.{}", idx);
                    
                    // Check if we're learning this control
                    let learning_trigger = self.midi_learn_target.as_ref() == Some(&trigger_control_id);
                    let learning_volume = self.midi_learn_target.as_ref() == Some(&volume_control_id);
                    let learning_speed = self.midi_learn_target.as_ref() == Some(&speed_control_id);
                    
                    // Flash effect during learn mode
                    let flash_alpha = if self.midi_learn_flash > 0.5 { 1.0 } else { 0.5 };
                    let learn_color = [1.0f32, 0.8, 0.0, flash_alpha]; // Amber flash
                    
                    // Trigger Learn button
                    let trigger_label = if learning_trigger {
                        "Learning...".to_string()
                    } else {
                        "Learn Trigger".to_string()
                    };
                    if learning_trigger {
                        ui.text_colored(learn_color, &trigger_label);
                    } else if ui.button(&trigger_label) {
                        self.send_command(UICommand::StartMidiLearn { control_id: trigger_control_id, min: 0.0, max: 1.0 });
                        ui.close_current_popup();
                    }
                    
                    // Volume Learn button
                    let volume_label = if learning_volume {
                        "Learning...".to_string()
                    } else {
                        "Learn Volume".to_string()
                    };
                    if learning_volume {
                        ui.text_colored(learn_color, &volume_label);
                    } else if ui.button(&volume_label) {
                        self.send_command(UICommand::StartMidiLearn { control_id: volume_control_id, min: 0.0, max: 1.0 });
                        ui.close_current_popup();
                    }
                    
                    // Speed Learn button
                    let speed_label = if learning_speed {
                        "Learning...".to_string()
                    } else {
                        "Learn Speed".to_string()
                    };
                    if learning_speed {
                        ui.text_colored(learn_color, &speed_label);
                    } else if ui.button(&speed_label) {
                        self.send_command(UICommand::StartMidiLearn { control_id: speed_control_id, min: -5.0, max: 5.0 });
                        ui.close_current_popup();
                    }
                    
                    if learning_trigger || learning_volume || learning_speed {
                        if ui.button("Cancel Learn") {
                            self.send_command(UICommand::CancelMidiLearn);
                            ui.close_current_popup();
                        }
                    }
                    
                    if pad.sample.is_some() {
                        ui.separator();
                        
                        // Trigger mode selection
                        let modes = ["Gate", "Latch", "One-Shot"];
                        let mut mode_idx = pad.trigger_mode as usize;
                        if ui.combo_simple_string("Trigger Mode", &mut mode_idx, &modes) {
                            pad.trigger_mode = match mode_idx {
                                0 => crate::sampler::TriggerMode::Gate,
                                1 => crate::sampler::TriggerMode::Latch,
                                2 => crate::sampler::TriggerMode::OneShot,
                                _ => crate::sampler::TriggerMode::Gate,
                            };
                        }
                        
                        // Loop toggle
                        let mut loop_enabled = pad.loop_enabled;
                        if ui.checkbox("Loop Playback", &mut loop_enabled) {
                            pad.loop_enabled = loop_enabled;
                        }
                        
                        ui.separator();
                        ui.text("Playback Settings");
                        
                        // Playback speed (-5.0 to 5.0, default 1.0)
                        // Note: Reverse playback (-speed) may be choppy with streaming decoder
                        ui.set_next_item_width(120.0);
                        if ui.slider("Speed", -5.0, 5.0, &mut pad.speed) {
                            pad.speed = pad.speed.clamp(-5.0, 5.0);
                            if pad.speed == 0.0 {
                                pad.speed = 0.01;
                            }
                            pad.direction = if pad.speed >= 0.0 { 1 } else { -1 };
                        }
                        
                        // Opacity (for video mixing)
                        ui.set_next_item_width(120.0);
                        ui.slider("Opacity", 0.0, 1.0, &mut pad.volume);
                        
                        ui.separator();
                        ui.text("Mix Mode");
                        
                        // Mix mode selection
                        use crate::sampler::pad::PadMixMode;
                        let mix_modes = [
                            ("Normal", PadMixMode::Normal),
                            ("Add", PadMixMode::Add),
                            ("Multiply", PadMixMode::Multiply),
                            ("Screen", PadMixMode::Screen),
                            ("Overlay", PadMixMode::Overlay),
                            ("Soft Light", PadMixMode::SoftLight),
                            ("Hard Light", PadMixMode::HardLight),
                            ("Difference", PadMixMode::Difference),
                            ("Lighten", PadMixMode::Lighten),
                            ("Darken", PadMixMode::Darken),
                            ("Chroma Key", PadMixMode::ChromaKey),
                            ("Luma Key", PadMixMode::LumaKey),
                        ];
                        
                        let current_name = match pad.mix_mode {
                            PadMixMode::Normal => "Normal",
                            PadMixMode::Add => "Add",
                            PadMixMode::Multiply => "Multiply",
                            PadMixMode::Screen => "Screen",
                            PadMixMode::Overlay => "Overlay",
                            PadMixMode::SoftLight => "Soft Light",
                            PadMixMode::HardLight => "Hard Light",
                            PadMixMode::Difference => "Difference",
                            PadMixMode::Lighten => "Lighten",
                            PadMixMode::Darken => "Darken",
                            PadMixMode::ChromaKey => "Chroma Key",
                            PadMixMode::LumaKey => "Luma Key",
                        };
                        
                        if let Some(_combo) = ui.begin_combo("Mix Mode", current_name) {
                            for (name, mode) in &mix_modes {
                                let is_selected = pad.mix_mode == *mode;
                                if ui.selectable_config(name).selected(is_selected).build() {
                                    pad.mix_mode = *mode;
                                }
                            }
                        }
                        
                        // Keying parameters
                        if matches!(pad.mix_mode, PadMixMode::ChromaKey | PadMixMode::LumaKey) {
                            ui.separator();
                            ui.text("Keying Parameters");
                            
                            if matches!(pad.mix_mode, PadMixMode::ChromaKey) {
                                // Chroma key color presets
                                ui.text("Key Color:");
                                ui.same_line();
                                if ui.button("Green") {
                                    pad.key_params.key_color = [0.0, 1.0, 0.0];
                                }
                                ui.same_line();
                                if ui.button("Blue") {
                                    pad.key_params.key_color = [0.0, 0.0, 1.0];
                                }
                                ui.same_line();
                                if ui.button("Red") {
                                    pad.key_params.key_color = [1.0, 0.0, 0.0];
                                }
                                
                                let mut r = pad.key_params.key_color[0];
                                let mut g = pad.key_params.key_color[1];
                                let mut b = pad.key_params.key_color[2];
                                ui.set_next_item_width(80.0);
                                if ui.slider("R", 0.0, 1.0, &mut r) {
                                    pad.key_params.key_color[0] = r;
                                }
                                ui.set_next_item_width(80.0);
                                if ui.slider("G", 0.0, 1.0, &mut g) {
                                    pad.key_params.key_color[1] = g;
                                }
                                ui.set_next_item_width(80.0);
                                if ui.slider("B", 0.0, 1.0, &mut b) {
                                    pad.key_params.key_color[2] = b;
                                }
                            }
                            
                            ui.set_next_item_width(120.0);
                            ui.slider("Threshold", 0.0, 1.0, &mut pad.key_params.threshold);
                            
                            ui.set_next_item_width(120.0);
                            ui.slider("Smoothness", 0.0, 1.0, &mut pad.key_params.smoothness);
                            
                            if matches!(pad.mix_mode, PadMixMode::LumaKey) {
                                ui.checkbox("Invert", &mut pad.key_params.invert);
                            }
                        }
                        
                        ui.separator();
                        ui.text("Range Settings");
                        
                        // In/Out points - need to get frame count from sample
                        if let Some(ref sample) = pad.sample {
                            let (frame_count, mut in_point, mut out_point) = {
                                if let Ok(sample_guard) = sample.try_lock() {
                                    (sample_guard.frame_count, sample_guard.in_point, sample_guard.out_point)
                                } else {
                                    (0, 0, 0)
                                }
                            };
                            
                            if frame_count > 0 {
                                ui.set_next_item_width(120.0);
                                if ui.slider(format!("In Point##in{}", idx), 0, frame_count - 1, &mut in_point) {
                                    if in_point < out_point {
                                        if let Ok(mut s) = sample.try_lock() {
                                            s.in_point = in_point;
                                        }
                                    }
                                }
                                
                                ui.set_next_item_width(120.0);
                                if ui.slider(format!("Out Point##out{}", idx), 0, frame_count - 1, &mut out_point) {
                                    if out_point > in_point {
                                        if let Ok(mut s) = sample.try_lock() {
                                            s.out_point = out_point;
                                        }
                                    }
                                }
                                
                                // Show clip length
                                let clip_length = out_point.saturating_sub(in_point);
                                ui.text(format!("Clip Length: {} frames", clip_length));
                                
                                ui.text(format!("Frame: {} / {}", pad.current_frame as u32, frame_count));
                            }
                        }
                    }
                }
                
                if col < 3 {
                    ui.same_line();
                }
            }
        }
    }
    
    fn draw_sequencer(&self, ui: &Ui, sequencer: &mut SequencerEngine) {
        ui.text("Sequencer");
        
        // Pattern display - get values first to avoid borrow issues
        let (pattern_name, current_step, track_count, pattern_length) = {
            let pattern = sequencer.current_pattern();
            (
                pattern.name.clone(),
                sequencer.current_step(),
                pattern.tracks.len(),
                pattern.length()
            )
        };
        
        ui.text(format!("Pattern: {} | Tracks: {} | Step: {}/{}", 
            pattern_name, track_count, current_step + 1, pattern_length));
        
        // Transport buttons row
        if ui.button("Clear All") {
            sequencer.current_pattern_mut().clear();
        }
        ui.same_line();
        if ui.button("Randomize") {
            sequencer.current_pattern_mut().randomize(0.3);
        }
        ui.same_line();
        
        // Pattern navigation
        if ui.button("< Prev") {
            sequencer.prev_pattern();
        }
        ui.same_line();
        let queued = sequencer.queued_pattern.map(|q| q + 1).unwrap_or(sequencer.current_pattern + 1);
        ui.text(format!("Pattern {}", queued));
        ui.same_line();
        if ui.button("Next >") {
            sequencer.next_pattern();
        }
        
        // Show first few tracks as a simple step grid
        let step_size = [18.0f32, 18.0];
        let max_display_tracks = 8; // Show first 8 tracks
        let max_display_steps = 16; // Show 16 steps
        
        for track_idx in 0..max_display_tracks.min(track_count) {
            // Get track info first
            let (track_name, is_muted) = {
                let track = &sequencer.current_pattern().tracks[track_idx];
                (track.display_name(), track.muted)
            };
            
            // Track label with mute indicator
            let label = if is_muted {
                format!("{:2}: [M]", track_idx + 1)
            } else {
                format!("{:2}:", track_idx + 1)
            };
            ui.text(&label);
            
            // Right-click on label to mute/unmute
            if ui.is_item_clicked_with_button(imgui::MouseButton::Right) {
                let track = &mut sequencer.current_pattern_mut().tracks[track_idx];
                track.muted = !track.muted;
            }
            
            // Tooltip with track name
            if ui.is_item_hovered() {
                ui.tooltip_text(&track_name);
            }
            
            ui.same_line();
            
            // Step buttons for this track
            for step_idx in 0..max_display_steps.min(pattern_length) {
                let (is_active, is_current) = {
                    let pattern = sequencer.current_pattern();
                    let step = &pattern.tracks[track_idx].steps[step_idx];
                    (step.active, step_idx == current_step)
                };
                
                let color = if is_current && is_active {
                    [0.0f32, 1.0, 0.0, 1.0] // Bright green - current playing
                } else if is_current {
                    [0.0f32, 0.5, 0.0, 1.0] // Dark green - current step
                } else if is_active {
                    [0.8f32, 0.8, 0.0, 1.0] // Yellow - active step
                } else {
                    [0.15f32, 0.15, 0.15, 1.0] // Dark gray - inactive
                };
                
                let _token = ui.push_style_color(imgui::StyleColor::Button, color);
                if ui.button_with_size(&format!("##t{}s{}", track_idx, step_idx), step_size) {
                    // Toggle step on click
                    sequencer.toggle_step(track_idx, step_idx);
                }
                drop(_token);
                
                // Tooltip showing step info
                if ui.is_item_hovered() {
                    let step_info = {
                        let pattern = sequencer.current_pattern();
                        let step = &pattern.tracks[track_idx].steps[step_idx];
                        format!(
                            "Track: {}\nStep: {}\nActive: {}\nVelocity: {:.0}%\nProb: {:.0}%",
                            track_name,
                            step_idx + 1,
                            if step.active { "Yes" } else { "No" },
                            step.velocity * 100.0,
                            step.probability * 100.0
                        )
                    };
                    ui.tooltip_text(step_info);
                }
                
                if step_idx < max_display_steps - 1 {
                    ui.same_line();
                }
            }
        }
    }
    
    /// Open file dialog to load any video (will be converted to HAP)
    fn open_file_dialog(&mut self, pad_index: usize) {
        self.selected_pad = Some(pad_index);
        
        // Spawn file dialog on async runtime
        let (tx, rx) = flume::bounded(1);
        self.file_dialog_receiver = Some(rx);
        
        // Clone control window for proper focus on macOS
        let control_window = self.control_window.clone();
        
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new()
                .add_filter("All Videos", &["mov", "mp4", "avi", "mkv", "hap", "webm", "flv"])
                .add_filter("HAP Video", &["mov", "hap", "avi"])
                .add_filter("MP4", &["mp4"])
                .add_filter("MOV", &["mov"])
                .set_title("Import Video (will convert to HAP)");
            
            // Set parent window for proper focus (especially on macOS)
            // Note: rfd expects something implementing HasWindowHandle
            #[cfg(target_os = "macos")]
            let dialog = if let Some(ref window) = control_window {
                dialog.set_parent(window.as_ref())
            } else {
                dialog
            };
            
            let path = dialog.pick_file();
            let _ = tx.send(path);
        });
    }
    
    /// Handle file dialog result and start import/conversion
    fn handle_file_dialog_result(
        &mut self, 
        _bank_manager: &mut BankManager,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue
    ) {
        if let Some(ref receiver) = self.file_dialog_receiver {
            if let Ok(Some(path)) = receiver.try_recv() {
                // Start the import process
                self.start_import(path);
                self.file_dialog_receiver = None;
            }
        }
    }
    
    /// Start video import with HAP conversion
    fn start_import(&mut self, path: PathBuf) {
        use crate::video::import::VideoImporter;
        
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        
        // Check if already HAP
        let is_hap = VideoImporter::is_hap_file(&path);
        log::info!("Starting import for: {} (is_hap: {})", filename, is_hap);
        
        self.importing_path = Some(filename);
        
        // Spawn import task
        let (tx, rx) = flume::bounded(1);
        self.import_receiver = Some(rx);
        
        std::thread::spawn(move || {
            use crate::video::import::import_video;
            
            // Import and convert to HAP (or just return path if already HAP)
            let result = import_video(&path, None);
            
            // Send result back
            let msg = match result {
                Ok(hap_path) => Ok(hap_path),
                Err(e) => {
                    log::error!("Import failed: {}", e);
                    Err(e.to_string())
                }
            };
            let _ = tx.send(msg);
        });
    }
    
    /// Handle import completion and load into pad
    fn handle_import_result(
        &mut self, 
        bank_manager: &mut BankManager,
        device: &wgpu::Device,
        queue: &wgpu::Queue
    ) {
        if let Some(ref receiver) = self.import_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.importing_path = None;
                
                match result {
                    Ok(hap_path) => {
                        log::info!("Import complete: {:?}", hap_path);
                        
                        // Load into selected pad
                        if let Some(pad_index) = self.selected_pad {
                            let bank = bank_manager.current_bank_mut();
                            if let Some(pad) = bank.get_pad_mut(pad_index) {
                                // Actually load the HAP file
                                match VideoSample::from_hap(&hap_path, device, queue) {
                                    Ok(sample) => {
                                        log::info!("Loaded HAP sample into pad {}: {} ({}x{} @ {}fps, {} frames)", 
                                            pad_index, sample.name,
                                            sample.resolution.0, sample.resolution.1,
                                            sample.fps, sample.frame_count);
                                        
                                        // Assign sample to pad
                                        pad.assign_sample(sample);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to load HAP file: {}", e);
                                        pad.name = format!("Error: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Import failed: {}", e);
                        // Show error on the selected pad
                        if let Some(pad_index) = self.selected_pad {
                            let bank = bank_manager.current_bank_mut();
                            if let Some(pad) = bank.get_pad_mut(pad_index) {
                                pad.name = format!("Import Error");
                            }
                        }
                    }
                }
                
                self.import_receiver = None;
                self.selected_pad = None;
            }
        }
    }
    
    /// Draw preset save/load panel
    fn draw_preset_panel(&mut self, ui: &Ui) {
        ui.text("Presets");
        ui.text(format!("Bank: {}", self.current_bank_name));
        
        // Preset name input for saving
        ui.set_next_item_width(150.0);
        ui.input_text("Name", &mut self.preset_name_input)
            .build();
        
        ui.same_line();
        if ui.button("Save") && !self.preset_name_input.is_empty() {
            let name = self.preset_name_input.clone();
            self.send_command(UICommand::SavePreset(name));
            self.preset_name_input.clear();
        }
        
        ui.same_line();
        if ui.button("Refresh") {
            // App will update preset list on next frame
        }
        
        // Preset list
        ui.text("Saved Presets:");
        
        let mut delete_index: Option<usize> = None;
        let mut load_index: Option<usize> = None;
        
        for (i, name) in self.preset_names.iter().enumerate() {
            let is_selected = self.selected_preset_index == i as i32;
            
            // Selection indicator
            let label = if is_selected { "> " } else { "  " };
            
            ui.text(label);
            ui.same_line();
            
            // Preset name as selectable
            if ui.selectable_config(name)
                .selected(is_selected)
                .build() {
                self.selected_preset_index = i as i32;
            }
            
            // Context menu for load/delete
            if ui.begin_popup_context_item().is_some() {
                if ui.button("Load") {
                    load_index = Some(i);
                    ui.close_current_popup();
                }
                if ui.button("Delete") {
                    delete_index = Some(i);
                    ui.close_current_popup();
                }
            }
        }
        
        if self.preset_names.is_empty() {
            ui.text_disabled("No presets saved");
        }
        
        // Action buttons
        ui.separator();
        if ui.button("Load Selected") {
            if self.selected_preset_index >= 0 && 
               (self.selected_preset_index as usize) < self.preset_names.len() {
                let name = self.preset_names[self.selected_preset_index as usize].clone();
                self.send_command(UICommand::LoadPreset(name));
            }
        }
        
        ui.same_line();
        if ui.button("Delete Selected") {
            if self.selected_preset_index >= 0 && 
               (self.selected_preset_index as usize) < self.preset_names.len() {
                delete_index = Some(self.selected_preset_index as usize);
            }
        }
        
        // Send commands outside the loop
        if let Some(idx) = load_index {
            let name = self.preset_names[idx].clone();
            self.send_command(UICommand::LoadPreset(name));
        }
        
        if let Some(idx) = delete_index {
            self.send_command(UICommand::DeletePreset(idx));
            // Reset selection if we deleted the selected one
            if self.selected_preset_index == idx as i32 {
                self.selected_preset_index = -1;
            }
        }
    }
    
    fn draw_mixer_panel(&self, ui: &Ui, bank_manager: &mut BankManager) {
        use crate::sampler::pad::PadMixMode;
        
        ui.text("Mixer - Blend Modes & Keying");
        
        let bank = bank_manager.current_bank_mut();
        
        // Show a row of channel controls for active pads
        for (i, pad) in bank.pads.iter_mut().enumerate().take(8) {
            if pad.sample.is_none() {
                continue; // Skip empty pads
            }
            
            let is_keying = matches!(pad.mix_mode, PadMixMode::ChromaKey | PadMixMode::LumaKey);
            
            // Channel label with color indicator for keying
            if is_keying {
                ui.text_colored([0.0, 1.0, 0.5, 1.0], format!("Ch {}: {}", i + 1, pad.name));
            } else {
                ui.text(format!("Ch {}: {}", i + 1, pad.name));
            }
            ui.same_line();
            
            // Opacity slider (for video mixing)
            ui.set_next_item_width(60.0);
            ui.slider(&format!("##opacity{}", i), 0.0, 1.0, &mut pad.volume);
            ui.same_line();
            
            // Mix mode combo
            let current_name = match pad.mix_mode {
                PadMixMode::Normal => "Normal",
                PadMixMode::Add => "Add",
                PadMixMode::Multiply => "Multiply",
                PadMixMode::Screen => "Screen",
                PadMixMode::Overlay => "Overlay",
                PadMixMode::SoftLight => "Soft Light",
                PadMixMode::HardLight => "Hard Light",
                PadMixMode::Difference => "Difference",
                PadMixMode::Lighten => "Lighten",
                PadMixMode::Darken => "Darken",
                PadMixMode::ChromaKey => "Chroma Key",
                PadMixMode::LumaKey => "Luma Key",
            };
            
            ui.set_next_item_width(90.0);
            if let Some(_combo) = ui.begin_combo(&format!("##mode{}", i), current_name) {
                let modes = [
                    ("Normal", PadMixMode::Normal),
                    ("Add", PadMixMode::Add),
                    ("Multiply", PadMixMode::Multiply),
                    ("Screen", PadMixMode::Screen),
                    ("Overlay", PadMixMode::Overlay),
                    ("Soft Light", PadMixMode::SoftLight),
                    ("Hard Light", PadMixMode::HardLight),
                    ("Difference", PadMixMode::Difference),
                    ("Lighten", PadMixMode::Lighten),
                    ("Darken", PadMixMode::Darken),
                    ("Chroma Key", PadMixMode::ChromaKey),
                    ("Luma Key", PadMixMode::LumaKey),
                ];
                
                for (name, mode) in &modes {
                    let is_selected = pad.mix_mode == *mode;
                    if ui.selectable_config(name).selected(is_selected).build() {
                        pad.mix_mode = *mode;
                    }
                }
            }
            
            // Keying parameter controls
            if is_keying {
                ui.same_line();
                
                if matches!(pad.mix_mode, PadMixMode::ChromaKey) {
                    // Chroma key color picker (simplified - just RGB)
                    ui.set_next_item_width(40.0);
                    let mut key_color = pad.key_params.key_color;
                    
                    // Show color as a small colored button
                    let color_btn = [key_color[0], key_color[1], key_color[2], 1.0f32];
                    let _color_token = ui.push_style_color(imgui::StyleColor::Button, color_btn);
                    ui.set_next_item_width(20.0);
                    if ui.button(&format!("##keycolor{}", i)) {
                        // Toggle between green and blue screen on click
                        if key_color[1] > 0.5 { // Currently green
                            key_color = [0.0, 0.0, 1.0]; // Switch to blue
                        } else {
                            key_color = [0.0, 1.0, 0.0]; // Switch to green
                        }
                        pad.key_params.key_color = key_color;
                    }
                    drop(_color_token);
                    
                    ui.same_line();
                    ui.set_next_item_width(50.0);
                    ui.slider(&format!("##thresh{}", i), 0.0, 1.0, &mut pad.key_params.threshold);
                } else {
                    // Luma key threshold
                    ui.set_next_item_width(60.0);
                    ui.slider(&format!("##lumathresh{}", i), 0.0, 1.0, &mut pad.key_params.threshold);
                    
                    ui.same_line();
                    let mut invert = pad.key_params.invert;
                    if ui.checkbox(&format!("Inv##inv{}", i), &mut invert) {
                        pad.key_params.invert = invert;
                    }
                }
            }
        }
        
        // If no pads loaded
        if bank.pads.iter().take(8).all(|p| p.sample.is_none()) {
            ui.text_disabled("Load samples to see mixer controls");
        }
    }
}

impl Default for MainWindow {
    fn default() -> Self {
        Self::new()
    }
}
