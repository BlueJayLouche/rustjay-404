//! Application configuration
//!
//! Dual-window setup:
//! - Output window: Video display, cursor hidden, clean presentation
//! - Control window: ImGui UI, cursor visible, standard window

use serde::{Deserialize, Serialize};
use crate::video::encoder::{HapEncodeFormat, GpuMode};

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Output window configuration (video display)
    pub output_window: WindowConfig,
    /// Control window configuration (ImGui UI)
    pub control_window: WindowConfig,
    /// Target FPS for output
    pub target_fps: u32,
    /// Enable VSync
    pub vsync: bool,
    /// Encoding settings
    #[serde(default)]
    pub encoding: EncodingConfig,
}

/// Encoding configuration for HAP video output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingConfig {
    /// HAP format for encoding/recording
    pub format: HapEncodeFormat,
    /// GPU encoding mode: auto, gpu, or cpu
    pub gpu_mode: GpuMode,
    /// Maximum pixel dimension (width or height) for imported videos.
    /// Larger videos are scaled down preserving aspect ratio.
    /// 0 = no limit (keep original resolution).
    /// Default: 1920 (keeps Retina screen recordings manageable)
    pub max_dimension: u32,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            format: HapEncodeFormat::Dxt1,
            gpu_mode: GpuMode::Auto,
            max_dimension: 1920,
        }
    }
}

/// Window configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Window width in pixels
    pub width: u32,
    /// Window height in pixels
    pub height: u32,
    /// X position (None for default)
    pub x: Option<i32>,
    /// Y position (None for default)
    pub y: Option<i32>,
    /// Window title
    pub title: String,
    /// Whether window is resizable
    pub resizable: bool,
    /// Whether window has decorations (title bar, borders)
    pub decorated: bool,
    /// Whether cursor is visible in this window
    pub cursor_visible: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_window: WindowConfig {
                width: 1280,
                height: 720,
                x: Some(100),
                y: Some(100),
                title: "Rusty-404 - Output".to_string(),
                resizable: true,
                decorated: true,
                cursor_visible: false,
            },
            control_window: WindowConfig {
                width: 1200,
                height: 800,
                x: Some(50),
                y: Some(50),
                title: "Rusty-404 - Control".to_string(),
                resizable: true,
                decorated: true,
                cursor_visible: true,
            },
            target_fps: 60,
            vsync: true,
            encoding: EncodingConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from file or create default
    pub fn load_or_default() -> Self {
        let config_path = std::path::PathBuf::from("rustjay404.toml");
        
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(contents) => {
                    match toml::from_str(&contents) {
                        Ok(config) => return config,
                        Err(e) => {
                            log::warn!("Failed to parse config: {}, using defaults", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read config: {}, using defaults", e);
                }
            }
        }
        
        let config = Self::default();
        
        // Save default config
        if let Ok(toml) = toml::to_string_pretty(&config) {
            let _ = std::fs::write(&config_path, toml);
        }
        
        config
    }
    
    /// Save configuration to file
    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = std::path::PathBuf::from("rustjay404.toml");
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, toml)?;
        Ok(())
    }
}
