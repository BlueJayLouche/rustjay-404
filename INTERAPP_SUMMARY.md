# Inter-App Video Sharing - Implementation Summary

## Overview

Created the foundation for sharing video between rusty-404 and other VJ applications
(Resolume, OBS, TouchDesigner, MadMapper, etc.) via platform-specific APIs.

## Architecture Created

### Core Module: `src/video/interapp/`

```
src/video/interapp/
├── mod.rs           # Common trait and factory function
├── syphon.rs        # macOS implementation (stub)
├── spout.rs         # Windows implementation (stub)
└── v4l2loopback.rs  # Linux implementation (stub)
```

### Common Trait: `InterAppVideo`

All platform implementations share this interface:

```rust
pub trait InterAppVideo: Send + Sync {
    fn publish_frame(&mut self, texture: &wgpu::Texture, queue: &wgpu::Queue);
    fn receive_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<wgpu::Texture>;
    fn name(&self) -> &str;
    fn is_available() -> bool;
}
```

## Platform Support

### macOS: Syphon (Priority 1)

**Status**: Stubs created, ready for implementation

**Key Points**:
- Uses IOSurface for zero-copy GPU texture sharing
- Requires Metal interop with wgpu
- Uses Objective-C runtime via `objc` crate
- Framework: `Syphon.framework`

**Implementation Notes**:
```rust
// Getting Metal device from wgpu (required for Syphon)
let raw_device = device.as_hal::<wgpu::hal::api::Metal, _, _>(|device| {
    device.device().lock().clone()
});
```

**To Complete**:
1. Uncomment macOS dependencies in `Cargo.toml`
2. Implement Objective-C bindings to Syphon.framework
3. Test with Resolume and OBS

### Windows: Spout (Priority 2)

**Status**: Stubs created

**Key Points**:
- Uses DirectX 11 shared textures
- **Important**: Requires wgpu to use DX11 backend (not default DX12)
- Can use `spout2` C library via FFI

**Limitation**:
Spout currently requires DirectX 11, but wgpu defaults to DX12 on Windows.
Users would need to force DX11 backend:
```rust
let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
    backends: wgpu::Backends::DX11,  // Force DX11 for Spout
    ..Default::default()
});
```

### Linux: v4l2loopback (Priority 3)

**Status**: Stubs created

**Key Points**:
- Creates virtual video devices at `/dev/videoX`
- Requires `v4l2loopback` kernel module
- CPU readback required (slower than GPU sharing)
- Most compatible with Linux apps (Zoom, OBS, Chrome)

**Setup**:
```bash
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label="Rusty-404"
```

## Integration Points

### 1. Output Selection

Can be added to `rusty404.toml`:
```toml
[output]
backend = "syphon"  # or "spout", "v4l2", "window"
server_name = "Rusty-404 Main"
```

Or UI dropdown in Settings window.

### 2. Input Selection

Right-click pad → "Input Source" → List of available:
- Syphon servers (macOS)
- Spout senders (Windows)
- v4l2 devices (Linux)

### 3. Factory Pattern

```rust
use crate::video::interapp::{create_output, InterAppOutput};

// Create appropriate backend for platform
let output = create_output(
    InterAppOutput::Syphon,
    "Rusty-404",
    &device,
    1920,
    1080
);
```

## Next Steps for Implementation

### Phase 1: macOS Syphon (Recommended First)

1. **Add Dependencies** (uncomment in Cargo.toml):
   ```toml
   [target.'cfg(target_os = "macos")'.dependencies]
   objc = "0.2"
   objc-foundation = "0.1"
   metal = "0.24"
   ```

2. **Link Framework** (in `.cargo/config` or build script):
   ```rust
   println!("cargo:rustc-link-lib=framework=Syphon");
   println!("cargo:rustc-link-lib=framework=IOSurface");
   println!("cargo:rustc-link-lib=framework=Metal");
   ```

3. **Implement** in `syphon.rs`:
   - `SyphonOutput`: Publish frames
   - `SyphonInput`: Receive frames
   - Use `objc` crate to call Syphon Objective-C API

4. **Test**:
   ```bash
   cargo run --release
   # In Resolume: Source → Syphon → "Rusty-404"
   ```

### Phase 2: Windows Spout

1. Evaluate `spout-rs` crate vs direct `spout2` FFI
2. Handle DX11 backend requirement
3. Test with Resolume and TouchDesigner

### Phase 3: Linux v4l2loopback

1. Add `v4l2` or use existing `nokhwa` dependency
2. Implement RGBA→RGB conversion
3. Test with OBS

## Dependencies Summary

| Platform | Crate | Purpose | Status |
|----------|-------|---------|--------|
| macOS | `objc` | Objective-C runtime | Ready to add |
| macOS | `metal` | Metal interop | Ready to add |
| Windows | `windows` | DirectX 11 | Ready to add |
| Linux | `v4l2` | Video4Linux2 | Ready to add |

All commented out in Cargo.toml to avoid build issues until implementation begins.

## Testing Strategy

### macOS
- **Send to**: Resolume Arena, OBS, MadMapper, Millumin
- **Receive from**: Same apps + Canon EOS Webcam Utility

### Windows
- **Send to**: Resolume, OBS, TouchDesigner
- **Receive from**: Same apps

### Linux
- **Send to**: OBS (as webcam), Zoom, Chrome
- **Receive from**: Physical webcams, other v4l2 apps

## Performance Considerations

| Platform | Copy Type | Performance |
|----------|-----------|-------------|
| Syphon | GPU→GPU (IOSurface) | Excellent (zero-copy) |
| Spout | GPU→GPU (DX11 shared) | Excellent (zero-copy) |
| v4l2loopback | GPU→CPU→Kernel | Good (1 copy required) |

## Documentation

- **Design Doc**: `INTER_APP_VIDEO.md`
- **API Stubs**: `src/video/interapp/`
- **This Summary**: `INTERAPP_SUMMARY.md`
