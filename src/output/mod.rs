//! Unified video output management.
//!
//! Ported from rustjay-template's output architecture. Routes the final
//! mixer output to all active sinks (display, Syphon, future NDI/V4L2).
//!
//! GPU readback uses a double-buffered staging pool so the render thread
//! never blocks waiting for a GPU→CPU copy to complete. Each frame the
//! render thread submits a copy into the *current* staging slot and harvests
//! the *previous* slot's data (which has had a full frame to finish mapping).

#[cfg(target_os = "macos")]
use crate::video::interapp::SyphonOutput;

#[cfg(feature = "ndi")]
pub mod ndi_output;
#[cfg(feature = "ndi")]
use ndi_output::NdiOutputSender;

/// Commands for output stream control
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCommand {
    None,
    #[cfg(target_os = "macos")]
    StartSyphon,
    #[cfg(target_os = "macos")]
    StopSyphon,
    #[cfg(feature = "ndi")]
    StartNdi,
    #[cfg(feature = "ndi")]
    StopNdi,
    ResizeOutput,
}

impl Default for OutputCommand {
    fn default() -> Self {
        OutputCommand::None
    }
}

// ---------------------------------------------------------------------------
// Async GPU readback pool
// ---------------------------------------------------------------------------

/// Number of staging buffers in the pool. Two is enough: one being filled
/// by the GPU while the CPU reads the other.
const READBACK_SLOTS: usize = 2;

/// State of a single staging buffer slot.
enum SlotState {
    /// Buffer is idle and available for a new copy.
    Available,
    /// A copy has been submitted and `map_async` requested; waiting for GPU.
    Pending {
        buffer: wgpu::Buffer,
        width: u32,
        height: u32,
        ready: std::sync::mpsc::Receiver<bool>,
    },
}

/// Double-buffered staging pool for non-blocking GPU→CPU readback.
struct ReadbackPool {
    slots: Vec<SlotState>,
    /// Index of the slot to write into this frame.
    current: usize,
}

impl ReadbackPool {
    fn new() -> Self {
        let mut slots = Vec::with_capacity(READBACK_SLOTS);
        for _ in 0..READBACK_SLOTS {
            slots.push(SlotState::Available);
        }
        Self { slots, current: 0 }
    }

    /// Harvest the *previous* slot if its map has completed, returning the
    /// BGRA pixel data. This never blocks — if the GPU hasn't finished yet
    /// we simply skip this frame's readback.
    fn harvest_previous(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        let prev = (self.current + READBACK_SLOTS - 1) % READBACK_SLOTS;
        let slot = &mut self.slots[prev];

        match slot {
            SlotState::Pending {
                buffer,
                width,
                height,
                ready,
            } => {
                match ready.try_recv() {
                    Ok(true) => {
                        let w = *width;
                        let h = *height;
                        let data = buffer.slice(..).get_mapped_range().to_vec();
                        buffer.unmap();
                        // Move buffer out so we can reuse the slot
                        let buf = match std::mem::replace(slot, SlotState::Available) {
                            SlotState::Pending { buffer, .. } => buffer,
                            _ => unreachable!(),
                        };
                        drop(buf);
                        Some((data, w, h))
                    }
                    _ => None,
                }
            }
            SlotState::Available => None,
        }
    }

    /// Submit a non-blocking copy from `texture` into the current staging
    /// slot and request an async map.
    fn submit_copy(&mut self, texture: &wgpu::Texture, device: &wgpu::Device, queue: &wgpu::Queue) {
        let width = texture.width();
        let height = texture.height();
        let bytes_per_row = width * 4;
        let buffer_size = (bytes_per_row * height) as u64;

        // If the current slot is still pending (GPU too slow), drop it.
        if matches!(self.slots[self.current], SlotState::Pending { .. }) {
            self.slots[self.current] = SlotState::Available;
            log::debug!("Readback slot {} overwritten (GPU too slow)", self.current);
        }

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback Staging"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Readback Copy"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(std::iter::once(encoder.finish()));

        // Request async map — the callback signals via channel.
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        staging_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result.is_ok());
            });

        self.slots[self.current] = SlotState::Pending {
            buffer: staging_buffer,
            width,
            height,
            ready: rx,
        };

        self.current = (self.current + 1) % READBACK_SLOTS;
    }

    /// Drain any pending slots (used during shutdown).
    fn drain(&mut self, device: &wgpu::Device) {
        for slot in &mut self.slots {
            if matches!(slot, SlotState::Pending { .. }) {
                device.poll(wgpu::PollType::Wait).ok();
                if let SlotState::Pending { buffer, .. } =
                    std::mem::replace(slot, SlotState::Available)
                {
                    drop(buffer);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// OutputManager
// ---------------------------------------------------------------------------

/// Manages all video output sinks.
pub struct OutputManager {
    /// Syphon output (macOS) — GPU zero-copy path
    #[cfg(target_os = "macos")]
    syphon_output: Option<SyphonOutput>,

    /// NDI network output (CPU-path via readback pool)
    #[cfg(feature = "ndi")]
    ndi_output: Option<NdiOutputSender>,

    /// Async readback pool for CPU-path outputs (NDI, V4L2).
    readback_pool: ReadbackPool,

    frame_count: u64,
}

impl OutputManager {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            syphon_output: None,
            #[cfg(feature = "ndi")]
            ndi_output: None,
            readback_pool: ReadbackPool::new(),
            frame_count: 0,
        }
    }

    // --- Syphon (macOS) ---

    #[cfg(target_os = "macos")]
    pub fn start_syphon(
        &mut self,
        name: &str,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> anyhow::Result<()> {
        let syphon = SyphonOutput::new(name, device, queue, width, height)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let is_zero_copy = syphon.is_zero_copy();
        self.syphon_output = Some(syphon);
        if is_zero_copy {
            log::info!("Syphon output '{}' started (zero-copy)", name);
        } else {
            log::info!("Syphon output '{}' started (CPU fallback)", name);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn stop_syphon(&mut self) {
        if self.syphon_output.take().is_some() {
            log::info!("Syphon output stopped");
        }
    }

    #[cfg(target_os = "macos")]
    pub fn is_syphon_active(&self) -> bool {
        self.syphon_output.is_some()
    }

    #[cfg(not(target_os = "macos"))]
    pub fn is_syphon_active(&self) -> bool {
        false
    }

    #[cfg(target_os = "macos")]
    pub fn syphon_client_count(&self) -> usize {
        self.syphon_output
            .as_ref()
            .map_or(0, |s| s.client_count())
    }

    #[cfg(target_os = "macos")]
    pub fn syphon_is_zero_copy(&self) -> bool {
        self.syphon_output
            .as_ref()
            .map_or(false, |s| s.is_zero_copy())
    }

    // --- NDI ---

    #[cfg(feature = "ndi")]
    pub fn start_ndi(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        include_alpha: bool,
    ) -> anyhow::Result<()> {
        let sender = NdiOutputSender::new(name, width, height, include_alpha)?;
        self.ndi_output = Some(sender);
        log::info!("NDI output started: {} ({}x{})", name, width, height);
        Ok(())
    }

    #[cfg(not(feature = "ndi"))]
    pub fn start_ndi(
        &mut self,
        _name: &str,
        _width: u32,
        _height: u32,
        _include_alpha: bool,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("NDI support not compiled. Enable the 'ndi' feature."))
    }

    #[cfg(feature = "ndi")]
    pub fn stop_ndi(&mut self) {
        if self.ndi_output.take().is_some() {
            log::info!("NDI output stopped");
        }
    }

    #[cfg(not(feature = "ndi"))]
    pub fn stop_ndi(&mut self) {}

    #[cfg(feature = "ndi")]
    pub fn is_ndi_active(&self) -> bool {
        self.ndi_output.is_some()
    }

    #[cfg(not(feature = "ndi"))]
    pub fn is_ndi_active(&self) -> bool {
        false
    }

    // --- Readback pool queries ---

    /// Returns true if any CPU-path output (NDI, V4L2) needs readback.
    fn needs_readback(&self) -> bool {
        #[cfg(feature = "ndi")]
        if self.ndi_output.is_some() {
            return true;
        }
        false
    }

    // --- Frame submission ---

    /// Submit the final output frame to all active sinks.
    ///
    /// GPU-path outputs (Syphon) receive the texture directly.
    /// CPU-path outputs (future NDI, V4L2) use the async readback pool —
    /// the render thread never blocks waiting for a GPU→CPU copy.
    pub fn submit_frame(
        &mut self,
        texture: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        self.frame_count += 1;

        // CPU-path outputs: harvest previous frame's readback, then submit new copy
        if self.needs_readback() {
            device.poll(wgpu::PollType::Poll).ok();

            if let Some((data, width, height)) = self.readback_pool.harvest_previous() {
                #[cfg(feature = "ndi")]
                if let Some(ref sender) = self.ndi_output {
                    sender.submit_frame(&data, width, height);
                }
            }

            self.readback_pool.submit_copy(texture, device, queue);
        }

        // Syphon output (GPU zero-copy on macOS)
        #[cfg(target_os = "macos")]
        if let Some(ref mut syphon) = self.syphon_output {
            use crate::video::interapp::InterAppVideo;
            syphon.publish_frame(texture, device, queue);
        }
    }

    /// Shutdown all outputs.
    pub fn shutdown(&mut self) {
        self.stop_ndi();
        #[cfg(target_os = "macos")]
        self.stop_syphon();
    }

    /// Drain readback pool (call when GPU device is still alive).
    pub fn drain_readback(&mut self, device: &wgpu::Device) {
        self.readback_pool.drain(device);
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OutputManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
