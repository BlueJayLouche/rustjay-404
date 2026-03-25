//! ImGui context and renderer setup

use imgui::{Context, FontConfig, FontSource};
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::sync::Arc;
use winit::window::Window;

/// Manages ImGui context, platform, and renderer
pub struct ImGuiContext {
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
    pub last_frame_time: std::time::Instant,
}

impl ImGuiContext {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window: &Window,
        surface_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        let mut imgui = Context::create();

        // Persist window layout between restarts
        if let Some(config_dir) = dirs::config_dir() {
            let app_dir = config_dir.join("rustjay404");
            let _ = std::fs::create_dir_all(&app_dir);
            let ini_path = app_dir.join("imgui.ini");
            imgui.set_ini_filename(Some(ini_path));
        }

        // Setup platform - use Locked HiDpi mode to prevent automatic scaling
        let mut platform = WinitPlatform::init(&mut imgui);
        platform.attach_window(imgui.io_mut(), window, HiDpiMode::Locked(1.0));

        // Configure fonts - use fixed font size
        imgui.fonts().add_font(&[FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels: 13.0,
                ..FontConfig::default()
            }),
        }]);

        // No scaling - we're handling HiDPI at the renderer level
        imgui.io_mut().font_global_scale = 1.0;
        
        // Set display framebuffer scale to 1.0 (physical pixels match logical pixels)
        imgui.io_mut().display_framebuffer_scale = [1.0, 1.0];

        // Create renderer
        let renderer_config = RendererConfig {
            texture_format: surface_format,
            ..Default::default()
        };

        let renderer = Renderer::new(&mut imgui, device, queue, renderer_config);

        Ok(Self {
            imgui,
            platform,
            renderer,
            last_frame_time: std::time::Instant::now(),
        })
    }

    /// Handle window events
    pub fn handle_event<T>(&mut self, window: &Window, event: &winit::event::Event<T>) {
        self.platform
            .handle_event(self.imgui.io_mut(), window, event);
    }

    /// Prepare for a new frame
    pub fn prepare_frame(&mut self, window: &Window) {
        let now = std::time::Instant::now();
        let delta = now - self.last_frame_time;
        self.last_frame_time = now;

        self.imgui.io_mut().update_delta_time(delta);
        
        // Set display size to physical pixel dimensions
        let size = window.inner_size();
        self.imgui.io_mut().display_size = [size.width as f32, size.height as f32];
        
        // Keep framebuffer scale at 1.0 (physical pixels)
        self.imgui.io_mut().display_framebuffer_scale = [1.0, 1.0];
        
        self.platform
            .prepare_frame(self.imgui.io_mut(), window)
            .expect("Failed to prepare frame");
    }

    /// Render the UI
    pub fn render(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        ui_builder: impl FnOnce(&imgui::Ui),
    ) -> anyhow::Result<()> {
        // Build UI
        let ui = self.imgui.frame();
        ui_builder(&ui);

        // Prepare render
        self.platform.prepare_render(&ui, window);

        // Get draw data from context
        let draw_data = self.imgui.render();

        // Render
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ImGui Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Preserve existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        self.renderer
            .render(draw_data, queue, device, &mut render_pass)?;

        Ok(())
    }

    /// Resize the renderer - uses physical pixel dimensions
    pub fn resize(&mut self, width: u32, height: u32, _scale_factor: f64) {
        // Width and height should be physical pixel dimensions
        self.imgui.io_mut().display_size = [width as f32, height as f32];
        // Always use 1.0 for framebuffer scale to prevent scissor rect issues
        self.imgui.io_mut().display_framebuffer_scale = [1.0, 1.0];
    }

    /// Create a preview texture that can be displayed in ImGui and updated via GPU copy
    pub fn create_preview_texture(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> imgui::TextureId {
        let config = imgui_wgpu::TextureConfig {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            label: Some("Preview Texture"),
            format: Some(format),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            ..Default::default()
        };
        let texture = imgui_wgpu::Texture::new(device, &self.renderer, config);
        self.renderer.textures.insert(texture)
    }

    /// Get the underlying wgpu texture for an ImGui texture ID (for GPU copies)
    pub fn get_preview_texture(&self, id: imgui::TextureId) -> Option<&wgpu::Texture> {
        self.renderer.textures.get(id).map(|t| t.texture())
    }
}
