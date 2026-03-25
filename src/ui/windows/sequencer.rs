//! Sequencer window - dedicated step sequencer with enhanced features

use crate::sequencer::SequencerEngine;
use imgui::Ui;

/// Dedicated sequencer window
pub struct SequencerWindow {
    /// Step edit popup state
    editing_track: Option<usize>,
    editing_step: Option<usize>,
}

impl SequencerWindow {
    pub fn new() -> Self {
        Self {
            editing_track: None,
            editing_step: None,
        }
    }

    pub fn draw(&mut self, ui: &Ui, sequencer: &mut SequencerEngine) {
        ui.window("Sequencer")
            .size([700.0, 450.0], imgui::Condition::FirstUseEver)
            .build(|| {
                self.draw_transport(ui, sequencer);
                ui.separator();
                self.draw_pattern_bank(ui, sequencer);
                ui.separator();
                self.draw_step_grid(ui, sequencer);
            });
    }

    fn draw_transport(&self, ui: &Ui, sequencer: &mut SequencerEngine) {
        let play_label = if sequencer.is_playing() { "STOP" } else { "PLAY" };
        if ui.button_with_size(play_label, [60.0, 24.0]) {
            sequencer.toggle_playback();
        }

        ui.same_line();
        let mut bpm = sequencer.bpm();
        ui.set_next_item_width(80.0);
        if ui.input_float("BPM##seq", &mut bpm).build() {
            sequencer.set_bpm(bpm.clamp(20.0, 300.0));
        }

        ui.same_line();
        ui.text(format!("Step: {}", sequencer.position_display()));

        ui.same_line();
        if ui.button("Clear All") {
            sequencer.current_pattern_mut().clear();
        }
        ui.same_line();
        if ui.button("Randomize") {
            sequencer.current_pattern_mut().randomize(0.3);
        }

        // Pattern length control
        ui.same_line();
        let mut length = sequencer.current_pattern().length() as i32;
        ui.set_next_item_width(60.0);
        if ui.input_int("Steps##len", &mut length).build() {
            let length = length.clamp(1, 64) as usize;
            sequencer.current_pattern_mut().set_length(length);
        }
    }

    fn draw_pattern_bank(&self, ui: &Ui, sequencer: &mut SequencerEngine) {
        ui.text("Patterns:");
        ui.same_line();

        for i in 0..16 {
            if i > 0 {
                ui.same_line();
            }

            let is_current = sequencer.current_pattern == i;
            let is_queued = sequencer.queued_pattern == Some(i);

            let color = if is_current {
                [0.0f32, 0.7, 0.0, 1.0] // Green - current
            } else if is_queued {
                [0.8f32, 0.8, 0.0, 1.0] // Yellow - queued
            } else {
                [0.25f32, 0.25, 0.25, 1.0] // Dark gray
            };

            let _token = ui.push_style_color(imgui::StyleColor::Button, color);
            if ui.button_with_size(&format!("{}", i + 1), [28.0, 22.0]) {
                if sequencer.is_playing() {
                    sequencer.queue_pattern(i);
                } else {
                    sequencer.switch_pattern(i);
                }
            }
            drop(_token);
        }
    }

    fn draw_step_grid(&mut self, ui: &Ui, sequencer: &mut SequencerEngine) {
        let current_step = sequencer.current_step();
        let pattern_length = sequencer.current_pattern().length();
        let track_count = sequencer.current_pattern().tracks.len();
        let max_display_tracks = 16;
        let step_size = [20.0f32, 18.0];

        // Check if any track has solo enabled
        let any_solo = (0..track_count).any(|i| sequencer.current_pattern().tracks[i].solo);

        for track_idx in 0..max_display_tracks.min(track_count) {
            let (track_name, is_muted, is_solo) = {
                let track = &sequencer.current_pattern().tracks[track_idx];
                (track.display_name(), track.muted, track.solo)
            };

            // Mute button
            let mute_color = if is_muted {
                [0.8f32, 0.0, 0.0, 1.0]
            } else {
                [0.3f32, 0.3, 0.3, 1.0]
            };
            let _mc = ui.push_style_color(imgui::StyleColor::Button, mute_color);
            if ui.button_with_size(&format!("M##m{}", track_idx), [20.0, 18.0]) {
                let track = &mut sequencer.current_pattern_mut().tracks[track_idx];
                track.muted = !track.muted;
            }
            drop(_mc);

            ui.same_line();

            // Solo button
            let solo_color = if is_solo {
                [0.8f32, 0.8, 0.0, 1.0]
            } else {
                [0.3f32, 0.3, 0.3, 1.0]
            };
            let _sc = ui.push_style_color(imgui::StyleColor::Button, solo_color);
            if ui.button_with_size(&format!("S##s{}", track_idx), [20.0, 18.0]) {
                let track = &mut sequencer.current_pattern_mut().tracks[track_idx];
                track.solo = !track.solo;
            }
            drop(_sc);

            ui.same_line();

            // Track label
            let dimmed = is_muted || (any_solo && !is_solo);
            if dimmed {
                ui.text_disabled(format!("{:2}:", track_idx + 1));
            } else {
                ui.text(format!("{:2}:", track_idx + 1));
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(&track_name);
            }

            ui.same_line();

            // Step buttons
            for step_idx in 0..pattern_length.min(64) {
                // Beat grouping gap every 4 steps
                if step_idx > 0 && step_idx % 4 == 0 {
                    ui.same_line_with_spacing(0.0, 6.0);
                } else if step_idx > 0 {
                    ui.same_line();
                }

                let (is_active, velocity) = {
                    let step = &sequencer.current_pattern().tracks[track_idx].steps[step_idx];
                    (step.active, step.velocity)
                };
                let is_current = step_idx == current_step;

                let color = if is_current && is_active {
                    [0.0f32, 1.0, 0.0, 1.0] // Bright green - playing active step
                } else if is_current {
                    [0.0f32, 0.4, 0.0, 1.0] // Dark green - playhead
                } else if is_active {
                    // Velocity-based intensity
                    let v = velocity.clamp(0.0, 1.0);
                    [0.3 + v * 0.5, 0.3 + v * 0.5, 0.0, 1.0] // Yellow, brighter with velocity
                } else {
                    [0.12f32, 0.12, 0.12, 1.0] // Dark gray - inactive
                };

                let _token = ui.push_style_color(imgui::StyleColor::Button, color);
                if ui.button_with_size(&format!("##t{}s{}", track_idx, step_idx), step_size) {
                    sequencer.toggle_step(track_idx, step_idx);
                }
                drop(_token);

                // Right-click for step edit popup
                if ui.is_item_clicked_with_button(imgui::MouseButton::Right) {
                    self.editing_track = Some(track_idx);
                    self.editing_step = Some(step_idx);
                    ui.open_popup(format!("step_edit_{}_{}", track_idx, step_idx));
                }

                // Step edit popup
                if let (Some(et), Some(es)) = (self.editing_track, self.editing_step) {
                    if et == track_idx && es == step_idx {
                        ui.popup(format!("step_edit_{}_{}", track_idx, step_idx), || {
                            ui.text(format!("Track {} - Step {}", track_idx + 1, step_idx + 1));
                            ui.separator();

                            let step = &mut sequencer.current_pattern_mut().tracks[track_idx].steps[step_idx];

                            let mut active = step.active;
                            if ui.checkbox("Active", &mut active) {
                                step.active = active;
                            }

                            ui.set_next_item_width(120.0);
                            ui.slider("Velocity", 0.0, 1.0, &mut step.velocity);

                            ui.set_next_item_width(120.0);
                            ui.slider("Probability", 0.0, 1.0, &mut step.probability);

                            ui.set_next_item_width(120.0);
                            ui.slider("Gate Length", 0.01, 1.0, &mut step.gate_length);

                            let mut ratchet = step.ratchet as i32;
                            ui.set_next_item_width(120.0);
                            if ui.input_int("Ratchet", &mut ratchet).build() {
                                step.ratchet = ratchet.clamp(1, 8) as u8;
                            }
                        });
                    }
                }

                // Tooltip
                if ui.is_item_hovered() {
                    let step = &sequencer.current_pattern().tracks[track_idx].steps[step_idx];
                    ui.tooltip_text(format!(
                        "Track: {}\nStep: {}\nActive: {}\nVelocity: {:.0}%\nProb: {:.0}%\nGate: {:.0}%\nRatchet: {}",
                        track_name,
                        step_idx + 1,
                        if step.active { "Yes" } else { "No" },
                        step.velocity * 100.0,
                        step.probability * 100.0,
                        step.gate_length * 100.0,
                        step.ratchet
                    ));
                }
            }
        }
    }
}

impl Default for SequencerWindow {
    fn default() -> Self {
        Self::new()
    }
}
