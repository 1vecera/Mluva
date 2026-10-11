//! PCM from an authenticated device, with the same private WAV/callback contract.
use crate::{
    AudioCaptureError, Result, pcm16_audio_level, recorder::AudioChunkCallback, wav::Pcm16Writer,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct ExternalRecorder {
    active: Option<(PathBuf, Pcm16Writer, Option<AudioChunkCallback>)>,
    bytes: usize,
}
impl ExternalRecorder {
    pub fn start(&mut self, path: &Path, callback: Option<AudioChunkCallback>) -> Result<()> {
        if self.active.is_some() {
            return Err(AudioCaptureError::Message("Already recording."));
        }
        crate::process::private_parent(path)?;
        let writer = Pcm16Writer::new(crate::process::create_private_file(path)?)?;
        self.bytes = 0;
        self.active = Some((path.into(), writer, callback));
        Ok(())
    }
    pub fn active(&self) -> bool {
        self.active.is_some()
    }
    pub fn append(&mut self, frames: &[u8]) -> Result<()> {
        if frames.is_empty()
            || !frames.len().is_multiple_of(2)
            || frames.len() > mluva_core::phone::MAX_CHUNK_BYTES
            || self.bytes + frames.len() > mluva_core::phone::MAX_PCM_BYTES
        {
            return Err(AudioCaptureError::Message(
                "Invalid or overlength phone audio.",
            ));
        }
        let (_, writer, callback) = self
            .active
            .as_mut()
            .ok_or(AudioCaptureError::Message("Phone recording stopped."))?;
        writer.write_frames(frames)?;
        self.bytes += frames.len();
        // A failed realtime consumer falls back at Stop; it cannot lose captured PCM.
        if let Some(consumer) = callback {
            for block in frames.chunks(crate::REALTIME_CHUNK_BYTES) {
                if !matches!(
                    catch_unwind(AssertUnwindSafe(|| consumer(
                        block,
                        pcm16_audio_level(block)
                    ))),
                    Ok(Ok(()))
                ) {
                    *callback = None;
                    break;
                }
            }
        }
        Ok(())
    }
    pub fn stop(&mut self) -> Result<PathBuf> {
        let (path, writer, _) = self
            .active
            .take()
            .ok_or(AudioCaptureError::Message("Phone recording stopped."))?;
        writer.finish()?;
        Ok(path)
    }
    pub fn cancel(&mut self) {
        if let Some((path, _, _)) = self.active.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}
