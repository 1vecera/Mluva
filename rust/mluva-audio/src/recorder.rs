use crate::process::{self, FINALIZATION_TIMEOUT};
use crate::wav::{self, Pcm16Writer};
use crate::{AudioCaptureError, REALTIME_CHUNK_BYTES, Result, pcm16_audio_level};
use mluva_core::text::trim;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub type AudioChunkCallback = Box<dyn FnMut(&[u8], f64) -> std::result::Result<(), String> + Send>;

struct Recording {
    child: Child,
    path: PathBuf,
    reader: JoinHandle<io::Result<()>>,
    stderr: File,
    level: Arc<AtomicU64>,
}

pub struct PipeWireRecorder {
    executable: PathBuf,
    target: Option<String>,
    recording: Option<Recording>,
}

impl PipeWireRecorder {
    pub fn new(executable: impl Into<PathBuf>, target: Option<String>) -> Self {
        Self {
            executable: executable.into(),
            target,
            recording: None,
        }
    }

    pub fn from_system(target: Option<String>) -> Result<Self> {
        Ok(Self::new(process::recorder_executable()?, target))
    }

    pub fn active(&self) -> bool {
        self.recording.is_some()
    }

    pub fn output_path(&self) -> Option<&Path> {
        self.recording
            .as_ref()
            .map(|recording| recording.path.as_path())
    }

    pub fn audio_level(&self) -> f64 {
        self.recording.as_ref().map_or(0.0, |recording| {
            f64::from_bits(recording.level.load(Ordering::Relaxed))
        })
    }

    pub fn start(&mut self, output: &Path, callback: Option<AudioChunkCallback>) -> Result<()> {
        if self.active() {
            return Err(AudioCaptureError::Message("A recording is already active."));
        }
        process::private_parent(output)?;
        if output.exists() {
            return Err(AudioCaptureError::Message(
                "The recording destination already exists.",
            ));
        }
        let file = process::create_private_file(output)?;
        let spawned = (|| {
            let stderr = tempfile::tempfile()?;
            let mut child = process::record_command(&self.executable, self.target.as_deref())
                .args(["--raw", "-"])
                .stdout(Stdio::piped())
                .stderr(Stdio::from(stderr.try_clone()?))
                .spawn()?;
            let stdout = child.stdout.take().expect("requested a stdout pipe");
            let level = Arc::new(AtomicU64::new(0.0_f64.to_bits()));
            let reader_level = level.clone();
            let reader = match thread::Builder::new()
                .name("pipewire-pcm-drain".into())
                .spawn(move || drain(stdout, file, reader_level, callback))
            {
                Ok(reader) => reader,
                Err(error) => {
                    process::terminate(&mut child);
                    return Err(error);
                }
            };
            Ok(Recording {
                child,
                path: output.into(),
                reader,
                stderr,
                level,
            })
        })();
        match spawned {
            Ok(recording) => {
                self.recording = Some(recording);
                Ok(())
            }
            Err(error) => {
                process::remove_file(output);
                Err(error.into())
            }
        }
    }

    pub fn stop(&mut self) -> Result<PathBuf> {
        let mut recording = self
            .recording
            .take()
            .ok_or(AudioCaptureError::Message("No recording is active."))?;
        let status = match process::finalize(&mut recording.child) {
            Ok(status) => status,
            Err(error) => {
                process::terminate(&mut recording.child);
                return Err(error.into());
            }
        };
        if join_reader(recording.reader).is_err() {
            return Err(AudioCaptureError::Message(
                "The microphone audio stream could not be saved.",
            ));
        }
        recording.stderr.seek(SeekFrom::Start(0))?;
        let mut stderr = Vec::new();
        recording.stderr.read_to_end(&mut stderr)?;
        let detail = String::from_utf8_lossy(&stderr);
        let detail = trim(&detail);
        let failure = if !wav::is_compatible_pcm_wav(&recording.path)? {
            Some("The microphone capture contained no audio.")
        } else if !process::accepted(status) {
            Some("PipeWire microphone capture failed.")
        } else {
            None
        };
        if let Some(message) = failure {
            return Err(if detail.is_empty() {
                AudioCaptureError::Message(message)
            } else {
                AudioCaptureError::Detail(detail.into())
            });
        }
        Ok(recording.path)
    }

    pub fn cancel(&mut self) {
        if let Some(mut recording) = self.recording.take() {
            process::terminate(&mut recording.child);
            let _ = join_reader(recording.reader);
            process::remove_file(&recording.path);
        }
    }
}

impl Drop for PipeWireRecorder {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn join_reader(reader: JoinHandle<io::Result<()>>) -> io::Result<()> {
    let deadline = Instant::now() + FINALIZATION_TIMEOUT;
    while !reader.is_finished() {
        if Instant::now() >= deadline {
            // A blocked optional consumer must not block the application indefinitely.
            // The detached reader owns only this capture's file and level snapshot.
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "PipeWire audio finalization timed out.",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    reader
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("The audio reader stopped unexpectedly.")))
}

fn drain(
    mut stdout: ChildStdout,
    file: File,
    level: Arc<AtomicU64>,
    mut callback: Option<AudioChunkCallback>,
) -> io::Result<()> {
    let mut writer = Pcm16Writer::new(file)?;
    let mut buffer = [0; REALTIME_CHUNK_BYTES];
    let mut pending = Vec::with_capacity(REALTIME_CHUNK_BYTES * 2);
    let result = (|| {
        loop {
            let count = match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            pending.extend_from_slice(&buffer[..count]);
            while pending.len() >= REALTIME_CHUNK_BYTES {
                publish(
                    &mut writer,
                    &pending[..REALTIME_CHUNK_BYTES],
                    &level,
                    &mut callback,
                )?;
                pending.drain(..REALTIME_CHUNK_BYTES);
            }
        }
        let complete = pending.len() - pending.len() % 2;
        if complete != 0 {
            publish(&mut writer, &pending[..complete], &level, &mut callback)?;
        }
        Ok(())
    })();
    let finalized = writer.finish();
    result.and(finalized)
}

fn publish(
    writer: &mut Pcm16Writer,
    frames: &[u8],
    level: &AtomicU64,
    callback: &mut Option<AudioChunkCallback>,
) -> io::Result<()> {
    writer.write_frames(frames)?;
    let rms = pcm16_audio_level(frames);
    level.store(rms.to_bits(), Ordering::Relaxed);
    if let Some(consumer) = callback {
        let result = catch_unwind(AssertUnwindSafe(|| consumer(frames, rms)));
        if !matches!(result, Ok(Ok(()))) {
            *callback = None;
        }
    }
    Ok(())
}
