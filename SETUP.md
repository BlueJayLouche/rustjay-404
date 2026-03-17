# Rusty-404 Setup Guide

## Prerequisites

### 1. Rust Toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Syphon Framework (macOS)

Syphon is included locally in the workspace:

```bash
# The framework is already at:
# ../crates/syphon/syphon-lib/Syphon.framework
```

No installation needed - the build script handles linking.

### 3. FFmpeg with Snappy (for HAP encoding/conversion)

The `hap_convert` binary calls `ffmpeg` and `ffprobe` as system commands. FFmpeg must be built with snappy support for the HAP codec to work. The standard `brew install ffmpeg` does **not** include snappy — use `ffmpeg-full`:

```bash
brew install snappy     # usually already installed
brew install ffmpeg-full
```

`ffmpeg-full` is keg-only but Homebrew links it automatically. Verify HAP support:

```bash
ffmpeg -codecs 2>&1 | grep -i hap
# Expected: DEVIL. hap   Vidvox Hap
```

### 4. HAP Codecs (Optional)

For HAP video support, install the HAP QuickTime codec or use the built-in software decoder.

## Building

```bash
# Standard build
cargo build --release

# Run main app
cargo run --release

# Run examples
cargo run --bin simple_player
cargo run --bin test_decoder
```

## Syphon Usage

### Output (Send to other apps)

```rust
use crate::video::interapp::SyphonOutput;

// Create output
let output = SyphonOutput::new(
    "Rusty-404", 
    &device, 
    &queue, 
    1920, 
    1080
)?;

// Publish each frame
output.publish_frame(&texture, &device, &queue);
```

### Input (Receive from other apps)

```rust
use crate::video::interapp::{
    SyphonInputReceiver, 
    SyphonDiscovery,
    SyphonInputIntegration
};

// Simple receiver
let mut receiver = SyphonInputReceiver::new();
receiver.connect("Resolume Arena")?;

if let Some(frame) = receiver.try_receive() {
    let data = frame.data; // BGRA
}

// Or use high-level integration
let mut input = SyphonInputIntegration::new();
input.connect("Resolume Arena")?;
input.update(); // Poll for frames
```

## Troubleshooting

### "Library not loaded"

The local framework should be used automatically. If issues:

```bash
# Check framework exists
ls ../crates/syphon/syphon-lib/Syphon.framework/

# Rebuild with verbose output
cargo clean && cargo build --release
```

### Crash when connecting

Update to latest syphon-core with autoreleasepool fixes:

```bash
cd ../crates/syphon/syphon-core
cargo build
```

## Architecture

```
rusty-404/
├── src/
│   ├── video/
│   │   └── interapp/
│   │       ├── mod.rs           # InterAppVideo trait
│   │       ├── syphon.rs        # Output implementation
│   │       └── syphon_input.rs  # Input implementation (NEW)
│   └── ...
└── build.rs                     # Links local Syphon framework
```

## Links

- [Syphon Framework](https://github.com/Syphon/Syphon-Framework)
- [Project Syphon Crate](../crates/syphon/README.md)
