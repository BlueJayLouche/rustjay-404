//! Unified video input management with hot-swappable backends.
//!
//! Ported from rustjay-template's input architecture. Provides a single
//! `VideoInputManager` that owns all capture backends (webcam, Syphon, NDI)
//! with async device discovery and dual-path frame handling.

use crate::video::capture::webcam::{self, CapturedFrame, WebcamCapture};
#[cfg(target_os = "macos")]
use crate::video::interapp::syphon_input::{
    SyphonDiscovery, SyphonFrame, SyphonInputReceiver, SyphonServerInfo,
};

use std::sync::mpsc;

#[cfg(feature = "ndi")]
pub mod ndi;
#[cfg(feature = "ndi")]
pub use ndi::{list_ndi_sources, NdiReceiver, NdiFrame};

#[cfg(not(feature = "ndi"))]
pub fn list_ndi_sources(_timeout_ms: u32) -> Vec<String> {
    vec![]
}

/// Commands sent from the UI to control input
#[derive(Debug, Clone)]
pub enum InputCommand {
    None,
    StartWebcam { device_index: u32 },
    #[cfg(target_os = "macos")]
    StartSyphon { server_name: String },
    #[cfg(feature = "ndi")]
    StartNdi { source_name: String },
    StopInput,
    RefreshDevices,
}

impl Default for InputCommand {
    fn default() -> Self {
        InputCommand::None
    }
}

/// Currently active input type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    None,
    Webcam,
    #[cfg(target_os = "macos")]
    Syphon,
    #[cfg(feature = "ndi")]
    Ndi,
}

/// Results from async device discovery
struct DiscoveryResults {
    webcam_devices: Vec<(u32, String)>,
    #[cfg(target_os = "macos")]
    syphon_servers: Vec<SyphonServerInfo>,
    #[cfg(feature = "ndi")]
    ndi_sources: Vec<String>,
}

/// Unified frame type for recording pipeline
pub enum InputFrame {
    /// RGB24 frame from webcam
    Webcam(CapturedFrame),
    /// RGBA frame from Syphon
    #[cfg(target_os = "macos")]
    Syphon(SyphonFrame),
    /// BGRA frame from NDI
    #[cfg(feature = "ndi")]
    Ndi(NdiFrame),
}

impl InputFrame {
    pub fn width(&self) -> u32 {
        match self {
            InputFrame::Webcam(f) => f.width,
            #[cfg(target_os = "macos")]
            InputFrame::Syphon(f) => f.width,
            #[cfg(feature = "ndi")]
            InputFrame::Ndi(f) => f.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            InputFrame::Webcam(f) => f.height,
            #[cfg(target_os = "macos")]
            InputFrame::Syphon(f) => f.height,
            #[cfg(feature = "ndi")]
            InputFrame::Ndi(f) => f.height,
        }
    }

    pub fn data(&self) -> &[u8] {
        match self {
            InputFrame::Webcam(f) => &f.data,
            #[cfg(target_os = "macos")]
            InputFrame::Syphon(f) => &f.data,
            #[cfg(feature = "ndi")]
            InputFrame::Ndi(f) => &f.data,
        }
    }

    /// Convert to CapturedFrame for recording compatibility
    pub fn to_captured_frame(&self) -> CapturedFrame {
        match self {
            InputFrame::Webcam(f) => f.clone(),
            #[cfg(target_os = "macos")]
            InputFrame::Syphon(f) => CapturedFrame {
                data: f.data.clone(),
                width: f.width,
                height: f.height,
                timestamp: f.timestamp,
            },
            #[cfg(feature = "ndi")]
            InputFrame::Ndi(f) => CapturedFrame {
                data: f.data.clone(),
                width: f.width,
                height: f.height,
                timestamp: f.timestamp,
            },
        }
    }
}

/// Manages video input backends with hot-swapping and async discovery.
pub struct VideoInputManager {
    input_type: InputType,
    active: bool,
    has_new_frame: bool,
    resolution: (u32, u32),

    // Backends
    webcam: Option<WebcamCapture>,
    #[cfg(target_os = "macos")]
    syphon: Option<SyphonInputReceiver>,
    #[cfg(feature = "ndi")]
    ndi_receiver: Option<NdiReceiver>,

    // Current frame buffer
    current_frame: Option<InputFrame>,

    // Device lists (populated by async discovery)
    pub webcam_devices: Vec<(u32, String)>,
    #[cfg(target_os = "macos")]
    pub syphon_servers: Vec<SyphonServerInfo>,
    #[cfg(feature = "ndi")]
    pub ndi_sources: Vec<String>,

    // Async discovery
    discovery_rx: Option<mpsc::Receiver<DiscoveryResults>>,
    discovering: bool,
}

impl VideoInputManager {
    pub fn new() -> Self {
        Self {
            input_type: InputType::None,
            active: false,
            has_new_frame: false,
            resolution: (0, 0),
            webcam: None,
            #[cfg(target_os = "macos")]
            syphon: None,
            #[cfg(feature = "ndi")]
            ndi_receiver: None,
            current_frame: None,
            webcam_devices: Vec::new(),
            #[cfg(target_os = "macos")]
            syphon_servers: Vec::new(),
            #[cfg(feature = "ndi")]
            ndi_sources: Vec::new(),
            discovery_rx: None,
            discovering: false,
        }
    }

    // --- State queries ---

    pub fn input_type(&self) -> InputType {
        self.input_type
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_discovering(&self) -> bool {
        self.discovering
    }

    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }

    pub fn has_new_frame(&self) -> bool {
        self.has_new_frame
    }

    // --- Device discovery (background thread) ---

    /// Spawn background thread to discover all available input devices.
    pub fn begin_refresh_devices(&mut self) {
        if self.discovering {
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.discovery_rx = Some(rx);
        self.discovering = true;

        std::thread::spawn(move || {
            log::info!("[VideoInput] Starting device discovery...");

            let webcam_devices = match webcam::list_cameras() {
                Ok(cams) => {
                    log::info!("[VideoInput] Found {} webcams", cams.len());
                    cams
                }
                Err(e) => {
                    log::warn!("[VideoInput] Webcam discovery failed: {}", e);
                    Vec::new()
                }
            };

            #[cfg(target_os = "macos")]
            let syphon_servers = {
                let discovery = SyphonDiscovery::new();
                let servers = discovery.discover_servers();
                log::info!("[VideoInput] Found {} Syphon servers", servers.len());
                servers
            };

            #[cfg(feature = "ndi")]
            let ndi_sources = {
                log::info!("[VideoInput] Discovering NDI sources...");
                let sources = list_ndi_sources(2000);
                log::info!("[VideoInput] Found {} NDI source(s)", sources.len());
                sources
            };

            let _ = tx.send(DiscoveryResults {
                webcam_devices,
                #[cfg(target_os = "macos")]
                syphon_servers,
                #[cfg(feature = "ndi")]
                ndi_sources,
            });
        });
    }

    /// Non-blocking poll for discovery results. Returns true if new results arrived.
    pub fn poll_discovery(&mut self) -> bool {
        if let Some(ref rx) = self.discovery_rx {
            match rx.try_recv() {
                Ok(results) => {
                    self.webcam_devices = results.webcam_devices;
                    #[cfg(target_os = "macos")]
                    {
                        self.syphon_servers = results.syphon_servers;
                    }
                    #[cfg(feature = "ndi")]
                    {
                        self.ndi_sources = results.ndi_sources;
                    }
                    self.discovering = false;
                    self.discovery_rx = None;
                    log::info!("[VideoInput] Device discovery complete");
                    true
                }
                Err(mpsc::TryRecvError::Empty) => false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.discovering = false;
                    self.discovery_rx = None;
                    false
                }
            }
        } else {
            false
        }
    }

    // --- Backend start/stop ---

    /// Stop all active inputs.
    pub fn stop(&mut self) {
        if let Some(mut cam) = self.webcam.take() {
            cam.stop();
            log::info!("[VideoInput] Webcam stopped");
        }

        #[cfg(target_os = "macos")]
        {
            if let Some(mut syphon) = self.syphon.take() {
                syphon.disconnect();
                log::info!("[VideoInput] Syphon input disconnected");
            }
        }

        #[cfg(feature = "ndi")]
        {
            if let Some(mut ndi) = self.ndi_receiver.take() {
                ndi.stop();
                log::info!("[VideoInput] NDI input stopped");
            }
        }

        self.input_type = InputType::None;
        self.active = false;
        self.has_new_frame = false;
        self.current_frame = None;
        self.resolution = (0, 0);
    }

    /// Start webcam capture (stops current input first).
    pub fn start_webcam(&mut self, device_index: u32) -> anyhow::Result<()> {
        self.stop();

        log::info!("[VideoInput] Starting webcam {}", device_index);
        let mut cam = WebcamCapture::new();
        cam.start(device_index)?;
        self.resolution = cam.resolution();
        self.webcam = Some(cam);
        self.input_type = InputType::Webcam;
        self.active = true;

        // Warm up: discard first few frames
        if let Some(ref mut cam) = self.webcam {
            for _ in 0..5 {
                let _ = cam.get_frame();
            }
        }

        log::info!(
            "[VideoInput] Webcam started at {}x{}",
            self.resolution.0,
            self.resolution.1
        );
        Ok(())
    }

    /// Start Syphon input (stops current input first).
    #[cfg(target_os = "macos")]
    pub fn start_syphon(&mut self, server_name: &str) -> anyhow::Result<()> {
        self.stop();

        log::info!("[VideoInput] Connecting to Syphon server: {}", server_name);
        let mut receiver = SyphonInputReceiver::new();
        receiver.connect(server_name)?;
        self.resolution = receiver.resolution();
        self.syphon = Some(receiver);
        self.input_type = InputType::Syphon;
        self.active = true;

        log::info!("[VideoInput] Syphon connected: {}", server_name);
        Ok(())
    }

    /// Start NDI input (stops current input first).
    #[cfg(feature = "ndi")]
    pub fn start_ndi(&mut self, source_name: &str) -> anyhow::Result<()> {
        self.stop();

        log::info!("[VideoInput] Connecting to NDI source: {}", source_name);
        let mut ndi = NdiReceiver::new(source_name);
        ndi.start()?;
        self.input_type = InputType::Ndi;
        self.active = true;
        self.ndi_receiver = Some(ndi);

        log::info!("[VideoInput] NDI started: {}", source_name);
        Ok(())
    }

    #[cfg(not(feature = "ndi"))]
    pub fn start_ndi(&mut self, _source_name: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("NDI support not compiled. Enable the 'ndi' feature."))
    }

    /// Returns true if the NDI source was lost
    #[cfg(feature = "ndi")]
    pub fn is_ndi_source_lost(&self) -> bool {
        self.ndi_receiver.as_ref().map(|r| r.is_source_lost()).unwrap_or(false)
    }

    #[cfg(not(feature = "ndi"))]
    pub fn is_ndi_source_lost(&self) -> bool {
        false
    }

    // --- Frame polling (called each frame) ---

    /// Poll the active backend for new frames.
    pub fn update(&mut self) {
        if !self.active {
            return;
        }

        match self.input_type {
            InputType::Webcam => {
                if let Some(ref mut cam) = self.webcam {
                    if let Some(frame) = cam.get_frame() {
                        self.resolution = (frame.width, frame.height);
                        self.current_frame = Some(InputFrame::Webcam(frame));
                        self.has_new_frame = true;
                    }
                }
            }
            #[cfg(target_os = "macos")]
            InputType::Syphon => {
                if let Some(ref mut syphon) = self.syphon {
                    if let Some(frame) = syphon.try_receive() {
                        self.resolution = (frame.width, frame.height);
                        self.current_frame = Some(InputFrame::Syphon(frame));
                        self.has_new_frame = true;
                    }
                }
            }
            #[cfg(feature = "ndi")]
            InputType::Ndi => {
                if let Some(ref mut ndi) = self.ndi_receiver {
                    if let Some(frame) = ndi.get_latest_frame() {
                        self.resolution = (frame.width, frame.height);
                        self.current_frame = Some(InputFrame::Ndi(frame));
                        self.has_new_frame = true;
                    }
                }
            }
            InputType::None => {}
        }
    }

    /// Take the current frame (consuming it). Returns None if no new frame.
    pub fn take_frame(&mut self) -> Option<InputFrame> {
        if self.has_new_frame {
            self.has_new_frame = false;
            self.current_frame.take()
        } else {
            None
        }
    }

    // --- Command dispatch ---

    /// Process an InputCommand. Returns Ok(()) or an error message.
    pub fn dispatch(&mut self, command: InputCommand) -> anyhow::Result<()> {
        match command {
            InputCommand::None => Ok(()),
            InputCommand::RefreshDevices => {
                self.begin_refresh_devices();
                Ok(())
            }
            InputCommand::StartWebcam { device_index } => self.start_webcam(device_index),
            #[cfg(target_os = "macos")]
            InputCommand::StartSyphon { server_name } => self.start_syphon(&server_name),
            #[cfg(feature = "ndi")]
            InputCommand::StartNdi { source_name } => self.start_ndi(&source_name),
            InputCommand::StopInput => {
                self.stop();
                Ok(())
            }
        }
    }
}

impl Default for VideoInputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VideoInputManager {
    fn drop(&mut self) {
        self.stop();
    }
}
