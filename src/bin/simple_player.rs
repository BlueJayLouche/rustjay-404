// Simple HAP video player with smooth reverse playback
// Usage: cargo run --bin simple_player -- <video_path> [--speed <speed>] [--loop]

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::{Duration, Instant};
use clap::Parser;

/// Simple HAP video player
#[derive(Parser, Debug)]
#[command(name = "simple_player")]
struct Args {
    /// Video file path
    video: PathBuf,
    
    /// Playback speed (negative for reverse)
    #[arg(short, long, default_value = "1.0")]
    speed: f32,
    
    /// Enable looping
    #[arg(short, long)]
    loop_playback: bool,
}

/// Frame buffer shared between decoder thread and render thread
struct FrameBuffer {
    frames: VecDeque<(u32, Vec<u8>)>, // (frame_num, raw rgba data)
    max_size: usize,
    current_frame: u32,
    speed: f32,
    loop_enabled: bool,
    frame_count: u32,
}

impl FrameBuffer {
    fn new(max_size: usize, frame_count: u32, speed: f32, loop_enabled: bool) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_size),
            max_size,
            current_frame: if speed >= 0.0 { 0 } else { frame_count - 1 },
            speed,
            loop_enabled,
            frame_count,
        }
    }
    
    /// Get the next frame to display and advance playback position
    fn get_display_frame(&mut self) -> Option<(u32, Vec<u8>)> {
        let target = self.current_frame;
        
        // Look for exact match
        if let Some((idx, (_, data))) = self.frames.iter().enumerate().find(|(_, (f, _))| *f == target) {
            let result = data.clone();
            // Remove this frame and all before it
            self.frames.drain(0..=idx);
            
            // Advance position
            self.advance_position();
            
            return Some((target, result));
        }
        
        None
    }
    
    fn advance_position(&mut self) {
        let next = self.current_frame as f32 + self.speed;
        
        if self.speed >= 0.0 {
            // Forward playback
            if next >= self.frame_count as f32 {
                if self.loop_enabled {
                    self.current_frame = 0;
                } else {
                    self.current_frame = self.frame_count - 1;
                }
            } else {
                self.current_frame = next as u32;
            }
        } else {
            // Reverse playback
            if next < 0.0 {
                if self.loop_enabled {
                    self.current_frame = self.frame_count - 1;
                } else {
                    self.current_frame = 0;
                }
            } else {
                self.current_frame = next as u32;
            }
        }
    }
    
    /// Add a decoded frame to the buffer
    fn add_frame(&mut self, frame_num: u32, data: Vec<u8>) {
        if self.frames.len() >= self.max_size {
            self.frames.pop_front();
        }
        self.frames.push_back((frame_num, data));
    }
    
    /// Get the frame number we need to decode next
    fn get_needed_frame(&self) -> u32 {
        let offset = if self.speed >= 0.0 { 
            self.frames.len() as i32 
        } else { 
            -(self.frames.len() as i32) 
        };
        
        let needed = self.current_frame as i32 + offset;
        needed.clamp(0, self.frame_count as i32 - 1) as u32
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();
    
    println!("Simple HAP Player");
    println!("Video: {:?}", args.video);
    println!("Speed: {:.1}x", args.speed);
    println!("Loop: {}", if args.loop_playback { "ON" } else { "OFF" });
    println!();
    
    // Create window
    let event_loop = winit::event_loop::EventLoop::new()?;
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title(format!("Simple Player - {:.1}x", args.speed))
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
    let window = std::sync::Arc::new(event_loop.create_window(window_attrs)?);
    
    // Create wgpu instance
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let surface = instance.create_surface(window.clone())?;
    
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .map_err(|_| anyhow::anyhow!("No adapter found"))?;
    
    // Check for HAP support (BC texture compression)
    if !adapter.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        return Err(anyhow::anyhow!(
            "GPU does not support BC texture compression. HAP playback requires TEXTURE_COMPRESSION_BC feature."
        ));
    }

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        },
    ))?;
    
    let surface_caps = surface.get_capabilities(&adapter);
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_caps.formats[0],
        width: window.inner_size().width,
        height: window.inner_size().height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);
    
    // Load video info first
    let (width, height, fps, frame_count) = probe_video(&args.video)?;
    println!("Video: {}x{} @ {:.2}fps, {} frames", width, height, fps, frame_count);
    
    // Create shared frame buffer
    let frame_buffer = Arc::new(Mutex::new(FrameBuffer::new(
        16, // buffer size
        frame_count,
        args.speed,
        args.loop_playback,
    )));
    
    // Spawn decoder thread
    let video_path = args.video.clone();
    let buffer_clone = frame_buffer.clone();
    
    std::thread::spawn(move || {
        decoder_thread(video_path, buffer_clone, width, height);
    });
    
    // Simple fullscreen texture shader
    const SHADER: &str = r#"
        @vertex
        fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
            var pos = array<vec2<f32>, 6>(
                vec2<f32>(-1.0, -1.0),
                vec2<f32>( 1.0, -1.0),
                vec2<f32>(-1.0,  1.0),
                vec2<f32>(-1.0,  1.0),
                vec2<f32>( 1.0, -1.0),
                vec2<f32>( 1.0,  1.0),
            );
            return vec4<f32>(pos[vertex_index], 0.0, 1.0);
        }

        @group(0) @binding(0)
        var t_diffuse: texture_2d<f32>;
        @group(0) @binding(1)
        var s_diffuse: sampler;

        @fragment
        fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
            let uv = frag_coord.xy / vec2<f32>(textureDimensions(t_diffuse));
            return textureSample(t_diffuse, s_diffuse, uv);
        }
    "#;

    // Create render pipeline
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Simple Player Shader"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(SHADER)),
    });
    
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Texture Bind Group Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    
    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
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
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    
    // Playback state
    let mut is_playing = true;
    let mut last_frame_time = Instant::now();
    let frame_duration = Duration::from_secs_f32(1.0 / fps);
    
    // Current texture cache
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    
    println!("Controls: Space=Play/Pause, Esc=Quit");
    println!();
    
    // Main loop
    event_loop.run(move |event, target| {
        target.set_control_flow(winit::event_loop::ControlFlow::Poll);
        
        match event {
            winit::event::Event::WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::CloseRequested => target.exit(),
                winit::event::WindowEvent::KeyboardInput { event, .. } => {
                    use winit::keyboard::{Key, NamedKey};
                    if event.state == winit::event::ElementState::Pressed {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => target.exit(),
                            Key::Named(NamedKey::Space) => {
                                is_playing = !is_playing;
                                println!("Playback: {}", if is_playing { "PLAYING" } else { "PAUSED" });
                            }
                            _ => {}
                        }
                    }
                }
                winit::event::WindowEvent::Resized(size) => {
                    config.width = size.width;
                    config.height = size.height;
                    surface.configure(&device, &config);
                }
                _ => {}
            },
            winit::event::Event::AboutToWait => {
                // Update playback timing
                if is_playing && last_frame_time.elapsed() >= frame_duration {
                    last_frame_time = Instant::now();
                    
                    // Try to get frame from buffer
                    let mut buffer = frame_buffer.lock().unwrap();
                    
                    if let Some((frame_num, frame_data)) = buffer.get_display_frame() {
                        // Create texture from raw data
                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            label: Some(&format!("Frame {}", frame_num)),
                            size: wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            &frame_data,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(width * 4),
                                rows_per_image: Some(height),
                            },
                            wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                        );
                        
                        current_texture = Some(Arc::new(texture));
                    } else {
                        // Buffer underrun
                        if current_texture.is_none() {
                            println!("Buffer underrun!");
                        }
                        // Hold last frame
                    }
                    
                    drop(buffer); // Release lock before rendering
                }
                
                // Render current frame
                if let Some(ref texture) = current_texture {
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(
                                    &texture.create_view(&wgpu::TextureViewDescriptor::default())
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                        label: Some("Frame Bind Group"),
                    });
                    
                    let output = surface.get_current_texture().unwrap();
                    let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    
                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Render Encoder"),
                    });
                    
                    {
                        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Render Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
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
                        
                        render_pass.set_pipeline(&render_pipeline);
                        render_pass.set_bind_group(0, &bind_group, &[]);
                        render_pass.draw(0..6, 0..1);
                    }
                    
                    queue.submit([encoder.finish()]);
                    output.present();
                }
                
                window.request_redraw();
            }
            _ => {}
        }
    })?;
    
    Ok(())
}

/// Probe video file for metadata
fn probe_video(path: &PathBuf) -> anyhow::Result<(u32, u32, f32, u32)> {
    let output = std::process::Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height,r_frame_rate,nb_frames",
            "-of", "csv=p=0",
            path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run ffprobe: {}", e))?;
    
    if !output.status.success() {
        return Err(anyhow::anyhow!("ffprobe failed"));
    }
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = output_str.trim().split(',').collect();
    
    let width = parts[0].parse::<u32>().unwrap_or(1920);
    let height = parts[1].parse::<u32>().unwrap_or(1080);
    
    let fps = if let Some(fps_str) = parts.get(2) {
        if fps_str.contains('/') {
            let fps_parts: Vec<&str> = fps_str.split('/').collect();
            fps_parts[0].parse::<f32>().unwrap_or(30.0) / 
            fps_parts[1].parse::<f32>().unwrap_or(1.0)
        } else {
            fps_str.parse::<f32>().unwrap_or(30.0)
        }
    } else {
        30.0
    };
    
    let frame_count = if let Some(fc_str) = parts.get(3) {
        fc_str.parse::<u32>().unwrap_or(0)
    } else {
        0
    };
    
    let frame_count = if frame_count == 0 {
        // Estimate from duration
        let dur_output = std::process::Command::new("ffprobe")
            .args(&[
                "-v", "error",
                "-show_entries", "format=duration",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output();
        
        if let Ok(dur) = dur_output {
            let duration = String::from_utf8_lossy(&dur.stdout)
                .trim()
                .parse::<f32>()
                .unwrap_or(0.0);
            (duration * fps) as u32
        } else {
            30
        }
    } else {
        frame_count
    };
    
    Ok((width, height, fps, frame_count))
}

/// Decoder thread - continuously decodes frames to fill buffer
fn decoder_thread(
    video_path: PathBuf,
    frame_buffer: Arc<Mutex<FrameBuffer>>,
    width: u32,
    height: u32,
) {
    use std::io::Read;
    
    let frame_size = (width * height * 4) as usize;
    let mut ffmpeg: Option<std::process::Child> = None;
    let mut reader: Option<std::io::BufReader<std::process::ChildStdout>> = None;
    let mut stream_position: u32 = 0;
    
    // Probe fps once at startup
    let fps = {
        let (_, _, f, _) = probe_video(&video_path).unwrap_or((1920, 1080, 30.0, 30));
        f.max(1.0)
    };
    
    loop {
        // Get the frame we need to decode
        let needed_frame = {
            let buffer = frame_buffer.lock().unwrap();
            buffer.get_needed_frame()
        };
        
        // Check if we need to restart ffmpeg
        let needs_restart = match (&ffmpeg, &reader) {
            (None, _) => true,
            _ => {
                // Check if needed frame is outside current stream range
                let buffer = frame_buffer.lock().unwrap();
                let is_forward = buffer.speed >= 0.0;
                
                if is_forward {
                    needed_frame < stream_position || 
                    needed_frame > stream_position + 16
                } else {
                    needed_frame > stream_position || 
                    needed_frame + 16 < stream_position
                }
            }
        };
        
        if needs_restart {
            // Kill old ffmpeg
            if let Some(mut child) = ffmpeg.take() {
                let _ = child.kill();
            }
            reader = None;
            
            // For reverse playback, start BEFORE the needed frame
            let buffer = frame_buffer.lock().unwrap();
            let start_frame = if buffer.speed >= 0.0 {
                needed_frame
            } else {
                // For reverse, decode a window ending at needed_frame
                if needed_frame > 16 { needed_frame - 16 } else { 0 }
            };
            drop(buffer);
            
            let timestamp = start_frame as f32 / fps;
            
            log::debug!("Decoder restarting at frame {} (needed: {})", start_frame, needed_frame);
            
            match std::process::Command::new("ffmpeg")
                .args(&[
                    "-ss", &format!("{:.3}", timestamp),
                    "-i", video_path.to_str().unwrap(),
                    "-f", "rawvideo",
                    "-pix_fmt", "rgba",
                    "-s", &format!("{}x{}", width, height),
                    "-threads", "4",
                    "-an", "-sn",
                    "-",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    stream_position = start_frame;
                    reader = Some(std::io::BufReader::with_capacity(
                        frame_size * 8,
                        child.stdout.take().unwrap()
                    ));
                    ffmpeg = Some(child);
                }
                Err(e) => {
                    log::error!("Failed to start ffmpeg: {}", e);
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        }
        
        // Read and decode frames
        if let Some(ref mut r) = reader {
            let mut frame_data = vec![0u8; frame_size];
            
            match r.read_exact(&mut frame_data) {
                Ok(_) => {
                    // Add raw frame data to buffer
                    let mut buffer = frame_buffer.lock().unwrap();
                    buffer.add_frame(stream_position, frame_data);
                    stream_position += 1;
                }
                Err(e) => {
                    log::debug!("Read error (likely end of stream): {}", e);
                    ffmpeg = None;
                    reader = None;
                }
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
