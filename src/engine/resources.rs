//! GPU resources (textures, buffers)

use wgpu;

/// Manages GPU resources for the engine
pub struct EngineResources {
    // Ping-pong buffers for effects
    pub buffer_a: wgpu::Texture,
    pub buffer_b: wgpu::Texture,
    
    // Feedback buffer
    pub feedback_buffer: wgpu::Texture,
    
    // Delay buffer
    pub delay_buffer: Vec<wgpu::Texture>,
}

impl EngineResources {
    pub fn new(_device: &wgpu::Device, _width: u32, _height: u32) -> Self {
        // TODO: Create textures
        let descriptor = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1280,
                height: 720,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        
        Self {
            buffer_a: _device.create_texture(&descriptor),
            buffer_b: _device.create_texture(&descriptor),
            feedback_buffer: _device.create_texture(&descriptor),
            delay_buffer: vec![],
        }
    }
}
