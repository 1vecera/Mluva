//! Linux capture services. No device is enumerated or opened during construction.

pub mod capture;
pub mod catalog;
pub mod external;
pub mod meeting;
pub mod meeting_capture;
mod process;
pub mod recorder;
pub mod volatile;
pub mod wav;

use std::io;

pub const SAMPLE_RATE: u32 = 16_000;
pub const CHANNEL_COUNT: u16 = 1;
pub const SAMPLE_WIDTH_BYTES: usize = 2;
pub const REALTIME_CHUNK_FRAMES: usize = 1_600;
pub const REALTIME_CHUNK_BYTES: usize = REALTIME_CHUNK_FRAMES * SAMPLE_WIDTH_BYTES;
pub const PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES: &str = "{ \"stream.capture.sink\": true }";

#[derive(Debug, thiserror::Error)]
pub enum AudioCaptureError {
    #[error("{0}")]
    Message(&'static str),
    #[error("{0}")]
    Detail(String),
    #[error("{0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, AudioCaptureError>;

/// Normalize little-endian PCM16 RMS; incomplete final samples are ignored.
pub fn pcm16_audio_level(frames: &[u8]) -> f64 {
    let samples = frames.chunks_exact(SAMPLE_WIDTH_BYTES);
    let count = samples.len();
    if count == 0 {
        return 0.0;
    }
    let squares: u64 = samples
        .map(|sample| {
            let value = i64::from(i16::from_le_bytes([sample[0], sample[1]]));
            (value * value) as u64
        })
        .sum();
    ((squares as f64 / count as f64).sqrt() / 32_768.0).min(1.0)
}
