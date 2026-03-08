# Rusty-404 Architecture Design

> A high-performance video sampler inspired by the SP-404, built in Rust with wgpu.

## Core Philosophy

- **Performance First**: 8 channels @ 720p30, OS-native decoding, GPU-accelerated everything
- **HAP Codec**: Native GPU texture streaming for all samples
- **Live Sampling**: Capture from webcam/NDI → HAP → pad in real-time
- **Polyphonic Sequencer**: Each pad has its own sequence (drum machine style)
- **Extensible Effects**: Modular shader-based effects pipeline
- **Pro I/O**: NDI in/out, MIDI, OSC for professional VJ workflows

---

## System Architecture Overview

```
INPUT LAYER → SEQUENCER ENGINE → PAD ENGINE → VIDEO PIPELINE → OUTPUT LAYER
     │                                                  ↑
     └────────────────── IMGUI UI ←─────────────────────┘
```

### Component Flow

1. **Input Layer**: MIDI, Keyboard, OSC, ImGui interactions
2. **Sequencer Engine**: Polyphonic drum machine (16 tracks, one per pad)
3. **Pad Engine**: SP-404 style banks, trigger modes, sample management
4. **Video Pipeline**: HAP decode → GPU textures → wgpu mixer/effects
5. **Output Layer**: NDI output, window display, ImGui overlay

---

## Module Structure

```
src/
├── main.rs                     # Application entry
├── app/
│   ├── mod.rs                  # App state machine, main loop
│   ├── config.rs               # Settings (JSON)
│   └── state.rs                # App modes (PERFORM, EDIT, SEQUENCE)
│
├── video/
│   ├── mod.rs
│   ├── decoder/
│   │   ├── mod.rs
│   │   ├── hap.rs              # HAP codec (AVFoundation on macOS)
│   │   └── ffmpeg.rs           # Fallback decoder
│   ├── capture/
│   │   ├── mod.rs
│   │   ├── ndi.rs              # NDI input
│   │   └── webcam.rs           # Camera capture
│   ├── converter.rs            # Live input → HAP conversion
│   └── texture_pool.rs         # GPU texture management
│
├── engine/
│   ├── mod.rs
│   ├── context.rs              # wgpu device/queue
│   ├── mixer.rs                # 8-channel video mixer
│   ├── effects/
│   │   ├── mod.rs
│   │   ├── chain.rs            # Effect chain management
│   │   └── shaders/            # .wgsl effect shaders
│   ├── stages/
│   │   ├── input_sampling.wgsl
│   │   ├── effects.wgsl
│   │   └── mixing.wgsl
│   └── output/                 
│       ├── mod.rs
│       └── ndi.rs              # NDI output
│
├── sampler/
│   ├── mod.rs
│   ├── pad.rs                  # SamplePad (trigger modes, playback)
│   ├── sample.rs               # VideoSample (HAP clip management)
│   ├── bank.rs                 # 16-pad bank with save/load
│   ├── thumbnail.rs            # Thumbnail generation/cache
│   └── library.rs              # Sample library management
│
├── sequencer/
│   ├── mod.rs
│   ├── engine.rs               # Polyphonic sequencer core
│   ├── track.rs                # Per-pad sequence track
│   ├── pattern.rs              # Pattern data structure
│   ├── step.rs                 # Step definition (velocity, prob, etc)
│   └── clock.rs                # BPM, sync, timing
│
├── input/
│   ├── mod.rs
│   ├── router.rs               # Route inputs to actions
│   ├── midi.rs                 # MIDI input handling
│   ├── keyboard.rs             # Keyboard shortcuts
│   └── osc.rs                  # OSC server
│
├── ui/
│   ├── mod.rs
│   ├── context.rs              # ImGui context setup
│   ├── style.rs                # Custom styling
│   ├── widgets/
│   │   ├── mod.rs
│   │   ├── pad_grid.rs         # 4x4 pad grid with thumbnails
│   │   ├── sequencer.rs        # Step sequencer UI
│   │   └── mixer.rs            # Channel mixer UI
│   └── windows/
│       ├── mod.rs
│       ├── main.rs             # Main layout
│       ├── browser.rs          # Sample browser
│       └── settings.rs         # Config window
│
└── util/
    ├── mod.rs
    ├── time.rs                 # Frame timing, FPS
    └── thread.rs               # Thread pools for async work
```

---

## Core Data Structures

### VideoSample

```rust
pub struct VideoSample {
    pub id: Uuid,
    pub name: String,
    pub filepath: PathBuf,
    
    // Playback
    pub duration: Duration,
    pub frame_count: u32,
    pub fps: f32,
    pub resolution: (u32, u32),
    
    // In/Out points (frame-accurate)
    pub in_point: u32,
    pub out_point: u32,
    
    // HAP specific
    pub hap_texture: Option<wgpu::Texture>,
    pub decoder: Option<Box<dyn HapDecoder>>,
    
    // Thumbnail
    pub thumbnail: Option<wgpu::Texture>,
}

impl VideoSample {
    pub fn load_hap(path: &Path) -> Result<Self>;
    pub fn get_frame(&mut self, frame: u32) -> Option<&wgpu::Texture>;
    pub fn generate_thumbnail(&mut self) -> Result<()>;
}
```

### SamplePad

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerMode {
    Gate,      // Play while held, stop on release
    Latch,     // Toggle on/off
    OneShot,   // Play once and stop
}

pub struct SamplePad {
    pub index: usize,           // 0-15 in bank
    pub name: String,
    pub color: [u8; 3],
    
    // Sample
    pub sample: Option<Arc<Mutex<VideoSample>>>,
    
    // Trigger settings
    pub trigger_mode: TriggerMode,
    pub loop_enabled: bool,
    pub speed: f32,             // Playback speed (-2.0 to 2.0)
    
    // State
    pub is_playing: bool,
    pub current_frame: f32,     // Sub-frame precision
    pub midi_note: Option<u8>,
    
    // Mixing
    pub volume: f32,            // 0.0 - 1.0
    pub blend_mode: BlendMode,
}

impl SamplePad {
    pub fn trigger(&mut self);
    pub fn release(&mut self);
    pub fn update(&mut self, dt: Duration);
    pub fn get_current_texture(&self) -> Option<&wgpu::Texture>;
}
```

### SequencerTrack

```rust
pub struct SequencerTrack {
    pub pad_index: usize,       // Which pad this track controls
    pub steps: Vec<Step>,       // 4-64 steps
    pub length: usize,          // Number of active steps
    
    // Playback
    pub current_step: usize,
    pub is_playing: bool,
    
    // Per-track settings
    pub muted: bool,
    pub solo: bool,
    pub probability_override: Option<f32>,
}

pub struct Step {
    pub active: bool,
    pub velocity: f32,          // 0.0 - 1.0
    pub probability: f32,       // 0.0 - 1.0 (chance to trigger)
    pub ratchet: u8,            // 1-8 repeats
    pub ratchet_spacing: f32,   // Time between ratchets
    
    // Per-step parameter locks (future)
    pub parameter_locks: HashMap<String, f32>,
}
```

### SequencerEngine

```rust
pub struct SequencerEngine {
    pub tracks: [SequencerTrack; 16],
    pub patterns: Vec<Pattern>,
    pub current_pattern: usize,
    pub queued_pattern: Option<usize>,
    
    // Timing
    pub bpm: f32,
    pub is_playing: bool,
    pub shuffle: f32,           // Swing amount
    
    // Recording
    pub is_recording: bool,
    pub record_quantize: QuantizeMode,
}

pub enum QuantizeMode {
    Off,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}
```

---

## Video Pipeline

### HAP Codec Strategy

HAP is key for performance - it stores frames as S3TC/DXT compressed textures that can be uploaded directly to GPU without decompression on CPU.

**On macOS**:
- Use AVFoundation with HAP plugin, or
- Custom HAP decoder using `core-graphics`/`core-video`
- Direct texture upload to Metal/wgpu

**Architecture**:

```
┌─────────────────────────────────────────────────────────────────┐
│                     VIDEO PIPELINE                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐          │
│  │   Source    │    │   Decode    │    │  GPU Upload │          │
│  │             │    │             │    │             │          │
│  │ • HAP file  │ →  │ • HAP: GPU  │ →  │ • Texture   │          │
│  │ • NDI       │    │   direct    │    │   pool      │          │
│  │ • Webcam    │    │ • Other:    │    │ • Async     │          │
│  │             │    │   CPU decode│    │   upload    │          │
│  └─────────────┘    └─────────────┘    └──────┬──────┘          │
│                                               │                  │
│                                               ↓                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    MIXER ENGINE (wgpu)                       ││
│  │                                                              ││
│  │   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     ││
│  │   │  8-Channel  │    │   Effects   │    │   Output    │     ││
│  │   │   Mixer     │ →  │    Chain    │ →  │   Buffer    │     ││
│  │   │             │    │             │    │             │     ││
│  │   │ • Blend     │    │ • HSB       │    │ • NDI out   │     ││
│  │   │   modes     │    │ • Kaleido   │    │ • Display   │     ││
│  │   │ • Layering  │    │ • Feedback  │    │ • ImGui     │     ││
│  │   │ • Layouts   │    │ • Delay     │    │   overlay   │     ││
│  │   └─────────────┘    └─────────────┘    └─────────────┘     ││
│  │                                                              ││
│  │   Feedback loop: Output → Delay Buffer → Next Frame         ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

### Mixer Shader Architecture

Multi-pass ping-pong rendering with per-channel uniforms:

```
For each active channel:
  1. Sample source texture (video) + destination texture (accumulated result)
  2. Apply blend mode in shader:
     - Normal, Add, Multiply, Screen
     - Overlay, Soft Light, Hard Light, Difference, Lighten, Darken
     - Chroma Key (green/blue screen)
     - Luma Key (brightness-based)
  3. Write to intermediate buffer or final output
  4. Swap read/write buffers for next channel
```

Key Design Decisions:
- **Per-channel uniform buffers**: Each of 8 channels has its own uniform buffer to avoid
  race conditions when updating parameters during multi-pass rendering
- **Ping-pong textures**: Two intermediate textures alternate as source/destination
- **Shader-based blending**: All blend modes implemented in WGSL shader, not fixed-function
- **Keying support**: Chroma and luma keying with configurable threshold and smoothness

---

## Sequencer Design

### Drum Machine Architecture

Unlike VP-404's single-track sequencer, Rusty-404 has 16 independent tracks (one per pad):

```
┌─────────────────────────────────────────────────────────────────┐
│                    SEQUENCER GRID                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│        Pad 0    Pad 1    Pad 2    ...   Pad 15                  │
│       ┌────┐   ┌────┐   ┌────┐          ┌────┐                  │
│  Step │ ▓▓ │   │    │   │ ▓▓ │          │    │   Step 0        │
│   0   │    │   │    │   │    │          │    │                  │
│       └────┘   └────┘   └────┘          └────┘                  │
│       ┌────┐   ┌────┐   ┌────┐          ┌────┐                  │
│  Step │    │   │ ▓▓ │   │    │          │ ▓▓ │   Step 1        │
│   1   │    │   │    │   │    │          │    │                  │
│       └────┘   └────┘   └────┘          └────┘                  │
│       ┌────┐   ┌────┐   ┌────┐          ┌────┐                  │
│  Step │ ▓▓ │   │    │   │ ▓▓ │          │    │   Step 2        │
│   2   │    │   │    │   │    │          │    │                  │
│       └────┘   └────┘   └────┘          └────┘                  │
│                                                                  │
│       ...                        ...                            │
│                                                                  │
│       ┌────┐   ┌────┐   ┌────┐          ┌────┐                  │
│  Step │    │   │ ▓▓ │   │    │          │ ▓▓ │   Step 31       │
│  31   │    │   │    │   │    │          │    │                  │
│       └────┘   └────┘   └────┘          └────┘                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern System

- 16+ patterns per bank
- Each pattern contains 16 tracks × 4-64 steps
- Pattern chain/queue for performance
- Live pattern switching (quantized to bar)

### Step Parameters

Each step has:
- **Active**: On/off
- **Velocity**: 0-100% (affects volume/opacity)
- **Probability**: 0-100% (chance to trigger)
- **Ratchet**: 1-8 repeats
- **Gate Length**: Duration the pad stays triggered (0.0-1.0, fraction of step)
- **Parameter Locks**: Per-step effect overrides (future)

### Gate Release System

Unlike traditional sequencers that only send note-on events, Rusty-404 tracks active gates:
- When a step triggers, a gate is opened with a duration based on `gate_length`
- The sequencer emits both `Trigger` and `Release` events
- This allows pads to automatically stop without manual intervention
- Works correctly with all trigger modes (Gate, Latch, One-Shot)

---

## Live Input → HAP Workflow

```
1. User hits RECORD on selected pad
   ↓
2. Capture starts (NDI or Webcam)
   ↓
3. Frames buffered in memory
   ↓
4. User stops recording
   ↓
5. Async conversion to HAP:
   a. Encode frames to HAP using hap-encoder
   b. Save to disk as .mov or .hap file
   c. Generate thumbnail
   ↓
6. Reload as VideoSample on the pad
   ↓
7. Ready for playback
```

**Performance Note**: HAP conversion happens in background thread to maintain 30fps. UI shows progress.

---

## I/O Architecture

### MIDI

```rust
pub struct MidiController {
    pub input: midir::MidiInputConnection,
    pub mappings: MidiMappings,
}

pub struct MidiMappings {
    pub pad_notes: [Option<u8>; 16],      // MIDI note per pad
    pub cc_mappings: HashMap<u8, Param>,  // CC → parameter
    pub clock_sync: bool,                  // MIDI clock sync
}
```

### OSC

```rust
pub struct OscServer {
    pub socket: UdpSocket,
    pub address: String,
    pub port: u16,
}

// OSC Commands:
// /rusty404/pad/0/trigger     (i: velocity)
// /rusty404/pad/0/release
// /rusty404/bank/load         (s: bank_name)
// /rusty404/transport/play
// /rusty404/transport/stop
// /rusty404/bpm               (f: bpm)
```

### NDI

**Input**: `ndi-sdk` or `ndi` crate for receiving NDI streams
**Output**: Send final composite as NDI stream for external mixers

---

## ImGui UI Design

### Custom Styling

Dark, SP-404 inspired aesthetic:
- Dark gray background (#1a1a1a)
- Colored pads matching sample colors
- Hardware-style controls
- Thumbnail previews on pads

### Main Layout

```
┌─────────────────────────────────────────────────────────────────┐
│  RUSTY-404                              [PERFORM] [EDIT] [SEQ]  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────┐  ┌─────────────────────────┐  │
│  │      4×4 PAD GRID           │  │     MIXER/FX SECTION    │  │
│  │                             │  │                         │  │
│  │  ┌───┐ ┌───┐ ┌───┐ ┌───┐   │  │  Channel 1 ████████░░   │  │
│  │  │ ▶ │ │   │ │ ▶ │ │   │   │  │  Channel 2 ████░░░░░░   │  │
│  │  └───┘ └───┘ └───┘ └───┘   │  │  ...                    │  │
│  │  ┌───┐ ┌───┐ ┌───┐ ┌───┐   │  │                         │  │
│  │  │   │ │ ▶ │ │   │ │ ▶ │   │  │  FX Chain:              │  │
│  │  └───┘ └───┘ └───┘ └───┘   │  │    [Kaleido] [Delay]    │  │
│  │     ... (16 pads)          │  │                         │  │
│  │                             │  │                         │  │
│  └─────────────────────────────┘  └─────────────────────────┘  │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │               STEP SEQUENCER (32 steps × 16 tracks)          ││
│  │                                                              ││
│  │  [Pad 0] ▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░   [M] [S]       ││
│  │  [Pad 1] ░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓░░▓▓   [M] [S]       ││
│  │  [Pad 2] ▓▓▓▓░░░░▓▓▓▓░░░░▓▓▓▓░░░░▓▓▓▓░░░░   [M] [S]       ││
│  │    ...                                                      ││
│  │                                                              ││
│  │  [PLAY] [STOP] [REC]  BPM: 128  [TAP]  Pattern: A1          ││
│  └─────────────────────────────────────────────────────────────┘│
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Views

1. **PERFORM Mode**: Pad grid large, minimal sequencer
2. **EDIT Mode**: Sample editing, in/out points, waveform
3. **SEQUENCE Mode**: Full sequencer grid, pattern editing

---

## Configuration & Persistence

### JSON Bank Format

```json
{
  "version": "1.0",
  "name": "Bank A",
  "pads": [
    {
      "index": 0,
      "name": "Kick Loop",
      "color": [255, 100, 100],
      "trigger_mode": "Gate",
      "loop_enabled": true,
      "speed": 1.0,
      "volume": 1.0,
      "midi_note": 36,
      "sample": {
        "filepath": "samples/kick_loop.hap",
        "in_point": 0,
        "out_point": 149,
        "duration_ms": 5000
      }
    }
  ],
  "patterns": [
    {
      "index": 0,
      "name": "Pattern 1",
      "length": 16,
      "tracks": [
        {
          "pad_index": 0,
          "steps": [
            {"active": true, "velocity": 1.0, "probability": 1.0, "ratchet": 1},
            {"active": false, ...}
          ]
        }
      ]
    }
  ],
  "midi_mappings": {
    "pad_notes": [36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51],
    "cc_mappings": {
      "1": "master_opacity"
    }
  }
}
```

---

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Resolution | 1280×720 | 720p output |
| Frame Rate | 30 fps | Consistent, no drops |
| Channels | 8 simultaneous | HAP decoded + mixed |
| Latency | < 50ms | End-to-end trigger to display |
| Startup | < 2s | App launch to ready |
| Bank Load | < 1s | 16 samples with thumbnails |

---

## Workspace Crates

The project is organized into a Cargo workspace with modular crates for HAP video processing:

### `crates/hap-parser`
Low-level HAP frame parser. Handles:
- HAP format type detection (Hap1, Hap5, HapY, HapA, Hap7, HapH)
- Snappy decompression
- DXT texture format detection
- Section header parsing

```rust
use hap_parser::HapFrame;

let frame = HapFrame::parse(&data)?;
println!("Format: {:?}, Size: {}x{}", 
    frame.texture_format, 
    frame.width, 
    frame.height
);
```

### `crates/hap-qt`
QuickTime/MP4 container parser. Handles:
- moov/trak/mdat atom parsing
- Sample table reading (stsd, stsz, stco, stsc, stts)
- Frame offset calculation
- HAP sample validation

```rust
use hap_qt::QtHapReader;

let reader = QtHapReader::open("video.mov")?;
println!("Resolution: {:?}", reader.resolution());
println!("Duration: {:.2}s", reader.duration());

for frame_idx in 0..reader.frame_count() {
    let hap_frame = reader.read_frame(frame_idx)?;
    // Use frame data...
}
```

### `crates/hap-wgpu`
GPU-accelerated HAP playback with wgpu. Features:
- Direct DXT texture upload to GPU (no CPU decompression)
- Playback state management (play/pause/seek)
- Loop modes (none/loop/palindrome)
- Frame caching for smooth playback
- Padded dimension handling (DXT requires multiples of 4)

```rust
use hap_wgpu::{HapPlayer, LoopMode};
use std::sync::Arc;

let device = Arc::new(device);  // wgpu::Device
let queue = Arc::new(queue);    // wgpu::Queue

let mut player = HapPlayer::open("video.mov", device, queue)?;
player.set_loop_mode(LoopMode::Loop);
player.play();

// In render loop:
if let Some(frame) = player.update() {
    // frame.texture and frame.view are ready to use
    // Bind frame.view to your render pipeline
}
```

---

## Main App Integration

The main `rusty-404` app uses the workspace crates through adapter modules:

### Video Decoder Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                   VIDEO DECODER STACK                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  App Layer                                                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  VideoSample::from_hap()                                │    │
│  │  └── uses HapWgpuDecoder (new, from hap-wgpu crate)    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                          │                                       │
│  Adapter Layer           ↓                                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  src/video/decoder/hap_wgpu.rs                          │    │
│  │  └── HapWgpuDecoder implements VideoDecoder trait      │    │
│  └─────────────────────────────────────────────────────────┘    │
│                          │                                       │
│  Crate Layer             ↓                                       │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  hap-wgpu::HapPlayer                                    │    │
│  │  ├── hap-qt::QtHapReader (QuickTime parsing)           │    │
│  │  └── hap-parser::HapFrame (frame decoding)             │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Usage in Sample Playback

```rust
// In SamplePad::get_current_frame()
pub fn get_current_frame(&mut self) -> Option<Arc<wgpu::Texture>> {
    let mut sample = self.sample.as_ref()?.try_lock().ok()?;
    let frame = self.current_frame as u32;
    sample.get_frame(frame)  // Uses HapWgpuDecoder
}
```

### File Import Flow

1. **VideoImporter::import()** checks if file is already HAP
2. **is_hap_file()** first tries native parser (`hap_qt::QtHapReader`)
3. Falls back to ffprobe if native parser fails
4. Converts to HAP using `HapEncoder` if needed
5. Loads via `VideoSample::from_hap()` which uses `HapWgpuDecoder`

### Texture Format Mapping

| HAP Type | DXT Format | wgpu Format | Bits/Pixel |
|----------|------------|-------------|------------|
| Hap1 | DXT1/BC1 | `Bc1RgbaUnorm` | 4 |
| Hap5 | DXT5/BC3 | `Bc3RgbaUnorm` | 8 |
| HapY | YCoCg-DXT5 | `Bc3RgbaUnorm` | 8 |
| HapA | BC4 | `Bc4RUnorm` | 4 |
| Hap7 | BC7 | `Bc7RgbaUnorm` | 8 |
| HapH | BC6H | `Bc6hRgbUfloat` | 8 |

---

## Dependencies

### Core
- `wgpu` - GPU compute/render
- `winit` - Windowing
- `imgui` + `imgui-wgpu` + `imgui-winit-support` - UI

### Video
- `hap` or custom HAP decoder
- `ffmpeg-next` - Fallback decoding
- `nokhwa` - Webcam capture
- `ndi-sdk` - NDI I/O

### Audio/MIDI/OSC
- `midir` - MIDI input
- `rosc` - OSC server

### Serialization
- `serde` + `serde_json` - Config/save files

### Async
- `tokio` - Async runtime for I/O

---

## Future Extensions

- [ ] Audio output (synced to video)
- [ ] Syphon/Spout output (macOS/Windows)
- [ ] Shader hot-reload
- [ ] Recording to disk
- [ ] Audio-reactive effects
- [ ] VST/AU plugin hosting
- [ ] Network sync (Ableton Link)
- [ ] TouchOSC template

---

## Next Steps

1. **Setup project structure** with Cargo workspace
2. **Implement wgpu context** and basic render loop
3. **HAP decoder** (macOS AVFoundation first)
4. **Pad engine** with sample loading
5. **Basic mixer** (2-channel, expand to 8)
6. **ImGui UI** with custom styling
7. **Sequencer engine** (polyphonic)
8. **Live input** (webcam → HAP)
9. **NDI output**
10. **MIDI/OSC** integration
