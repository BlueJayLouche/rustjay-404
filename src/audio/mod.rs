//! # Audio Module
//!
//! Real-time audio analysis with FFT and beat detection.

pub mod device;
pub mod fft;
pub mod routing;

pub use device::{default_audio_device, list_audio_devices};

use crate::audio::device::{build_stream_f32, build_stream_i16, build_stream_u16};
use crate::audio::fft::{AudioConfig, AudioOutput, DEFAULT_FFT_SIZE};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Commands for audio device and stream control
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCommand {
    None,
    RefreshDevices,
    SelectDevice(String),
    Start,
    Stop,
    SetFftSize(usize),
}

/// Audio analyzer running in real-time
pub struct AudioAnalyzer {
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    /// Set true by the cpal error callback; checked by main thread for reconnect
    stream_error: Arc<AtomicBool>,
    /// Lock-free: written by audio callback, read by main thread
    output: Arc<AudioOutput>,
    /// Lock-free: written by main thread, read by audio callback
    config: Arc<AudioConfig>,
    /// Current FFT size (requires stream rebuild to change)
    fft_size: usize,
}

impl AudioAnalyzer {
    pub fn new() -> Self {
        Self {
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
            stream_error: Arc::new(AtomicBool::new(false)),
            output: Arc::new(AudioOutput::new()),
            config: Arc::new(AudioConfig::new()),
            fft_size: DEFAULT_FFT_SIZE,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Returns true if the stream encountered an error since the last call
    /// (atomically clears the flag)
    pub fn take_stream_error(&self) -> bool {
        self.stream_error.swap(false, Ordering::Relaxed)
    }

    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    pub fn set_fft_size(&mut self, size: usize) {
        self.fft_size = size;
    }

    /// Start audio analysis with default device
    pub fn start(&mut self) -> anyhow::Result<String> {
        self.start_with_device(None)
    }

    /// Start audio analysis with specific device (None for default).
    /// Returns the actual device name used.
    pub fn start_with_device(&mut self, device_name: Option<&str>) -> anyhow::Result<String> {
        log::info!("[Audio] start_with_device called with: {:?}, fft_size: {}", device_name, self.fft_size);

        // Signal old callback to stop, then drop the old stream.
        // Old callback still holds its own Arc clones — let them become orphans
        // so any in-flight callbacks write to Arcs nobody reads.
        if self.stream.is_some() {
            log::info!("[Audio] Stopping existing stream first");
            self.running.store(false, Ordering::Release);
            self.stream = None;
        }

        // Fresh Arcs for the new stream — old callback can't pollute these
        self.running = Arc::new(AtomicBool::new(false));
        self.output = Arc::new(AudioOutput::new());
        self.stream_error = Arc::new(AtomicBool::new(false));

        let host = cpal::default_host();

        match host.input_devices() {
            Ok(devices) => {
                log::info!("[Audio] Available input devices:");
                for d in devices {
                    if let Ok(name) = d.name() {
                        log::info!("  - {}", name);
                    }
                }
            }
            Err(e) => log::warn!("[Audio] Failed to list input devices: {}", e),
        }

        let device = match device_name {
            Some(name) => {
                log::info!("[Audio] Looking for device: '{}'", name);
                host.input_devices()?
                    .find(|d| {
                        let dev_name = d.name().ok();
                        let matches = dev_name.as_deref() == Some(name);
                        log::debug!(
                            "[Audio] Checking device: {:?}, matches: {}",
                            dev_name,
                            matches
                        );
                        matches
                    })
                    .ok_or_else(|| anyhow::anyhow!("Audio device '{}' not found", name))?
            }
            None => {
                log::info!("[Audio] Using default input device");
                host.default_input_device()
                    .ok_or_else(|| anyhow::anyhow!("No default input device"))?
            }
        };

        let actual_device_name = device.name()?;
        log::info!("[Audio] Selected device: {:?}", actual_device_name);

        let config = device.default_input_config()?;
        log::info!("Audio config: {:?}", config);

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let fft_size = self.fft_size;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream_f32(
                &device, &config.into(), sample_rate, channels, fft_size,
                Arc::clone(&self.running), Arc::clone(&self.output),
                Arc::clone(&self.config), Arc::clone(&self.stream_error),
            )?,
            cpal::SampleFormat::I16 => build_stream_i16(
                &device, &config.into(), sample_rate, channels, fft_size,
                Arc::clone(&self.running), Arc::clone(&self.output),
                Arc::clone(&self.config), Arc::clone(&self.stream_error),
            )?,
            cpal::SampleFormat::U16 => build_stream_u16(
                &device, &config.into(), sample_rate, channels, fft_size,
                Arc::clone(&self.running), Arc::clone(&self.output),
                Arc::clone(&self.config), Arc::clone(&self.stream_error),
            )?,
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);
        self.running.store(true, Ordering::Release);

        log::info!("Audio analyzer started (FFT size: {}, device: {})", fft_size, actual_device_name);
        Ok(actual_device_name)
    }

    /// Stop audio analysis
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.stream = None;
        self.output.reset();
        log::info!("Audio analyzer stopped");
    }

    // --- Lock-free read accessors (safe to call from main thread) ---

    pub fn get_fft(&self) -> [f32; 8] {
        std::array::from_fn(|i| {
            f32::from_bits(self.output.fft[i].load(Ordering::Relaxed))
        })
    }

    pub fn get_volume(&self) -> f32 {
        f32::from_bits(self.output.volume.load(Ordering::Relaxed))
    }

    /// Returns true if a beat was detected since the last call (clears flag)
    pub fn is_beat(&self) -> bool {
        self.output.beat.swap(false, Ordering::Relaxed)
    }

    pub fn get_beat_phase(&self) -> f32 {
        f32::from_bits(self.output.beat_phase.load(Ordering::Relaxed))
    }

    // --- Lock-free config setters (written by main thread, read by callback) ---

    pub fn set_amplitude(&self, amplitude: f32) {
        self.config.amplitude.store(amplitude.to_bits(), Ordering::Relaxed);
    }

    pub fn set_smoothing(&self, smoothing: f32) {
        self.config
            .smoothing
            .store(smoothing.clamp(0.0, 0.99).to_bits(), Ordering::Relaxed);
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
        self.config.pink_noise_shaping.store(enabled, Ordering::Relaxed);
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
