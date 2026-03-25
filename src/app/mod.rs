pub mod config;
pub mod state;

use crate::engine::mixer::{ColorSpace, MixMode, VideoMixer};
use crate::input::InputRouter;
use crate::preset::{PresetData, PresetManager};
use crate::sampler::BankManager;
use crate::sequencer::SequencerEngine;
use crate::ui::context::ImGuiContext;
use crate::ui::windows::main::MainWindow;
use crate::ui::windows::video_settings::VideoSettingsWindow;
use crate::video::recorder::LiveSampler;
use crate::video::capture::webcam::list_cameras;
#[cfg(target_os = "macos")]
use crate::video::interapp::InterAppVideo;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use self::config::AppConfig;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

fn debug_log(msg: &str) {
    use std::sync::OnceLock;
    static DEBUG_LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
    
    let mutex = DEBUG_LOG.get_or_init(|| {
        Mutex::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/rustjay404_debug.log")
                .unwrap()
        )
    });
    
    if let Ok(mut file) = mutex.lock() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let _ = writeln!(file, "[{:.3}] {}", timestamp, msg);
        let _ = file.flush();
    }
}

/// Control window rendering context (shares device with output)
struct ControlContext {
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    imgui: ImGuiContext,
}

/// Main application state managing dual windows
pub struct App {
    // Configuration
    config: AppConfig,
    
    // Shared state
    rt: Arc<tokio::runtime::Runtime>,
    bank_manager: BankManager,
    sequencer: SequencerEngine,
    last_update: Instant,
    
    // Shared wgpu resources (from output window)
    wgpu_instance: Option<wgpu::Instance>,
    wgpu_device: Option<Arc<wgpu::Device>>,
    wgpu_queue: Option<Arc<wgpu::Queue>>,
    
    // Output window (video display)
    output_window: Option<Arc<Window>>,
    output_surface: Option<wgpu::Surface<'static>>,
    output_surface_config: Option<wgpu::SurfaceConfiguration>,
    output_mixer: Option<VideoMixer>,
    output_fullscreen: bool,
    
    // Inter-app video output (Syphon/Spout)
    #[cfg(target_os = "macos")]
    syphon_output: Option<crate::video::interapp::SyphonOutput>,
    
    // Control window (ImGui UI)
    control_window: Option<Arc<Window>>,
    control_context: Option<ControlContext>,
    main_window: MainWindow,
    
    // Modifier keys
    shift_pressed: bool,
    
    // Live sampler for recording
    live_sampler: Option<LiveSampler>,
    recording_pad: Option<usize>, // Which pad is currently recording
    
    // UI command receiver
    ui_command_receiver: Option<flume::Receiver<crate::ui::windows::main::UICommand>>,
    
    // Input router (MIDI/OSC)
    input_router: InputRouter,
    
    // Preset manager
    preset_manager: PresetManager,
    
    // Video settings window
    video_settings: VideoSettingsWindow,
    // Current video device index
    video_device_index: u32,
    // Pending Syphon server refresh (macOS only) - deferred to avoid event handler issues
    #[cfg(target_os = "macos")]
    syphon_refresh_pending: bool,
    // Channel for receiving Syphon discovery results from background thread
    #[cfg(target_os = "macos")]
    syphon_discovery_receiver: Option<flume::Receiver<Vec<String>>>,
}

impl App {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> anyhow::Result<Self> {
        log::info!("Initializing Rusty-404...");
        
        // Load configuration
        let config = AppConfig::load_or_default();
        log::info!("Configuration loaded: {:?}", config);

        let bank_manager = BankManager::new();
        let sequencer = SequencerEngine::new();
        let input_router = InputRouter::new();
        let preset_manager = PresetManager::new();

        Ok(Self {
            config,
            rt,
            bank_manager,
            sequencer,
            last_update: Instant::now(),
            wgpu_instance: None,
            wgpu_device: None,
            wgpu_queue: None,
            output_window: None,
            output_surface: None,
            output_surface_config: None,
            output_mixer: None,
            output_fullscreen: false,
            #[cfg(target_os = "macos")]
            syphon_output: None,
            control_window: None,
            control_context: None,
            main_window: MainWindow::new(),
            shift_pressed: false,
            live_sampler: None,
            recording_pad: None,
            ui_command_receiver: None,
            input_router,
            preset_manager,
            video_settings: VideoSettingsWindow::new(),
            video_device_index: 0,
            #[cfg(target_os = "macos")]
            syphon_refresh_pending: false,
            #[cfg(target_os = "macos")]
            syphon_discovery_receiver: None,
        })
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);
        
        event_loop.run_app(&mut self)?;
        Ok(())
    }
    
    /// Create output window (video display)
    fn create_output_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.output_window.is_some() {
            return;
        }
        
        let window_attrs = winit::window::WindowAttributes::default()
            .with_title(&self.config.output_window.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.output_window.width,
                self.config.output_window.height,
            ))
            .with_resizable(self.config.output_window.resizable)
            .with_decorations(self.config.output_window.decorated);
        
        match event_loop.create_window(window_attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                
                // Set window position if specified
                if let (Some(x), Some(y)) = (self.config.output_window.x, self.config.output_window.y) {
                    let _ = window.request_inner_size(winit::dpi::LogicalSize::new(
                        self.config.output_window.width,
                        self.config.output_window.height,
                    ));
                    window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
                }
                
                // Set initial cursor visibility
                window.set_cursor_visible(self.config.output_window.cursor_visible);
                
                log::info!("Output window created: {}x{} @ {:?}", 
                    self.config.output_window.width,
                    self.config.output_window.height,
                    window.outer_position()
                );
                
                self.output_window = Some(window);
            }
            Err(e) => {
                log::error!("Failed to create output window: {}", e);
            }
        }
    }
    
    /// Create control window (ImGui UI)
    fn create_control_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.control_window.is_some() {
            return;
        }
        
        let window_attrs = winit::window::WindowAttributes::default()
            .with_title(&self.config.control_window.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.control_window.width,
                self.config.control_window.height,
            ))
            .with_resizable(self.config.control_window.resizable)
            .with_decorations(self.config.control_window.decorated);
        
        match event_loop.create_window(window_attrs) {
            Ok(window) => {
                let window = Arc::new(window);
                
                // Set window position if specified
                if let (Some(x), Some(y)) = (self.config.control_window.x, self.config.control_window.y) {
                    window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
                }
                
                // Ensure cursor is visible in control window
                window.set_cursor_visible(true);
                
                log::info!("Control window created: {}x{} @ {:?}",
                    self.config.control_window.width,
                    self.config.control_window.height,
                    window.outer_position()
                );
                
                // Store control window reference for file dialogs (fixes macOS focus)
                self.main_window.set_control_window(window.clone());
                
                self.control_window = Some(window);
            }
            Err(e) => {
                log::error!("Failed to create control window: {}", e);
            }
        }
    }
    
    /// Initialize wgpu for both windows (output creates device, control shares it)
    async fn init_wgpu(&mut self) -> anyhow::Result<()> {
        // Create shared wgpu instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        
        // Create output surface
        let output_window = self.output_window.as_ref().unwrap().clone();
        let output_surface = instance.create_surface(output_window.clone())?;
        
        // Get adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&output_surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| anyhow::anyhow!("Failed to find suitable adapter"))?;
        
        log::info!("Using adapter: {:?}", adapter.get_info());
        
        // Check for HAP support (BC texture compression)
        if !adapter.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
            return Err(anyhow::anyhow!(
                "GPU does not support BC texture compression. HAP playback requires TEXTURE_COMPRESSION_BC feature."
            ));
        }
        
        // Create device and queue (shared between windows)
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
                    required_limits: wgpu::Limits::default(),
                    label: Some("Shared Device"),
                    memory_hints: wgpu::MemoryHints::Performance,
                    trace: wgpu::Trace::Off,
                },
            )
            .await?;
        
        let device = Arc::new(device);
        let queue = Arc::new(queue);
        
        // Configure output surface - use actual window size (physical pixels)
        let output_caps = output_surface.get_capabilities(&adapter);
        let output_format = output_caps.formats.iter().copied()
            .find(|f| f.is_srgb())
            .unwrap_or(output_caps.formats[0]);
        
        let output_size = output_window.inner_size();
        let output_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: output_format,
            width: output_size.width,
            height: output_size.height,
            present_mode: if self.config.vsync { 
                wgpu::PresentMode::AutoVsync 
            } else { 
                wgpu::PresentMode::AutoNoVsync 
            },
            alpha_mode: output_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        output_surface.configure(&device, &output_config);
        
        // Create video mixer with actual surface size
        let mixer = VideoMixer::new(
            &device,
            &queue,
            output_size.width,
            output_size.height,
            output_format,
        );
        
        self.wgpu_instance = Some(instance);
        self.wgpu_device = Some(device.clone());
        self.wgpu_queue = Some(queue.clone());
        self.output_surface = Some(output_surface);
        self.output_surface_config = Some(output_config);
        self.output_mixer = Some(mixer);
        
        // Initialize Syphon output on macOS
        #[cfg(target_os = "macos")]
        {
            if crate::video::interapp::SyphonOutput::is_available() {
                match crate::video::interapp::SyphonOutput::new(
                    "Rusty-404",
                    &device,
                    &queue,
                    output_size.width,
                    output_size.height,
                ) {
                    Ok(syphon) => {
                        let is_zero_copy = syphon.is_zero_copy();
                        self.syphon_output = Some(syphon);
                        if is_zero_copy {
                            log::info!("Syphon output initialized with zero-copy support");
                        } else {
                            log::info!("Syphon output initialized (CPU fallback mode)");
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to initialize Syphon output: {}", e);
                        log::warn!("Ensure Syphon.framework is installed at /Library/Frameworks/");
                    }
                }
            } else {
                log::warn!("Syphon not available on this system");
            }
        }
        
        log::info!("Output wgpu initialized: {}x{} @ {:?}", 
            self.config.output_window.width,
            self.config.output_window.height,
            output_format
        );
        
        // Set up UI command channel
        let cmd_receiver = self.main_window.setup_command_channel();
        self.ui_command_receiver = Some(cmd_receiver);
        
        // Initialize live sampler
        self.init_live_sampler();
        
        // Initialize MIDI/OSC input
        self.init_input();
        
        // Now initialize control window with shared device/queue
        if self.control_window.is_some() {
            self.init_control_wgpu().await?;
        }
        
        Ok(())
    }
    
    /// Initialize control window surface (shares device/queue with output)
    async fn init_control_wgpu(&mut self) -> anyhow::Result<()> {
        let instance = self.wgpu_instance.as_ref().unwrap();
        let device = self.wgpu_device.as_ref().unwrap().clone();
        let queue = self.wgpu_queue.as_ref().unwrap().clone();
        
        let control_window = self.control_window.as_ref().unwrap().clone();
        let control_surface = instance.create_surface(control_window.clone())?;
        
        // Get adapter for surface capabilities
        let adapters = instance.enumerate_adapters(wgpu::Backends::all());
        let control_caps = if let Some(adapter) = adapters.first() {
            control_surface.get_capabilities(adapter)
        } else {
            anyhow::bail!("No adapters available for control window");
        };
        
        let control_format = control_caps.formats.iter().copied()
            .find(|f| f.is_srgb())
            .unwrap_or(control_caps.formats[0]);
        
        // Use actual window size (physical pixels) for surface
        let control_size = control_window.inner_size();
        let control_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: control_format,
            width: control_size.width,
            height: control_size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: control_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        control_surface.configure(&device, &control_config);
        
        // Initialize ImGui with shared device/queue
        let imgui = ImGuiContext::new(&device, &queue, &control_window, control_format)?;
        
        self.control_context = Some(ControlContext {
            surface: control_surface,
            surface_config: control_config,
            imgui,
        });
        
        log::info!("Control wgpu initialized: {}x{} @ {:?}",
            self.config.control_window.width,
            self.config.control_window.height,
            control_format
        );
        
        Ok(())
    }
    
    /// Toggle fullscreen for output window
    fn toggle_fullscreen(&mut self) {
        if let Some(ref window) = self.output_window {
            self.output_fullscreen = !self.output_fullscreen;
            
            let fullscreen_mode = if self.output_fullscreen {
                Some(winit::window::Fullscreen::Borderless(None))
            } else {
                None
            };
            
            window.set_fullscreen(fullscreen_mode);
            log::info!("Output fullscreen: {}", self.output_fullscreen);
        }
    }
    
    /// Handle window events for output window
    fn handle_output_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::CursorEntered { .. } => {
                // Hide cursor when entering output window
                if !self.config.output_window.cursor_visible {
                    if let Some(ref window) = self.output_window {
                        window.set_cursor_visible(false);
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                // Show cursor when leaving output window
                if !self.config.output_window.cursor_visible {
                    if let Some(ref window) = self.output_window {
                        window.set_cursor_visible(true);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Track modifier state
                match &event.logical_key {
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Shift) => {
                        self.shift_pressed = event.state == winit::event::ElementState::Pressed;
                    }
                    _ => {}
                }
                
                if event.state == winit::event::ElementState::Pressed {
                    match &event.logical_key {
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) => {
                            event_loop.exit();
                        }
                        winit::keyboard::Key::Character(ch) => {
                            let key = ch.to_lowercase();
                            
                            if self.shift_pressed && key == "f" {
                                self.toggle_fullscreen();
                            } else if self.shift_pressed && key == " " {
                                self.sequencer.toggle_playback();
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut config) = self.output_surface_config {
                    config.width = size.width;
                    config.height = size.height;
                    if let Some(ref device) = self.wgpu_device {
                        if let Some(ref surface) = self.output_surface {
                            surface.configure(device, config);
                        }
                    }
                }
                if let Some(ref mut mixer) = self.output_mixer {
                    if let Some(ref device) = self.wgpu_device {
                        mixer.resize(device, size.width, size.height);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let _ = self.render_output();
            }
            _ => {}
        }
    }
    
    /// Handle window events for control window
    fn handle_control_event(&mut self, _event_loop: &ActiveEventLoop, event: WindowEvent) {
        // Pass events to ImGui first
        if let Some(ref mut ctx) = self.control_context {
            if let Some(ref window) = self.control_window {
                ctx.imgui.handle_event::<()>(window, &winit::event::Event::WindowEvent { 
                    window_id: window.id(), 
                    event: event.clone() 
                });
            }
        }
        
        match event {
            WindowEvent::CloseRequested => {
                // Close control window but keep output running
                self.control_window = None;
                self.control_context = None;
                log::info!("Control window closed (output still running)");
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Track modifier state
                match &event.logical_key {
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Shift) => {
                        self.shift_pressed = event.state == winit::event::ElementState::Pressed;
                    }
                    _ => {}
                }
                
                if event.state == winit::event::ElementState::Pressed {
                    if let winit::keyboard::Key::Character(ch) = &event.logical_key {
                        let key = ch.to_lowercase();
                        
                        if self.shift_pressed && key == " " {
                            self.sequencer.toggle_playback();
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut ctx) = self.control_context {
                    ctx.surface_config.width = size.width;
                    ctx.surface_config.height = size.height;
                    if let Some(ref device) = self.wgpu_device {
                        ctx.surface.configure(device, &ctx.surface_config);
                    }
                    ctx.imgui.resize(size.width, size.height, 1.0);
                }
            }
            WindowEvent::RedrawRequested => {
                let _ = self.render_control();
            }
            _ => {}
        }
    }
    
    /// Initialize the live sampler (call when wgpu is ready)
    fn init_live_sampler(&mut self) {
        if self.live_sampler.is_none() {
            let mut sampler = LiveSampler::new();
            // Try to init webcam, but don't fail if no camera
            if let Err(e) = sampler.init_webcam(0) {
                log::warn!("Could not initialize webcam: {}", e);
            } else {
                log::info!("Live sampler initialized with webcam");
            }
            self.live_sampler = Some(sampler);
        }
    }
    
    /// Initialize MIDI and OSC input
    fn init_input(&mut self) {
        // Try to auto-connect MIDI
        match self.input_router.auto_connect_midi() {
            Ok(()) => {
                if let Some(port) = self.input_router.midi_status() {
                    log::info!("MIDI connected to: {}", port);
                }
            }
            Err(e) => {
                log::warn!("MIDI not available: {}", e);
            }
        }
        
        // Try to start OSC
        match self.input_router.auto_start_osc() {
            Ok(port) => {
                log::info!("OSC server started on port {}", port);
                log::info!("OSC addresses:");
                log::info!("  /rustjay404/trigger <pad>    - Trigger pad (0-15)");
                log::info!("  /rustjay404/release <pad>    - Release pad");
                log::info!("  /rustjay404/volume <pad> <vol> - Set pad volume (0.0-1.0)");
                log::info!("  /rustjay404/speed <pad> <spd>  - Set pad speed (-2.0-2.0)");
                log::info!("  /rustjay404/bpm <bpm>        - Set BPM (20-999)");
                log::info!("  /rustjay404/stop             - Stop all pads");
            }
            Err(e) => {
                log::warn!("OSC server not started: {}", e);
            }
        }
    }
    
    /// Start recording to a specific pad
    fn start_recording(&mut self, pad_index: usize) -> anyhow::Result<()> {
        if self.live_sampler.is_none() {
            self.init_live_sampler();
        }
        
        let sampler = self.live_sampler.as_mut().ok_or_else(|| anyhow::anyhow!("No sampler available"))?;
        
        if sampler.state() == crate::video::recorder::RecordingState::Recording {
            return Err(anyhow::anyhow!("Already recording"));
        }
        
        sampler.start_recording()?;
        self.recording_pad = Some(pad_index);
        
        log::info!("Started recording to pad {}", pad_index);
        Ok(())
    }
    
    /// Stop recording and assign to the pad
    fn stop_recording(&mut self) -> anyhow::Result<()> {
        let sampler = self.live_sampler.as_mut().ok_or_else(|| anyhow::anyhow!("No sampler available"))?;
        let pad_index = self.recording_pad.ok_or_else(|| anyhow::anyhow!("Not recording"))?;
        
        // Create samples/recorded folder for recordings
        let samples_dir = PathBuf::from("samples").join("recorded");
        std::fs::create_dir_all(&samples_dir)?;
        
        // Generate unique filename
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("pad{}_rec_{}.mov", pad_index, timestamp);
        let output_path = samples_dir.join(&filename);
        
        // Stop recording and save using the configured HAP format
        let hap_format = self.config.encoding.format;
        let hap_path = sampler.stop_recording(&output_path, hap_format)?;
        
        // Load the HAP file into the pad
        if let Some(device) = &self.wgpu_device {
            if let Some(queue) = &self.wgpu_queue {
                match crate::sampler::sample::VideoSample::from_hap(&hap_path, device, queue) {
                    Ok(sample) => {
                        let bank = self.bank_manager.current_bank_mut();
                        if let Some(pad) = bank.get_pad_mut(pad_index) {
                            pad.assign_sample(sample);
                            log::info!("Recording assigned to pad {}", pad_index);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to load recorded sample: {}", e);
                    }
                }
            }
        }
        
        self.recording_pad = None;
        Ok(())
    }
    
    /// Cancel current recording
    fn cancel_recording(&mut self) {
        if let Some(sampler) = self.live_sampler.as_mut() {
            sampler.cancel_recording();
        }
        self.recording_pad = None;
    }
    
    /// Update application state
    fn update(&mut self) {
        // Handle UI commands - collect first to avoid borrow issues
        let commands: Vec<_> = if let Some(ref receiver) = self.ui_command_receiver {
            receiver.try_iter().collect()
        } else {
            Vec::new()
        };
        
        for cmd in commands {
            match cmd {
                crate::ui::windows::main::UICommand::StartRecording(pad) => {
                    if let Err(e) = self.start_recording(pad) {
                        log::error!("Failed to start recording: {}", e);
                    }
                }
                crate::ui::windows::main::UICommand::StopRecording => {
                    if let Err(e) = self.stop_recording() {
                        log::error!("Failed to stop recording: {}", e);
                    }
                }
                crate::ui::windows::main::UICommand::CancelRecording => {
                    self.cancel_recording();
                }
                crate::ui::windows::main::UICommand::StartMidiLearn { control_id, min, max } => {
                    self.input_router.start_learn(&control_id, min, max);
                }
                crate::ui::windows::main::UICommand::CancelMidiLearn => {
                    self.input_router.cancel_learn();
                }
                crate::ui::windows::main::UICommand::SavePreset(name) => {
                    let data = PresetData::from_app(&self.bank_manager, &self.sequencer, &name);
                    match self.preset_manager.save_preset(&name, &data) {
                        Ok(path) => log::info!("Saved preset '{}' to {:?}", name, path),
                        Err(e) => log::error!("Failed to save preset '{}': {}", name, e),
                    }
                }
                crate::ui::windows::main::UICommand::LoadPreset(name) => {
                    match self.preset_manager.load_preset(&name) {
                        Ok(data) => {
                            // Get list of samples to load with their in/out points
                            let samples_with_points: Vec<(usize, String, u32, u32)> = data.pads.iter()
                                .filter_map(|pad| {
                                    pad.sample_path.as_ref().map(|path| {
                                        (pad.index, path.clone(), pad.in_point, pad.out_point)
                                    })
                                })
                                .collect();
                            
                            // Apply preset settings (without samples)
                            if let Err(e) = data.apply_to_app(&mut self.bank_manager, &mut self.sequencer) {
                                log::error!("Failed to apply preset '{}': {}", name, e);
                            }
                            
                            // Load samples that exist and apply in/out points
                            if let (Some(device), Some(queue)) = (&self.wgpu_device, &self.wgpu_queue) {
                                for (pad_index, sample_path, in_point, out_point) in samples_with_points {
                                    let path = PathBuf::from(&sample_path);
                                    if path.exists() {
                                        match crate::sampler::sample::VideoSample::from_hap(&path, device, queue) {
                                            Ok(mut sample) => {
                                                // Apply in/out points from preset
                                                sample.in_point = in_point.min(sample.frame_count.saturating_sub(1));
                                                sample.out_point = out_point.min(sample.frame_count);
                                                if sample.out_point <= sample.in_point {
                                                    sample.out_point = sample.frame_count;
                                                }
                                                log::info!("Applied in={} out={} for pad {}", sample.in_point, sample.out_point, pad_index);
                                                
                                                let bank = self.bank_manager.current_bank_mut();
                                                if let Some(pad) = bank.get_pad_mut(pad_index) {
                                                    pad.assign_sample(sample);
                                                    log::info!("Loaded sample for pad {}: {:?}", pad_index, path);
                                                }
                                            }
                                            Err(e) => {
                                                log::warn!("Failed to load sample for pad {}: {}", pad_index, e);
                                            }
                                        }
                                    } else {
                                        log::warn!("Sample not found for pad {}: {:?}", pad_index, path);
                                    }
                                }
                            }
                        }
                        Err(e) => log::error!("Failed to load preset '{}': {}", name, e),
                    }
                }
                crate::ui::windows::main::UICommand::DeletePreset(index) => {
                    if let Err(e) = self.preset_manager.delete_preset(index) {
                        log::error!("Failed to delete preset at index {}: {}", index, e);
                    }
                }
                crate::ui::windows::main::UICommand::RefreshVideoDevices => {
                    self.refresh_video_devices();
                }
                crate::ui::windows::main::UICommand::SelectVideoDevice(index) => {
                    if index != self.video_device_index {
                        self.switch_video_device(index);
                    }
                }
                crate::ui::windows::main::UICommand::RefreshSyphonServers => {
                    self.refresh_syphon_servers();
                }
                crate::ui::windows::main::UICommand::SelectSyphonServer(server_name) => {
                    self.switch_syphon_server(&server_name);
                }
            }
        }
        
        // Update UI with preset list
        let preset_names = self.preset_manager.get_preset_names();
        let bank_name = self.preset_manager.get_current_bank().to_string();
        self.main_window.set_preset_list(preset_names, bank_name);
        
        // Update UI with MIDI learn state
        let learn_target = self.input_router.learn_target().map(|s| s.to_string());
        let learn_flash = self.input_router.learn_flash();
        self.main_window.set_midi_learn_state(learn_target, learn_flash);
        
        // Update UI with recording state
        let is_recording = self.live_sampler.as_ref()
            .map(|s| s.state() == crate::video::recorder::RecordingState::Recording)
            .unwrap_or(false);
        self.main_window.set_recording_state(is_recording, self.recording_pad);
        
        let now = Instant::now();
        let dt = now - self.last_update;
        self.last_update = now;
        
        // Update live sampler (capture frames if recording)
        if let Some(sampler) = self.live_sampler.as_mut() {
            sampler.update();
        }
        
        // Update bank (pad playback)
        let bank = self.bank_manager.current_bank_mut();
        // DISABLED FOR PERFORMANCE: debug_log(&format!("[UPDATE] Bank @ {:p}, Pad 0 playing={}", bank, bank.pads[0].is_playing));
        bank.update(dt);
        // DISABLED FOR PERFORMANCE: debug_log(&format!("[UPDATE] After update, Pad 0 playing={}", bank.pads[0].is_playing));
        
        // Update sequencer
        let events = self.sequencer.update();
        
        // Process sequencer events
        for event in events {
            match event {
                crate::sequencer::SequencerEvent::Trigger { pad, .. } => {
                    self.bank_manager.current_bank_mut().trigger_pad(*pad);
                }
                crate::sequencer::SequencerEvent::Release { pad } => {
                    self.bank_manager.current_bank_mut().release_pad(*pad);
                }
                _ => {}
            }
        }
        
        // Process MIDI/OSC input events
        self.input_router.process_events(&mut self.bank_manager, &mut self.sequencer);
        
        // Perform pending Syphon server refresh (macOS only)
        // This is done here to avoid calling Objective-C from event handlers
        #[cfg(target_os = "macos")]
        if self.syphon_refresh_pending {
            self.perform_syphon_refresh();
        }
    }
    
    /// Render output window (video)
    fn render_output(&mut self) -> anyhow::Result<()> {
        let surface = self.output_surface.as_ref().unwrap();
        let device = self.wgpu_device.as_ref().unwrap();
        let queue = self.wgpu_queue.as_ref().unwrap();
        
        let output = surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Output Render Encoder"),
        });
        
        // Render video mixer
        if let Some(mixer) = self.output_mixer.as_mut() {
            let bank = self.bank_manager.current_bank_mut();
            
            // Debug logging disabled for performance
            // To re-enable, uncomment the debug_log lines below
            
            // Update mixer channels from playing pads
            let mut _active_channels = 0;
            for (i, pad) in bank.pads.iter_mut().enumerate().take(8) {
                if pad.is_playing {
                    match pad.get_current_frame() {
                        Some(frame_texture) => {
                            let color_space = match pad.color_space() {
                                crate::sampler::sample::ColorSpace::Rgb => ColorSpace::Rgb,
                                crate::sampler::sample::ColorSpace::YcoCg => ColorSpace::YcoCg,
                            };
                            // Convert pad mix mode to engine mix mode
                            use crate::sampler::pad::PadMixMode;
                            let mix_mode = match pad.mix_mode {
                                PadMixMode::Normal => MixMode::Normal,
                                PadMixMode::Add => MixMode::Add,
                                PadMixMode::Multiply => MixMode::Multiply,
                                PadMixMode::Screen => MixMode::Screen,
                                PadMixMode::Overlay => MixMode::Overlay,
                                PadMixMode::SoftLight => MixMode::SoftLight,
                                PadMixMode::HardLight => MixMode::HardLight,
                                PadMixMode::Difference => MixMode::Difference,
                                PadMixMode::Lighten => MixMode::Lighten,
                                PadMixMode::Darken => MixMode::Darken,
                                PadMixMode::ChromaKey => MixMode::ChromaKey,
                                PadMixMode::LumaKey => MixMode::LumaKey,
                            };
                            
                            // Convert key params
                            let key_params = crate::engine::mixer::KeyParams {
                                key_color: pad.key_params.key_color,
                                threshold: pad.key_params.threshold,
                                smoothness: pad.key_params.smoothness,
                                invert: pad.key_params.invert,
                            };
                            mixer.set_channel_with_key(i, Some(frame_texture.clone()), mix_mode, pad.volume, color_space, key_params);
                            mixer.set_channel_enabled(i, true);
                            _active_channels += 1;
                        }
                        None => {
                            mixer.set_channel_enabled(i, false);
                        }
                    }
                } else {
                    mixer.set_channel_enabled(i, false);
                }
            }
            
            // _active_channels available for debugging if needed
            
            mixer.render(device, &mut encoder, queue, &view);
        }
        
        // Publish to Syphon (macOS only)
        #[cfg(target_os = "macos")]
        {
            if let Some(ref mut syphon) = self.syphon_output {
                // Publish the output texture to Syphon
                syphon.publish_frame(&output.texture, device, queue);
            }
        }
        
        queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
    
    /// Render control window (ImGui UI)
    fn render_control(&mut self) -> anyhow::Result<()> {
        let device = self.wgpu_device.as_ref().unwrap().clone();
        let queue = self.wgpu_queue.as_ref().unwrap().clone();
        
        let output = self.control_context.as_ref().unwrap().surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Control Render Encoder"),
        });
        
        // Clear background
        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.05,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        drop(_render_pass);
        
        // Render ImGui
        let window = self.control_window.as_ref().unwrap();
        let ctx = self.control_context.as_mut().unwrap();
        ctx.imgui.prepare_frame(window);
        
        let bank_manager = &mut self.bank_manager;
        let sequencer = &mut self.sequencer;
        let main_window = &mut self.main_window;
        
        let video_settings = &mut self.video_settings;
        
        ctx.imgui.render(
            window,
            &device,
            &queue,
            &mut encoder,
            &view,
            |ui: &imgui::Ui| {
                main_window.draw(ui, bank_manager, sequencer, &device, &queue, video_settings);
                video_settings.draw(ui);
            }
        )?;
        
        // Handle video settings after render
        if self.video_settings.needs_refresh() {
            self.refresh_video_devices();
        }

        // Start webcam when the user explicitly clicks "Start Device"
        if self.video_settings.take_start_webcam_requested() {
            let selected_device = self.video_settings.selected_camera();
            self.switch_video_device(selected_device);
        }

        // Handle Syphon server selection (macOS only)
        #[cfg(target_os = "macos")]
        {
            if self.video_settings.syphon_needs_refresh() {
                self.refresh_syphon_servers();
            }

            // Connect to Syphon only when the user explicitly clicks "Start Syphon"
            if self.video_settings.take_start_syphon_requested() {
                if let Some(server_name) = self.video_settings.selected_syphon_server().map(|s| s.to_string()) {
                    self.switch_syphon_server(&server_name);
                }
            }
        }
        
        queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
    }
    
    fn clear_screen(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.05,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
    }
    
    /// Get list of available cameras
    fn get_camera_list(&self) -> Vec<(u32, String)> {
        match list_cameras() {
            Ok(cameras) => cameras,
            Err(e) => {
                log::error!("Failed to list cameras: {}", e);
                Vec::new()
            }
        }
    }
    
    /// Refresh video device list
    fn refresh_video_devices(&mut self) {
        let cameras = self.get_camera_list();
        self.video_settings.update_camera_list(cameras);
    }
    
    /// Switch to a different video device
    fn switch_video_device(&mut self, device_index: u32) {
        // Don't switch if already initializing this device
        if self.video_settings.is_initializing() {
            return;
        }
        
        log::info!("Switching to video device {}", device_index);
        self.video_settings.set_initializing(true);
        self.video_settings.clear_error();
        
        // If there's an active recording, cancel it first
        if let Some(ref mut sampler) = self.live_sampler {
            if sampler.state() == crate::video::recorder::RecordingState::Recording {
                log::warn!("Cannot switch device while recording - canceling recording");
                sampler.cancel_recording();
            }
        }
        
        // Store the old device index in case we need to restore
        let old_device_index = self.video_device_index;
        
        // Drop the current sampler and create a new one with the new device
        self.live_sampler = None;
        
        let mut sampler = LiveSampler::new();
        match sampler.init_webcam(device_index) {
            Ok(()) => {
                log::info!("Live sampler reinitialized with device {}", device_index);
                self.video_device_index = device_index;
                self.live_sampler = Some(sampler);
                self.video_settings.set_initializing(false);
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                log::error!("Failed to initialize webcam device {}: {}", device_index, error_msg);
                self.video_settings.set_error(Some(error_msg));
                self.video_settings.set_initializing(false);
                
                // Restore the UI selection to the old device
                self.video_settings.update_camera_list(self.get_camera_list());
                
                // Try to restore the old device if it wasn't the one we just tried
                if old_device_index != device_index {
                    log::info!("Attempting to restore previous device {}", old_device_index);
                    let mut restore_sampler = LiveSampler::new();
                    if restore_sampler.init_webcam(old_device_index).is_ok() {
                        log::info!("Restored previous device {}", old_device_index);
                        self.video_device_index = old_device_index;
                        self.live_sampler = Some(restore_sampler);
                    } else {
                        log::error!("Failed to restore previous device");
                    }
                }
            }
        }
    }
    
    /// Request Syphon server list refresh (macOS only)
    /// Runs discovery in a background thread to avoid NSRunLoop re-entrancy issues
    #[cfg(target_os = "macos")]
    fn refresh_syphon_servers(&mut self) {
        if self.syphon_discovery_receiver.is_some() {
            return;
        }
        
        let (tx, rx) = flume::bounded(1);
        self.syphon_discovery_receiver = Some(rx);
        
        std::thread::spawn(move || {
            use crate::video::interapp::SyphonDiscovery;
            
            let discovery = SyphonDiscovery::new();
            let servers: Vec<String> = discovery.discover_servers()
                .into_iter()
                .map(|s| s.display_name().to_string())
                .collect();
            
            log::info!("Discovered {} Syphon servers (background thread)", servers.len());
            let _ = tx.send(servers);
        });
        
        log::debug!("Syphon server refresh started in background thread");
    }
    
    /// Stub for non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    fn refresh_syphon_servers(&mut self) {
        // No-op on non-macOS platforms
    }
    
    /// Check for completed Syphon discovery and update UI (macOS only)
    /// This is called from update() to avoid event handler issues
    #[cfg(target_os = "macos")]
    fn perform_syphon_refresh(&mut self) {
        if let Some(ref receiver) = self.syphon_discovery_receiver {
            if let Ok(servers) = receiver.try_recv() {
                log::info!("Received {} Syphon servers from discovery thread", servers.len());
                self.video_settings.update_syphon_servers(servers);
                self.syphon_discovery_receiver = None;
                self.syphon_refresh_pending = false;
            }
        } else {
            // No active discovery, clear pending flag
            self.syphon_refresh_pending = false;
        }
    }
    
    /// Switch to a different Syphon server (macOS only)
    #[cfg(target_os = "macos")]
    fn switch_syphon_server(&mut self, server_name: &str) {
        use crate::video::recorder::LiveSampler;
        
        // Don't switch if already initializing
        if self.video_settings.is_initializing() {
            return;
        }
        
        log::info!("Switching to Syphon server: {}", server_name);
        self.video_settings.set_initializing(true);
        self.video_settings.clear_error();
        
        // If there's an active recording, cancel it first
        if let Some(ref mut sampler) = self.live_sampler {
            if sampler.state() == crate::video::recorder::RecordingState::Recording {
                log::warn!("Cannot switch server while recording - canceling recording");
                sampler.cancel_recording();
            }
        }
        
        // Drop the current sampler and create a new one with the Syphon source
        self.live_sampler = None;
        
        let mut sampler = LiveSampler::new();
        match sampler.init_syphon(server_name) {
            Ok(()) => {
                log::info!("Live sampler reinitialized with Syphon server: {}", server_name);
                self.live_sampler = Some(sampler);
                self.video_settings.set_initializing(false);
            }
            Err(e) => {
                let error_msg = format!("{}", e);
                log::error!("Failed to initialize Syphon server '{}': {}", server_name, error_msg);
                self.video_settings.set_error(Some(error_msg));
                self.video_settings.set_initializing(false);
            }
        }
    }
    
    /// Stub for non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    fn switch_syphon_server(&mut self, _server_name: &str) {
        log::warn!("Syphon is only available on macOS");
    }
}

/// winit ApplicationHandler implementation
impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("=== App::resumed() called ===");
        
        // Create output window first
        log::info!("Creating output window...");
        self.create_output_window(event_loop);
        
        if self.output_window.is_none() {
            log::error!("Output window creation failed - exiting");
            event_loop.exit();
            return;
        }
        
        // Create control window
        log::info!("Creating control window...");
        self.create_control_window(event_loop);
        
        // Initialize wgpu (output creates device, control shares it)
        log::info!("Initializing wgpu...");
        let rt = self.rt.clone();
        if let Err(e) = rt.block_on(self.init_wgpu()) {
            log::error!("Failed to initialize wgpu: {}", e);
            event_loop.exit();
            return;
        }
        
        log::info!("=== Resume complete ===");
        log::info!("Output window: {}", if self.output_window.is_some() { "OK" } else { "FAIL" });
        log::info!("Control window: {}", if self.control_window.is_some() { "OK" } else { "FAIL" });
        log::info!("Control context: {}", if self.control_context.is_some() { "OK" } else { "FAIL" });
    }
    
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route to appropriate window handler
        if let Some(ref output_window) = self.output_window {
            if window_id == output_window.id() {
                self.handle_output_event(event_loop, event);
                return;
            }
        }
        
        if let Some(ref control_window) = self.control_window {
            if window_id == control_window.id() {
                self.handle_control_event(event_loop, event);
                return;
            }
        }
    }
    
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Update application state
        self.update();
        
        // Request redraws for both windows
        if let Some(ref window) = self.output_window {
            window.request_redraw();
        }
        if let Some(ref window) = self.control_window {
            window.request_redraw();
        }
    }
}
