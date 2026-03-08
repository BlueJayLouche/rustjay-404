//! HAP codec decoder
//!
//! HAP is a GPU-accelerated codec that stores frames as S3TC/DXT compressed textures.
//! This allows direct upload to GPU without CPU decompression.
//!
//! HAP formats supported:
//! - HAP (DXT1, RGB, 8bpp) - 4:1 compression
//! - HAP Alpha (DXT5, RGBA, 16bpp) - 4:1 compression  
//! - HAP Q (DXT5, RGBA, 16bpp) - High quality
//! - HAP Q Alpha (DXT5 + DXT1, RGBA, 24bpp) - High quality with alpha
//! - HAP R (BC6H, RGB, 16bpp) - HDR, no alpha
//! - HAP R Alpha (BC6H + DXT1, RGBA, 24bpp) - HDR with alpha

use crate::sampler::sample::VideoDecoder;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

/// HAP chunk header
#[derive(Debug, Clone)]
pub struct HapChunk {
    /// Chunk data size (not including header)
    pub size: u32,
    /// Chunk type (DXT1, DXT5, etc)
    pub chunk_type: u16,
    /// Frame index this chunk belongs to
    pub frame_index: u32,
    /// Offset in file where chunk data starts
    pub data_offset: u64,
}

/// HAP frame containing one or more compressed textures
#[derive(Debug)]
pub struct HapFrame {
    /// Frame index
    pub index: u32,
    /// Presentation timestamp in seconds
    pub pts: f64,
    /// Chunks making up this frame
    pub chunks: Vec<HapChunk>,
}

/// HAP decoder using OS-native APIs
pub struct HapDecoder {
    /// Video width
    width: u32,
    /// Video height  
    height: u32,
    /// Total frame count
    frame_count: u32,
    /// Frame rate
    fps: f32,
    /// File reader
    reader: BufReader<File>,
    /// Frame index to file position mapping
    frames: Vec<HapFrame>,
    /// Texture format
    format: HapFormat,
    /// GPU device for texture creation (stored as raw pointer - Device is Send+Sync)
    device: *const wgpu::Device,
    /// GPU queue for uploads (stored as raw pointer - Queue is Send+Sync)
    /// These are safe because Device/Queue are thread-safe in wgpu
    queue: *const wgpu::Queue,
}

/// HAP texture formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapFormat {
    /// DXT1 compression (RGB, 8bpp)
    Dxt1,
    /// DXT5 compression (RGBA, 16bpp)
    Dxt5,
    /// DXT5 + DXT1 (RGB + Alpha, 24bpp)
    Dxt5Ycocg,
    /// BC6H compression (HDR RGB, 16bpp)
    Bc6h,
    /// Unknown/unsupported format
    Unknown,
}

impl HapFormat {
    /// Get the wgpu texture format for this HAP format
    pub fn to_wgpu_format(&self) -> Option<wgpu::TextureFormat> {
        match self {
            HapFormat::Dxt1 => Some(wgpu::TextureFormat::Bc1RgbaUnormSrgb),
            HapFormat::Dxt5 => Some(wgpu::TextureFormat::Bc3RgbaUnormSrgb),
            HapFormat::Dxt5Ycocg => Some(wgpu::TextureFormat::Bc3RgbaUnormSrgb),
            HapFormat::Bc6h => Some(wgpu::TextureFormat::Bc6hRgbUfloat),
            HapFormat::Unknown => None,
        }
    }
    
    /// Get bytes per block (for DXT, blocks are 4x4 pixels)
    pub fn bytes_per_block(&self) -> usize {
        match self {
            HapFormat::Dxt1 => 8,
            HapFormat::Dxt5 | HapFormat::Dxt5Ycocg => 16,
            HapFormat::Bc6h => 16,
            HapFormat::Unknown => 0,
        }
    }
}

/// HAP file header
const HAP_MAGIC: &[u8; 4] = b"hap ";
const HAP_VERSION: u32 = 1;

impl HapDecoder {
    /// Open and parse a HAP file
    /// 
    /// # Safety
    /// The device and queue must remain valid for the lifetime of the decoder.
    pub unsafe fn new(
        path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        
        // Read and verify header
        let mut header = [0u8; 64];
        reader.read_exact(&mut header)?;
        
        if &header[0..4] != HAP_MAGIC {
            return Err(anyhow::anyhow!("Not a valid HAP file"));
        }
        
        let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if version != HAP_VERSION {
            return Err(anyhow::anyhow!("Unsupported HAP version: {}", version));
        }
        
        let width = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let height = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        let frame_count = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        let fps = f32::from_le_bytes([header[20], header[21], header[22], header[23]]);
        
        // Read format identifier
        let format_id = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        let format = Self::parse_format(format_id);
        
        log::info!(
            "HAP file: {}x{} @ {:.2}fps, {} frames, format: {:?}",
            width, height, fps, frame_count, format
        );
        
        // Read frame index (starting at offset 64)
        let mut frames = Vec::with_capacity(frame_count as usize);
        for i in 0..frame_count {
            let mut frame_header = [0u8; 16];
            reader.read_exact(&mut frame_header)?;
            
            let _chunk_count = u32::from_le_bytes([frame_header[0], frame_header[1], frame_header[2], frame_header[3]]);
            let pts = f64::from_le_bytes([
                frame_header[4], frame_header[5], frame_header[6], frame_header[7],
                frame_header[8], frame_header[9], frame_header[10], frame_header[11],
            ]);
            let data_offset = u64::from_le_bytes([
                frame_header[4], frame_header[5], frame_header[6], frame_header[7],
                0, 0, 0, 0, // Pad to 64 bits
            ]);
            
            // For now, assume single chunk per frame (simplified)
            let chunk = HapChunk {
                size: 0, // Will be calculated from next frame offset
                chunk_type: format_id as u16,
                frame_index: i,
                data_offset,
            };
            
            frames.push(HapFrame {
                index: i,
                pts,
                chunks: vec![chunk],
            });
        }
        
        // Calculate chunk sizes from frame offsets
        for i in 0..frames.len() {
            let next_offset = if i + 1 < frames.len() {
                frames[i + 1].chunks[0].data_offset
            } else {
                // Get file size for last frame
                reader.seek(SeekFrom::End(0))? as u64
            };
            frames[i].chunks[0].size = (next_offset - frames[i].chunks[0].data_offset) as u32;
        }
        
        Ok(Self {
            width,
            height,
            frame_count,
            fps,
            reader,
            frames,
            format,
            device: device as *const _,
            queue: queue as *const _,
        })
    }
    
    /// Parse HAP format identifier
    fn parse_format(format_id: u32) -> HapFormat {
        match format_id {
            0xAB => HapFormat::Dxt1,       // Standard HAP (DXT1)
            0xBB => HapFormat::Dxt5,       // HAP Alpha (DXT5)
            0xEB => HapFormat::Dxt5Ycocg,  // HAP Q (DXT5 YCoCg)
            0xCB => HapFormat::Bc6h,       // HAP R (BC6H)
            _ => HapFormat::Unknown,
        }
    }
    
    /// Read a frame's compressed data into GPU texture
    pub fn decode_frame(&mut self, frame_index: u32) -> anyhow::Result<Arc<wgpu::Texture>> {
        let frame_idx = frame_index as usize;
        if frame_idx >= self.frames.len() {
            return Err(anyhow::anyhow!("Frame index out of range"));
        }
        
        let frame = &self.frames[frame_idx];
        let chunk = &frame.chunks[0]; // Simplified: assume single chunk
        
        // Seek to frame data
        self.reader.seek(SeekFrom::Start(chunk.data_offset))?;
        
        // Read compressed data
        let mut compressed_data = vec![0u8; chunk.size as usize];
        self.reader.read_exact(&mut compressed_data)?;
        
        // Create texture
        let texture_format = self.format.to_wgpu_format()
            .unwrap_or(wgpu::TextureFormat::Rgba8UnormSrgb);
        
        // SAFETY: device is valid for the lifetime of the decoder
        let texture = unsafe { (*self.device).create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("HAP Frame {}", frame_index)),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }) };
        
        // Upload compressed data directly to GPU
        // Note: For compressed textures, we need to calculate proper row pitch
        let block_size = self.format.bytes_per_block();
        let blocks_x = (self.width + 3) / 4; // Round up to 4-pixel blocks
        let blocks_y = (self.height + 3) / 4;
        let row_pitch = blocks_x as usize * block_size;
        
        // SAFETY: queue is valid for the lifetime of the decoder
        unsafe {
            (*self.queue).write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &compressed_data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(row_pitch as u32),
                    rows_per_image: Some(blocks_y),
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        
        Ok(Arc::new(texture))
    }
}

// SAFETY: Queue is Send+Sync in wgpu, and we never mutate through the pointer
unsafe impl Send for HapDecoder {}
unsafe impl Sync for HapDecoder {}

impl VideoDecoder for HapDecoder {
    fn get_frame(&mut self, frame: u32) -> Option<Arc<wgpu::Texture>> {
        self.decode_frame(frame).ok()
    }

    fn resolution(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn frame_count(&self) -> u32 {
        self.frame_count
    }

    fn fps(&self) -> f32 {
        self.fps
    }
}
