# Proposal: syphon Crate Workspace

## Summary

Create a modular Rust workspace for Syphon inter-app video sharing on macOS,
following the successful pattern of the `hap-*` crates.

## Motivation

Similar to how we modularized HAP video decoding:
- **syphon-core**: Low-level bindings (like hap-parser)
- **syphon-metal**: Metal interop (like hap-qt for containers)
- **syphon-wgpu**: High-level integration (like hap-wgpu)

This structure allows:
- Other GPU libraries (glium, glow, raw Metal) to use syphon-core/syphon-metal
- The wgpu ecosystem to have first-class Syphon support
- Community contributions for other backends

## Workspace Structure

```
syphon/
├── Cargo.toml                    # Workspace root
├── syphon-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               # Main exports
│       ├── error.rs             # Error types
│       ├── server.rs            # SyphonServer
│       ├── client.rs            # SyphonClient
│       ├── directory.rs         # SyphonServerDirectory
│       └── image.rs             # SyphonImage (IOSurface wrapper)
├── syphon-metal/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs               # SyphonMetalTexture + utilities
├── syphon-wgpu/                 # PRIMARY USER-FACING CRATE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               # Re-exports + utilities
│       ├── output.rs            # SyphonOutput
│       └── input.rs             # SyphonInput
└── syphon-examples/
    └── src/
        ├── simple_sender.rs     # Publish to Syphon
        ├── simple_receiver.rs   # Receive from Syphon
        └── wgpu_integration.rs  # Full wgpu example
```

## API Design

### User-Facing API (syphon-wgpu)

```rust
use syphon_wgpu::{SyphonOutput, SyphonInput, list_servers};

// Publishing
let mut output = SyphonOutput::new("My App", &device, 1920, 1080)?;
output.publish(&my_rendered_texture, &queue);

// Receiving
let mut input = SyphonInput::connect("Resolume Arena")?;
if let Some(texture) = input.try_receive(&device) {
    // Use texture...
}

// Discovery
for server in list_servers() {
    println!("{}: {}x{}", server.name, server.width, server.height);
}
```

## Technical Challenges

### 1. wgpu HAL Access

**Problem**: Need to get underlying Metal textures from wgpu

**Solution**:
```rust
// In syphon-wgpu/src/lib.rs
pub fn get_metal_device(wgpu_device: &wgpu::Device) -> metal::Device {
    wgpu_device.as_hal::<wgpu::hal::api::Metal, _, _>(|device| {
        device.device().lock().clone()
    })
}
```

### 2. Texture Copy

**Problem**: wgpu textures need to be copied to IOSurface-backed Metal textures

**Solutions**:
1. **Blit pass**: Use wgpu command encoder to copy to Metal texture
2. **Render pass**: Draw fullscreen quad to IOSurface texture
3. **Direct IOSurface**: Create wgpu texture from IOSurface (requires HAL support)

### 3. Objective-C Bindings

**Approach**: Use `objc` crate with manual bindings

```rust
use objc::runtime::Object;
use objc::{msg_send, sel, sel_impl};

let server: *mut Object = unsafe {
    let cls = class!(SyphonServer);
    let obj: *mut Object = msg_send![cls, alloc];
    let obj: *mut Object = msg_send![obj, initWithName:ns_name];
    obj
};
```

## Comparison to hap-* Crates

| Aspect | hap-* | syphon-* |
|--------|-------|----------|
| **Scope** | File format | Runtime IPC |
| **Platforms** | Cross-platform | macOS only |
| **Dependencies** | Pure Rust + Snappy | Objective-C |
| **Complexity** | Binary parsing | GPU interop |
| **Publish to crates.io?** | ✅ Yes | ✅ Yes (macOS only) |

## Implementation Timeline

### Week 1: syphon-core
- Objective-C bindings for Syphon.framework
- Basic server/client/directory types
- Error handling

### Week 2: syphon-metal + syphon-wgpu
- Metal texture utilities
- wgpu interop (HAL access)
- Texture copy implementation

### Week 3: Examples + Polish
- Simple sender/receiver examples
- wgpu integration example
- Documentation
- Publish to crates.io

## Publishing Strategy

### Crate Names
- `syphon-core` - Low-level bindings
- `syphon-metal` - Metal utilities
- `syphon-wgpu` - Main integration (most downloads expected)
- `syphon` - Meta-crate that re-exports syphon-wgpu

### Documentation
- Book-style guide at `docs.rs/syphon-wgpu`
- Examples for common use cases
- Migration guide from Objective-C Syphon

### CI/CD
- GitHub Actions on macOS runners
- Test with real Syphon servers
- Automated publishing on tag

## Benefits to Rust Ecosystem

1. **Creative Coding**: Bevy, nannou, macroquad apps can share video
2. **VJ Tools**: Rust-based VJ apps can interoperate with Resolume/etc
3. **Pro A/V**: Professional video workflows on macOS
4. **Learning**: Example of wgpu ↔ native GPU interop

## Future Extensions

- **syphon-glium**: OpenGL integration
- **syphon-bevy**: Bevy engine plugin
- **syphon-ffmpeg**: Bridge to ffmpeg

## Conclusion

The syphon crate workspace would fill a gap in the Rust multimedia ecosystem,
providing the same quality of Syphon bindings that we achieved with HAP.

The modular structure allows users to choose their level of abstraction:
- `syphon-core` for custom integrations
- `syphon-wgpu` for the 90% use case

**Recommendation**: Proceed with implementation, starting with syphon-core.
