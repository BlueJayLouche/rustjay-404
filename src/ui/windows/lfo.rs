//! LFO control window

use crate::lfo::{beat_division_to_hz, LfoBank, LfoTarget, Waveform};
use imgui::Ui;

/// LFO control window
pub struct LfoWindow;

impl LfoWindow {
    pub fn new() -> Self {
        Self
    }

    pub fn draw(&self, ui: &Ui, lfo_bank: &mut LfoBank, bpm: f32) {
        ui.window("LFO Control")
            .size([520.0, 500.0], imgui::Condition::FirstUseEver)
            .build(|| {
                ui.text("Low Frequency Oscillator Modulation");
                ui.text_disabled("Each LFO can modulate pad opacity or speed");
                ui.separator();
                ui.text(format!("Tempo: {:.1} BPM", bpm));
                ui.spacing();

                let waveforms = ["Sine", "Triangle", "Ramp Up", "Ramp Down", "Square"];
                let divisions = ["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "8"];

                for i in 0..4 {
                    let _id_token = ui.push_id(format!("lfo_{}", i));

                    let enabled = lfo_bank.lfos[i].enabled;
                    let header_label = format!(
                        "LFO {} - {}",
                        i + 1,
                        if enabled { "ON" } else { "OFF" }
                    );

                    if ui.collapsing_header(&header_label, imgui::TreeNodeFlags::DEFAULT_OPEN) {
                        // Enable checkbox
                        ui.checkbox("Enabled", &mut lfo_bank.lfos[i].enabled);

                        ui.separator();

                        // Rate control
                        let tempo_sync = lfo_bank.lfos[i].tempo_sync;
                        if tempo_sync {
                            let mut div = lfo_bank.lfos[i].division;
                            let _w = ui.push_item_width(100.0);
                            if ui.combo_simple_string("Beat Division", &mut div, &divisions) {
                                lfo_bank.lfos[i].division = div;
                            }
                        } else {
                            let _w = ui.push_item_width(200.0);
                            ui.slider("Rate (Hz)", 0.01, 10.0, &mut lfo_bank.lfos[i].rate);
                        }

                        // Tempo sync toggle
                        ui.checkbox("Tempo Sync", &mut lfo_bank.lfos[i].tempo_sync);
                        if tempo_sync {
                            ui.same_line();
                            ui.text_disabled(format!(
                                "= {:.2} Hz",
                                beat_division_to_hz(lfo_bank.lfos[i].division, bpm)
                            ));
                        }

                        ui.separator();

                        // Waveform selection
                        ui.text("Waveform:");
                        let current_wf = lfo_bank.lfos[i].waveform as usize;
                        for (wf_idx, wf_name) in waveforms.iter().enumerate() {
                            if wf_idx > 0 {
                                ui.same_line();
                            }
                            let is_selected = current_wf == wf_idx;
                            if is_selected {
                                let _color = ui.push_style_color(
                                    imgui::StyleColor::Button,
                                    [0.2, 0.6, 0.8, 1.0],
                                );
                                ui.button(wf_name);
                            } else if ui.button(wf_name) {
                                lfo_bank.lfos[i].waveform = match wf_idx {
                                    0 => Waveform::Sine,
                                    1 => Waveform::Triangle,
                                    2 => Waveform::Ramp,
                                    3 => Waveform::Saw,
                                    4 => Waveform::Square,
                                    _ => Waveform::Sine,
                                };
                            }
                        }

                        // Phase offset
                        {
                            let _w = ui.push_item_width(200.0);
                            ui.slider(
                                "Phase Offset",
                                0.0,
                                360.0,
                                &mut lfo_bank.lfos[i].phase_offset,
                            );
                        }

                        // Amplitude
                        {
                            let _w = ui.push_item_width(200.0);
                            ui.slider("Amplitude", -1.0, 1.0, &mut lfo_bank.lfos[i].amplitude);
                        }

                        ui.separator();

                        // Target selection
                        Self::draw_target_selector(ui, &mut lfo_bank.lfos[i].target);

                        // Visual indicator
                        if lfo_bank.lfos[i].enabled && lfo_bank.lfos[i].target != LfoTarget::None {
                            ui.spacing();
                            let output = lfo_bank.lfos[i].output;
                            let bar_width = (output.abs() * 100.0).min(200.0);
                            ui.text(format!("Output: {:.3}", output));
                            let draw_list = ui.get_window_draw_list();
                            let pos = ui.cursor_screen_pos();
                            let color = if output >= 0.0 {
                                [0.0, 0.8, 0.2, 1.0]
                            } else {
                                [0.8, 0.2, 0.0, 1.0]
                            };
                            draw_list
                                .add_rect(pos, [pos[0] + bar_width, pos[1] + 6.0], color)
                                .filled(true)
                                .build();
                            ui.dummy([200.0, 8.0]);
                        }
                    }
                }

                ui.separator();
                if ui.button("Reset All LFOs") {
                    lfo_bank.reset_all();
                }
            });
    }

    fn draw_target_selector(ui: &Ui, target: &mut LfoTarget) {
        // Build target list: None, Pad 1-16 Opacity, Pad 1-16 Speed, Master Opacity
        let target_names: Vec<String> = std::iter::once("None".to_string())
            .chain((0..16).map(|i| format!("Pad {} Opacity", i + 1)))
            .chain((0..16).map(|i| format!("Pad {} Speed", i + 1)))
            .chain(std::iter::once("Master Opacity".to_string()))
            .collect();

        // Map current target to index
        let current_idx = match *target {
            LfoTarget::None => 0,
            LfoTarget::PadOpacity(i) => 1 + i,
            LfoTarget::PadSpeed(i) => 17 + i,
            LfoTarget::MasterOpacity => 33,
        };

        let mut selected = current_idx;
        let names_refs: Vec<&str> = target_names.iter().map(|s| s.as_str()).collect();

        let _w = ui.push_item_width(160.0);
        if ui.combo_simple_string("Target", &mut selected, &names_refs) {
            *target = match selected {
                0 => LfoTarget::None,
                1..=16 => LfoTarget::PadOpacity(selected - 1),
                17..=32 => LfoTarget::PadSpeed(selected - 17),
                33 => LfoTarget::MasterOpacity,
                _ => LfoTarget::None,
            };
        }
    }
}

impl Default for LfoWindow {
    fn default() -> Self {
        Self::new()
    }
}
