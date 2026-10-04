use crate::process;
use crate::wav::{self, WaveReader};
use crate::{AudioCaptureError, PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES, Result};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};

#[derive(Debug)]
pub struct MeetingCaptureResult {
    pub path: PathBuf,
    pub audio_sources: Vec<&'static str>,
    pub warnings: Vec<&'static str>,
    pub duration_seconds: f64,
}

struct MeetingRecording {
    microphone: Child,
    system: Child,
    path: PathBuf,
    microphone_path: PathBuf,
    system_path: PathBuf,
    keep_output: bool,
}

impl Drop for MeetingRecording {
    fn drop(&mut self) {
        process::terminate(&mut self.microphone);
        process::terminate(&mut self.system);
        process::remove_file(&self.microphone_path);
        process::remove_file(&self.system_path);
        if !self.keep_output {
            process::remove_file(&self.path);
        }
    }
}

pub struct PipeWireMeetingRecorder {
    executable: PathBuf,
    microphone_target: Option<String>,
    system_target: Option<String>,
    recording: Option<MeetingRecording>,
}

impl PipeWireMeetingRecorder {
    pub fn new(
        executable: impl Into<PathBuf>,
        microphone_target: Option<String>,
        system_target: Option<String>,
    ) -> Self {
        Self {
            executable: executable.into(),
            microphone_target,
            system_target,
            recording: None,
        }
    }

    pub fn from_system(
        microphone_target: Option<String>,
        system_target: Option<String>,
    ) -> Result<Self> {
        Ok(Self::new(
            process::recorder_executable()?,
            microphone_target,
            system_target,
        ))
    }

    pub fn active(&self) -> bool {
        self.recording.is_some()
    }

    pub fn source_paths(&self) -> Option<(&Path, &Path)> {
        self.recording.as_ref().map(|recording| {
            (
                recording.microphone_path.as_path(),
                recording.system_path.as_path(),
            )
        })
    }

    pub fn start(&mut self, output: &Path) -> Result<()> {
        if self.active() {
            return Err(AudioCaptureError::Message(
                "A meeting recording is already active.",
            ));
        }
        if output.exists() {
            return Err(AudioCaptureError::Message(
                "The meeting recording destination already exists.",
            ));
        }
        process::private_parent(output)?;
        let name = output
            .file_name()
            .ok_or_else(|| std::io::Error::other("A recording needs a filename."))?;
        let part = |suffix: &str| {
            let mut filename = OsString::from(".");
            filename.push(name);
            filename.push(suffix);
            output.with_file_name(filename)
        };
        let microphone_path = part(".microphone.part.wav");
        let system_path = part(".system.part.wav");
        if microphone_path.exists() || system_path.exists() {
            return Err(AudioCaptureError::Message(
                "Unfinished meeting capture files already exist at this destination.",
            ));
        }
        let start_failure =
            || AudioCaptureError::Message("Meeting capture could not start both PipeWire streams.");
        let mut microphone =
            process::record_command(&self.executable, self.microphone_target.as_deref())
                .arg(&microphone_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|_| start_failure())?;
        let system = process::record_command(&self.executable, self.system_target.as_deref())
            .args(["--properties", PIPEWIRE_SYSTEM_CAPTURE_PROPERTIES])
            .arg(&system_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let system = match system {
            Ok(system) => system,
            Err(_) => {
                process::terminate(&mut microphone);
                process::remove_file(&microphone_path);
                process::remove_file(&system_path);
                process::remove_file(output);
                return Err(start_failure());
            }
        };
        self.recording = Some(MeetingRecording {
            microphone,
            system,
            path: output.into(),
            microphone_path,
            system_path,
            keep_output: false,
        });
        Ok(())
    }

    pub fn stop(&mut self) -> Result<MeetingCaptureResult> {
        let mut recording = self.recording.take().ok_or(AudioCaptureError::Message(
            "No meeting recording is active.",
        ))?;
        let mic_status = process::finalize(&mut recording.microphone)?;
        let sys_status = process::finalize(&mut recording.system)?;
        let microphone_valid = process::accepted(mic_status)
            && wav::is_compatible_pcm_wav(&recording.microphone_path)?;
        let system_valid =
            process::accepted(sys_status) && wav::is_compatible_pcm_wav(&recording.system_path)?;
        let (audio_sources, warnings) = match (microphone_valid, system_valid) {
            (true, true) => {
                wav::mix_pcm16_wav(
                    &recording.microphone_path,
                    &recording.system_path,
                    &recording.path,
                )?;
                (vec!["microphone", "system"], vec![])
            }
            (true, false) => {
                wav::copy_private_audio(&recording.microphone_path, &recording.path)?;
                (
                    vec!["microphone"],
                    vec![
                        "System audio was unavailable; this meeting contains microphone audio only.",
                    ],
                )
            }
            (false, true) => {
                wav::copy_private_audio(&recording.system_path, &recording.path)?;
                (
                    vec!["system"],
                    vec![
                        "Microphone audio was unavailable; this meeting contains system audio only.",
                    ],
                )
            }
            (false, false) => {
                return Err(AudioCaptureError::Message(
                    "Meeting capture produced no usable microphone or system audio.",
                ));
            }
        };
        let duration_seconds = WaveReader::open(&recording.path)?.duration_seconds();
        recording.keep_output = true;
        Ok(MeetingCaptureResult {
            path: recording.path.clone(),
            audio_sources,
            warnings,
            duration_seconds,
        })
    }

    pub fn cancel(&mut self) {
        self.recording = None;
    }
}
