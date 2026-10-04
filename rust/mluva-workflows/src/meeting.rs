//! Explicit finalized Meeting diarization and retry, with no dictation delivery.

use chrono::{DateTime, SecondsFormat, Utc};
use mluva_audio::meeting::MeetingCaptureResult;
use mluva_core::{
    config::AppConfig,
    database::{StoreError, StoreResult},
    meeting::{
        MeetingAudioSource, MeetingRecognitionStatus, MeetingRecord, MeetingSpeakerSegment,
        MeetingStore, extract_meeting_insights, validate_identifier,
    },
};
use mluva_providers::{TranscriptionResult, elevenlabs::ElevenLabsClient};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingStage {
    Archive,
    Recognition,
    Persistence,
}
impl MeetingStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Recognition => "recognition",
            Self::Persistence => "persistence",
        }
    }
}
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct MeetingFailure {
    pub message: &'static str,
    pub stage: MeetingStage,
    pub meeting: Option<MeetingRecord>,
    pub retained_audio_path: Option<PathBuf>,
}
#[derive(Debug, thiserror::Error)]
pub enum MeetingError {
    #[error(transparent)]
    Invalid(#[from] StoreError),
    #[error(transparent)]
    Failed(#[from] Box<MeetingFailure>),
}
pub type MeetingResult<T> = Result<T, MeetingError>;
#[derive(Clone, Debug)]
pub struct MeetingWorkflowResult {
    pub meeting: MeetingRecord,
    pub transcription: TranscriptionResult,
    pub incognito: bool,
}
#[derive(Default)]
pub struct MeetingCompletion {
    pub incognito: bool,
    pub identifier: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
}

pub struct MeetingWorkflow {
    pub config: AppConfig,
    pub client: Arc<ElevenLabsClient>,
    pub store: Arc<Mutex<MeetingStore>>,
}
/// Ownership follows the future through provider failure, cancellation and drop.
struct PrivateAudio(Option<PathBuf>);
impl Drop for PrivateAudio {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
fn failure(
    message: &'static str,
    stage: MeetingStage,
    meeting: Option<MeetingRecord>,
    retained_audio_path: Option<PathBuf>,
) -> MeetingError {
    Box::new(MeetingFailure {
        message,
        stage,
        meeting,
        retained_audio_path,
    })
    .into()
}
impl MeetingWorkflow {
    fn archive(&self) -> StoreResult<MutexGuard<'_, MeetingStore>> {
        self.store
            .lock()
            .map_err(|_| StoreError::Invalid("Meeting archive owner is unavailable.".into()))
    }
    pub async fn complete(
        &self,
        capture: MeetingCaptureResult,
        completion: MeetingCompletion,
    ) -> MeetingResult<MeetingWorkflowResult> {
        let _private_audio = PrivateAudio(completion.incognito.then(|| capture.path.clone()));
        let identifier = completion
            .identifier
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        validate_identifier(&identifier)?;
        let started = completion.started_at.unwrap_or_else(Utc::now);
        let timestamp = started.to_rfc3339_opts(
            if started.timestamp_subsec_micros() == 0 {
                SecondsFormat::Secs
            } else {
                SecondsFormat::Micros
            },
            true,
        );
        let sources = capture
            .audio_sources
            .iter()
            .map(|source| MeetingAudioSource::parse(source))
            .collect::<StoreResult<Vec<_>>>()?;
        let (recording_path, recording_filename) = if completion.incognito {
            (capture.path.clone(), None)
        } else {
            let path = self
                .archive()?
                .archive_recording(&capture.path, &identifier)
                .map_err(|_| {
                    failure(
                        "Meeting audio could not be placed in the private archive.",
                        MeetingStage::Archive,
                        None,
                        capture.path.exists().then(|| capture.path.clone()),
                    )
                })?;
            let filename = path.file_name().unwrap().to_string_lossy().into_owned();
            (path, Some(filename))
        };
        let transcription = match self
            .client
            .transcribe_meeting(
                &recording_path,
                &self.config.language_code,
                &self.config.transcription_model,
            )
            .await
        {
            Ok(value) => value,
            Err(_) if completion.incognito => {
                return Err(failure(
                    "Meeting transcription failed and Incognito audio was erased.",
                    MeetingStage::Recognition,
                    None,
                    None,
                ));
            }
            Err(_) => {
                let mut failed = MeetingRecord::new(String::new(), capture.duration_seconds);
                failed.identifier = identifier;
                failed.timestamp = timestamp;
                failed.language = self.config.language_code.clone();
                failed.audio_sources = sources;
                failed.recording_filename = recording_filename;
                failed.recognition_status = MeetingRecognitionStatus::Failed;
                failed.warnings = capture
                    .warnings
                    .iter()
                    .map(|value| (*value).into())
                    .collect();
                let failed = failed.normalized()?;
                if self.archive()?.save(failed.clone()).is_err() {
                    return Err(failure(
                        "Meeting transcription failed; its audio remains private but the archive index could not be saved.",
                        MeetingStage::Persistence,
                        Some(failed),
                        Some(recording_path),
                    ));
                }
                return Err(failure(
                    "Meeting transcription failed. Its private recording is retained for explicit retry.",
                    MeetingStage::Recognition,
                    Some(failed),
                    Some(recording_path),
                ));
            }
        };
        let duration = if capture.duration_seconds > 0.0 {
            capture.duration_seconds
        } else {
            transcription
                .audio_duration_seconds
                .unwrap_or(capture.duration_seconds)
        };
        let mut meeting = MeetingRecord::new(transcription.text.clone(), duration);
        meeting.identifier = identifier;
        meeting.timestamp = timestamp;
        meeting.language = transcription.language_code.clone();
        meeting.audio_sources = sources;
        meeting.speakers = speakers(&transcription);
        meeting.insights = extract_meeting_insights(&transcription.text);
        meeting.recording_filename = recording_filename;
        meeting.transcription_id = transcription.transcription_id.clone();
        meeting.warnings = capture
            .warnings
            .iter()
            .map(|value| (*value).into())
            .collect();
        let meeting = meeting.normalized()?;
        if !completion.incognito && self.archive()?.save(meeting.clone()).is_err() {
            return Err(failure(
                "Meeting transcript completed, but the private archive index could not be saved.",
                MeetingStage::Persistence,
                Some(meeting),
                Some(recording_path),
            ));
        }
        Ok(MeetingWorkflowResult {
            meeting,
            transcription,
            incognito: completion.incognito,
        })
    }
    pub async fn retry(&self, identifier: &str) -> MeetingResult<MeetingWorkflowResult> {
        let (meeting, path) = {
            let archive = self.archive()?;
            let meeting = archive.find(identifier)?.clone();
            let path = archive.recording_path(&meeting).ok_or_else(|| {
                failure(
                    "This meeting has no managed recording available for retry.",
                    MeetingStage::Recognition,
                    Some(meeting.clone()),
                    None,
                )
            })?;
            (meeting, path)
        };
        let transcription = self
            .client
            .transcribe_meeting(&path, &meeting.language, &self.config.transcription_model)
            .await
            .map_err(|_| {
                failure(
                    "Meeting transcription failed again. Its private recording remains available.",
                    MeetingStage::Recognition,
                    Some(meeting.clone()),
                    Some(path.clone()),
                )
            })?;
        let mut recovered = meeting;
        recovered.transcript = transcription.text.clone();
        recovered.speakers = speakers(&transcription);
        recovered.insights = extract_meeting_insights(&transcription.text);
        recovered.language = transcription.language_code.clone();
        recovered.recognition_status = MeetingRecognitionStatus::Completed;
        recovered.transcription_id = transcription.transcription_id.clone();
        if self.archive()?.save(recovered.clone()).is_err() {
            return Err(failure(
                "Meeting retry completed, but the private archive index could not be saved.",
                MeetingStage::Persistence,
                Some(recovered),
                Some(path),
            ));
        }
        Ok(MeetingWorkflowResult {
            meeting: recovered,
            transcription,
            incognito: false,
        })
    }
    pub fn cancel(&self) {
        self.client.cancel();
    }
    pub fn close(&self) {
        self.client.close();
    }
}
fn speakers(value: &TranscriptionResult) -> Vec<MeetingSpeakerSegment> {
    value
        .speaker_segments
        .iter()
        .map(|segment| {
            MeetingSpeakerSegment::new(
                segment.speaker.clone(),
                segment.text.clone(),
                segment.started_at_seconds,
                segment.ended_at_seconds,
            )
        })
        .collect()
}
