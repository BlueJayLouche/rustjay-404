//! Inter-app video sharing (Syphon, Spout, v4l2loopback)
//!
//! Platform-specific implementations for sharing video between applications.

use wgpu;

/// Platform-agnostic inter-app video trait
pub trait InterAppVideo: Send + Sync {
    /// Publish a frame to other applications
    fn publish_frame(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue);
    
    /// Try to receive a frame from other applications
    /// Returns None if no new frame available
    fn receive_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<wgpu::Texture>;
    
    /// Get the server/application name
    fn name(&self) -> &str;
    
    /// Check if this backend is available on the current platform
    fn is_available() -> bool where Self: Sized;
}

/// Input source types for inter-app video
#[derive(Debug, Clone)]
pub enum InterAppInput {
    /// Syphon server (macOS only)
    #[cfg(target_os = "macos")]
    Syphon { server_name: String },
    
    /// Spout sender (Windows only)
    #[cfg(target_os = "windows")]
    Spout { sender_name: String },
    
    /// v4l2loopback device (Linux only)
    #[cfg(target_os = "linux")]
    V4l2Loopback { device_path: String },
}

/// Output backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterAppOutput {
    /// Syphon server (macOS only)
    #[cfg(target_os = "macos")]
    Syphon,
    
    /// Spout sender (Windows only)
    #[cfg(target_os = "windows")]
    Spout,
    
    /// v4l2loopback device (Linux only)
    #[cfg(target_os = "linux")]
    V4l2Loopback,
}

impl InterAppOutput {
    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            InterAppOutput::Syphon => "Syphon",
            #[cfg(target_os = "windows")]
            InterAppOutput::Spout => "Spout",
            #[cfg(target_os = "linux")]
            InterAppOutput::V4l2Loopback => "v4l2loopback",
        }
    }
    
    /// Get all available backends for current platform
    pub fn available_backends() -> Vec<InterAppOutput> {
        let mut backends = Vec::new();
        
        #[cfg(target_os = "macos")]
        backends.push(InterAppOutput::Syphon);
        
        #[cfg(target_os = "windows")]
        backends.push(InterAppOutput::Spout);
        
        #[cfg(target_os = "linux")]
        backends.push(InterAppOutput::V4l2Loopback);
        
        backends
    }
}

// Platform-specific modules
#[cfg(target_os = "macos")]
pub mod syphon;
#[cfg(target_os = "macos")]
pub mod syphon_input;

#[cfg(target_os = "windows")]
pub mod spout;

#[cfg(target_os = "linux")]
pub mod v4l2loopback;

// Re-export platform-specific types
#[cfg(target_os = "macos")]
pub use syphon::{SyphonOutput, SyphonOutputError, Result as SyphonResult};
#[cfg(target_os = "macos")]
pub use syphon_input::{SyphonInputReceiver, SyphonDiscovery, SyphonInputIntegration, SyphonServerInfo, SyphonFrame};

#[cfg(target_os = "windows")]
pub use spout::SpoutOutput;

#[cfg(target_os = "linux")]
pub use v4l2loopback::V4l2Output;

/// Factory function to create the appropriate output backend
/// 
/// # Arguments
/// * `backend` - The inter-app video backend to use
/// * `name` - Server name visible to clients
/// * `device` - The wgpu device
/// * `queue` - The wgpu queue (required for zero-copy GPU transfer)
/// * `width` - Frame width in pixels
/// * `height` - Frame height in pixels
/// 
/// # Panics
/// Panics if the requested backend cannot be created (e.g., Syphon framework not installed).
/// Use `try_create_output()` for a fallible version.
pub fn create_output(
    backend: InterAppOutput,
    name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
) -> Box<dyn InterAppVideo> {
    match backend {
        #[cfg(target_os = "macos")]
        InterAppOutput::Syphon => {
            match SyphonOutput::new(name, device, queue, width, height) {
                Ok(output) => Box::new(output),
                Err(e) => {
                    panic!("Failed to create Syphon output: {}. \
                        Ensure Syphon.framework is installed at /Library/Frameworks/ \
                        (download from https://github.com/Syphon/Syphon-Framework/releases)", e)
                }
            }
        }
        
        #[cfg(target_os = "windows")]
        InterAppOutput::Spout => {
            Box::new(SpoutOutput::new(name, device, width, height))
        }
        
        #[cfg(target_os = "linux")]
        InterAppOutput::V4l2Loopback => {
            Box::new(V4l2Output::new(name, device, width, height))
        }
    }
}

/// Try to create an output backend, returning None if unavailable
///
/// This is useful for gracefully degrading when a backend (like Syphon)
/// is not installed or available.
pub fn try_create_output(
    backend: InterAppOutput,
    name: &str,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
) -> Option<Box<dyn InterAppVideo>> {
    match backend {
        #[cfg(target_os = "macos")]
        InterAppOutput::Syphon => {
            SyphonOutput::try_new(name, device, queue, width, height)
                .map(|o| Box::new(o) as Box<dyn InterAppVideo>)
        }
        
        #[cfg(target_os = "windows")]
        InterAppOutput::Spout => {
            Some(Box::new(SpoutOutput::new(name, device, width, height)))
        }
        
        #[cfg(target_os = "linux")]
        InterAppOutput::V4l2Loopback => {
            Some(Box::new(V4l2Output::new(name, device, width, height)))
        }
    }
}
