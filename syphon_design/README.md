# Syphon Crate Workspace Design

A modular Rust workspace for macOS Syphon frame sharing, similar in structure to the hap-* crates.

## Workspace Structure

```
syphon/
├── Cargo.toml           # Workspace root
├── syphon-core/         # Low-level Objective-C bindings
├── syphon-metal/        # Metal texture interop
├── syphon-wgpu/         # wgpu integration (primary use case)
└── syphon-examples/     # Example applications
```

## Crate Details

### 1. syphon-core

**Purpose**: Low-level bindings to Syphon.framework

**Key Types**:
```rust
pub struct SyphonServer {
    inner: *mut Object,  // SyphonServer*
}

pub struct SyphonClient {
    inner: *mut Object,  // SyphonClient*
}

pub struct SyphonServerDirectory {
    // Wrapper around SyphonServerDirectory
}

pub struct SyphonImage {
    // Wraps IOSurface + texture info
    width: u32,
    height: u32,
    io_surface: IOSurfaceRef,
}
```

**Features**:
- List available servers
- Create/destroy servers
- Create/destroy clients
- Publish frames (IOSurface-based)
- Receive frames

**Dependencies**:
- `objc` - Objective-C runtime
- `objc-foundation` - Foundation types
- `core-foundation` - CF types (IOSurface)
- `metal` - For MTLTexture access

---

### 2. syphon-metal

**Purpose**: Metal-specific texture utilities

**Key Types**:
```rust
/// Trait for Metal-aware texture providers
pub trait MetalTextureProvider {
    fn as_metal_texture(&self) -> &metal::Texture;
}

/// Wraps a Metal texture for Syphon publishing
pub struct SyphonMetalTexture {
    texture: metal::Texture,
    io_surface: IOSurfaceRef,
}

impl SyphonMetalTexture {
    /// Create from existing Metal texture
    pub fn from_metal_texture(texture: metal::Texture) -> Self;
    
    /// Create new IOSurface-backed texture
    pub fn new(device: &metal::Device, width: u32, height: u32) -> Self;
    
    /// Get the IOSurface for Syphon
    pub fn io_surface(&self) -> IOSurfaceRef;
}
```

---

### 3. syphon-wgpu

**Purpose**: High-level wgpu integration (the main user-facing crate)

**Key Types**:
```rust
/// Syphon output from a wgpu application
pub struct SyphonOutput {
    server: syphon_core::SyphonServer,
    metal_texture: syphon_metal::SyphonMetalTexture,
    // wgpu-specific
    device: wgpu::Device,
    blit_pipeline: wgpu::RenderPipeline,  // For copying to metal
}

impl SyphonOutput {
    /// Create from wgpu device
    pub fn new(
        name: &str,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Self, Error>;
    
    /// Publish a wgpu texture
    pub fn publish(&mut self, texture: &wgpu::Texture, queue: &wgpu::Queue);
}

/// Syphon input to a wgpu application
pub struct SyphonInput {
    client: syphon_core::SyphonClient,
    // ...
}

impl SyphonInput {
    /// Connect to a server by name
    pub fn connect(name: &str) -> Result<Self, Error>;
    
    /// Try to receive a frame as wgpu texture
    pub fn try_receive(&mut self, device: &wgpu::Device) -> Option<wgpu::Texture>;
}
```

---

## Usage Example

```rust
use syphon_wgpu::{SyphonOutput, SyphonInput};

// Create output (publish to other apps)
let mut output = SyphonOutput::new(
    "My Rust App",
    &device,
    1920,
    1080
).expect("Failed to create Syphon output");

// In render loop
output.publish(&my_texture, &queue);

// Create input (receive from other apps)
let mut input = SyphonInput::connect("Resolume Arena").expect("Server not found");

// Try to receive
if let Some(texture) = input.try_receive(&device) {
    // Use texture...
}
```

---

## Technical Challenges & Solutions

### Challenge 1: wgpu to Metal Interop

**Problem**: wgpu abstracts the graphics backend. We need to get the underlying Metal device/texture.

**Solution**: Use wgpu's HAL (Hardware Abstraction Layer):

```rust
// Get Metal device from wgpu device
let metal_device = device.as_hal::<wgpu::hal::api::Metal, _, _>(|device| {
    device.device().lock().clone()
});

// Get Metal texture from wgpu texture  
let metal_texture = texture.as_hal::<wgpu::hal::api::Metal, _, _>(|texture| {
    texture.texture().clone()
});
```

### Challenge 2: Platform-Specific Code

**Problem**: Syphon is macOS-only.

**Solution**: Proper cfg gating:

```rust
#![cfg(target_os = "macos")]

#[cfg(not(target_os = "macos"))]
compile_error!("Syphon is only available on macOS");
```

### Challenge 3: Objective-C Memory Management

**Problem**: Syphon uses Objective-C objects that need proper retain/release.

**Solution**: Use `objc::rc::StrongPtr` or manual retain counting via `msg_send!`:

```rust
use objc::rc::StrongPtr;
use objc::runtime::Object;
use objc::{msg_send, sel, sel_impl};

let server: *mut Object = unsafe {
    let cls = class!(SyphonServer);
    let obj: *mut Object = msg_send![cls, alloc];
    let obj: *mut Object = msg_send![obj, initWithName:name];
    obj
};

// Wrap in StrongPtr for auto-release
let server = unsafe { StrongPtr::new(server) };
```

### Challenge 4: IOSurface Lifecycle

**Problem**: IOSurfaces need to be properly managed for zero-copy sharing.

**Solution**: Create IOSurface-backed Metal textures:

```rust
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::base::TCFType;
use io_surface::IOSurface;

fn create_iosurface_backed_texture(
    device: &metal::Device,
    width: u32,
    height: u32,
) -> metal::Texture {
    // Create IOSurface properties
    let properties = CFDictionary::from_CFType_pairs(&[
        // ...
    ]);
    
    let io_surface = IOSurface::new(&properties);
    
    // Create Metal texture from IOSurface
    let descriptor = metal::TextureDescriptor::new();
    descriptor.set_width(width as u64);
    descriptor.set_height(height as u64);
    descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
    
    device.new_texture_from_iosurface(&io_surface, &descriptor)
}
```

---

## Build Configuration

### Cargo.toml (syphon-core)

```toml
[package]
name = "syphon-core"
version = "0.1.0"
edition = "2021"

[dependencies]
objc = "0.2"
objc-foundation = "0.1"
core-foundation = "0.9"
core-graphics = "0.23"
metal = "0.24"

[build-dependencies]
# Optional: bindgen if we want to generate bindings from headers

[package.metadata.system-deps]
# Framework dependencies
core-foundation = { framework = true }
core-graphics = { framework = true }
iokit = { framework = true }
iosurface = { framework = true }
metal = { framework = true }
metalkit = { framework = true }
syphon = { framework = true }  # User needs to install Syphon.framework
```

### Build Script

```rust
// build.rs
fn main() {
    println!("cargo:rustc-link-lib=framework=Syphon");
    println!("cargo:rustc-link-lib=framework=IOSurface");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}
```

---

## Comparison to hap-* Crates

| Aspect | hap-* | syphon-* |
|--------|-------|----------|
| **Scope** | File format parsing | Runtime inter-app sharing |
| **Platforms** | Cross-platform | macOS only |
| **Dependencies** | Pure Rust + Snappy | Objective-C frameworks |
| **Complexity** | Medium (binary parsing) | High (Obj-C interop, GPU) |
| **Zero-copy** | Yes (GPU upload) | Yes (IOSurface sharing) |
| **Use case** | File playback | Real-time sharing |

---

## Publishing to crates.io

### Naming

- `syphon-core` - Low-level bindings
- `syphon-metal` - Metal utilities  
- `syphon-wgpu` - wgpu integration (most users will use this)

### Documentation

- Each crate needs comprehensive rustdocs
- Examples for each crate
- Book-style guide for common patterns

### Testing

- Unit tests where possible (mock Obj-C runtime?)
- Integration tests requiring macOS + Syphon
- CI on macOS runners

---

## Future Extensions

1. **syphon-glium**: Integration with glium (OpenGL)
2. **syphon-bevy**: Bevy engine plugin
3. **syphon-ffmpeg**: Pipe to/from ffmpeg

---

## Implementation Priority

1. **syphon-core**: Foundation with basic server/client
2. **syphon-metal**: Metal texture utilities
3. **syphon-wgpu**: Full wgpu integration (main deliverable)
4. **Examples**: Real-world usage demos
5. **Documentation**: Publish to crates.io

## Estimation

- **syphon-core**: 2-3 days (Objective-C binding complexity)
- **syphon-metal**: 1 day (mostly wrapper types)
- **syphon-wgpu**: 2-3 days (HAL interop testing)
- **Examples + Docs**: 1-2 days

**Total**: ~1 week for MVP
