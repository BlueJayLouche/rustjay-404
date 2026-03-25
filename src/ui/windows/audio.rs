//! Audio analysis and routing window

use crate::audio::routing::{FftBand, ModulationTarget, RoutingMatrix};
use crate::audio::AudioAnalyzer;
use imgui::Ui;

/// Audio control window state
pub struct AudioWindow {
    /// Cached device list
    pub audio_devices: Vec<String>,
    /// Currently selected device index in the dropdown
    pub selected_device: usize,
    /// Currently active device name
    pub active_device: Option<String>,
    /// Tap tempo history
    tap_times: Vec<f64>,
    last_tap_time: f64,
    pub tap_tempo_info: String,
    /// New route selection state
    selected_band: usize,
    selected_target: usize,
    /// Show routing matrix sub-window
    show_routing: bool,
}

impl AudioWindow {
    pub fn new() -> Self {
        Self {
            audio_devices: Vec::new(),
            selected_device: 0,
            active_device: None,
            tap_times: Vec::new(),
            last_tap_time: 0.0,
            tap_tempo_info: String::new(),
            selected_band: 1,  // Bass
            selected_target: 0,
            show_routing: false,
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        analyzer: &mut AudioAnalyzer,
        routing: &mut RoutingMatrix,
        bpm: &mut f32,
    ) {
        ui.window("Audio Analysis")
            .size([520.0, 620.0], imgui::Condition::FirstUseEver)
            .build(|| {
                self.draw_device_section(ui, analyzer);
                ui.separator();
                self.draw_controls(ui, analyzer);
                ui.separator();
                self.draw_tempo_section(ui, bpm);
                ui.separator();
                self.draw_fft_bars(ui, analyzer);
                ui.separator();
                self.draw_routing_section(ui, routing);
            });

        if self.show_routing {
            self.draw_routing_window(ui, routing);
        }
    }

    fn draw_device_section(&mut self, ui: &Ui, analyzer: &mut AudioAnalyzer) {
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Input Device");

        if ui.button("Refresh Devices") {
            self.audio_devices = crate::audio::list_audio_devices();
        }

        ui.spacing();

        if !self.audio_devices.is_empty() {
            let names: Vec<&str> = self.audio_devices.iter().map(|s| s.as_str()).collect();

            // Sync selected index with active device
            if let Some(ref active) = self.active_device {
                if let Some(idx) = self.audio_devices.iter().position(|d| d == active) {
                    self.selected_device = idx;
                }
            }

            ui.combo_simple_string("Device", &mut self.selected_device, &names);

            if ui.button("Start") {
                let device_name = self.audio_devices.get(self.selected_device).cloned();
                match analyzer.start_with_device(device_name.as_deref()) {
                    Ok(()) => {
                        self.active_device = device_name;
                    }
                    Err(e) => log::error!("Failed to start audio: {}", e),
                }
            }
            ui.same_line();
            if ui.button("Stop") {
                analyzer.stop();
                self.active_device = None;
            }

            if let Some(ref device) = self.active_device {
                ui.same_line();
                ui.text_colored([0.0, 0.8, 0.0, 1.0], format!("Active: {}", device));
            }
        } else {
            ui.text_disabled("No devices found. Click Refresh.");
        }
    }

    fn draw_controls(&self, ui: &Ui, analyzer: &AudioAnalyzer) {
        ui.text("Processing");

        let mut amplitude = analyzer.get_fft().iter().sum::<f32>().max(0.1); // read-back proxy
        // We'll use direct config values instead
        let _ = amplitude;

        let mut normalize = analyzer.get_normalize();
        if ui.checkbox("Normalize Bands", &mut normalize) {
            analyzer.set_normalize(normalize);
        }
        ui.same_line();
        ui.text_disabled("(Scales all bands to max)");

        let mut pink = analyzer.get_pink_noise_shaping();
        if ui.checkbox("+3dB/Octave Shaping", &mut pink) {
            analyzer.set_pink_noise_shaping(pink);
        }
        ui.same_line();
        ui.text_disabled("(Compensates for pink noise)");
    }

    fn draw_tempo_section(&mut self, ui: &Ui, bpm: &mut f32) {
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Tempo");
        ui.text(format!("BPM: {:.1}", bpm));

        let _btn = ui.push_style_color(imgui::StyleColor::Button, [0.8, 0.3, 0.3, 1.0]);
        let _hov = ui.push_style_color(imgui::StyleColor::ButtonHovered, [0.9, 0.4, 0.4, 1.0]);
        let _act = ui.push_style_color(imgui::StyleColor::ButtonActive, [1.0, 0.5, 0.5, 1.0]);

        if ui.button_with_size("TAP", [60.0, 30.0]) {
            self.handle_tap_tempo(bpm);
        }

        ui.same_line();
        if !self.tap_tempo_info.is_empty() {
            ui.text_disabled(&self.tap_tempo_info);
        }
    }

    fn draw_fft_bars(&self, ui: &Ui, analyzer: &AudioAnalyzer) {
        ui.text("Frequency Bands");
        let fft = analyzer.get_fft();
        let volume = analyzer.get_volume();
        let band_names = [
            "Sub", "Bass", "Low", "Mid", "HiMid", "High", "VHigh", "Pres",
        ];

        for (&value, name) in fft.iter().zip(band_names.iter()) {
            let bar_width = (200.0 * value).min(200.0);
            ui.text(format!("{:>5}: {:.2}", name, value));

            let draw_list = ui.get_window_draw_list();
            let pos = ui.cursor_screen_pos();
            let color = [0.0, 0.9, 0.3, 0.9];
            draw_list
                .add_rect(pos, [pos[0] + bar_width, pos[1] + 8.0], color)
                .filled(true)
                .build();
            ui.dummy([200.0, 10.0]);
        }

        ui.spacing();
        ui.text(format!("Volume: {:.2}", volume));
    }

    fn draw_routing_section(&mut self, ui: &Ui, routing: &RoutingMatrix) {
        ui.text_colored([0.0, 1.0, 1.0, 1.0], "Audio Routing");

        if ui.button("Open Routing Matrix") {
            self.show_routing = !self.show_routing;
        }

        let count = routing.len();
        if count > 0 {
            ui.text(format!("Active routes: {}", count));
            for (i, route) in routing.routes().iter().enumerate() {
                if !route.enabled {
                    continue;
                }
                ui.text(format!(
                    "  {} -> {} ({:.0}%)",
                    route.band.short_name(),
                    route.target.name(),
                    route.amount * 100.0
                ));
                if i >= 3 {
                    let remaining = count.saturating_sub(4);
                    if remaining > 0 {
                        ui.text_disabled(format!("  ... and {} more", remaining));
                    }
                    break;
                }
            }
        } else {
            ui.text_disabled("No routes. Open Routing Matrix to add.");
        }
    }

    fn draw_routing_window(&mut self, ui: &Ui, routing: &mut RoutingMatrix) {
        let mut is_open = true;

        ui.window("Audio Routing Matrix")
            .position([500.0, 100.0], imgui::Condition::FirstUseEver)
            .size([450.0, 550.0], imgui::Condition::FirstUseEver)
            .opened(&mut is_open)
            .build(|| {
                let can_add = routing.can_add_route();
                ui.text(format!(
                    "Routes: {}/{}",
                    routing.len(),
                    routing.max_routes()
                ));
                ui.same_line();
                if ui.button("Clear All") {
                    routing.clear();
                }

                ui.separator();
                ui.text_colored([0.0, 1.0, 1.0, 1.0], "Add New Route");

                // Band selector
                let bands: Vec<&str> = FftBand::all().iter().map(|b| b.name()).collect();
                ui.combo_simple_string("Band##new", &mut self.selected_band, &bands);

                // Target selector
                let all_targets = ModulationTarget::all_options();
                let target_names: Vec<String> = all_targets.iter().map(|t| t.name()).collect();
                let target_refs: Vec<&str> = target_names.iter().map(|s| s.as_str()).collect();
                ui.combo_simple_string("Target##new", &mut self.selected_target, &target_refs);

                ui.same_line();
                if can_add {
                    if ui.button("Add Route") {
                        if let Some(band) = FftBand::from_index(self.selected_band) {
                            if let Some(target) = all_targets.get(self.selected_target) {
                                routing.add_route(band, *target);
                            }
                        }
                    }
                } else {
                    ui.text_disabled("Max routes reached");
                }

                ui.separator();
                ui.text_colored([0.0, 1.0, 1.0, 1.0], "Active Routes");

                // Collect route data to avoid borrow issues
                let routes_data: Vec<_> = routing
                    .routes()
                    .iter()
                    .map(|r| {
                        (
                            r.id,
                            r.band,
                            r.target,
                            r.amount,
                            r.attack,
                            r.release,
                            r.enabled,
                            r.current_value,
                        )
                    })
                    .collect();

                let mut to_remove = None;

                for (id, band, target, amount, attack, release, enabled, current) in &routes_data {
                    let _id_token = ui.push_id(format!("route_{}", id));

                    let mut is_enabled = *enabled;
                    if ui.checkbox("##enabled", &mut is_enabled) {
                        if let Some(route) = routing.get_route_mut(*id) {
                            route.enabled = is_enabled;
                        }
                    }
                    ui.same_line();
                    ui.text(format!("{} -> {}", band.short_name(), target.name()));
                    ui.same_line();
                    ui.text_colored([0.0, 1.0, 0.0, 1.0], format!("{:.2}", current));
                    ui.same_line();
                    if ui.button("X") {
                        to_remove = Some(*id);
                    }

                    let mut amt = *amount;
                    if ui.slider("Amount", -1.0, 1.0, &mut amt) {
                        if let Some(route) = routing.get_route_mut(*id) {
                            route.amount = amt;
                        }
                    }

                    ui.columns(2, "attack_release", false);
                    let mut atk = *attack;
                    if ui.slider("Attack", 0.001, 1.0, &mut atk) {
                        if let Some(route) = routing.get_route_mut(*id) {
                            route.attack = atk;
                        }
                    }
                    ui.next_column();
                    let mut rel = *release;
                    if ui.slider("Release", 0.001, 1.0, &mut rel) {
                        if let Some(route) = routing.get_route_mut(*id) {
                            route.release = rel;
                        }
                    }
                    ui.columns(1, "", false);

                    ui.separator();
                }

                if let Some(id) = to_remove {
                    routing.remove_route(id);
                }

                if routes_data.is_empty() {
                    ui.text_disabled("No routes configured. Add one above.");
                }
            });

        if !is_open {
            self.show_routing = false;
        }
    }

    fn handle_tap_tempo(&mut self, bpm: &mut f32) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Reset if >2s since last tap
        if now - self.last_tap_time > 2.0 {
            self.tap_times.clear();
            self.tap_tempo_info = "Reset: new tempo sequence".to_string();
        } else {
            self.tap_tempo_info = format!("{} taps recorded", self.tap_times.len() + 1);
        }

        self.tap_times.push(now);
        self.last_tap_time = now;

        if self.tap_times.len() > 8 {
            self.tap_times.remove(0);
        }

        // Need at least 4 taps for accuracy
        if self.tap_times.len() >= 4 {
            let mut intervals = Vec::new();
            for i in 1..self.tap_times.len() {
                intervals.push(self.tap_times[i] - self.tap_times[i - 1]);
            }

            let avg_interval: f64 = intervals.iter().sum::<f64>() / intervals.len() as f64;

            if avg_interval > 0.1 && avg_interval < 3.0 {
                let new_bpm = (60.0 / avg_interval) as f32;
                *bpm = new_bpm.clamp(40.0, 200.0);
                self.tap_tempo_info = format!("BPM: {:.1}", bpm);
            }
        }
    }
}

impl Default for AudioWindow {
    fn default() -> Self {
        Self::new()
    }
}
