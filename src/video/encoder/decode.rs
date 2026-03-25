//! Native video decoder for MP4/H.264 files
//!
//! Uses the `mp4` crate for container parsing and `openh264` for H.264 decoding.
//! No ffmpeg dependency required for H.264 content in MP4/MOV containers.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

/// Streaming MP4/H.264 decoder
///
/// Decodes one frame at a time to keep memory usage low.
/// Detects cascading decode failures and returns an error so callers can
/// fall back to ffmpeg for streams OpenH264 can't handle.
pub struct Mp4Decoder {
    mp4: mp4::Mp4Reader<BufReader<File>>,
    decoder: openh264::decoder::Decoder,
    track_id: u32,
    sample_count: u32,
    current_sample: u32,
    nal_length_size: usize,
    width: u32,
    height: u32,
    fps: f32,
    consecutive_failures: u32,
}

impl Mp4Decoder {
    /// Open an MP4/MOV file for native H.264 decoding
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).context("Failed to open video file")?;
        let size = file.metadata()?.len();
        let reader = BufReader::new(file);

        let mut mp4 =
            mp4::Mp4Reader::read_header(reader, size).context("Failed to parse MP4 container")?;

        // Find video track
        let track_id = find_video_track(&mp4)?;
        let track = mp4
            .tracks()
            .get(&track_id)
            .ok_or_else(|| anyhow!("Video track not found"))?;

        let width = track.width() as u32;
        let height = track.height() as u32;
        let fps = track.frame_rate() as f32;
        let sample_count = track.sample_count();

        // Get AVCC configuration (SPS/PPS and NAL length size)
        let stsd = &track.trak.mdia.minf.stbl.stsd;
        let avc1 = stsd
            .avc1
            .as_ref()
            .ok_or_else(|| anyhow!("Not an H.264 video (no avc1 box found)"))?;
        let avcc = &avc1.avcc;
        let nal_length_size = (avcc.length_size_minus_one + 1) as usize;

        // Initialize H.264 decoder
        let mut decoder = openh264::decoder::Decoder::new()
            .map_err(|e| anyhow!("Failed to create H.264 decoder: {}", e))?;

        // Feed SPS/PPS parameter sets to initialize the decoder
        for sps in &avcc.sequence_parameter_sets {
            let mut annexb = vec![0x00, 0x00, 0x00, 0x01];
            annexb.extend_from_slice(&sps.bytes);
            let _ = decoder.decode(&annexb);
        }
        for pps in &avcc.picture_parameter_sets {
            let mut annexb = vec![0x00, 0x00, 0x00, 0x01];
            annexb.extend_from_slice(&pps.bytes);
            let _ = decoder.decode(&annexb);
        }

        log::info!(
            "Opened MP4 (H.264): {}x{} @ {:.2} fps, {} samples",
            width,
            height,
            fps,
            sample_count
        );

        Ok(Self {
            mp4,
            decoder,
            track_id,
            sample_count,
            current_sample: 1, // mp4 samples are 1-indexed
            nal_length_size,
            width,
            height,
            fps,
            consecutive_failures: 0,
        })
    }

    /// Decode the next frame as RGBA pixels
    ///
    /// Returns `None` when all samples have been consumed.
    /// Returns `Err` if the decoder encounters cascading failures (e.g. unsupported
    /// H.264 profile/level), allowing the caller to fall back to ffmpeg.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        // 3 consecutive failures means the decoder can't handle this stream
        // (once a reference frame fails, all dependent frames will also fail)
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        while self.current_sample <= self.sample_count {
            if self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                return Err(anyhow!(
                    "OpenH264 decoder failed on {} consecutive frames at sample {} — \
                     stream likely uses unsupported H.264 features (High profile at {}x{})",
                    self.consecutive_failures,
                    self.current_sample,
                    self.width,
                    self.height,
                ));
            }

            let sample_id = self.current_sample;
            self.current_sample += 1;

            let sample = self
                .mp4
                .read_sample(self.track_id, sample_id)
                .context(format!("Failed to read sample {}", sample_id))?;

            let sample = match sample {
                Some(s) => s,
                None => continue,
            };

            // Convert AVCC NAL units to Annex B format for openh264
            let annexb = avcc_to_annexb(&sample.bytes, self.nal_length_size);

            match self.decoder.decode(&annexb) {
                Ok(Some(yuv)) => {
                    self.consecutive_failures = 0;
                    // YUV420: UV planes are half-resolution, so full dims = 2x UV dims
                    let (uv_w, uv_h) = yuv.dimensions_uv();
                    let (w, h) = (uv_w * 2, uv_h * 2);
                    let mut rgba = vec![0u8; w * h * 4];
                    yuv.write_rgba8(&mut rgba);
                    return Ok(Some(rgba));
                }
                Ok(None) => {
                    // Decoder is buffering (B-frame reordering) - continue to next sample
                }
                Err(e) => {
                    self.consecutive_failures += 1;
                    log::warn!(
                        "Failed to decode sample {} ({}/{}): {}",
                        sample_id,
                        self.consecutive_failures,
                        MAX_CONSECUTIVE_FAILURES,
                        e
                    );
                }
            }
        }

        Ok(None)
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn fps(&self) -> f32 {
        self.fps
    }
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

/// Extract video metadata from an MP4/MOV file without decoding frames
pub fn probe_mp4(path: &Path) -> Result<super::VideoInfo> {
    let file = File::open(path).context("Failed to open video file")?;
    let size = file.metadata()?.len();
    let reader = BufReader::new(file);

    let mp4 =
        mp4::Mp4Reader::read_header(reader, size).context("Failed to parse MP4 container")?;

    let track_id = find_video_track(&mp4)?;
    let track = mp4
        .tracks()
        .get(&track_id)
        .ok_or_else(|| anyhow!("Video track not found"))?;

    Ok(super::VideoInfo {
        width: track.width() as u32,
        height: track.height() as u32,
        fps: track.frame_rate() as f32,
        frames: track.sample_count(),
    })
}

/// Extended probe info for the import system
pub struct ProbeInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub duration_secs: f32,
    pub codec: String,
    pub is_hap: bool,
}

/// Probe an MP4/MOV file for import-level metadata (codec, duration, etc.)
pub fn probe_mp4_extended(path: &Path) -> Result<ProbeInfo> {
    let file = File::open(path).context("Failed to open video file")?;
    let size = file.metadata()?.len();
    let reader = BufReader::new(file);

    let mp4 =
        mp4::Mp4Reader::read_header(reader, size).context("Failed to parse MP4 container")?;

    let track_id = find_video_track(&mp4)?;
    let track = mp4
        .tracks()
        .get(&track_id)
        .ok_or_else(|| anyhow!("Video track not found"))?;

    let duration_ms = track.duration().as_millis() as f32;
    let duration_secs = duration_ms / 1000.0;

    // Detect codec from sample description
    let stsd = &track.trak.mdia.minf.stbl.stsd;
    let (codec, is_hap) = if stsd.avc1.is_some() {
        ("h264".to_string(), false)
    } else if stsd.hev1.is_some() {
        ("hevc".to_string(), false)
    } else if stsd.vp09.is_some() {
        ("vp9".to_string(), false)
    } else {
        // Could be HAP or another codec
        let codec_str = track
            .media_type()
            .map(|mt| format!("{:?}", mt))
            .unwrap_or_else(|_| "unknown".to_string());
        let is_hap = codec_str.to_lowercase().contains("hap");
        (codec_str, is_hap)
    };

    Ok(ProbeInfo {
        width: track.width() as u32,
        height: track.height() as u32,
        fps: track.frame_rate() as f32,
        duration_secs,
        codec,
        is_hap,
    })
}

fn find_video_track<R: std::io::Read + std::io::Seek>(mp4: &mp4::Mp4Reader<R>) -> Result<u32> {
    for (track_id, track) in mp4.tracks() {
        if track
            .track_type()
            .map_or(false, |tt| tt == mp4::TrackType::Video)
        {
            return Ok(*track_id);
        }
    }
    Err(anyhow!("No video track found in MP4 container"))
}

/// Convert AVCC-format NAL units (length-prefixed) to Annex B format (start-code prefixed)
fn avcc_to_annexb(data: &[u8], nal_length_size: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len() + 64);
    let mut offset = 0;

    while offset + nal_length_size <= data.len() {
        // Read big-endian NAL unit length
        let mut length = 0usize;
        for i in 0..nal_length_size {
            length = (length << 8) | data[offset + i] as usize;
        }
        offset += nal_length_size;

        if length == 0 || offset + length > data.len() {
            break;
        }

        // Replace length prefix with Annex B start code
        result.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        result.extend_from_slice(&data[offset..offset + length]);
        offset += length;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avcc_to_annexb() {
        // 4-byte length prefix: length=5, then 5 bytes of data
        let avcc = vec![0x00, 0x00, 0x00, 0x05, 0x65, 0x01, 0x02, 0x03, 0x04];
        let annexb = avcc_to_annexb(&avcc, 4);
        assert_eq!(
            annexb,
            vec![0x00, 0x00, 0x00, 0x01, 0x65, 0x01, 0x02, 0x03, 0x04]
        );
    }

    #[test]
    fn test_avcc_to_annexb_multiple_nals() {
        // Two NAL units: length=2 + data, length=3 + data
        let avcc = vec![
            0x00, 0x00, 0x00, 0x02, 0xAA, 0xBB, 0x00, 0x00, 0x00, 0x03, 0xCC, 0xDD, 0xEE,
        ];
        let annexb = avcc_to_annexb(&avcc, 4);
        assert_eq!(
            annexb,
            vec![0x00, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0x00, 0x00, 0x00, 0x01, 0xCC, 0xDD, 0xEE]
        );
    }
}
