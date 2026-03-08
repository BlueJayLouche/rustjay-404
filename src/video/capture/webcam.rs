//! Webcam capture using nokhwa

use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use std::time::Instant;

/// Information about a camera device
#[derive(Clone, Debug)]
pub struct CameraInfo {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub is_virtual: bool,
}

/// A captured frame from webcam
#[derive(Clone, Debug)]
pub struct CapturedFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp: Instant,
}

/// Webcam capture handle
pub struct WebcamCapture {
    camera: Option<Camera>,
    resolution: (u32, u32),
}

impl WebcamCapture {
    /// Create new webcam capture (but don't start yet)
    pub fn new() -> Self {
        Self {
            camera: None,
            resolution: (640, 480),
        }
    }
    
    /// Initialize and start the webcam
    pub fn start(&mut self, device_index: u32) -> anyhow::Result<()> {
        // Find available cameras
        let cameras = nokhwa::query(nokhwa::utils::ApiBackend::Auto)?;
        
        if cameras.is_empty() {
            return Err(anyhow::anyhow!("No cameras found"));
        }
        
        let camera_info = if (device_index as usize) < cameras.len() {
            &cameras[device_index as usize]
        } else {
            &cameras[0]
        };
        
        log::info!("Opening webcam: {}", camera_info.human_name());
        
        // Request a format
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        
        let index = CameraIndex::Index(device_index);
        let mut camera = Camera::new(index, requested)?;
        camera.open_stream()?;
        
        // Get actual resolution
        let actual_res = camera.resolution();
        self.resolution = (actual_res.width(), actual_res.height());
        
        self.camera = Some(camera);
        
        Ok(())
    }
    
    /// Get the current/latest frame
    pub fn get_frame(&mut self) -> Option<CapturedFrame> {
        if let Some(ref mut camera) = self.camera {
            match camera.frame() {
                Ok(frame) => {
                    // Decode frame to RGB
                    let decoded = frame.decode_image::<RgbFormat>().ok()?;
                    let width = decoded.width();
                    let height = decoded.height();
                    let data = decoded.into_raw();
                    
                    Some(CapturedFrame {
                        data,
                        width,
                        height,
                        timestamp: Instant::now(),
                    })
                }
                Err(e) => {
                    log::error!("Failed to capture frame: {}", e);
                    None
                }
            }
        } else {
            None
        }
    }
    
    /// Get current resolution
    pub fn resolution(&self) -> (u32, u32) {
        self.resolution
    }
    
    /// Stop the webcam
    pub fn stop(&mut self) {
        if let Some(mut camera) = self.camera.take() {
            let _ = camera.stop_stream();
        }
    }
}

impl Drop for WebcamCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// List available webcams
pub fn list_cameras() -> anyhow::Result<Vec<(u32, String)>> {
    let cameras = nokhwa::query(nokhwa::utils::ApiBackend::Auto)?;
    Ok(cameras
        .into_iter()
        .enumerate()
        .map(|(i, info)| (i as u32, info.human_name().to_string()))
        .collect())
}

// Helper type for RGB format
use nokhwa::pixel_format::RgbFormat;
