# Inter-App Video Sharing Design

## Overview

Support for sharing video between rusty-404 and other applications via:
- **macOS**: Syphon (IOSurface-based texture sharing)
- **Windows**: Spout (DirectX texture sharing)
- **Linux**: v4l2loopback (virtual video devices)

## Architecture

### Common Trait

```rust
/// Platform-agnostic inter-app video trait
pub trait InterAppVideo {
    /// Publish a frame to other applications
    fn publish_frame(&mut self, texture: &wgpu::Texture, queue: &wgpu::Queue);
    
    /// Try to receive a frame from other applications
    fn receive_frame(&mut self, device: &wgpu::Device) -> Option<wgpu::Texture>;
    
    /// Get the server/application name
    fn name(&self) -> &str;
}
```

### Platform-Specific Implementations

#### macOS: Syphon

**Output (Rusty-404 → Other Apps)**
- Create `SyphonServer` with Metal device from wgpu
- On each frame: copy wgpu texture to Metal texture
- Publish via IOSurface

**Input (Other Apps → Rusty-404)**
- Create `SyphonClient` subscribed to available servers
- Receive IOSurface from server
- Create wgpu texture from IOSurface

**Dependencies**:
- Bind to `Syphon.framework` using `objc` crate
- Use `metal-rs` for Metal interop with wgpu

#### Windows: Spout

**Output**
- Create Spout sender with DirectX11 shared texture
- Copy wgpu texture (DX11 backend) to shared texture
- Send frame via spout

**Input**
- Create Spout receiver
- Receive shared texture handle
- Create wgpu texture from shared handle

**Dependencies**:
- `spout2` C library or `spout-rs` bindings
- Requires wgpu to use DX11 backend for sharing

#### Linux: v4l2loopback

**Output**
- Open `/dev/videoX` (v4l2loopback device)
- Copy wgpu texture to memory-mapped buffer
- Write via v4l2 API

**Input**
- Standard video capture via `nokhwa` or `v4l2-rs`

**Dependencies**:
- `v4l2-rs` crate
- v4l2loopback kernel module

## Integration Points

### 1. Output Module Extension

```rust
// src/engine/output/mod.rs
pub enum OutputBackend {
    Window,           // Current: render to window
    Ndi,             // NDI output
    #[cfg(target_os = "macos")]
    Syphon,          // macOS only
    #[cfg(target_os = "windows")]
    Spout,           // Windows only
    #[cfg(target_os = "linux")]
    V4l2Loopback,    // Linux only
}
```

### 2. Input Module Extension

```rust
// src/video/capture/mod.rs
pub enum CaptureSource {
    Webcam { index: usize },
    Ndi { source_name: String },
    #[cfg(target_os = "macos")]
    Syphon { server_name: String },
    #[cfg(target_os = "windows")]
    Spout { sender_name: String },
}
```

### 3. UI Integration

**Output Selection**:
- Settings window: "Output Backend" dropdown
- When Syphon/Spout selected: "Server Name" input

**Input Selection**:
- Right-click pad: "Input Source" submenu
- List available Syphon servers / Spout senders
- Auto-refresh list every few seconds

## Implementation Phases

### Phase 1: macOS Syphon (Priority 1)

**Rationale**: Syphon is mature, well-documented, and widely used in the VJ community on macOS.

**Steps**:
1. Create `src/engine/output/syphon.rs`
2. Bind to Syphon.framework using `objc` and `objc-foundation`
3. Implement Metal texture sharing with wgpu
4. Add UI for Syphon server name
5. Test with Resolume, OBS, MadMapper

**Code Sketch**:
```rust
use objc::runtime::Object;
use metal::{Device, Texture};

pub struct SyphonOutput {
    server: *mut Object,  // SyphonServer*
    metal_device: Device,
    shared_texture: Option<Texture>,
}

impl SyphonOutput {
    pub fn new(name: &str, wgpu_device: &wgpu::Device) -> Self {
        // Get underlying Metal device from wgpu
        // Create SyphonServer with Metal device
        // Set up IOSurface sharing
    }
    
    pub fn publish(&mut self, wgpu_texture: &wgpu::Texture) {
        // Copy wgpu texture content to Metal texture
        // Publish to Syphon
    }
}
```

### Phase 2: Windows Spout

**Steps**:
1. Integrate `spout2` library
2. Implement DX11 texture sharing
3. Test with Resolume, TouchDesigner, OBS

### Phase 3: Linux v4l2loopback

**Steps**:
1. Implement using `v4l2-rs`
2. Test with OBS, VLC

## Technical Challenges

### 1. Texture Format Compatibility

- **Syphon**: Supports RGBA, BGRA, YUV - we use RGBA
- **Spout**: Supports RGBA8, BGRA8, RGBX8
- **v4l2**: Various formats - need conversion if needed

### 2. wgpu Backend Selection

- **macOS**: Must use Metal backend (required for Syphon)
- **Windows**: Must use DX11 backend for Spout (spout doesn't support DX12 yet)
- **Linux**: Vulkan works fine with v4l2

### 3. Performance

- **Zero-copy where possible**: Use shared textures, avoid CPU readback
- **Fallback**: If zero-copy fails, use intermediate buffer

### 4. Frame Rate Sync

- Publishers should respect the receiver's frame rate
- Option to enable/disable vsync for output

## API Design

### Configuration

```rust
// rusty404.toml
[output]
backend = "syphon"  # or "spout", "v4l2", "ndi", "window"
server_name = "Rusty-404 Main"

[input]
source = "syphon"
server_name = "Resolume Arena"
```

### Runtime Toggle

```rust
// In app::update()
if user_changed_output_backend {
    self.output = match new_backend {
        OutputBackend::Syphon => Box::new(SyphonOutput::new("Rusty-404", &device)),
        OutputBackend::Window => Box::new(WindowOutput::new()),
        // ...
    };
}
```

## Testing Strategy

1. **Output Tests**:
   - Send video to OBS (all platforms)
   - Send video to Resolume Arena (macOS/Windows)
   - Verify frame rate and latency

2. **Input Tests**:
   - Receive from Resolume
   - Receive from webcam (existing functionality)
   - Stress test: 8 channels receiving simultaneously

3. **Compatibility**:
   - Test color accuracy
   - Test alpha channel (transparency)
   - Test different resolutions

## Future Extensions

- **NDI**: Already planned, similar architecture
- **SPOUT2**: When Spout adds DX12 support
- **DeckLink**: Blackmagic capture cards
- **Virtual Camera**: macOS Virtual Camera API (Monterey+)

## References

- **Syphon**: https://syphon.v002.info/
- **Spout**: https://spout.zeal.co/
- **v4l2loopback**: https://github.com/umlaeute/v4l2loopback
- **wgpu Metal Interop**: https://github.com/gfx-rs/wgpu/issues/2770
