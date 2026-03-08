//! Syphon Output Implementation for macOS
//!
//! Publishes frames to Syphon clients using the syphon-wgpu crate.
//! For input (receiving from other apps), see syphon_input.rs

use super::InterAppVideo;
use wgpu;

// Import SyphonError from syphon_wgpu re-export
use syphon_wgpu::SyphonError;

// Re-export input types for convenience
pub use super::syphon_input::{
    SyphonInputReceiver, 
    SyphonDiscovery, 
    SyphonInputIntegration, 
    SyphonServerInfo,
    SyphonFrame,
};

/// Errors that can occur when creating a Syphon output
#[derive(Debug)]
pub enum SyphonOutputError {
    /// Syphon framework not found or not properly installed
    FrameworkNotFound(String),
    /// Failed to create the Syphon server
    CreateFailed(String),
}

impl std::fmt::Display for SyphonOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyphonOutputError::FrameworkNotFound(msg) => {
                write!(f, "Syphon framework not found: {}. Install from https://github.com/Syphon/Syphon-Framework/releases", msg)
            }
            SyphonOutputError::CreateFailed(msg) => {
                write!(f, "Failed to create Syphon output: {}", msg)
            }
        }
    }
}

impl std::error::Error for SyphonOutputError {}

/// Result type for Syphon output operations
pub type Result<T> = std::result::Result<T, SyphonOutputError>;

/// Syphon video output using wgpu integration
pub struct SyphonOutput {
    inner: syphon_wgpu::SyphonWgpuOutput,
}

// Metal objects are thread-safe on macOS
unsafe impl Send for SyphonOutput {}
unsafe impl Sync for SyphonOutput {}

impl SyphonOutput {
    /// Create a new Syphon output server
    /// 
    /// Uses zero-copy GPU-to-GPU transfer when available.
    ///
    /// # Errors
    /// Returns `SyphonOutputError::FrameworkNotFound` if the Syphon framework is not
    /// installed or has an incorrect install name.
    /// Returns `SyphonOutputError::CreateFailed` for other creation failures.
    pub fn new(name: &str, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Result<Self> {
        log::info!("Creating Syphon output '{}' ({}x{})", name, width, height);
        
        let inner = match syphon_wgpu::SyphonWgpuOutput::new(name, device, queue, width, height) {
            Ok(output) => output,
            Err(SyphonError::FrameworkNotFound(msg)) => {
                log::error!("Syphon framework not found: {}", msg);
                log::error!("Install Syphon from: https://github.com/Syphon/Syphon-Framework/releases");
                return Err(SyphonOutputError::FrameworkNotFound(msg));
            }
            Err(e) => {
                return Err(SyphonOutputError::CreateFailed(e.to_string()));
            }
        };
        
        if inner.is_zero_copy() {
            log::info!("Syphon server '{}' created (zero-copy enabled)", name);
        } else {
            log::info!("Syphon server '{}' created (CPU fallback)", name);
        }
        
        Ok(Self { inner })
    }
    
    /// Try to create a Syphon output, returning None if Syphon is not available
    ///
    /// This is useful for applications that want to gracefully degrade when
    /// Syphon is not installed.
    pub fn try_new(name: &str, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Option<Self> {
        match Self::new(name, device, queue, width, height) {
            Ok(output) => Some(output),
            Err(e) => {
                log::warn!("Syphon not available: {}", e);
                None
            }
        }
    }
    

    
    /// List available Syphon servers on the system
    pub fn list_servers() -> Vec<String> {
        syphon_wgpu::list_servers()
    }
    
    /// Check if Syphon is available
    pub fn is_available() -> bool {
        syphon_wgpu::is_available()
    }
    
    /// Get number of connected clients
    pub fn client_count(&self) -> usize {
        self.inner.client_count()
    }
    
    /// Check if any clients are connected
    pub fn has_clients(&self) -> bool {
        self.inner.has_clients()
    }
    
    /// Check if zero-copy GPU-to-GPU transfer is active
    pub fn is_zero_copy(&self) -> bool {
        self.inner.is_zero_copy()
    }
}

impl InterAppVideo for SyphonOutput {
    fn publish_frame(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.inner.publish(texture, device, queue);
    }
    
    fn receive_frame(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> Option<wgpu::Texture> {
        None
    }
    
    fn name(&self) -> &str {
        self.inner.name()
    }
    
    fn is_available() -> bool {
        Self::is_available()
    }
}

impl Drop for SyphonOutput {
    fn drop(&mut self) {
        log::info!("Destroying Syphon output '{}'", self.inner.name());
    }
}

/// Syphon input client for receiving video from other apps
pub struct SyphonInput {
    server_name: String,
}

impl SyphonInput {
    /// Create a new Syphon input client
    pub fn new(server_name: &str) -> Self {
        log::info!("Creating Syphon input from '{}'", server_name);
        
        Self {
            server_name: server_name.to_string(),
        }
    }
    
    /// Try to receive a new frame
    pub fn try_receive(&mut self, _device: &wgpu::Device) -> Option<wgpu::Texture> {
        // TODO: Implement using syphon-wgpu input
        None
    }
    
    /// Check if still connected
    pub fn is_connected(&self) -> bool {
        false
    }
}
