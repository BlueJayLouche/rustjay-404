# Rusty-404

A high-performance video sampler inspired by the Roland SP-404, built in Rust with wgpu.

## Features

- **8-Channel Video Sampler**: Play up to 8 simultaneous HAP-encoded video clips
- **Polyphonic Sequencer**: 16-track drum machine-style sequencer with:
  - Step sequencing (16-64 steps per pattern)
  - Probability and ratcheting per step
  - Automatic gate release (pads stop after gate duration)
  - Pattern chaining and switching
- **Advanced Video Mixing**: GPU-accelerated mixer with 12 blend modes:
  - **Basic**: Normal, Add, Multiply, Screen
  - **Advanced**: Overlay, Soft Light, Hard Light, Difference, Lighten, Darken
  - **Keying**: Chroma Key (green/blue screen), Luma Key (brightness-based)
- **Per-Channel Controls**: Opacity, mix mode, and keying parameters per channel
- **Live Sampling**: Capture from webcam, auto-convert to HAP, assign to pads
- **Pro I/O**: MIDI input for pad triggering, OSC server for remote control
- **SP-404 Style Interface**: 16-pad grid with GATE, LATCH, and ONE-SHOT trigger modes

## Quick Start

```bash
# Build and run
cargo run --release

# Convert video to HAP format
cargo run -- encode input.mp4 --output ./samples --format dxt5

# Run with specific video (simple player mode)
cargo run -- --simple --file video.hap.mov --loop-playback
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  INPUT → SEQUENCER → PAD ENGINE → VIDEO PIPELINE → OUTPUT       │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

- **VideoSample**: HAP video clip with frame-accurate in/out points
- **SamplePad**: SP-404 style triggering (Gate/Latch/One-Shot), mix mode, opacity
- **SampleBank**: 16-pad banks with JSON save/load
- **SequencerEngine**: Polyphonic 16-track sequencer with gate-based release
- **VideoMixer**: wgpu-based 8-channel mixer with 12 blend modes and keying

## Usage

### Pad Grid
- **Left-click**: Trigger pad (behavior depends on trigger mode)
  - **Gate**: Play while held
  - **Latch**: Toggle on/off
  - **One-Shot**: Play once to end
- **Right-click**: Context menu with settings
  - Load video, adjust speed, set in/out points
  - Change mix mode and keying parameters
  - MIDI learn for external controllers

### Sequencer
- Click step buttons to toggle on/off
- **Clear All**: Clear current pattern
- **Randomize**: Generate random pattern
- **Prev/Next**: Switch between 16 patterns
- **Play/Stop**: Control playback

### Mixer Panel
- **Opacity**: Per-channel transparency (0.0 = invisible, 1.0 = fully visible)
- **Mix Mode**: 12 blend modes including chroma/luma keying
- **Keying Controls** (when Chroma/Luma Key selected):
  - **Key Color**: Green/Blue/Red presets or custom RGB
  - **Threshold**: Sensitivity of the key
  - **Smoothness**: Edge feathering
  - **Invert** (Luma Key): Swap kept/removed areas

### MIDI/OSC
- **MIDI**: Auto-connects to available MIDI devices
- **OSC**: Server runs on port 9000
  - `/rusty404/trigger <pad>` - Trigger pad (0-15)
  - `/rusty404/release <pad>` - Release pad
  - `/rusty404/bpm <bpm>` - Set BPM (20-999)

## Status

Working implementation with:

- ✅ HAP video playback (Hap1, Hap5, HapY color spaces)
- ✅ 8-channel video mixer with 12 blend modes
- ✅ Polyphonic sequencer with gate release
- ✅ Per-channel opacity and keying controls
- ✅ MIDI input for pad triggering
- ✅ OSC server for remote control
- ✅ Live webcam sampling
- ✅ JSON preset save/load

See [ROADMAP.md](ROADMAP.md) for future plans.

## Building

Requires Rust 1.70+ and a GPU with BC texture compression support.

```bash
cargo build --release
```

## License

MIT OR Apache-2.0
