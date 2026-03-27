# RustJay 404

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
- **Live Sampling**: Capture from webcam or Syphon input, auto-convert to HAP, assign to pads
- **NDI I/O**: Network Device Interface input and output for video over IP
- **Syphon I/O**: Zero-copy GPU-path Syphon input and output (macOS)
- **Pro I/O**: MIDI input for pad triggering, OSC server for remote control
- **Tap Tempo**: Shift+T for tap tempo with automatic phase reset
- **SP-404 Style Interface**: 16-pad grid with GATE, LATCH, and ONE-SHOT trigger modes
- **Persistent Layout**: Window positions and sizes saved between sessions

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
- Beat numbers and alternating group shading every 4 steps for visual clarity
- **Clear All**: Clear current pattern
- **Randomize**: Generate random pattern
- **Prev/Next**: Switch between 16 patterns
- **Play/Stop**: Control playback

### Keyboard Shortcuts
- **Space**: Play/stop sequencer
- **Shift+Space**: Reset sequencer position to beat 1
- **Shift+T**: Tap tempo (single tap resets phase, consecutive taps set BPM)
- **Shift+F**: Toggle fullscreen

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
- ✅ NDI input/output (Network Device Interface)
- ✅ Syphon input/output with zero-copy GPU path (macOS)
- ✅ Tap tempo with phase reset
- ✅ Persistent window layout
- ✅ JSON preset save/load

See [ROADMAP.md](ROADMAP.md) for future plans.

## Install

Pre-built binaries are available on the [Releases](https://github.com/BlueJayLouche/rustjay-404/releases) page.

| Platform | Format | Notes |
|----------|--------|-------|
| macOS Apple Silicon | `.dmg` | Ad-hoc signed. Right-click → Open on first launch. |
| macOS Intel | `.dmg` | Ad-hoc signed. Right-click → Open on first launch. |
| Linux x86_64 | `.tar.gz` | Requires Vulkan-capable GPU. |
| Windows x86_64 | `.zip` | Extract and run. |

On macOS, download the `.dmg`, open it, and drag RustJay 404 to your Applications folder.

> NDI is not included in release builds. For NDI support, install the NDI SDK and build from source.

## Dependencies

### FFmpeg with Snappy (required for HAP encoding/conversion)

The `hap_convert` tool and HAP-related utilities require FFmpeg built with snappy support. The standard `brew install ffmpeg` does **not** include snappy. Install `ffmpeg-full` instead:

```bash
# snappy should already be installed, but just in case:
brew install snappy

# Install ffmpeg-full (includes --enable-libsnappy and the HAP codec)
brew install ffmpeg-full
```

`ffmpeg-full` is keg-only but Homebrew links it automatically, so `ffmpeg` and `ffprobe` will be available in your PATH.

To verify HAP support is present:

```bash
ffmpeg -codecs 2>&1 | grep -i hap
# Should show: DEVIL. hap   Vidvox Hap
```

### Syphon (macOS Only)

RustJay 404 can receive and send video via Syphon on macOS. The framework is provided by [`syphon-rs`](https://github.com/BlueJayLouche/syphon-rs), fetched automatically via Cargo. The build script finds `Syphon.framework` from the cargo git cache, a sibling `syphon-rs/` checkout, or the `SYPHON_FRAMEWORK_DIR` environment variable.

If you see `dyld: Library not loaded: Syphon.framework` at runtime:
```bash
SYPHON_FRAMEWORK_DIR=/path/to/syphon-rs/syphon-lib cargo build --release
```

### NDI (Network Device Interface)

NDI support is enabled by default via the `ndi` feature flag. It requires the NDI SDK for Apple to be installed.

**Install the NDI SDK:**
1. Download from [ndi.video](https://ndi.video/tools/download/)
2. Run the installer — it places `libndi.dylib` in `/Library/NDI SDK for Apple/lib/macOS/`

The build script automatically adds the NDI SDK library path to the binary's rpath. To disable NDI:
```bash
cargo build --release --no-default-features
```

### HAP Playback (`hap-rs`)

HAP video decoding uses the [`hap-rs`](https://github.com/BlueJayLouche/hap-rs) crate (fetched automatically via Cargo). No additional installation is required.

For encoding your source videos to HAP format, see the FFmpeg section above.

### Building from Source

Requires Rust 1.70+ and a GPU with BC texture compression support.

```bash
cargo build --release
```

To build without NDI:

```bash
cargo build --release --no-default-features
```

## License

MIT OR Apache-2.0
