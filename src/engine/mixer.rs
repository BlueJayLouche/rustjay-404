//! 8-channel video mixer with advanced blend modes and keying
//!
//! Composites up to 8 video inputs using shader-based mixing.
//! Supports chroma key (green/blue screen), luma key, and various blend modes.

use wgpu;
use std::sync::Arc;

/// Blend/Mix mode for channel mixing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MixMode {
    /// Normal alpha blending (src over dst)
    #[default]
    Normal,
    /// Additive blending (src + dst)
    Add,
    /// Multiply blending (src * dst)
    Multiply,
    /// Screen blending (1 - (1-src) * (1-dst))
    Screen,
    /// Overlay blending
    Overlay,
    /// Soft light blending
    SoftLight,
    /// Hard light blending
    HardLight,
    /// Difference blending
    Difference,
    /// Lighten only
    Lighten,
    /// Darken only
    Darken,
    /// Chroma key (green/blue screen)
    ChromaKey,
    /// Luma key (brightness-based)
    LumaKey,
}

impl MixMode {
    /// Convert to shader blend mode index
    pub fn to_shader_index(&self) -> u32 {
        match self {
            MixMode::Normal => 0,
            MixMode::Add => 1,
            MixMode::Multiply => 2,
            MixMode::Screen => 3,
            MixMode::Overlay => 4,
            MixMode::SoftLight => 5,
            MixMode::HardLight => 6,
            MixMode::Difference => 7,
            MixMode::Lighten => 8,
            MixMode::Darken => 9,
            MixMode::ChromaKey => 10,
            MixMode::LumaKey => 11,
        }
    }

    /// Get display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            MixMode::Normal => "Normal",
            MixMode::Add => "Add",
            MixMode::Multiply => "Multiply",
            MixMode::Screen => "Screen",
            MixMode::Overlay => "Overlay",
            MixMode::SoftLight => "Soft Light",
            MixMode::HardLight => "Hard Light",
            MixMode::Difference => "Difference",
            MixMode::Lighten => "Lighten",
            MixMode::Darken => "Darken",
            MixMode::ChromaKey => "Chroma Key",
            MixMode::LumaKey => "Luma Key",
        }
    }

    /// Get all available modes
    pub fn all_modes() -> &'static [MixMode] {
        &[
            MixMode::Normal,
            MixMode::Add,
            MixMode::Multiply,
            MixMode::Screen,
            MixMode::Overlay,
            MixMode::SoftLight,
            MixMode::HardLight,
            MixMode::Difference,
            MixMode::Lighten,
            MixMode::Darken,
            MixMode::ChromaKey,
            MixMode::LumaKey,
        ]
    }

    /// Check if this mode uses keying parameters
    pub fn is_keying(&self) -> bool {
        matches!(self, MixMode::ChromaKey | MixMode::LumaKey)
    }
}

/// Color space for video rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSpace {
    /// Standard RGB (no conversion needed)
    #[default]
    Rgb,
    /// YCoCg (requires shader conversion to RGB)
    YcoCg,
}

/// Keying parameters for chroma/luma key
#[derive(Debug, Clone, Copy)]
pub struct KeyParams {
    /// Key color for chroma key (RGB, 0-1)
    pub key_color: [f32; 3],
    /// Distance threshold for chroma key (0-1)
    pub threshold: f32,
    /// Edge smoothness (0-1)
    pub smoothness: f32,
    /// Invert the key (for luma key)
    pub invert: bool,
}

impl Default for KeyParams {
    fn default() -> Self {
        Self {
            key_color: [0.0, 1.0, 0.0], // Default green screen
            threshold: 0.3,
            smoothness: 0.1,
            invert: false,
        }
    }
}

/// Per-channel settings
#[derive(Debug, Clone)]
pub struct ChannelSettings {
    /// Whether channel is active
    pub enabled: bool,
    /// Mix mode for this channel
    pub mix_mode: MixMode,
    /// Opacity (0.0 - 1.0)
    pub opacity: f32,
    /// Keying parameters
    pub key_params: KeyParams,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mix_mode: MixMode::Normal,
            opacity: 1.0,
            key_params: KeyParams::default(),
        }
    }
}

/// Single channel input
#[derive(Debug, Clone, Default)]
pub struct ChannelInput {
    /// The texture to display
    pub texture: Option<Arc<wgpu::Texture>>,
    /// Mix mode
    pub mix_mode: MixMode,
    /// Opacity
    pub opacity: f32,
    /// Color space for proper rendering
    pub color_space: ColorSpace,
    /// Keying parameters
    pub key_params: KeyParams,
}

/// GPU uniform data for mix parameters (must match shader struct)
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct MixParamsUniform {
    blend_mode: u32,
    opacity: f32,
    key_color_r: f32,
    key_color_g: f32,
    key_color_b: f32,
    key_threshold: f32,
    key_smoothness: f32,
    luma_threshold: f32,
    luma_smoothness: f32,
    luma_invert: u32,
    color_space: u32,
    _padding: f32,
}

impl MixParamsUniform {
    fn new(channel: &ChannelInput) -> Self {
        let is_luma = matches!(channel.mix_mode, MixMode::LumaKey);
        let is_chroma = matches!(channel.mix_mode, MixMode::ChromaKey);
        
        Self {
            blend_mode: channel.mix_mode.to_shader_index(),
            opacity: channel.opacity,
            key_color_r: channel.key_params.key_color[0],
            key_color_g: channel.key_params.key_color[1],
            key_color_b: channel.key_params.key_color[2],
            key_threshold: if is_chroma { channel.key_params.threshold } else { 0.0 },
            key_smoothness: if is_chroma { channel.key_params.smoothness } else { 0.0 },
            luma_threshold: if is_luma { channel.key_params.threshold } else { 0.0 },
            luma_smoothness: if is_luma { channel.key_params.smoothness } else { 0.0 },
            luma_invert: if is_luma && channel.key_params.invert { 1 } else { 0 },
            color_space: match channel.color_space {
                ColorSpace::Rgb => 0,
                ColorSpace::YcoCg => 1,
            },
            _padding: 0.0,
        }
    }
}

/// 8-channel video mixer with shader-based mixing
pub struct VideoMixer {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    
    // Render pipeline
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    
    // Per-channel uniform buffers (one per channel to avoid race conditions)
    params_buffers: [wgpu::Buffer; 8],
    
    // Intermediate textures for multi-pass rendering (ping-pong)
    intermediate_textures: [wgpu::Texture; 2],
    intermediate_views: [wgpu::TextureView; 2],
    
    // Sampler
    sampler: wgpu::Sampler,
    
    // Channel inputs
    pub channels: [ChannelInput; 8],
}

impl VideoMixer {
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        // Create bind group layout - source texture, dest texture, sampler, params
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Mixer Bind Group Layout"),
            entries: &[
                // Source texture
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // Destination texture (accumulated result)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Mix parameters uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Mixer Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mixer Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("mixer_advanced.wgsl"))),
        });

        // Create single pipeline (blending is done in shader now)
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Mixer Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE), // No blending - we do it in shader
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // Create intermediate textures (ping-pong)
        let create_texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        let tex0 = create_texture("Mixer Buffer 0");
        let tex1 = create_texture("Mixer Buffer 1");
        
        let view0 = tex0.create_view(&wgpu::TextureViewDescriptor::default());
        let view1 = tex1.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        
        // Create per-channel uniform buffers
        let params_buffers = std::array::from_fn(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Mixer Params Buffer {}", i)),
                size: std::mem::size_of::<MixParamsUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });

        Self {
            width,
            height,
            format,
            pipeline,
            bind_group_layout,
            params_buffers,
            intermediate_textures: [tex0, tex1],
            intermediate_views: [view0, view1],
            sampler,
            channels: Default::default(),
        }
    }

    /// Create a bind group for textures and parameters
    fn create_bind_group(
        &self,
        device: &wgpu::Device,
        source_view: &wgpu::TextureView,
        dest_view: &wgpu::TextureView,
        channel_index: usize,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("Mixer Bind Group {}", channel_index)),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dest_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params_buffers[channel_index],
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        })
    }

    /// Render the mixed output to a target view
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        output_view: &wgpu::TextureView,
    ) {
        // Collect active channels
        let active_channels: Vec<_> = self.channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| ch.texture.is_some())
            .map(|(i, ch)| (i, ch))
            .collect();

        if active_channels.is_empty() {
            // Clear to black if no channels
            self.clear_output(encoder, output_view);
            return;
        }

        // We need 3 render passes to avoid texture usage conflicts:
        // 1. Clear intermediate[0] to black (acts as initial "destination")
        // 2. For each channel: blend source with accumulated result
        //    - Source = channel texture
        //    - Dest = intermediate texture (we ping-pong between 0 and 1)
        // 3. Final output is in one of the intermediates, copy to output
        
        // Ping-pong between intermediate textures
        // Clear both to ensure clean state
        self.clear_output(encoder, &self.intermediate_views[0]);
        self.clear_output(encoder, &self.intermediate_views[1]);
        
        let mut read_idx = 0;   // Index of texture to read from (accumulated result)
        let mut write_idx = 1;  // Index of texture to write to

        for (render_idx, (channel_idx, channel)) in active_channels.iter().enumerate() {
            let texture = channel.texture.as_ref().unwrap();
            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            
            // Write parameters for this channel to its dedicated buffer
            let params = MixParamsUniform::new(channel);
            queue.write_buffer(&self.params_buffers[*channel_idx], 0, bytemuck::cast_slice(&[params]));
            
            // Determine destination: output surface for last channel, intermediate otherwise
            let is_last = render_idx == active_channels.len() - 1;
            let dest_view: &wgpu::TextureView = if is_last {
                output_view
            } else {
                &self.intermediate_views[write_idx]
            };
            
            // Create bind group with this channel's dedicated uniform buffer
            let bind_group = self.create_bind_group(
                device, 
                &texture_view,
                &self.intermediate_views[read_idx],
                *channel_idx
            );
            
            // Render pass - LOAD (don't clear!) to preserve accumulated result
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&format!("Mixer Channel {} Pass", channel_idx)),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dest_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,  // PRESERVE existing content
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..6, 0..1);
            drop(render_pass);
            
            // Swap read/write indices for next iteration
            if !is_last {
                std::mem::swap(&mut read_idx, &mut write_idx);
            }
        }
    }

    /// Clear output to black
    fn clear_output(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Mixer Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
    }

    /// Set a channel's input
    pub fn set_channel(
        &mut self,
        index: usize,
        texture: Option<Arc<wgpu::Texture>>,
        mix_mode: MixMode,
        opacity: f32,
        color_space: ColorSpace,
    ) {
        if index >= 8 {
            return;
        }
        
        self.channels[index] = ChannelInput {
            texture,
            mix_mode,
            opacity: opacity.clamp(0.0, 1.0),
            color_space,
            key_params: KeyParams::default(),
        };
    }

    /// Set a channel with keying parameters
    pub fn set_channel_with_key(
        &mut self,
        index: usize,
        texture: Option<Arc<wgpu::Texture>>,
        mix_mode: MixMode,
        opacity: f32,
        color_space: ColorSpace,
        key_params: KeyParams,
    ) {
        if index >= 8 {
            return;
        }
        
        self.channels[index] = ChannelInput {
            texture,
            mix_mode,
            opacity: opacity.clamp(0.0, 1.0),
            color_space,
            key_params,
        };
    }

    /// Enable/disable a channel
    pub fn set_channel_enabled(&mut self, index: usize, enabled: bool) {
        if index >= 8 {
            return;
        }
        
        if !enabled {
            self.channels[index].texture = None;
        }
    }

    /// Set channel mix mode
    pub fn set_channel_mix_mode(&mut self, index: usize, mix_mode: MixMode) {
        if index >= 8 {
            return;
        }
        self.channels[index].mix_mode = mix_mode;
    }

    /// Set channel opacity
    pub fn set_channel_opacity(&mut self, index: usize, opacity: f32) {
        if index >= 8 {
            return;
        }
        self.channels[index].opacity = opacity.clamp(0.0, 1.0);
    }

    /// Set channel keying parameters
    pub fn set_channel_key_params(&mut self, index: usize, key_params: KeyParams) {
        if index >= 8 {
            return;
        }
        self.channels[index].key_params = key_params;
    }

    /// Resize the mixer
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;

        let create_texture = |label: &str| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };

        self.intermediate_textures[0] = create_texture("Mixer Buffer 0");
        self.intermediate_textures[1] = create_texture("Mixer Buffer 1");
        self.intermediate_views[0] = self.intermediate_textures[0].create_view(&wgpu::TextureViewDescriptor::default());
        self.intermediate_views[1] = self.intermediate_textures[1].create_view(&wgpu::TextureViewDescriptor::default());
    }
}
