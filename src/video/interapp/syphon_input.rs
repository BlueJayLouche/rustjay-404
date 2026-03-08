//! Syphon Input Implementation for macOS
//!
//! Receives video frames from Syphon servers using syphon-core crate.
//! This provides a consistent API with rustjay_waaaves.

use std::time::Instant;

/// Re-export from syphon-core
pub use syphon_core::ServerInfo as SyphonServerInfo;

/// A received Syphon frame
pub struct SyphonFrame {
    pub width: u32,
    pub height: u32,
    /// RGBA pixel data (converted from BGRA for GPU compatibility)
    pub data: Vec<u8>,
    pub timestamp: Instant,
}

/// Syphon input receiver
///
/// Connects to a Syphon server and receives frames.
/// Uses syphon_core::SyphonClient for the actual communication.
pub struct SyphonInputReceiver {
    #[cfg(target_os = "macos")]
    client: Option<syphon_core::SyphonClient>,
    server_name: Option<String>,
    resolution: (u32, u32),
}

impl SyphonInputReceiver {
    /// Create a new Syphon input receiver
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            client: None,
            server_name: None,
            resolution: (1920, 1080),
        }
    }
    
    /// Check if Syphon is available
    pub fn is_available() -> bool {
        syphon_core::is_available()
    }
    
    /// Connect to a Syphon server by name
    pub fn connect(&mut self, server_name: impl Into<String>) -> anyhow::Result<()> {
        let server_name = server_name.into();
        
        if self.is_connected() {
            self.disconnect();
        }
        
        log::info!("[Syphon Input] Connecting to server: {}", server_name);
        
        #[cfg(target_os = "macos")]
        {
            let client = syphon_core::SyphonClient::connect(&server_name)
                .map_err(|e| anyhow::anyhow!("Failed to connect to '{}': {}", server_name, e))?;
            
            log::info!("[Syphon Input] Connected to '{}'", server_name);
            self.client = Some(client);
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            return Err(anyhow::anyhow!("Syphon is only available on macOS"));
        }
        
        self.server_name = Some(server_name);
        Ok(())
    }
    
    /// Try to receive a new frame
    pub fn try_receive(&mut self) -> Option<SyphonFrame> {
        #[cfg(target_os = "macos")]
        {
            let client = self.client.as_ref()?;
            
            match client.try_receive() {
                Ok(Some(mut frame)) => {
                    self.resolution = (frame.width, frame.height);
                    
                    match frame.to_vec() {
                        Ok(bgra_data) => {
                            // Convert BGRA to RGBA (Syphon uses BGRA, but wgpu expects RGBA)
                            let rgba_data = convert_bgra_to_rgba(&bgra_data, frame.width, frame.height);
                            
                            return Some(SyphonFrame {
                                width: frame.width,
                                height: frame.height,
                                data: rgba_data,
                                timestamp: Instant::now(),
                            });
                        }
                        Err(e) => {
                            log::warn!("[Syphon Input] Failed to read frame data: {}", e);
                            return None;
                        }
                    }
                }
                Ok(None) => {
                    // No new frame available
                    return None;
                }
                Err(e) => {
                    log::warn!("[Syphon Input] Error receiving frame: {}", e);
                    return None;
                }
            }
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
    
    /// Disconnect from current server
    pub fn disconnect(&mut self) {
        if !self.is_connected() {
            return;
        }
        
        log::info!("[Syphon Input] Disconnecting from: {:?}", self.server_name);
        
        #[cfg(target_os = "macos")]
        {
            self.client = None;
        }
        
        self.server_name = None;
    }
    
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.client.is_some()
        }
        
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    
    /// Get current resolution
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
    
    /// Get connected server name
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }
}

impl Default for SyphonInputReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SyphonInputReceiver {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Syphon server discovery
///
/// Scans for available Syphon servers on the local machine.
pub struct SyphonDiscovery;

impl SyphonDiscovery {
    /// Create new discovery
    pub fn new() -> Self {
        Self
    }
    
    /// Discover available Syphon servers
    pub fn discover_servers(&self) -> Vec<SyphonServerInfo> {
        log::debug!("[Syphon] Discovering servers...");
        
        let servers = syphon_core::SyphonServerDirectory::servers();
        
        log::info!("[Syphon] Discovered {} servers", servers.len());
        for server in &servers {
            log::debug!("  - {} (app: {})", server.display_name(), server.app_name);
        }
        
        servers
    }
    
    /// Check if a specific server is still available
    pub fn is_server_available(&self, name: &str) -> bool {
        syphon_core::SyphonServerDirectory::server_exists(name)
    }
}

impl Default for SyphonDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience integration struct
///
/// Combines discovery and receiver for easier use.
pub struct SyphonInputIntegration {
    receiver: Option<SyphonInputReceiver>,
    discovery: SyphonDiscovery,
    cached_servers: Vec<SyphonServerInfo>,
    last_discovery: Option<Instant>,
}

impl SyphonInputIntegration {
    /// Create new integration
    pub fn new() -> Self {
        Self {
            receiver: None,
            discovery: SyphonDiscovery::new(),
            cached_servers: Vec::new(),
            last_discovery: None,
        }
    }
    
    /// Check if Syphon is available
    pub fn is_available() -> bool {
        SyphonInputReceiver::is_available()
    }
    
    /// Refresh the list of available servers
    pub fn refresh_servers(&mut self) {
        self.cached_servers = self.discovery.discover_servers();
        self.last_discovery = Some(Instant::now());
    }
    
    /// Get cached server list
    pub fn servers(&self) -> &[SyphonServerInfo] {
        &self.cached_servers
    }
    
    /// Connect to a server by name
    pub fn connect(&mut self, server_name: &str) -> anyhow::Result<()> {
        if self.receiver.is_some() {
            self.disconnect();
        }
        
        let mut receiver = SyphonInputReceiver::new();
        receiver.connect(server_name)?;
        self.receiver = Some(receiver);
        
        Ok(())
    }
    
    /// Disconnect from current server
    pub fn disconnect(&mut self) {
        self.receiver = None;
    }
    
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.receiver.as_ref().map_or(false, |r| r.is_connected())
    }
    
    /// Get latest frame
    pub fn get_frame(&mut self) -> Option<SyphonFrame> {
        let receiver = self.receiver.as_mut()?;
        receiver.try_receive()
    }
    
    /// Update (called each frame)
    pub fn update(&mut self) {
        // Auto-refresh discovery every 5 seconds
        if self.last_discovery.map_or(true, |t| t.elapsed().as_secs() > 5) {
            self.refresh_servers();
        }
    }
    
    /// Get connected server name
    pub fn connected_server(&self) -> Option<&str> {
        self.receiver.as_ref()?.server_name()
    }
}

impl Default for SyphonInputIntegration {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export syphon_core types
pub use syphon_core::{SyphonClient, SyphonServerDirectory};

/// Convert BGRA data to RGBA
/// 
/// Syphon uses BGRA format (native macOS), but wgpu/shaders expect RGBA.
fn convert_bgra_to_rgba(bgra_data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let pixel_count = (width * height) as usize;
    let mut rgba_data = vec![0u8; pixel_count * 4];
    
    // Calculate stride - IOSurface often uses aligned rows
    let actual_stride = if height > 0 {
        bgra_data.len() / height as usize
    } else {
        width as usize * 4
    };
    
    let expected_stride = width as usize * 4;
    
    log::debug!("[Syphon Input] Converting frame: {}x{}, data_len={}, actual_stride={}, expected_stride={}",
        width, height, bgra_data.len(), actual_stride, expected_stride);

    for y in 0..height as usize {
        for x in 0..width as usize {
            let src_idx = y * actual_stride + x * 4;
            let dst_idx = (y * width as usize + x) * 4;
            
            if src_idx + 3 < bgra_data.len() && dst_idx + 3 < rgba_data.len() {
                // BGRA -> RGBA: swap B and R
                rgba_data[dst_idx] = bgra_data[src_idx + 2];     // R <- B
                rgba_data[dst_idx + 1] = bgra_data[src_idx + 1]; // G <- G
                rgba_data[dst_idx + 2] = bgra_data[src_idx];     // B <- R
                rgba_data[dst_idx + 3] = bgra_data[src_idx + 3]; // A <- A
            }
        }
    }

    rgba_data
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_receiver_creation() {
        let receiver = SyphonInputReceiver::new();
        assert!(!receiver.is_connected());
    }
    
    #[test]
    fn test_discovery_creation() {
        let discovery = SyphonDiscovery::new();
        let servers = discovery.discover_servers();
        println!("Found {} servers", servers.len());
    }
    
    #[test]
    fn test_integration_creation() {
        let integration = SyphonInputIntegration::new();
        assert!(!integration.is_connected());
        assert!(integration.servers().is_empty());
    }
}
