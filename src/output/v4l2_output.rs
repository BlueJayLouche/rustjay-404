//! V4L2 loopback output (Linux only)
//!
//! Writes BGRA frames to a `/dev/videoN` virtual device created by the
//! `v4l2loopback` kernel module.  Any V4L2-capable app (OBS, Zoom, ffplay,
//! browsers) can read from it as though it were a real webcam.
//!
//! ## Prerequisites
//!
//! Load the kernel module before use:
//! ```bash
//! sudo modprobe v4l2loopback devices=1 video_nr=10 \
//!     card_label="RustJay 404" exclusive_caps=1
//! ```
//! Or install `v4l2loopback-dkms` for a persistent udev-rule-based setup.

#![cfg(target_os = "linux")]

use anyhow::Context;
use v4l::{
    buffer::Type,
    format::fourcc::FourCC,
    io::{mmap::Stream, traits::OutputStream},
    video::Output,
    Device,
};

/// Writes BGRA frames to a V4L2 loopback device (BGR3 on the wire).
///
/// # Lifetime note
///
/// `v4l::io::mmap::Stream<'a>` normally borrows `&'a Device`.  Internally
/// both `Device` and `Stream` hold an `Arc<Handle>` that wraps the file
/// descriptor, so the stream keeps the FD alive independently.  We transmute
/// the borrow lifetime to `'static` after verifying this and drop the `Device`
/// after stream creation — the FD remains alive inside the stream's `Arc`.
pub struct V4l2LoopbackOutput {
    /// mmap output stream — owns the underlying FD via Arc<Handle>.
    stream: Stream<'static>,
    pub width: u32,
    pub height: u32,
    device_path: String,
}

impl V4l2LoopbackOutput {
    /// Open `device_path` (e.g. `"/dev/video10"`), negotiate BGR3 format, and
    /// start the mmap output stream.
    pub fn new(device_path: &str, width: u32, height: u32) -> anyhow::Result<Self> {
        let device =
            Device::with_path(device_path).context("Failed to open V4L2 device")?;

        // Read whatever format is currently set, then override the fields we
        // care about.  This preserves any driver-specific flags we don't know.
        let mut fmt = device
            .format()
            .context("Failed to query V4L2 output format")?;
        fmt.width = width;
        fmt.height = height;
        fmt.fourcc = FourCC::new(b"BGR3"); // 24-bit BGR, no alpha

        device
            .set_format(&fmt)
            .context("Failed to set V4L2 output format (BGR3)")?;

        log::info!(
            "[V4L2] Format set: {} {}x{} BGR3",
            device_path,
            width,
            height
        );

        // Allocate 4 mmap output buffers.
        //
        // SAFETY: `Stream<'_>` holds `Arc<Handle>` (the FD) independently.
        // Once constructed the stream keeps the device alive; we can drop the
        // `Device` value.  Transmuting to `'static` is sound because the FD
        // referenced by the phantom lifetime stays alive inside the Arc.
        let stream_borrowed =
            Stream::with_buffers(&device, Type::VideoOutput, 4)
                .context("Failed to allocate V4L2 output buffers")?;
        let stream: Stream<'static> = unsafe { std::mem::transmute(stream_borrowed) };
        // `device` drops here — FD survives in stream's Arc<Handle>

        log::info!("[V4L2] Output stream started on {}", device_path);

        Ok(Self {
            stream,
            width,
            height,
            device_path: device_path.to_string(),
        })
    }

    /// Write one frame of BGRA pixel data to the loopback device.
    ///
    /// Converts BGRA → BGR3 (drops the alpha channel) inline into the
    /// kernel-mapped buffer — no intermediate allocation.
    ///
    /// `bgra` must be at least `width * height * 4` bytes.
    pub fn send_frame(&mut self, bgra: &[u8]) -> anyhow::Result<()> {
        let (buf, _meta) = self
            .stream
            .next()
            .context("V4L2 output stream: next() failed")?;

        let expected_bgra = (self.width * self.height * 4) as usize;
        if bgra.len() < expected_bgra {
            anyhow::bail!(
                "V4L2 send_frame: input too small ({} < {} bytes)",
                bgra.len(),
                expected_bgra
            );
        }

        let expected_bgr = (self.width * self.height * 3) as usize;
        if buf.len() < expected_bgr {
            anyhow::bail!(
                "V4L2 mmap buffer too small ({} < {} bytes)",
                buf.len(),
                expected_bgr
            );
        }

        // BGRA → BGR3: write 3 bytes per pixel, skip the alpha.
        for (dst, src) in buf[..expected_bgr]
            .chunks_exact_mut(3)
            .zip(bgra.chunks_exact(4))
        {
            dst[0] = src[0]; // B
            dst[1] = src[1]; // G
            dst[2] = src[2]; // R
        }

        Ok(())
    }

    pub fn device_path(&self) -> &str {
        &self.device_path
    }
}
