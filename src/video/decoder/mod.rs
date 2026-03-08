//! Video decoders

pub mod ffmpeg;
pub mod hap;
pub mod hap_ffmpeg;
pub mod hap_wgpu;  // New hap-wgpu crate integration
pub mod streaming;
pub mod cached;
pub mod boundary_cached;

// Re-export the new HAP decoder as the primary one
pub use hap_wgpu::{HapWgpuDecoder, probe_hap_file, is_hap_file};
