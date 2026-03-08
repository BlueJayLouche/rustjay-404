//! Video settings window - device selection, resolution, etc.

use imgui::Ui;

/// Input source type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSourceType {
    Webcam,
    #[cfg(target_os = "macos")]
    Syphon,
}

impl InputSourceType {
    pub fn name(&self) -> &'static str {
        match self {
            InputSourceType::Webcam => "Webcam",
            #[cfg(target_os = "macos")]
            InputSourceType::Syphon => "Syphon",
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
    /// Syphon server list (macOS only)
    #[cfg(target_os = "macos")]
    syphon_servers: Vec<String>,
    /// Selected Syphon server index
    #[cfg(target_os = "macos")]
    selected_syphon_server: i32,
    /// Syphon server list needs refresh
    #[cfg(target_os = "macos")]
    syphon_needs_refresh: bool,
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
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
            #[cfg(target_os = "macos")]
            selected_syphon_server: -1,
            #[cfg(target_os = "macos")]
            syphon_needs_refresh: true,
        }
    }
    
    /// Show/hide the window
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.needs_refresh = true;
        }
    }
    
    /// Check if window is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    
    /// Get the selected camera index
    pub fn selected_camera(&self) -> u32 {
        self.selected_camera as u32
    }
    
    /// Refresh camera list
    pub fn refresh_cameras(&mut self) {
        self.needs_refresh = true;
    }
    
    /// Update camera list from external source
    pub fn update_camera_list(&mut self, cameras: Vec<(u32, String)>) {
        self.cameras = cameras;
        // Ensure selection is still valid
        if self.selected_camera >= self.cameras.len() as i32 {
            self.selected_camera = 0;
        }
        self.needs_refresh = false;
    }
    
    /// Check if camera list needs refresh
    pub fn needs_refresh(&self) -> bool {
        self.needs_refresh
    }
    
    /// Get current input source type
    pub fn input_source_type(&self) -> InputSourceType {
        self.input_source_type
    }
    
    /// Set input source type
    pub fn set_input_source_type(&mut self, source_type: InputSourceType) {
        self.input_source_type = source_type;
    }
    
    /// Get selected Syphon server name (macOS only)
    #[cfg(target_os = "macos")]
    pub fn selected_syphon_server(&self) -> Option<&str> {
        if self.selected_syphon_server >= 0 && 
           (self.selected_syphon_server as usize) < self.syphon_servers.len() {
            Some(&self.syphon_servers[self.selected_syphon_server as usize])
        } else {
            None
        }
    }
    
    /// Update Syphon server list (macOS only)
    #[cfg(target_os = "macos")]
    pub fn update_syphon_servers(&mut self, servers: Vec<String>) {
        self.syphon_servers = servers;
        if self.selected_syphon_server >= self.syphon_servers.len() as i32 {
            self.selected_syphon_server = if self.syphon_servers.is_empty() { -1 } else { 0 };
        }
        self.syphon_needs_refresh = false;
    }
    
    /// Check if Syphon server list needs refresh (macOS only)
    #[cfg(target_os = "macos")]
    pub fn syphon_needs_refresh(&self) -> bool {
        self.syphon_needs_refresh
    }
    
    /// Set initialization state
    pub fn set_initializing(&mut self, initializing: bool) {
        self.initializing = initializing;
    }
    
    /// Check if initializing
    pub fn is_initializing(&self) -> bool {
        self.initializing
    }
    
    /// Set error message
    pub fn set_error(&mut self, error: Option<String>) {
        self.last_error = error;
    }
    
    /// Clear error
    pub fn clear_error(&mut self) {
        self.last_error = None;
    }
    
    /// Draw the settings window
    pub fn draw(&mut self, ui: &Ui) {
        if !self.visible {
            return;
        }
        
        // Use a local variable for the opened flag to avoid borrow issues
        let mut opened = self.visible;
        
        ui.window("Video Settings")
            .size([400.0, 300.0], imgui::Condition::FirstUseEver)
            .position([50.0, 400.0], imgui::Condition::FirstUseEver)
            .opened(&mut opened)
            .build(|| {
                // Input source type selector
                ui.text("Input Source Type");
                ui.separator();
                
                let _source_types = [InputSourceType::Webcam];
                let mut type_idx = match self.input_source_type {
                    InputSourceType::Webcam => 0,
                    #[cfg(target_os = "macos")]
                    InputSourceType::Syphon => 1,
                };
                
                // Build list of available source types
                let source_names: Vec<&str> = [
                    Some(InputSourceType::Webcam.name()),
                    #[cfg(target_os = "macos")]
                    Some(InputSourceType::Syphon.name()),
                ].into_iter().flatten().collect();
                
                if !source_names.is_empty() {
                    ui.set_next_item_width(200.0);
                    if ui.combo_simple_string("Source Type", &mut type_idx, &source_names) {
                        self.input_source_type = match type_idx {
                            0 => InputSourceType::Webcam,
                            #[cfg(target_os = "macos")]
                            1 => InputSourceType::Syphon,
                            _ => InputSourceType::Webcam,
                        };
                    }
                }
                
                ui.spacing();
                
                // Show appropriate controls based on source type
                // Note: draw_*_controls methods are called via a helper to avoid borrow issues
                let input_type = self.input_source_type;
                match input_type {
                    InputSourceType::Webcam => {
                        Self::draw_webcam_controls_helper(&mut self.cameras, &mut self.selected_camera, 
                            &mut self.needs_refresh, self.initializing, &mut self.last_error, ui);
                    }
                    #[cfg(target_os = "macos")]
                    InputSourceType::Syphon => {
                        Self::draw_syphon_controls_helper(&mut self.syphon_servers, &mut self.selected_syphon_server, 
                            &mut self.syphon_needs_refresh, self.initializing, &mut self.last_error, ui);
                    }
                }
            });
        
        self.visible = opened;
    }
    
    fn draw_webcam_controls_helper(
        cameras: &mut Vec<(u32, String)>,
        selected_camera: &mut i32,
        needs_refresh: &mut bool,
        initializing: bool,
        last_error: &mut Option<String>,
        ui: &Ui
    ) {
        ui.text("Camera Device");
        ui.separator();
        
        // Refresh button
        if ui.button("Refresh Devices") {
            *needs_refresh = true;
        }
        
        ui.same_line();
        
        // Show device count
        if cameras.is_empty() {
            ui.text_disabled("(No devices found)");
        } else {
            ui.text(format!("({} devices)", cameras.len()));
        }
        
        ui.spacing();
        
        // Show initialization status
        if initializing {
            ui.text_colored([1.0, 0.8, 0.0, 1.0], "Initializing device...");
        }
        
        // Show error if any
        if let Some(ref error) = last_error {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], "Error:");
            ui.text_wrapped(error);
            if ui.button("Clear Error") {
                *last_error = None;
            }
            ui.spacing();
        }
        
        // Camera dropdown
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
        } else {
            ui.text_disabled("No cameras available");
            if *needs_refresh {
                ui.text("(Click Refresh to scan for devices)");
            }
        }
        
        ui.spacing();
        ui.separator();
        
        // Current selection info
        if let Some((id, name)) = cameras.get(*selected_camera as usize) {
            ui.text("Selected:");
            ui.text(format!("  Index: {}", id));
            ui.text(format!("  Name: {}", name));
        }
    }
    
    #[cfg(target_os = "macos")]
    fn draw_syphon_controls_helper(
        syphon_servers: &mut Vec<String>,
        selected_syphon_server: &mut i32,
        syphon_needs_refresh: &mut bool,
        initializing: bool,
        last_error: &mut Option<String>,
        ui: &Ui
    ) {
        ui.text("Syphon Server");
        ui.separator();
        
        // Refresh button
        if ui.button("Refresh Servers") {
            *syphon_needs_refresh = true;
        }
        
        ui.same_line();
        
        // Show server count
        if syphon_servers.is_empty() {
            ui.text_disabled("(No servers found)");
        } else {
            ui.text(format!("({} servers)", syphon_servers.len()));
        }
        
        ui.spacing();
        
        // Show initialization status
        if initializing {
            ui.text_colored([1.0, 0.8, 0.0, 1.0], "Connecting...");
        }
        
        // Show error if any
        if let Some(ref error) = last_error {
            ui.text_colored([1.0, 0.0, 0.0, 1.0], "Error:");
            ui.text_wrapped(error);
            if ui.button("Clear Error") {
                *last_error = None;
            }
            ui.spacing();
        }
        
        // Server dropdown
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
        
        // Current selection info
        if *selected_syphon_server >= 0 && 
           (*selected_syphon_server as usize) < syphon_servers.len() {
            ui.text("Selected:");
            ui.text(format!("  Server: {}", syphon_servers[*selected_syphon_server as usize]));
        }
    }
}

impl Default for VideoSettingsWindow {
    fn default() -> Self {
        Self::new()
    }
}
