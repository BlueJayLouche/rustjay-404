use rustjay_404::app::App;
use rustjay_404::video::encoder::{HapEncoder, HapEncoderConfig, HapEncodeFormat, GpuMode, batch_encode, convert_capture_to_hap};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rustjay-404")]
#[command(about = "High-performance video sampler inspired by SP-404")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the video sampler (default)
    Run {
        /// Sample library path
        #[arg(short, long)]
        library: Option<PathBuf>,
        
        /// Simple mode: direct HAP playback without sampler UI
        #[arg(long)]
        simple: bool,
        
        /// Video file to play in simple mode
        #[arg(short, long, requires = "simple")]
        file: Option<PathBuf>,
        
        /// Loop playback in simple mode
        #[arg(long, requires = "simple")]
        loop_playback: bool,
    },
    
    /// Encode video to HAP format
    Encode {
        /// Input video file(s)
        #[arg(required = true)]
        input: Vec<PathBuf>,
        
        /// Output directory
        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
        
        /// HAP format: dxt1, dxt5, dxt5-ycocg, bc6h
        #[arg(short, long, default_value = "dxt5")]
        format: String,
        
        /// Target width
        #[arg(short = 'W', long)]
        width: Option<u32>,
        
        /// Target height
        #[arg(short = 'H', long)]
        height: Option<u32>,
        
        /// Target FPS
        #[arg(short = 'r', long)]
        fps: Option<u32>,
        
        /// Number of chunks for multi-threaded decoding
        #[arg(short, long, default_value = "1")]
        chunks: u32,
    },
    
    /// Convert captured video to HAP
    Convert {
        /// Input video file
        input: PathBuf,
        
        /// Output HAP file
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// HAP format: dxt1, dxt5, dxt5-ycocg, bc6h
        #[arg(short, long, default_value = "dxt5")]
        format: String,
    },
    
    /// Check system for HAP encoding support
    Check,
    
    /// Test MIDI input - logs events to file for 10 seconds
    MidiTest {
        /// Output log file path
        #[arg(short, long, default_value = "midi_test.log")]
        output: PathBuf,
        
        /// Test duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
    },
}

/// Run simple HAP player mode
fn run_simple_player(video_path: &PathBuf, loop_playback: bool) -> anyhow::Result<()> {
    use std::borrow::Cow;
    use std::sync::Arc;
    use std::time::Instant;
    use winit::{
        event::{Event, WindowEvent},
        event_loop::{ControlFlow, EventLoop},
    };

    println!("Simple HAP Player");
    println!("=================");
    println!("File: {:?}", video_path);
    println!("Loop: {}", loop_playback);
    println!();

    // Create event loop and window
    let event_loop = EventLoop::new()?;
    let window_attrs = winit::window::WindowAttributes::default()
        .with_title("Simple HAP Player")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
    
    let window = Arc::new(event_loop.create_window(window_attrs)?);

    // Initialize wgpu
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let surface = instance.create_surface(window.clone())?;
    
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .map_err(|_| anyhow::anyhow!("Failed to find suitable adapter"))?;

    println!("Using adapter: {:?}", adapter.get_info());

    // Check for HAP support (BC texture compression)
    if !adapter.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        return Err(anyhow::anyhow!(
            "GPU does not support BC texture compression. HAP playback requires TEXTURE_COMPRESSION_BC feature."
        ));
    }

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Simple Player Device"),
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }
    ))?;

    let device = Arc::new(device);
    let queue = Arc::new(queue);

    // Configure surface
    let surface_caps = surface.get_capabilities(&adapter);
    let surface_format = surface_caps.formats.iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(surface_caps.formats[0]);

    let window_size = window.inner_size();
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: window_size.width,
        height: window_size.height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    // Load HAP video using BoundaryCachedDecoder for smooth looping
    println!("Loading HAP video: {:?}", video_path);
    use rustjay_404::video::decoder::boundary_cached::BoundaryCachedDecoder;
    use rustjay_404::sampler::sample::VideoDecoder;
    
    let mut decoder = BoundaryCachedDecoder::new(video_path, device.clone(), queue.clone())?;
    let (width, height) = decoder.resolution();
    let fps = decoder.fps();
    let frame_count = decoder.frame_count();
    
    println!("BoundaryCachedDecoder: {}x{} @ {}fps, {} frames", width, height, fps, frame_count);

    // Create a simple texture renderer
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Simple Player Shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("engine/mixer.wgsl"))),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Texture Bind Group Layout"),
        entries: &[
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

    // Create sampler
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
    let mut current_frame: u32 = 0;
    let mut last_frame_time = Instant::now();
    let frame_duration = std::time::Duration::from_secs_f32(1.0 / fps);
    let mut loop_enabled = loop_playback;

    println!();
    println!("Controls:");
    println!("  Space - Play/Pause");
    println!("  Left/Right - Step frame");
    println!("  L - Toggle loop");
    println!("  R - Reset to start");
    println!("  Escape - Quit");
    println!();

    // Main loop
    event_loop.run(move |event, target| {
        target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::KeyboardInput { event, .. } => {
                    use winit::keyboard::{Key, NamedKey};
                    if event.state == winit::event::ElementState::Pressed {
                        match &event.logical_key {
                            Key::Named(NamedKey::Escape) => {
                                target.exit();
                            }
                            Key::Named(NamedKey::Space) => {
                                is_playing = !is_playing;
                                println!("Playback: {}", if is_playing { "PLAYING" } else { "PAUSED" });
                            }
                            Key::Named(NamedKey::ArrowLeft) => {
                                is_playing = false;
                                current_frame = current_frame.saturating_sub(1);
                                println!("Frame: {}", current_frame);
                            }
                            Key::Named(NamedKey::ArrowRight) => {
                                is_playing = false;
                                current_frame = (current_frame + 1).min(frame_count.saturating_sub(1));
                                println!("Frame: {}", current_frame);
                            }
                            Key::Character(c) if c.to_lowercase() == "l" => {
                                loop_enabled = !loop_enabled;
                                println!("Loop: {}", if loop_enabled { "ON" } else { "OFF" });
                            }
                            Key::Character(c) if c.to_lowercase() == "r" => {
                                current_frame = 0;
                                println!("Reset to frame 0");
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::Resized(size) => {
                    config.width = size.width;
                    config.height = size.height;
                    surface.configure(&device, &config);
                }
                _ => {}
            },
            Event::AboutToWait => {
                // Update playback
                if is_playing {
                    let now = Instant::now();
                    if now - last_frame_time >= frame_duration {
                        last_frame_time = now;
                        current_frame += 1;
                        
                        if current_frame >= frame_count {
                            if loop_enabled {
                                current_frame = 0;
                            } else {
                                current_frame = frame_count.saturating_sub(1);
                                is_playing = false;
                                println!("Playback finished");
                            }
                        }
                    }
                }

                // Get frame texture from cached decoder
                if let Some(frame_texture) = decoder.get_frame(current_frame) {
                    // Create bind group for this frame
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(
                                    &frame_texture.create_view(&wgpu::TextureViewDescriptor::default())
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                        label: Some("Frame Bind Group"),
                    });

                    // Render
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

                    queue.submit(std::iter::once(encoder.finish()));
                    output.present();
                }

                window.request_redraw();
            }
            _ => {}
        }
    })?;

    Ok(())
}

/// Test MIDI input and log events to file
fn run_midi_test(output_path: &PathBuf, duration_secs: u64) -> anyhow::Result<()> {
    use std::io::Write;
    use std::time::{Duration, Instant};
    
    println!("MIDI Test");
    println!("=========");
    println!("Logging MIDI events to: {}", output_path.display());
    println!("Duration: {} seconds", duration_secs);
    println!("Press pads on your MIDI controller now...");
    println!();
    
    // Create log file
    let mut log_file = std::fs::File::create(output_path)?;
    writeln!(log_file, "MIDI Test Log - {}", chrono::Local::now())?;
    writeln!(log_file, "===========================")?;
    writeln!(log_file)?;
    
    // Setup MIDI
    let (tx, rx) = std::sync::mpsc::channel();
    let mut midi = rustjay_404::input::midi::MidiController::new(tx)?;
    
    // Try to connect
    match midi.auto_connect() {
        Ok(()) => {
            let port_name = midi.current_port().unwrap_or("Unknown");
            println!("Connected to: {}", port_name);
            writeln!(log_file, "Connected to: {}", port_name)?;
        }
        Err(e) => {
            println!("Failed to connect to MIDI: {}", e);
            println!("Available ports:");
            writeln!(log_file, "Failed to connect: {}", e)?;
            for (idx, name) in rustjay_404::input::midi::MidiController::list_ports()? {
                println!("  [{}] {}", idx, name);
                writeln!(log_file, "  [{}] {}", idx, name)?;
            }
            return Err(e);
        }
    }
    
    // Collect events for duration
    let start = Instant::now();
    let mut event_count = 0;
    
    while start.elapsed() < Duration::from_secs(duration_secs) {
        // Print countdown every second
        let elapsed = start.elapsed().as_secs();
        let remaining = duration_secs - elapsed;
        if remaining > 0 && remaining != duration_secs - elapsed + 1 {
            print!("\rTime remaining: {}s, Events captured: {}", remaining, event_count);
            std::io::stdout().flush()?;
        }
        
        // Check for MIDI events
        while let Ok((event, source)) = rx.try_recv() {
            let timestamp = start.elapsed().as_secs_f64();
            let log_line = format!("[{:>8.3}s] {:?} from {:?}", timestamp, event, source);
            println!("\r{}", log_line);
            writeln!(log_file, "{}", log_line)?;
            event_count += 1;
        }
        
        std::thread::sleep(Duration::from_millis(1));
    }
    
    println!("\r\nTest complete! Captured {} events.", event_count);
    println!("Log saved to: {}", output_path.display());
    writeln!(log_file)?;
    writeln!(log_file, "Test complete. Total events: {}", event_count)?;
    
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        None | Some(Commands::Run { simple: false, .. }) => {
            // Run the main application
            log::info!("Starting Rustjay-404...");
            
            let rt = Arc::new(tokio::runtime::Runtime::new()?);
            let app = App::new(rt)?;
            app.run()?;
        }
        
        Some(Commands::Run { simple: true, file, loop_playback, .. }) => {
            // Run simple HAP player mode
            if let Some(video_path) = file {
                run_simple_player(&video_path, loop_playback)?;
            } else {
                eprintln!("Error: --file required in simple mode");
                std::process::exit(1);
            }
        }
        
        Some(Commands::Encode { input, output, format, width, height, fps, chunks }) => {
            log::info!("Encoding {} file(s) to HAP format...", input.len());
            
            // Parse format
            let hap_format = format.parse::<HapEncodeFormat>()?;
            log::info!("Using format: {}", hap_format);
            
            // Create encoder config
            let config = HapEncoderConfig {
                format: hap_format,
                width: width.unwrap_or(0),
                height: height.unwrap_or(0),
                fps: fps.unwrap_or(0),
                chunks,
                quality: 5,
                gpu_mode: GpuMode::Auto,
            };
            
            // Run batch encoding
            let results = batch_encode(&input, &output, &config)?;
            
            // Report results
            let mut success = 0;
            let mut failed = 0;
            
            for (input_path, result) in results {
                match result {
                    Ok(()) => {
                        log::info!("✓ Encoded: {}", input_path.display());
                        success += 1;
                    }
                    Err(e) => {
                        log::error!("✗ Failed to encode {}: {}", input_path.display(), e);
                        failed += 1;
                    }
                }
            }
            
            log::info!("Encoding complete: {} succeeded, {} failed", success, failed);
        }
        
        Some(Commands::Convert { input, output, format }) => {
            log::info!("Converting capture to HAP...");
            
            let hap_format = format.parse::<HapEncodeFormat>()?;
            
            // Determine output path
            let output_path = output.unwrap_or_else(|| {
                let stem = input.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                PathBuf::from(format!("{}_converted.hap.mov", stem))
            });
            
            convert_capture_to_hap(&input, &output_path, hap_format)?;
            log::info!("Converted: {} -> {}", input.display(), output_path.display());
        }
        
        Some(Commands::Check) => {
            log::info!("Checking HAP encoding support...");
            
            match HapEncoder::new().check_ffmpeg() {
                Ok(()) => {
                    log::info!("✓ ffmpeg is installed with HAP support");
                    
                    // Print additional info
                    let output = std::process::Command::new("ffmpeg")
                        .args(&["-version"])
                        .output()?;
                    
                    let version = String::from_utf8_lossy(&output.stdout);
                    if let Some(line) = version.lines().next() {
                        log::info!("  {}", line);
                    }
                }
                Err(e) => {
                    log::error!("✗ ffmpeg check failed: {}", e);
                    log::info!("");
                    log::info!("To encode videos to HAP format, you need ffmpeg with HAP support.");
                    log::info!("Install it with:");
                    log::info!("  macOS:   brew install ffmpeg");
                    log::info!("  Ubuntu:  sudo apt install ffmpeg");
                    log::info!("  Windows: choco install ffmpeg");
                }
            }
        }
        
        Some(Commands::MidiTest { output, duration }) => {
            run_midi_test(&output, duration)?;
        }
    }
    
    Ok(())
}
