//! Released 16 kHz CPU feature extraction, including its short-input normalization.
use crate::{Error, Result};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Whisper,
    Parakeet,
}

pub struct Features {
    /// Row-major [1, mel bands, frames].
    pub values: Vec<f32>,
    pub bands: usize,
    pub frames: usize,
    pub valid_frames: i64,
}

pub struct Frontend {
    kind: Kind,
    fft: Arc<dyn Fft<f64>>,
    window: Vec<f64>,
    mel: Vec<Vec<(usize, f32)>>,
}

fn float32(data: &[u8]) -> Vec<f32> {
    data.chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

impl Frontend {
    pub fn new(kind: Kind) -> Self {
        let (fft_size, bands, matrix, window) = match kind {
            Kind::Whisper => (
                400,
                80,
                float32(include_bytes!("../resources/whisper80-mel.f32le")),
                float32(include_bytes!("../resources/whisper80-window.f32le"))
                    .into_iter()
                    .map(f64::from)
                    .collect(),
            ),
            Kind::Parakeet => (
                512,
                128,
                float32(include_bytes!("../resources/nemo128-mel.f32le")),
                include_bytes!("../resources/nemo128-window.f64le")
                    .chunks_exact(8)
                    .map(|bytes| f64::from_le_bytes(bytes.try_into().unwrap()))
                    .collect(),
            ),
        };
        let mel = (0..bands)
            .map(|band| {
                (0..=fft_size / 2)
                    .filter_map(|bin| {
                        let weight = matrix[bin * bands + band];
                        (weight != 0.0).then_some((bin, weight))
                    })
                    .collect()
            })
            .collect();
        Self {
            kind,
            fft: FftPlanner::new().plan_fft_forward(fft_size),
            window,
            mel,
        }
    }

    pub fn extract(&self, pcm: &[f32]) -> Result<Features> {
        if pcm.len() > 400_000 || pcm.iter().any(|sample| !sample.is_finite()) {
            return Err(Error::Audio);
        }
        let size = self.fft.len();
        let half = size / 2;
        let frames = match self.kind {
            Kind::Whisper => 3000,
            Kind::Parakeet => pcm.len() / 160 + 1,
        };
        let valid_frames = match self.kind {
            Kind::Whisper => 3000,
            Kind::Parakeet => pcm.len() as i64 / 160,
        };
        let mut audio = match self.kind {
            Kind::Whisper => {
                let mut padded = vec![0.0; 480_000];
                padded[..pcm.len()].copy_from_slice(pcm);
                padded
            }
            Kind::Parakeet => pcm
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    value - 0.97_f32 * index.checked_sub(1).map_or(0.0, |i| pcm[i])
                })
                .collect(),
        };
        // Both pads precede the first frame; Whisper reflects after its 30-second zero pad.
        let mut padded = Vec::with_capacity(audio.len() + size);
        if self.kind == Kind::Whisper {
            padded.extend(audio[1..=half].iter().rev().copied());
            padded.append(&mut audio);
            let end = padded.len();
            padded.extend(
                (end - half - 1..end - 1)
                    .rev()
                    .map(|index| padded[index])
                    .collect::<Vec<_>>(),
            );
        } else {
            padded.resize(half, 0.0);
            padded.append(&mut audio);
            padded.resize(padded.len() + half, 0.0);
        }
        let mut buffer = vec![Complex::default(); size];
        let mut scratch = vec![Complex::default(); self.fft.get_inplace_scratch_len()];
        let mut power = vec![0.0_f32; half + 1];
        let mut values = vec![0.0; frames * self.mel.len()];
        for frame in 0..frames {
            let start = frame * 160;
            for (offset, complex) in buffer.iter_mut().enumerate() {
                let sample = padded[start + offset];
                complex.re = if self.kind == Kind::Whisper {
                    f64::from(sample * self.window[offset] as f32)
                } else {
                    f64::from(sample) * self.window[offset]
                };
                complex.im = 0.0;
            }
            self.fft.process_with_scratch(&mut buffer, &mut scratch);
            for (bin, value) in power.iter_mut().enumerate() {
                let complex = buffer[bin];
                let magnitude = if self.kind == Kind::Whisper {
                    (complex.re as f32).hypot(complex.im as f32)
                } else {
                    complex.re.hypot(complex.im) as f32
                };
                *value = magnitude * magnitude;
            }
            for (band, weights) in self.mel.iter().enumerate() {
                let energy = weights
                    .iter()
                    .map(|(bin, weight)| power[*bin] * weight)
                    .sum::<f32>();
                values[band * frames + frame] = match self.kind {
                    Kind::Whisper => energy.max(1e-10).log10(),
                    Kind::Parakeet => (energy + 2.0_f32.powi(-24)).ln(),
                };
            }
        }
        if self.kind == Kind::Whisper {
            let floor = values.iter().copied().fold(f32::NEG_INFINITY, f32::max) - 8.0;
            for value in &mut values {
                *value = (value.max(floor) + 4.0) / 4.0;
            }
        } else {
            let count = valid_frames as usize;
            for band in values.chunks_exact_mut(frames) {
                let mean = band[..count].iter().sum::<f32>() / count as f32;
                let variance = band[..count]
                    .iter()
                    .map(|value| (value - mean).powi(2))
                    .sum::<f32>()
                    / (count as f32 - 1.0);
                let deviation = variance.sqrt() + 1e-5;
                for value in &mut band[..count] {
                    *value = (*value - mean) / deviation;
                }
                band[count..].fill(0.0);
            }
        }
        Ok(Features {
            values,
            bands: self.mel.len(),
            frames,
            valid_frames,
        })
    }
}
