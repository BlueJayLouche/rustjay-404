//! FFmpeg fallback decoder

use crate::sampler::sample::VideoDecoder;
use std::sync::Arc;

pub struct FfmpegDecoder;

impl VideoDecoder for FfmpegDecoder {
    fn get_frame(&mut self, _frame: u32) -> Option<Arc<wgpu::Texture>> {
        None
    }

    fn resolution(&self) -> (u32, u32) {
        (0, 0)
    }

    fn frame_count(&self) -> u32 {
        0
    }

    fn fps(&self) -> f32 {
        0.0
    }
}
