//! v4l2loopback output implementation for Linux
//!
//! v4l2loopback is a kernel module that creates virtual video devices.
//! It allows applications to send video to other apps that expect V4L2 input
//! (like OBS, Zoom, Chrome, etc.)

use super::InterAppVideo;
use wgpu;

/// v4l2loopback video output
///
/// Writes frames to a v4l2loopback device, making them available as a
/// virtual webcam to other applications.
pub struct V4l2Output {
    name: String,
    width: u32,
    height: u32,
    device_path: String,
    // TODO: v4l2 device handle
    // TODO: Buffer pool
}

impl V4l2Output {
    /// Create a new v4l2loopback output
    /// 
    /// # Arguments
    /// * `name` - Name for the virtual device
    /// * `device` - wgpu device (for texture reading)
    /// * `width` - Output width
    /// * `height` - Output height
    pub fn new(name: &str, device: &wgpu::Device, width: u32, height: u32) -> Self {
        let device_path = Self::find_or_create_device();
        
        log::info!(
            "Creating v4l2loopback output '{}' on {} ({}x{})",
            name, device_path, width, height
        );
        
        // TODO:
        // 1. Open the v4l2loopback device
        // 2. Set video format (RGB24 or YUYV)
        // 3. Allocate output buffers
        // 4. Start streaming
        
        Self {
            name: name.to_string(),
            width,
            height,
            device_path,
        }
    }
    
    /// Find an available v4l2loopback device or create one
    fn find_or_create_device() -> String {
        // Common paths for v4l2loopback devices
        for i in 0..10 {
            let path = format!("/dev/video{}", i);
            if Self::is_v4l2loopback_device(&path) {
                return path;
            }
        }
        
        // Default to video10 (often created by v4l2loopback-dkms)
        "/dev/video10".to_string()
    }
    
    /// Check if a device is a v4l2loopback device
    fn is_v4l2loopback_device(path: &str) -> bool {
        // TODO: Use v4l2 API to query device capabilities
        // Check for V4L2_CAP_VIDEO_OUTPUT and driver name
        false
    }
    
    /// List available v4l2loopback devices
    pub fn list_devices() -> Vec<(String, String)> {
        // Returns Vec<(device_path, device_name)>
        // TODO: Scan /dev/video* and filter for loopback devices
        Vec::new()
    }
    
    /// Check if v4l2loopback is available
    pub fn is_available() -> bool {
        // Check if any v4l2loopback device exists
        std::path::Path::new("/dev/video10").exists()
    }
}

impl InterAppVideo for V4l2Output {
    fn publish_frame(&mut self, _texture: &wgpu::Texture, _device: &wgpu::Device, _queue: &wgpu::Queue) {
        // TODO:
        // 1. Copy texture to CPU-readable buffer
        // 2. Convert format if needed (RGBA -> RGB24 or YUYV)
        // 3. Write to v4l2 device
        // 4. Queue buffer for output
        
        log::trace!(
            "Publishing frame to v4l2loopback '{}'",
            self.device_path
        );
    }
    
    fn receive_frame(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue) -> Option<wgpu::Texture> {
        // Output implementation doesn't receive frames
        None
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn is_available() -> bool {
        Self::is_available()
    }
}

impl Drop for V4l2Output {
    fn drop(&mut self) {
        log::info!("Destroying v4l2loopback output '{}'", self.name);
        // TODO: Stop streaming and close device
    }
}

/// v4l2loopback input for receiving video (if another app is writing to the device)
pub struct V4l2Input {
    device_path: String,
    // TODO: v4l2 capture handle
}

impl V4l2Input {
    /// Create a new v4l2 input
    pub fn new(device_path: &str) -> Self {
        log::info!("Creating v4l2 input from '{}'", device_path);
        
        // TODO:
        // 1. Open device
        // 2. Set capture format
        // 3. Allocate capture buffers
        // 4. Start capture
        
        Self {
            device_path: device_path.to_string(),
        }
    }
    
    /// Try to receive a new frame
    pub fn try_receive(&mut self, device: &wgpu::Device) -> Option<wgpu::Texture> {
        // TODO: Dequeue buffer from v4l2
        // TODO: Create wgpu texture from buffer data
        None
    }
}

// Implementation Notes:
//
// 1. v4l2loopback Setup:
//    Users need to have v4l2loopback kernel module loaded:
//    ```
//    sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="Rusty-404"
//    ```
//    Or install v4l2loopback-dkms for persistent setup.
//
// 2. Rust Crates:
//    - `v4l2-rs`: V4L2 bindings for Rust
//    - Alternative: Use `nokhwa` (already a dependency) which has V4L2 backend
//
// 3. Format Conversion:
//    v4l2loopback typically wants:
//    - RGB24 (3 bytes per pixel)
//    - YUYV/YUY2 (YUV 4:2:2)
//    - MJPEG (compressed)
//    
//    wgpu textures are RGBA8. We'll need to convert:
//    - RGBA -> RGB (drop alpha)
//    - Or RGBA -> YUYV (color space conversion)
//
// 4. Performance:
//    Unlike Syphon/Spout (GPU-to-GPU), v4l2loopback requires CPU readback
//    from GPU texture -> system memory -> kernel driver.
//    This is slower but more compatible with Linux apps.
//
// 5. Alternative: pipewire:
//    Modern Linux systems are moving to PipeWire for video.
//    We might want to support PipeWire video sink as well.
