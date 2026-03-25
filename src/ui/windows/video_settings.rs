//! Video settings window - device selection, resolution, output controls.

use imgui::Ui;

/// Input source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSourceType {
    Webcam,
    #[cfg(target_os = "macos")]
    Syphon,
    #[cfg(feature = "ndi")]
    Ndi,
}

impl InputSourceType {
    pub fn name(&self) -> &'static str {
        match self {
            InputSourceType::Webcam => "Webcam",
            #[cfg(target_os = "macos")]
            InputSourceType::Syphon => "Syphon",
            #[cfg(feature = "ndi")]
            InputSourceType::Ndi => "NDI",
        }
    }
}

/// Video settings window
pub struct VideoSettingsWindow {
    /// List of available cameras
    cameras: Vec<(u32, String)>,
    /// Currently selected camera index
    selected_camera: i32,
    /// Camera list needs refresh
    needs_refresh: bool,
    /// Is window visible
    visible: bool,
    /// Last error message
    last_error: Option<String>,
    /// Device is currently being initialized
    initializing: bool,
    /// Current input source type
    input_source_type: InputSourceType,
    /// User clicked "Start Device" — app should start the selected webcam
    start_webcam_requested: bool,
    /// Syphon server list (macOS only)
    #[cfg(target_os = "macos")]
    syphon_servers: Vec<String>,
    /// Selected Syphon server index
    #[cfg(target_os = "macos")]
    selected_syphon_server: i32,
    /// Syphon server list needs refresh
    #[cfg(target_os = "macos")]
    syphon_needs_refresh: bool,
    /// User clicked "Start Syphon" — app should connect the selected server
    #[cfg(target_os = "macos")]
    start_syphon_requested: bool,
    /// NDI source list
    #[cfg(feature = "ndi")]
    ndi_sources: Vec<String>,
    /// Selected NDI source index
    #[cfg(feature = "ndi")]
    selected_ndi_source: i32,
    /// NDI source list needs refresh
    #[cfg(feature = "ndi")]
    ndi_needs_refresh: bool,
    /// User clicked "Start NDI" — app should connect the selected source
    #[cfg(feature = "ndi")]
    start_ndi_requested: bool,

    // --- Output controls ---

    /// Syphon output server name
    #[cfg(target_os = "macos")]
    syphon_output_name: String,
    /// User requested Syphon output start
    #[cfg(target_os = "macos")]
    start_syphon_output_requested: bool,
    /// User requested Syphon output stop
    #[cfg(target_os = "macos")]
    stop_syphon_output_requested: bool,
    /// Whether Syphon output is currently active (set by app)
    #[cfg(target_os = "macos")]
    syphon_output_active: bool,

    /// NDI output stream name
    #[cfg(feature = "ndi")]
    ndi_output_name: String,
    /// User requested NDI output start
    #[cfg(feature = "ndi")]
    start_ndi_output_requested: bool,
    /// User requested NDI output stop
    #[cfg(feature = "ndi")]
    stop_ndi_output_requested: bool,
    /// Whether NDI output is currently active (set by app)
    #[cfg(feature = "ndi")]
    ndi_output_active: bool,
}

impl VideoSettingsWindow {
    pub fn new() -> Self {
        Self {
            cameras: Vec::new(),
            selected_camera: 0,
            needs_refresh: true,
            visible: false,
            last_error: None,
            initializing: false,
            input_source_type: InputSourceType::Webcam,
            start_webcam_requested: false,
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
            #[cfg(target_os = "macos")]
            selected_syphon_server: -1,
            #[cfg(target_os = "macos")]
            syphon_needs_refresh: true,
            #[cfg(target_os = "macos")]
            start_syphon_requested: false,
            #[cfg(feature = "ndi")]
            ndi_sources: Vec::new(),
            #[cfg(feature = "ndi")]
            selected_ndi_source: -1,
            #[cfg(feature = "ndi")]
            ndi_needs_refresh: true,
            #[cfg(feature = "ndi")]
            start_ndi_requested: false,
            // Output controls
            #[cfg(target_os = "macos")]
            syphon_output_name: "Rusty-404".to_string(),
            #[cfg(target_os = "macos")]
            start_syphon_output_requested: false,
            #[cfg(target_os = "macos")]
            stop_syphon_output_requested: false,
            #[cfg(target_os = "macos")]
            syphon_output_active: false,
            #[cfg(feature = "ndi")]
            ndi_output_name: "Rusty-404".to_string(),
            #[cfg(feature = "ndi")]
            start_ndi_output_requested: false,
            #[cfg(feature = "ndi")]
            stop_ndi_output_requested: false,
            #[cfg(feature = "ndi")]
            ndi_output_active: false,
        }
    }

    /// Show/hide the window
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.needs_refresh = true;
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn selected_camera(&self) -> u32 {
        self.selected_camera as u32
    }

    pub fn refresh_cameras(&mut self) {
        self.needs_refresh = true;
    }

    pub fn update_camera_list(&mut self, cameras: Vec<(u32, String)>) {
        self.cameras = cameras;
        if self.selected_camera >= self.cameras.len() as i32 {
            self.selected_camera = 0;
        }
        self.needs_refresh = false;
    }

    pub fn needs_refresh(&self) -> bool {
        self.needs_refresh
    }

    pub fn input_source_type(&self) -> InputSourceType {
        self.input_source_type
    }

    pub fn set_input_source_type(&mut self, source_type: InputSourceType) {
        self.input_source_type = source_type;
    }

    pub fn take_start_webcam_requested(&mut self) -> bool {
        let v = self.start_webcam_requested;
        self.start_webcam_requested = false;
        v
    }

    #[cfg(target_os = "macos")]
    pub fn take_start_syphon_requested(&mut self) -> bool {
        let v = self.start_syphon_requested;
        self.start_syphon_requested = false;
        v
    }

    #[cfg(target_os = "macos")]
    pub fn selected_syphon_server(&self) -> Option<&str> {
        if self.selected_syphon_server >= 0 &&
           (self.selected_syphon_server as usize) < self.syphon_servers.len() {
            Some(&self.syphon_servers[self.selected_syphon_server as usize])
        } else {
            None
        }
    }

    #[cfg(target_os = "macos")]
    pub fn update_syphon_servers(&mut self, servers: Vec<String>) {
        self.syphon_servers = servers;
        if self.selected_syphon_server >= self.syphon_servers.len() as i32 {
            self.selected_syphon_server = if self.syphon_servers.is_empty() { -1 } else { 0 };
        }
        self.syphon_needs_refresh = false;
    }

    #[cfg(target_os = "macos")]
    pub fn syphon_needs_refresh(&self) -> bool {
        self.syphon_needs_refresh
    }

    // --- NDI input accessors ---

    #[cfg(feature = "ndi")]
    pub fn take_start_ndi_requested(&mut self) -> bool {
        let v = self.start_ndi_requested;
        self.start_ndi_requested = false;
        v
    }

    #[cfg(feature = "ndi")]
    pub fn selected_ndi_source(&self) -> Option<&str> {
        if self.selected_ndi_source >= 0 &&
           (self.selected_ndi_source as usize) < self.ndi_sources.len() {
            Some(&self.ndi_sources[self.selected_ndi_source as usize])
        } else {
            None
        }
    }

    #[cfg(feature = "ndi")]
    pub fn update_ndi_sources(&mut self, sources: Vec<String>) {
        self.ndi_sources = sources;
        if self.selected_ndi_source >= self.ndi_sources.len() as i32 {
            self.selected_ndi_source = if self.ndi_sources.is_empty() { -1 } else { 0 };
        }
        self.ndi_needs_refresh = false;
    }

    #[cfg(feature = "ndi")]
    pub fn ndi_needs_refresh(&self) -> bool {
        self.ndi_needs_refresh
    }

    // --- Output control accessors ---

    #[cfg(target_os = "macos")]
    pub fn take_start_syphon_output_requested(&mut self) -> bool {
        let v = self.start_syphon_output_requested;
        self.start_syphon_output_requested = false;
        v
    }

    #[cfg(target_os = "macos")]
    pub fn take_stop_syphon_output_requested(&mut self) -> bool {
        let v = self.stop_syphon_output_requested;
        self.stop_syphon_output_requested = false;
        v
    }

    #[cfg(target_os = "macos")]
    pub fn set_syphon_output_active(&mut self, active: bool) {
        self.syphon_output_active = active;
    }

    #[cfg(target_os = "macos")]
    pub fn syphon_output_name(&self) -> &str {
        &self.syphon_output_name
    }

    #[cfg(feature = "ndi")]
    pub fn ndi_output_name(&self) -> &str {
        &self.ndi_output_name
    }

    #[cfg(feature = "ndi")]
    pub fn take_start_ndi_output_requested(&mut self) -> bool {
        let v = self.start_ndi_output_requested;
        self.start_ndi_output_requested = false;
        v
    }

    #[cfg(feature = "ndi")]
    pub fn take_stop_ndi_output_requested(&mut self) -> bool {
        let v = self.stop_ndi_output_requested;
        self.stop_ndi_output_requested = false;
        v
    }

    #[cfg(feature = "ndi")]
    pub fn set_ndi_output_active(&mut self, active: bool) {
        self.ndi_output_active = active;
    }

    pub fn set_initializing(&mut self, initializing: bool) {
        self.initializing = initializing;
    }

    pub fn is_initializing(&self) -> bool {
        self.initializing
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.last_error = error;
    }

    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    /// Draw the settings window
    pub fn draw(&mut self, ui: &Ui) {
        if !self.visible {
            return;
        }

        let mut opened = self.visible;

        ui.window("Video Settings")
            .size([400.0, 450.0], imgui::Condition::FirstUseEver)
            .position([50.0, 400.0], imgui::Condition::FirstUseEver)
            .opened(&mut opened)
            .build(|| {
                // ========== INPUT SECTION ==========
                ui.text("Input Source");
                ui.separator();

                let mut type_idx = match self.input_source_type {
                    InputSourceType::Webcam => 0,
                    #[cfg(target_os = "macos")]
                    InputSourceType::Syphon => 1,
                    #[cfg(feature = "ndi")]
                    InputSourceType::Ndi => {
                        #[cfg(target_os = "macos")]
                        { 2 }
                        #[cfg(not(target_os = "macos"))]
                        { 1 }
                    }
                };

                let source_names: Vec<&str> = [
                    Some(InputSourceType::Webcam.name()),
                    #[cfg(target_os = "macos")]
                    Some(InputSourceType::Syphon.name()),
                    #[cfg(feature = "ndi")]
                    Some(InputSourceType::Ndi.name()),
                ].into_iter().flatten().collect();

                if !source_names.is_empty() {
                    ui.set_next_item_width(200.0);
                    if ui.combo_simple_string("Source Type", &mut type_idx, &source_names) {
                        self.input_source_type = match type_idx {
                            0 => InputSourceType::Webcam,
                            #[cfg(target_os = "macos")]
                            1 => InputSourceType::Syphon,
                            #[cfg(all(target_os = "macos", feature = "ndi"))]
                            2 => InputSourceType::Ndi,
                            #[cfg(all(not(target_os = "macos"), feature = "ndi"))]
                            1 => InputSourceType::Ndi,
                            _ => InputSourceType::Webcam,
                        };
                    }
                }

                ui.spacing();

                let input_type = self.input_source_type;
                match input_type {
                    InputSourceType::Webcam => {
                        if Self::draw_webcam_controls_helper(&mut self.cameras, &mut self.selected_camera,
                            &mut self.needs_refresh, self.initializing, &mut self.last_error, ui) {
                            self.start_webcam_requested = true;
                        }
                    }
                    #[cfg(target_os = "macos")]
                    InputSourceType::Syphon => {
                        if Self::draw_syphon_controls_helper(&mut self.syphon_servers, &mut self.selected_syphon_server,
                            &mut self.syphon_needs_refresh, self.initializing, &mut self.last_error, ui) {
                            self.start_syphon_requested = true;
                        }
                    }
                    #[cfg(feature = "ndi")]
                    InputSourceType::Ndi => {
                        if Self::draw_ndi_controls_helper(&mut self.ndi_sources, &mut self.selected_ndi_source,
                            &mut self.ndi_needs_refresh, self.initializing, &mut self.last_error, ui) {
                            self.start_ndi_requested = true;
                        }
                    }
                }

                ui.spacing();
                ui.spacing();

                // ========== OUTPUT SECTION ==========
                ui.text("Output Streams");
                ui.separator();
                ui.spacing();

                // Syphon output (macOS)
                #[cfg(target_os = "macos")]
                {
                    let syphon_active = self.syphon_output_active;
                    ui.set_next_item_width(200.0);
                    ui.input_text("Syphon Name", &mut self.syphon_output_name).build();
                    if syphon_active {
                        ui.text_colored([0.0, 1.0, 0.0, 1.0], "Syphon: Active");
                        ui.same_line();
                        if ui.button("Stop Syphon Output") {
                            self.stop_syphon_output_requested = true;
                        }
                    } else {
                        ui.text_disabled("Syphon: Inactive");
                        ui.same_line();
                        if ui.button("Start Syphon Output") {
                            self.start_syphon_output_requested = true;
                        }
                    }
                    ui.spacing();
                }

                // NDI output
                #[cfg(feature = "ndi")]
                {
                    let ndi_active = self.ndi_output_active;
                    ui.set_next_item_width(200.0);
                    ui.input_text("NDI Name", &mut self.ndi_output_name).build();
                    if ndi_active {
                        ui.text_colored([0.0, 1.0, 0.0, 1.0], "NDI: Active");
                        ui.same_line();
                        if ui.button("Stop NDI Output") {
                            self.stop_ndi_output_requested = true;
                        }
                    } else {
                        ui.text_disabled("NDI: Inactive");
                        ui.same_line();
                        if ui.button("Start NDI Output") {
                            self.start_ndi_output_requested = true;
                        }
                    }
                }
            });

        self.visible = opened;
    }

    /// Returns true if the user clicked "Start Device".
    fn draw_webcam_controls_helper(
        cameras: &mut Vec<(u32, String)>,
        selected_camera: &mut i32,
        needs_refresh: &mut bool,
        initializing: bool,
        last_error: &mut Option<String>,
        ui: &Ui
    ) -> bool {
        let mut start_requested = false;

        ui.text("Camera Device");
        ui.separator();

        if ui.button("Refresh Devices") {
            *needs_refresh = true;
        }

        ui.same_line();

        if cameras.is_empty() {
            ui.text_disabled("(No devices found)");
        } else {
            ui.text(format!("({} devices)", cameras.len()));
        }

        ui.spacing();

        if initializing {
            ui.text_colored([1.0, 0.8, 0.0, 1.0], "Initializing device...");
        }

        if let Some(ref error) = last_error {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], "Error:");
            ui.text_wrapped(error);
            if ui.button("Clear Error") {
                *last_error = None;
            }
            ui.spacing();
        }

        if !cameras.is_empty() {
            let preview = if initializing {
                "Initializing...".to_string()
            } else if *selected_camera < cameras.len() as i32 {
                cameras[*selected_camera as usize].1.clone()
            } else {
                "Select camera...".to_string()
            };

            ui.set_next_item_width(300.0);
            if let Some(_combo) = ui.begin_combo("Device", &preview) {
                for (_idx, (id, name)) in cameras.iter().enumerate() {
                    let is_selected = *selected_camera == *id as i32;
                    if ui.selectable_config(name)
                        .selected(is_selected)
                        .build() {
                        *selected_camera = *id as i32;
                    }
                }
            }

            ui.spacing();
            if !initializing && ui.button("Start Device") {
                start_requested = true;
            }
        } else {
            ui.text_disabled("No cameras available");
            if *needs_refresh {
                ui.text("(Click Refresh to scan for devices)");
            }
        }

        ui.spacing();
        ui.separator();

        if let Some((id, name)) = cameras.get(*selected_camera as usize) {
            ui.text("Selected:");
            ui.text(format!("  Index: {}", id));
            ui.text(format!("  Name: {}", name));
        }

        start_requested
    }

    /// Returns true if the user clicked "Start Syphon".
    #[cfg(target_os = "macos")]
    fn draw_syphon_controls_helper(
        syphon_servers: &mut Vec<String>,
        selected_syphon_server: &mut i32,
        syphon_needs_refresh: &mut bool,
        initializing: bool,
        last_error: &mut Option<String>,
        ui: &Ui
    ) -> bool {
        let mut start_requested = false;

        ui.text("Syphon Server");
        ui.separator();

        if ui.button("Refresh Servers") {
            *syphon_needs_refresh = true;
        }

        ui.same_line();

        if syphon_servers.is_empty() {
            ui.text_disabled("(No servers found)");
        } else {
            ui.text(format!("({} servers)", syphon_servers.len()));
        }

        ui.spacing();

        if initializing {
            ui.text_colored([1.0, 0.8, 0.0, 1.0], "Connecting...");
        }

        if let Some(ref error) = last_error {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], "Error:");
            ui.text_wrapped(error);
            if ui.button("Clear Error") {
                *last_error = None;
            }
            ui.spacing();
        }

        if !syphon_servers.is_empty() {
            let preview = if initializing {
                "Connecting...".to_string()
            } else if *selected_syphon_server >= 0 &&
                      (*selected_syphon_server as usize) < syphon_servers.len() {
                syphon_servers[*selected_syphon_server as usize].clone()
            } else {
                "Select server...".to_string()
            };

            ui.set_next_item_width(300.0);
            if let Some(_combo) = ui.begin_combo("Server", &preview) {
                for (idx, name) in syphon_servers.iter().enumerate() {
                    let is_selected = *selected_syphon_server == idx as i32;
                    if ui.selectable_config(name)
                        .selected(is_selected)
                        .build() {
                        *selected_syphon_server = idx as i32;
                    }
                }
            }

            ui.spacing();
            if !initializing
                && *selected_syphon_server >= 0
                && (*selected_syphon_server as usize) < syphon_servers.len()
                && ui.button("Start Syphon")
            {
                start_requested = true;
            }
        } else {
            ui.text_disabled("No Syphon servers available");
            if *syphon_needs_refresh {
                ui.text("(Click Refresh to scan for servers)");
            } else {
                ui.text("Make sure a Syphon server is running (e.g., Resolume, Simple Server)");
            }
        }

        ui.spacing();
        ui.separator();

        if *selected_syphon_server >= 0 &&
           (*selected_syphon_server as usize) < syphon_servers.len() {
            ui.text("Selected:");
            ui.text(format!("  Server: {}", syphon_servers[*selected_syphon_server as usize]));
        }

        start_requested
    }

    /// Returns true if the user clicked "Start NDI".
    #[cfg(feature = "ndi")]
    fn draw_ndi_controls_helper(
        ndi_sources: &mut Vec<String>,
        selected_ndi_source: &mut i32,
        ndi_needs_refresh: &mut bool,
        initializing: bool,
        last_error: &mut Option<String>,
        ui: &Ui
    ) -> bool {
        let mut start_requested = false;

        ui.text("NDI Source");
        ui.separator();

        if ui.button("Refresh Sources") {
            *ndi_needs_refresh = true;
        }

        ui.same_line();

        if ndi_sources.is_empty() {
            ui.text_disabled("(No sources found)");
        } else {
            ui.text(format!("({} sources)", ndi_sources.len()));
        }

        ui.spacing();

        if initializing {
            ui.text_colored([1.0, 0.8, 0.0, 1.0], "Connecting...");
        }

        if let Some(ref error) = last_error {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], "Error:");
            ui.text_wrapped(error);
            if ui.button("Clear Error") {
                *last_error = None;
            }
            ui.spacing();
        }

        if !ndi_sources.is_empty() {
            let preview = if initializing {
                "Connecting...".to_string()
            } else if *selected_ndi_source >= 0 &&
                      (*selected_ndi_source as usize) < ndi_sources.len() {
                ndi_sources[*selected_ndi_source as usize].clone()
            } else {
                "Select source...".to_string()
            };

            ui.set_next_item_width(300.0);
            if let Some(_combo) = ui.begin_combo("Source", &preview) {
                for (idx, name) in ndi_sources.iter().enumerate() {
                    let is_selected = *selected_ndi_source == idx as i32;
                    if ui.selectable_config(name)
                        .selected(is_selected)
                        .build() {
                        *selected_ndi_source = idx as i32;
                    }
                }
            }

            ui.spacing();
            if !initializing
                && *selected_ndi_source >= 0
                && (*selected_ndi_source as usize) < ndi_sources.len()
                && ui.button("Start NDI")
            {
                start_requested = true;
            }
        } else {
            ui.text_disabled("No NDI sources available");
            if *ndi_needs_refresh {
                ui.text("(Click Refresh to scan for sources)");
            } else {
                ui.text("Make sure an NDI source is running on the network");
            }
        }

        ui.spacing();
        ui.separator();

        if *selected_ndi_source >= 0 &&
           (*selected_ndi_source as usize) < ndi_sources.len() {
            ui.text("Selected:");
            ui.text(format!("  Source: {}", ndi_sources[*selected_ndi_source as usize]));
        }

        start_requested
    }
}

impl Default for VideoSettingsWindow {
    fn default() -> Self {
        Self::new()
    }
}
