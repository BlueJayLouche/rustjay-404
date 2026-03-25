//! Lock-free audio I/O types and real-time FFT processing.
//!
//! All types in this module are safe to use from the real-time audio callback:
//! no allocations, no mutexes -- only atomics.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Lock-free audio output (written by real-time callback, read by main thread)
pub struct AudioOutput {
    pub fft: [AtomicU32; 8],
    pub volume: AtomicU32,
    pub beat: AtomicBool,
    pub beat_phase: AtomicU32,
}

impl AudioOutput {
    pub fn new() -> Self {
        Self {
            fft: std::array::from_fn(|_| AtomicU32::new(0)),
            volume: AtomicU32::new(0),
            beat: AtomicBool::new(false),
            beat_phase: AtomicU32::new(0),
        }
    }

    pub fn reset(&self) {
        for f in &self.fft {
            f.store(0f32.to_bits(), Ordering::Relaxed);
        }
        self.volume.store(0f32.to_bits(), Ordering::Relaxed);
        self.beat.store(false, Ordering::Relaxed);
        self.beat_phase.store(0f32.to_bits(), Ordering::Relaxed);
    }

    pub fn read_fft(&self) -> [f32; 8] {
        std::array::from_fn(|i| f32::from_bits(self.fft[i].load(Ordering::Relaxed)))
    }

    pub fn read_volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    pub fn read_beat_phase(&self) -> f32 {
        f32::from_bits(self.beat_phase.load(Ordering::Relaxed))
    }

    pub fn take_beat(&self) -> bool {
        self.beat.swap(false, Ordering::Relaxed)
    }
}

/// Lock-free audio config (written by main thread, read by real-time callback)
pub struct AudioConfig {
    pub amplitude: AtomicU32,
    pub smoothing: AtomicU32,
    pub normalize: AtomicBool,
    pub pink_noise_shaping: AtomicBool,
}

impl AudioConfig {
    pub fn new() -> Self {
        Self {
            amplitude: AtomicU32::new(1.0f32.to_bits()),
            smoothing: AtomicU32::new(0.5f32.to_bits()),
            normalize: AtomicBool::new(true),
            pink_noise_shaping: AtomicBool::new(false),
        }
    }

    pub fn amplitude(&self) -> f32 {
        f32::from_bits(self.amplitude.load(Ordering::Relaxed))
    }

    pub fn smoothing(&self) -> f32 {
        f32::from_bits(self.smoothing.load(Ordering::Relaxed))
    }

    pub fn set_amplitude(&self, val: f32) {
        self.amplitude.store(val.to_bits(), Ordering::Relaxed);
    }

    pub fn set_smoothing(&self, val: f32) {
        self.smoothing.store(val.to_bits(), Ordering::Relaxed);
    }
}

/// Process a single audio frame on the real-time audio callback thread.
pub fn process_audio_frame(
    frame: &[f32],
    sample_rate: f32,
    fft_size: usize,
    r2c: &Arc<dyn realfft::RealToComplex<f32>>,
    scratch: &mut [rustfft::num_complex::Complex<f32>],
    windowed_buf: &mut Vec<f32>,
    spectrum_buf: &mut Vec<rustfft::num_complex::Complex<f32>>,
    magnitudes_buf: &mut Vec<f32>,
    beat_energy: &mut f32,
    beat_history: &mut VecDeque<f32>,
    beat_counter: &mut u32,
    output: &Arc<AudioOutput>,
    config: &Arc<AudioConfig>,
) {
    // Apply Hann window
    for (i, (&s, w_out)) in frame.iter().zip(windowed_buf.iter_mut()).enumerate() {
        let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
        *w_out = s * w;
    }

    if r2c
        .process_with_scratch(windowed_buf, spectrum_buf, scratch)
        .is_err()
    {
        return;
    }

    for (m, c) in magnitudes_buf.iter_mut().zip(spectrum_buf.iter()) {
        *m = c.norm();
    }

    let bands = calculate_bands(magnitudes_buf, sample_rate, fft_size);
    let volume: f32 = frame.iter().map(|&s| s.abs()).sum::<f32>() / fft_size as f32;

    // Beat detection
    let instant_energy: f32 = bands.iter().sum();
    beat_history.push_back(instant_energy);
    if beat_history.len() > 43 {
        beat_history.pop_front();
    }

    let local_average = if beat_history.len() >= 43 {
        beat_history.iter().sum::<f32>() / beat_history.len() as f32
    } else {
        instant_energy
    };

    let variance: f32 = beat_history
        .iter()
        .map(|&e| (e - local_average).powi(2))
        .sum::<f32>()
        / beat_history.len().max(1) as f32;
    let sensitivity = (-0.0025714 * variance + 1.5142857).clamp(1.2, 2.0);

    let is_beat = instant_energy > sensitivity * local_average && instant_energy > 0.1;

    if is_beat {
        *beat_counter += 1;
        *beat_energy = instant_energy;
    }

    let phase = ((*beat_counter as f32
        + (instant_energy / beat_energy.max(0.001)).min(1.0))
        * 0.1)
        % 1.0;

    // Read config atomically
    let smoothing = config.smoothing();
    let amplitude = config.amplitude();
    let normalize = config.normalize.load(Ordering::Relaxed);
    let pink_noise_shaping = config.pink_noise_shaping.load(Ordering::Relaxed);

    let max_band = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);

    // Write results atomically
    for (i, band) in bands.iter().enumerate() {
        let pink_factor = if pink_noise_shaping {
            1.0 + (i as f32 * 0.26)
        } else {
            1.0
        };

        let normalized_band = if normalize {
            (band / max_band) * pink_factor
        } else {
            band * pink_factor
        };

        let prev = f32::from_bits(output.fft[i].load(Ordering::Relaxed));
        let smoothed = prev * smoothing + normalized_band * (1.0 - smoothing);
        output.fft[i].store((smoothed * amplitude).to_bits(), Ordering::Relaxed);
    }

    let prev_volume = f32::from_bits(output.volume.load(Ordering::Relaxed));
    let smoothed_volume = prev_volume * smoothing + volume * (1.0 - smoothing);
    output
        .volume
        .store((smoothed_volume * amplitude).to_bits(), Ordering::Relaxed);

    if is_beat {
        output.beat.store(true, Ordering::Relaxed);
    }

    output.beat_phase.store(phase.to_bits(), Ordering::Relaxed);
}

/// Calculate 8 logarithmic frequency bands from FFT magnitudes
pub fn calculate_bands(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> [f32; 8] {
    let mut bands = [0.0f32; 8];
    let freq_resolution = sample_rate / fft_size as f32;

    let ranges = [
        (20.0, 60.0),
        (60.0, 120.0),
        (120.0, 250.0),
        (250.0, 500.0),
        (500.0, 1000.0),
        (1000.0, 2000.0),
        (2000.0, 4000.0),
        (4000.0, 8000.0),
    ];

    for (i, (low, high)) in ranges.iter().enumerate() {
        let low_bin = (low / freq_resolution) as usize;
        let high_bin =
            ((high / freq_resolution) as usize).min(magnitudes.len().saturating_sub(1));

        if low_bin < magnitudes.len() && high_bin > low_bin {
            let sum: f32 = magnitudes[low_bin..=high_bin].iter().sum();
            bands[i] = sum / (high_bin - low_bin + 1) as f32;
        }
    }

    let max_band = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);
    for band in bands.iter_mut() {
        *band = (*band / max_band).min(1.0);
    }

    bands
}
