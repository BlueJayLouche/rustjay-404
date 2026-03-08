# Rust HAP Video Decoder - Implementation Plan

## Overview

Create a native Rust HAP video decoder crate that provides smooth, GPU-accelerated playback without ffmpeg subprocess overhead.

## Why This Matters

- Current Rust video playback relies on ffmpeg subprocess (high latency) or CPU decoding (slow)
- HAP is the VJ industry standard for GPU-accelerated video
- No mature HAP decoder exists for Rust ecosystem

## Architecture

### Phase 1: Core HAP Frame Parser
**Goal:** Parse HAP frames from raw bytes

**Components:**
- `HapFrame` struct representing a single frame
- Section header parsing (4-byte and 8-byte variants)
- Top-level section type detection (DXT1, DXT5, YCoCg, etc.)
- Snappy decompression support

**Reference:**
- `HAP_reference_resources/hap/source/hap.c`
- Section 2.1 of HapVideoDRAFT.md

**Deliverable:** `hap-parser` crate that can parse individual HAP frames

---

### Phase 2: QuickTime Container Reader
**Goal:** Read HAP frames from .mov files without ffmpeg

**Components:**
- `QuickTimeReader` - parse moov, mdat, trak atoms
- Sample table parsing (stsz, stco, stsc boxes)
- Frame index construction
- Extract compressed HAP chunks

**Reference:**
- ofxHapPlayer Demuxer class
- ISO Base Media File Format spec

**Deliverable:** `hap-qt` crate that can read HAP from QuickTime containers

---

### Phase 3: GPU Upload (wgpu Integration)
**Goal:** Upload compressed DXT textures directly to GPU

**Components:**
- `HapTexture` - wgpu texture wrapper
- DXT1/DXT5 format support (wgpu has Bc1RgbaUnorm/Bc3RgbaUnorm)
- Compressed texture upload via `write_texture`

**Reference:**
- wgpu compressed texture formats
- ofxHapPlayer texture upload code

**Deliverable:** `hap-wgpu` crate for GPU playback

---

### Phase 4: Player with Packet Cache
**Goal:** Smooth playback with background loading

**Components:**
- `HapPlayer` - main playback interface
- Background demuxer thread (like ofxHapPlayer)
- Packet cache (compressed data, not decoded)
- Playback clock with loop/palindrome modes

**API:**
```rust
let mut player = HapPlayer::new("video.mov", &device, &queue)?;
player.play();
player.set_speed(-2.0);
player.set_loop_mode(LoopMode::Loop);

// In render loop
if let Some(texture) = player.update() {
    // Use texture
}
```

**Reference:**
- ofxHapPlayer.cpp update() and read() logic
- ofxHap::Clock for timing

---

## Implementation Strategy

### Week 1: HAP Frame Parser
1. Port `hap.c` frame parsing to Rust
2. Add snappy decompression (use `snap` crate)
3. Unit tests with sample HAP frames

### Week 2: QuickTime Container
1. Implement atom parsing (mp4-rust crate as reference)
2. Build frame index from sample tables
3. Test with real HAP files

### Week 3: wgpu Integration
1. Create compressed texture upload path
2. Handle format conversion (DXT1→Bc1, DXT5→Bc3)
3. Basic playback test

### Week 4: Player & Cache
1. Background demuxer thread
2. Packet cache with LRU eviction
3. Playback timing (clock)
4. Loop/seamless playback

## Key Differences from ofxHapPlayer

| Aspect | ofxHapPlayer | Our Approach |
|--------|--------------|--------------|
| Language | C++ | Rust |
| GPU API | OpenGL | wgpu (cross-platform) |
| Threading | pthread/std::thread | tokio/std::thread |
| Snappy | Custom | `snap` crate |
| Audio | Included | Separate concern |

## File Structure

```
rusty-hap/
├── hap-parser/          # Core HAP frame parsing
│   ├── src/lib.rs
│   └── tests/
├── hap-qt/              # QuickTime container reader
│   ├── src/lib.rs
│   └── tests/
├── hap-wgpu/            # GPU integration
│   ├── src/lib.rs
│   └── examples/
└── hap-player/          # High-level player
    ├── src/lib.rs
    └── examples/
```

## Testing Strategy

1. **Unit tests:** Parse individual HAP frames
2. **Integration tests:** Full playback of sample videos
3. **Performance tests:** 8+ simultaneous streams at 1080p
4. **Reference tests:** Compare frame output to ofxHapPlayer

## Success Criteria

- [ ] Parse all HAP variants (Hap1, Hap5, HapY, HapM)
- [ ] Smooth playback at 1x forward
- [ ] Seamless looping (no pause at boundary)
- [ ] Reverse playback at -1x without stutter
- [ ] Memory usage < 500MB for 8x 1080p clips
- [ ] CPU usage < 20% during playback (M1 Mac)

## Resources

- HAP spec: `HAP_reference_resources/hap/documentation/`
- Reference implementation: `ofxHapPlayer/`
- wgpu compressed textures: https://docs.rs/wgpu/latest/wgpu/enum.TextureFormat.html

## Next Steps

1. Review this plan
2. Set up repo structure
3. Start with `hap-parser` crate
4. Create test assets (small HAP files)

Would you like to proceed with Phase 1, or would you prefer to adjust the scope?
