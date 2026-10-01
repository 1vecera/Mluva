//! Bounded RIFF/PCM storage and mixing. Metadata checks retain released WAV semantics.

use crate::{CHANNEL_COUNT, SAMPLE_RATE, SAMPLE_WIDTH_BYTES};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Debug)]
struct RiffBoundaryError;
impl std::fmt::Display for RiffBoundaryError {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}
impl std::error::Error for RiffBoundaryError {}

pub struct Pcm16Writer {
    file: File,
    bytes: u32,
    first_write: bool,
}

impl Pcm16Writer {
    pub fn new(mut file: File) -> io::Result<Self> {
        file.write_all(&header(0))?;
        Ok(Self {
            file,
            bytes: 0,
            first_write: true,
        })
    }

    pub fn write_frames(&mut self, frames: &[u8]) -> io::Result<()> {
        let count =
            u32::try_from(frames.len()).map_err(|_| invalid("WAV exceeds the RIFF size limit."))?;
        let bytes = self
            .bytes
            .checked_add(count)
            .filter(|bytes| *bytes <= u32::MAX - 36)
            .ok_or_else(|| invalid("WAV exceeds the RIFF size limit."))?;
        if self.first_write {
            // The released recorder declares the first chunk before appending PCM.
            // Keep that recovery metadata if the app dies before normal finalization.
            self.file.seek(SeekFrom::Start(0))?;
            self.file.write_all(&header(count))?;
            self.first_write = false;
        }
        self.file.write_all(frames)?;
        self.bytes = bytes;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.bytes))?;
        self.file.flush()?;
        self.file.set_permissions(fs::Permissions::from_mode(0o600))
    }
}

fn header(bytes: u32) -> [u8; 44] {
    let mut header = [0; 44];
    header[..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(36 + bytes).to_le_bytes());
    header[8..16].copy_from_slice(b"WAVEfmt ");
    header[16..20].copy_from_slice(&16_u32.to_le_bytes());
    header[20..22].copy_from_slice(&1_u16.to_le_bytes());
    header[22..24].copy_from_slice(&CHANNEL_COUNT.to_le_bytes());
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&(SAMPLE_RATE * SAMPLE_WIDTH_BYTES as u32).to_le_bytes());
    header[32..34].copy_from_slice(&(SAMPLE_WIDTH_BYTES as u16).to_le_bytes());
    header[34..36].copy_from_slice(&16_u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&bytes.to_le_bytes());
    header
}

#[derive(Clone, Copy, Debug)]
pub struct WaveMetadata {
    pub channels: u16,
    pub sample_width: u16,
    pub sample_rate: u32,
    pub frame_count: u32,
}

pub struct WaveReader {
    file: File,
    remaining: u64,
    pub metadata: WaveMetadata,
}

impl WaveReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut riff = [0; 12];
        file.read_exact(&mut riff).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(io::ErrorKind::UnexpectedEof, "")
            } else {
                error
            }
        })?;
        if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
            return Err(invalid("Not a RIFF/WAVE recording."));
        }
        let end = 8 + u64::from(u32::from_le_bytes(riff[4..8].try_into().unwrap()));
        let mut format = None;
        loop {
            let position = file.stream_position()?;
            if position + 8 > end {
                return Err(invalid("WAV format or data chunk is missing."));
            }
            let mut chunk = [0; 8];
            file.read_exact(&mut chunk)?;
            let size = u32::from_le_bytes(chunk[4..].try_into().unwrap());
            let body = position + 8;
            if &chunk[..4] == b"data" {
                let (channels, sample_width, sample_rate) =
                    format.ok_or_else(|| invalid("WAV data precedes its format."))?;
                return Ok(Self {
                    file,
                    remaining: u64::from(size).min(end.saturating_sub(body)),
                    metadata: WaveMetadata {
                        channels,
                        sample_width,
                        sample_rate,
                        frame_count: size / (u32::from(channels) * u32::from(sample_width)),
                    },
                });
            }
            if &chunk[..4] == b"fmt " {
                if size < 16 || body + 16 > end {
                    return Err(invalid("WAV format is truncated."));
                }
                let mut fields = [0; 16];
                file.read_exact(&mut fields)?;
                let tag = u16::from_le_bytes(fields[..2].try_into().unwrap());
                let channels = u16::from_le_bytes(fields[2..4].try_into().unwrap());
                let sample_rate = u32::from_le_bytes(fields[4..8].try_into().unwrap());
                let bits = u16::from_le_bytes(fields[14..].try_into().unwrap());
                let width = bits.div_ceil(8);
                if width == 0 || channels == 0 || !matches!(tag, 1 | 0xfffe) {
                    return Err(invalid("WAV is not uncompressed PCM."));
                }
                if tag == 0xfffe {
                    if size < 40 || body + 40 > end {
                        return Err(invalid("Extended WAV format is truncated."));
                    }
                    let mut extension = [0; 24];
                    file.read_exact(&mut extension)?;
                    if extension[8..] != [1, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]
                    {
                        return Err(invalid("Extended WAV format is not PCM."));
                    }
                }
                format = Some((channels, width, sample_rate));
            }
            let next = body + u64::from(size) + u64::from(size % 2);
            if next > end {
                return Err(io::Error::other(RiffBoundaryError));
            }
            file.seek(SeekFrom::Start(next))?;
        }
    }

    pub fn compatible(&self) -> bool {
        self.metadata.channels == CHANNEL_COUNT
            && usize::from(self.metadata.sample_width) == SAMPLE_WIDTH_BYTES
            && self.metadata.sample_rate == SAMPLE_RATE
            && self.metadata.frame_count > 0
    }

    pub fn duration_seconds(&self) -> f64 {
        f64::from(self.metadata.frame_count) / f64::from(self.metadata.sample_rate)
    }

    pub fn read_frames(&mut self, count: usize) -> io::Result<Vec<u8>> {
        let frame_size =
            usize::from(self.metadata.channels) * usize::from(self.metadata.sample_width);
        let limit = count
            .saturating_mul(frame_size)
            .min(self.remaining as usize);
        let mut frames = vec![0; limit];
        let mut filled = 0;
        while filled < limit {
            match self.file.read(&mut frames[filled..]) {
                Ok(0) => break,
                Ok(count) => filled += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        self.remaining -= filled as u64;
        frames.truncate(filled);
        Ok(frames)
    }
}

pub fn is_compatible_pcm_wav(path: &Path) -> io::Result<bool> {
    match WaveReader::open(path) {
        Ok(recording) => Ok(recording.compatible()),
        Err(error)
            if error
                .get_ref()
                .is_some_and(|error| error.is::<RiffBoundaryError>()) =>
        {
            Err(error)
        }
        Err(_) => Ok(false),
    }
}

/// Average paired samples with truncation toward zero; retain the longer source's tail.
pub fn mix_pcm16_wav(microphone: &Path, system: &Path, output: &Path) -> io::Result<()> {
    let file = super::process::create_private_file(output)?;
    let result = (|| {
        let mut microphone = WaveReader::open(microphone)?;
        let mut system = WaveReader::open(system)?;
        let mut writer = Pcm16Writer::new(file)?;
        loop {
            let microphone = microphone.read_frames(4_096)?;
            let system = system.read_frames(4_096)?;
            if microphone.is_empty() && system.is_empty() {
                break;
            }
            if microphone.len() % 2 != 0 || system.len() % 2 != 0 {
                return Err(invalid("bytes length not a multiple of item size"));
            }
            let mut mixed = Vec::with_capacity(microphone.len().max(system.len()));
            for offset in (0..microphone.len().max(system.len())).step_by(2) {
                let mic = microphone
                    .get(offset..offset + 2)
                    .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()));
                let sys = system
                    .get(offset..offset + 2)
                    .map(|bytes| i16::from_le_bytes(bytes.try_into().unwrap()));
                let sample = match (mic, sys) {
                    (Some(mic), Some(sys)) => ((i32::from(mic) + i32::from(sys)) / 2) as i16,
                    (Some(sample), None) | (None, Some(sample)) => sample,
                    (None, None) => unreachable!(),
                };
                mixed.extend_from_slice(&sample.to_le_bytes());
            }
            writer.write_frames(&mixed)?;
        }
        writer.finish()
    })();
    if result.is_err() {
        super::process::remove_file(output);
    }
    result
}

pub(crate) fn copy_private_audio(source: &Path, output: &Path) -> io::Result<()> {
    let mut destination = super::process::create_private_file(output)?;
    let result = (|| {
        io::copy(&mut File::open(source)?, &mut destination)?;
        destination.set_permissions(fs::Permissions::from_mode(0o600))
    })();
    if result.is_err() {
        super::process::remove_file(output);
    }
    result
}
