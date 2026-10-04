//! One immutable Meeting capture with explicit staging, archive and exit ownership.

use crate::meeting::{MeetingCompletion, MeetingError, MeetingWorkflow, MeetingWorkflowResult};
use chrono::{DateTime, Utc};
use mluva_audio::{
    AudioCaptureError, capture::CaptureStorage, meeting::PipeWireMeetingRecorder,
    meeting_capture::MeetingRecorder,
};
use mluva_core::diagnostics::{
    DiagnosticOutcome, DiagnosticProvider, DiagnosticStage, DiagnosticsStore,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MeetingPhase {
    Preparing,
    Recording,
    Processing,
    Completed,
    Failed,
    Cancelled,
    Closed,
}
#[derive(Debug, thiserror::Error)]
pub enum MeetingSessionError {
    #[error(transparent)]
    Audio(#[from] AudioCaptureError),
    #[error(transparent)]
    Workflow(#[from] MeetingError),
    #[error("The Meeting was cancelled.")]
    Cancelled,
}
pub type SessionResult<T> = Result<T, MeetingSessionError>;

#[derive(Clone, Copy, Default)]
pub struct MeetingTimings {
    pub ready_seconds: Option<f64>,
    pub capture_seconds: Option<f64>,
    pub recognition_seconds: Option<f64>,
}

pub struct MeetingSession {
    pub identifier: String,
    pub started_at: DateTime<Utc>,
    pub incognito: bool,
    pub workflow: Arc<MeetingWorkflow>,
    recorder: Arc<MeetingRecorder>,
    storage: Mutex<Option<CaptureStorage>>,
    path: Mutex<Option<PathBuf>>,
    started: Mutex<Option<Instant>>,
    phase: AtomicU8,
    lifetime: CancellationToken,
    operation: tokio::sync::Mutex<()>,
    timings: Mutex<MeetingTimings>,
    diagnostics: Option<DiagnosticsStore>,
}
impl MeetingSession {
    /// The application freezes the identifier, wall timestamp and privacy before
    /// staging/start. Each session owns a fresh workflow/client and recorder.
    pub fn new(
        workflow: MeetingWorkflow,
        recorder: PipeWireMeetingRecorder,
        storage: CaptureStorage,
        completion: MeetingCompletion,
        diagnostics: Option<DiagnosticsStore>,
    ) -> SessionResult<Arc<Self>> {
        let identifier = completion
            .identifier
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        mluva_core::meeting::validate_identifier(&identifier).map_err(MeetingError::from)?;
        if completion.incognito != matches!(storage, CaptureStorage::Incognito { .. }) {
            return Err(
                AudioCaptureError::Message("Meeting privacy and audio storage disagree.").into(),
            );
        }
        if let CaptureStorage::Persistent(path) = &storage {
            let archive = workflow
                .store
                .lock()
                .map_err(|_| AudioCaptureError::Message("Meeting archive owner is unavailable."))?;
            if mluva_core::private_files::resolve_path(path).map_err(AudioCaptureError::from)?
                != mluva_core::private_files::resolve_path(&archive.recordings_directory)
                    .map_err(AudioCaptureError::from)?
            {
                return Err(AudioCaptureError::Message(
                    "Meeting recording must use its private archive directory.",
                )
                .into());
            }
        }
        Ok(Arc::new(Self {
            identifier,
            started_at: completion.started_at.unwrap_or_else(Utc::now),
            incognito: completion.incognito,
            workflow: Arc::new(workflow),
            recorder: Arc::new(MeetingRecorder::new(recorder)?),
            storage: Mutex::new(Some(storage)),
            path: Mutex::new(None),
            started: Mutex::new(None),
            phase: AtomicU8::new(MeetingPhase::Preparing as u8),
            lifetime: CancellationToken::new(),
            operation: tokio::sync::Mutex::new(()),
            timings: Mutex::new(MeetingTimings::default()),
            diagnostics,
        }))
    }
    pub fn phase(&self) -> MeetingPhase {
        match self.phase.load(Ordering::Acquire) {
            0 => MeetingPhase::Preparing,
            1 => MeetingPhase::Recording,
            2 => MeetingPhase::Processing,
            3 => MeetingPhase::Completed,
            4 => MeetingPhase::Failed,
            5 => MeetingPhase::Cancelled,
            _ => MeetingPhase::Closed,
        }
    }
    fn set_phase(&self, phase: MeetingPhase) {
        self.phase.store(phase as u8, Ordering::Release);
    }
    pub fn audio_path(&self) -> Option<PathBuf> {
        self.path.lock().unwrap().clone()
    }
    pub fn elapsed_seconds(&self) -> Option<f64> {
        self.started
            .lock()
            .unwrap()
            .map(|time| time.elapsed().as_secs_f64())
    }
    pub fn timings(&self) -> MeetingTimings {
        *self.timings.lock().unwrap()
    }
    fn record(
        &self,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        success: bool,
        seconds: f64,
    ) {
        if !self.incognito
            && let Some(store) = &self.diagnostics
        {
            let _ = store.record(
                &self.identifier,
                "meeting",
                stage,
                provider,
                if success {
                    DiagnosticOutcome::Completed
                } else {
                    DiagnosticOutcome::Failed
                },
                seconds,
            );
        }
    }
    pub fn cancel(&self) {
        self.lifetime.cancel();
    }
    pub async fn start(&self) -> SessionResult<PathBuf> {
        let _operation = self.operation.lock().await;
        if self.lifetime.is_cancelled() {
            return Err(MeetingSessionError::Cancelled);
        }
        let storage = self
            .storage
            .lock()
            .unwrap()
            .take()
            .ok_or(AudioCaptureError::Message(
                "This Meeting capture has already started.",
            ))?;
        let prepared = self
            .recorder
            .prepare_destination(storage, format!("{}.wav", self.identifier))
            .await;
        let path = match prepared {
            Ok(path) => path,
            Err(error) => {
                self.set_phase(MeetingPhase::Failed);
                let _ = self.recorder.close().await;
                return Err(error.into());
            }
        };
        self.path.lock().unwrap().replace(path.clone());
        if self.lifetime.is_cancelled() {
            self.set_phase(MeetingPhase::Cancelled);
            self.recorder.close().await?;
            return Err(MeetingSessionError::Cancelled);
        }
        let ready = Instant::now();
        let start = self.recorder.start(path.clone()).await;
        let ready_seconds = ready.elapsed().as_secs_f64();
        self.timings.lock().unwrap().ready_seconds = Some(ready_seconds);
        self.record(
            DiagnosticStage::CaptureReady,
            DiagnosticProvider::Pipewire,
            start.is_ok(),
            ready_seconds,
        );
        if let Err(error) = start {
            self.set_phase(MeetingPhase::Failed);
            self.recorder.close().await?;
            return Err(error.into());
        }
        if self.lifetime.is_cancelled() {
            self.set_phase(MeetingPhase::Cancelled);
            self.recorder.close().await?;
            return Err(MeetingSessionError::Cancelled);
        }
        self.started.lock().unwrap().replace(Instant::now());
        self.set_phase(MeetingPhase::Recording);
        Ok(path)
    }
    /// Run this Send future on the application runtime's background owner; archive
    /// I/O and provider work never need the GTK context.
    pub async fn finish(&self) -> SessionResult<MeetingWorkflowResult> {
        let _operation = self.operation.lock().await;
        if self
            .phase
            .compare_exchange(
                MeetingPhase::Recording as u8,
                MeetingPhase::Processing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return Err(AudioCaptureError::Message("No meeting recording is active.").into());
        }
        let finalizing = Instant::now();
        let stopped = self.recorder.stop().await;
        let capture_seconds = finalizing.elapsed().as_secs_f64();
        self.timings.lock().unwrap().capture_seconds = Some(capture_seconds);
        self.record(
            DiagnosticStage::Capture,
            DiagnosticProvider::Pipewire,
            stopped.is_ok(),
            capture_seconds,
        );
        let capture = match stopped {
            Ok(capture) => capture,
            Err(error) => {
                self.set_phase(MeetingPhase::Failed);
                self.recorder.close().await?;
                return Err(error.into());
            }
        };
        let recognition = Instant::now();
        let outcome = tokio::select! { biased;
            _=self.lifetime.cancelled()=>{ self.set_phase(MeetingPhase::Cancelled);self.recorder.close().await?;return Err(MeetingSessionError::Cancelled); },
            value=self.workflow.complete(capture,MeetingCompletion {incognito:self.incognito,identifier:Some(self.identifier.clone()),started_at:Some(self.started_at)})=>value,
        };
        let recognition_seconds = recognition.elapsed().as_secs_f64();
        self.timings.lock().unwrap().recognition_seconds = Some(recognition_seconds);
        self.record(
            DiagnosticStage::Recognition,
            DiagnosticProvider::ElevenlabsScribeV2,
            outcome.is_ok(),
            recognition_seconds,
        );
        let retained = !self.incognito
            && match &outcome {
                Ok(_) => true,
                Err(MeetingError::Failed(failure)) => failure.retained_audio_path.is_some(),
                Err(_) => false,
            };
        if retained {
            self.recorder.retain_audio().await?;
        }
        self.recorder.close().await?;
        self.set_phase(if outcome.is_ok() {
            MeetingPhase::Completed
        } else {
            MeetingPhase::Failed
        });
        outcome.map_err(Into::into)
    }
    pub async fn close(&self) -> SessionResult<()> {
        self.lifetime.cancel();
        self.workflow.close();
        // Finish owns archive publication and recorder transfer until it returns.
        // Closing waits for that owner, so it cannot erase indexed audio or allow
        // a later terminal phase/index write after shutdown has completed.
        let _operation = self.operation.lock().await;
        self.recorder.close().await?;
        self.set_phase(MeetingPhase::Closed);
        Ok(())
    }
}
impl Drop for MeetingSession {
    fn drop(&mut self) {
        self.lifetime.cancel();
        self.workflow.close();
    }
}
