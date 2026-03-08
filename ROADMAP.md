# Rusty-404 Implementation Roadmap

## Phase 0: HAP Parser Crates ✅

### hap-parser (Low-level HAP frame parsing)
- [x] Parse all HAP variants: Hap1/DXT1, Hap5/DXT5, HapY/YCoCg, HapA/BC4, Hap7/BC7, HapH/BC6H
- [x] Snappy decompression support
- [x] Section header parsing (4-byte and 8-byte size encoding)
- [x] 8 unit tests passing

### hap-qt (QuickTime/MP4 container parsing)
- [x] Parse moov/trak/mdat atoms
- [x] Sample table reading (stsd, stsz, stco, stsc, stts)
- [x] Frame offset calculation for random access
- [x] Memory-mapped file I/O with `memmap2`
- [x] Works with real HAP files (tested with 1280x720, 313 frames)

### hap-wgpu (GPU-accelerated playback)
- [x] Direct DXT texture upload to GPU (no CPU decompression)
- [x] wgpu format mapping for all HAP variants
- [x] Padded dimension handling (DXT requires multiples of 4)
- [x] `HapPlayer` with playback controls (play/pause/seek/speed)
- [x] Loop modes: None, Loop, Palindrome
- [x] Frame caching for smooth playback
- [x] Example: `cargo run --example player -- <hap-file>`

---

## Phase 1: Foundation ✅

### Video Pipeline
- [x] HAP codec decoder (DXT1, DXT5, BC6H formats)
- [x] HAP file loading and GPU texture upload
- [x] Basic video playback (no effects)
- [x] 8-channel video mixer with 12 blend modes

### Core Playback
- [x] Pad triggering with all modes (Gate/Latch/One-Shot)
- [x] Frame-accurate seeking
- [x] Playback speed control (including reverse)
- [x] Loop points

### UI Basics
- [x] ImGui integration
- [x] 4x4 pad grid display
- [x] Basic transport controls

### Window System ✅
- [x] Dual-window architecture (Output + Control)
- [x] Output window: cursor hidden, clean presentation
- [x] Control window: cursor visible, ImGui UI
- [x] Independent window management

## Phase 2: Sequencer ✅

### Sequencer Core
- [x] Pattern playback
- [x] Step editing UI (click to toggle steps)
- [x] Gate release (pads automatically stop after gate duration)
- [x] Probability and ratcheting (backend ready)
- [x] Pattern chaining (Prev/Next buttons)

### Recording
- [ ] Live pattern recording
- [ ] Quantization
- [ ] Step parameter locks

## Phase 3: Mixing & Effects ✅

### Mixer
- [x] 8-channel video mixing
- [x] 12 Blend modes:
  - [x] Basic: Normal, Add, Multiply, Screen
  - [x] Advanced: Overlay, Soft Light, Hard Light, Difference, Lighten, Darken
  - [x] Keying: Chroma Key (green/blue screen), Luma Key (brightness)
- [x] Per-channel opacity controls
- [x] Per-channel keying parameters (threshold, smoothness, color)
- [ ] Per-channel transforms (position, scale, rotation)
- [ ] Channel layout presets

### Effects
- [ ] HSB adjustments
- [ ] Kaleidoscope
- [ ] Feedback/delay

## Phase 4: Live Input ✅

### Capture
- [x] Webcam capture (nokhwa)
- [ ] NDI input
- [x] Real-time HAP conversion (via ffmpeg)
- [x] Auto-assign to pads

## Phase 5: I/O ✅

### MIDI
- [x] MIDI input handling
- [x] Pad note mapping
- [x] CC control (Volume, Speed)
- [x] MIDI learn functionality

### OSC
- [x] OSC server (port 9000)
- [x] Control commands (trigger, release, BPM)

### NDI Output
- [ ] NDI output

## Phase 6: Persistence ✅

### Data
- [x] Bank save/load (JSON format)
- [x] Preset save/load
- [x] Settings

## Phase 7: Polish 🚧

### UI
- [ ] Custom styling
- [ ] Pad thumbnails
- [ ] Waveform display
- [ ] Performance metrics

### Library
- [ ] Sample library management
- [ ] Recent files
- [ ] Drag & drop import

---

## Completed Features Summary

### Video System
- HAP decoder with direct GPU upload (no CPU decompression)
- Multi-format support: DXT1, DXT5, DXT5-YCoCg, BC6H
- 8-channel real-time mixer with 12 blend modes
- Shader-based mixing with per-channel uniforms
- Chroma key and luma key support
- Ping-pong multi-pass rendering

### Sequencer
- 16 independent tracks (one per pad)
- Polyphonic playback with gate release
- Step sequencer UI with click-to-toggle
- BPM control with tap tempo
- Pattern system (16 patterns)
- Clear All / Randomize functions

### I/O
- CLI with HAP encoding (`encode`, `convert`, `check` commands)
- MIDI input with auto-connect
- OSC server for remote control
- Keyboard shortcuts (Shift+Space for play/stop)

### Platform
- wgpu 25.0 with Metal/DirectX/Vulkan backends
- ImGui 0.12 for UI
- HiDPI-safe rendering on macOS Retina displays
- Dual-window architecture

---

## Future Ideas

- [ ] Audio output (synced to video)
- [ ] Syphon/Spout output (macOS/Windows interop)
- [ ] Shader hot-reload
- [ ] Recording to disk
- [ ] Audio-reactive effects
- [ ] VST/AU plugin hosting
- [ ] Ableton Link sync
- [ ] TouchOSC template
- [ ] Web-based remote control
