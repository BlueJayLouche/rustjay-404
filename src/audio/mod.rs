//! Real-time audio analysis with FFT and beat detection.

pub mod device;
pub mod fft;
pub mod routing;

pub use device::{default_audio_device, list_audio_devices};

use crate::audio::device::{build_stream_f32, build_stream_i16, build_stream_u16};
use crate::audio::fft::{AudioConfig, AudioOutput};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Audio analyzer managing a cpal input stream with real-time FFT
pub struct AudioAnalyzer {
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    stream_error: Arc<AtomicBool>,
    output: Arc<AudioOutput>,
    config: Arc<AudioConfig>,
}

impl AudioAnalyzer {
    pub fn new() -> Self {
        Self {
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
            stream_error: Arc::new(AtomicBool::new(false)),
            output: Arc::new(AudioOutput::new()),
            config: Arc::new(AudioConfig::new()),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn take_stream_error(&self) -> bool {
        self.stream_error.swap(false, Ordering::Relaxed)
    }

    pub fn start(&mut self) -> anyhow::Result<()> {
        self.start_with_device(None)
    }

    pub fn start_with_device(&mut self, device_name: Option<&str>) -> anyhow::Result<()> {
        if self.stream.is_some() {
            self.stop();
        }

        let host = cpal::default_host();

        let device = match device_name {
            Some(name) => host
                .input_devices()?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| anyhow::anyhow!("Audio device '{}' not found", name))?,
            None => host
                .default_input_device()
                .ok_or_else(|| anyhow::anyhow!("No default input device"))?,
        };

        log::info!("Audio device: {:?}", device.name()?);
        self.output.reset();

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream_f32(
                &device,
                &config.into(),
                sample_rate,
                channels,
                Arc::clone(&self.running),
                Arc::clone(&self.output),
                Arc::clone(&self.config),
                Arc::clone(&self.stream_error),
            )?,
            cpal::SampleFormat::I16 => build_stream_i16(
                &device,
                &config.into(),
                sample_rate,
                channels,
                Arc::clone(&self.running),
                Arc::clone(&self.output),
                Arc::clone(&self.config),
                Arc::clone(&self.stream_error),
            )?,
            cpal::SampleFormat::U16 => build_stream_u16(
                &device,
                &config.into(),
                sample_rate,
                channels,
                Arc::clone(&self.running),
                Arc::clone(&self.output),
                Arc::clone(&self.config),
                Arc::clone(&self.stream_error),
            )?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);
        self.running.store(true, Ordering::Release);
        log::info!("Audio analyzer started");
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.stream = None;
        self.output.reset();
        log::info!("Audio analyzer stopped");
    }

    // --- Lock-free read accessors (main thread) ---

    pub fn get_fft(&self) -> [f32; 8] {
        self.output.read_fft()
    }

    pub fn get_volume(&self) -> f32 {
        self.output.read_volume()
    }

    pub fn is_beat(&self) -> bool {
        self.output.take_beat()
    }

    pub fn get_beat_phase(&self) -> f32 {
        self.output.read_beat_phase()
    }

    // --- Lock-free config setters (main thread → callback) ---

    pub fn set_amplitude(&self, amplitude: f32) {
        self.config.set_amplitude(amplitude);
    }

    pub fn set_smoothing(&self, smoothing: f32) {
        self.config.set_smoothing(smoothing.clamp(0.0, 0.99));
    }

    pub fn get_normalize(&self) -> bool {
        self.config.normalize.load(Ordering::Relaxed)
    }

    pub fn set_normalize(&self, normalize: bool) {
        self.config.normalize.store(normalize, Ordering::Relaxed);
    }

    pub fn get_pink_noise_shaping(&self) -> bool {
        self.config.pink_noise_shaping.load(Ordering::Relaxed)
    }

    pub fn set_pink_noise_shaping(&self, enabled: bool) {
        self.config
            .pink_noise_shaping
            .store(enabled, Ordering::Relaxed);
    }
}

impl Default for AudioAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioAnalyzer {
    fn drop(&mut self) {
        self.stop();
    }
}
