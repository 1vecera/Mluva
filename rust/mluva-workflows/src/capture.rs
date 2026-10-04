//! A frozen recording transaction. GTK targets are supplied only at completion.

use crate::{
    dictation::{
        Completion, DeliveryTarget, DictationWorkflow, WorkflowError, WorkflowFailure,
        WorkflowOutcome, WorkflowResult,
    },
    preparation::TranscriptPreparationSnapshot,
    segment_cleanup::{
        CodexSegmentCleanupAttempt, SegmentAttemptFactory, SegmentCleanupConfiguration,
        SegmentCleanupSession,
    },
};
use mluva_audio::{
    capture::{CaptureRecorder, CaptureStorage},
    recorder::PipeWireRecorder,
};
use mluva_core::{
    config::AudioRetentionPolicy,
    diagnostics::{DiagnosticOutcome, DiagnosticProvider, DiagnosticStage},
    prompt_catalog::SavedStyle,
    screenshots::ImageInput,
};
use mluva_providers::{
    batch_preview::{BatchPreviewClient, BatchPreviewSession},
    local_preview::LocalPreviewClient,
    realtime::{
        CommittedCallback, ElevenLabsRealtimeClient, PreviewCallback, RealtimePreview,
        RealtimeSession, RealtimeSessionResult,
    },
};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Instant,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CapturePhase {
    Preparing,
    Recording,
    Processing,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug)]
pub struct CaptureOptions {
    pub mode: String,
    pub use_cleanup: bool,
    pub allow_auto_paste: bool,
    pub incognito: bool,
    pub audio_retention: AudioRetentionPolicy,
    pub selected_text: Option<String>,
    pub application_identifier: Option<String>,
    pub style_identifier: Option<String>,
    pub use_saved_style: bool,
    pub defer_delivery: bool,
    /// The Live owner enables compatible-provider previews only for its active session.
    /// Local speech previews remain independent of optional text rewriting.
    pub preview_enabled: bool,
}
impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            mode: "dictation".into(),
            use_cleanup: false,
            allow_auto_paste: false,
            incognito: false,
            audio_retention: AudioRetentionPolicy::Failures,
            selected_text: None,
            application_identifier: None,
            style_identifier: None,
            use_saved_style: true,
            defer_delivery: false,
            preview_enabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureReady {
    pub fallback_reason: Option<String>,
    pub model_identifier: Option<String>,
}
impl CaptureReady {
    pub fn status(&self) -> &'static str {
        if self.fallback_reason.is_some() {
            "Ready · microphone capture started with Scribe v2 batch fallback."
        } else {
            "Ready · microphone capture started with Scribe v2 realtime."
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureCancelled {
    pub before_ready: bool,
    pub audio_was_streamed: bool,
}
impl CaptureCancelled {
    pub fn status(&self) -> &'static str {
        if self.before_ready {
            "Recognition preparation cancelled. No microphone audio was recorded or sent."
        } else if self.audio_was_streamed {
            "Recording cancelled. Local audio was erased; already processed audio cannot be recalled."
        } else {
            "Recording cancelled. Local audio was erased."
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("{0}")]
    Failed(String),
    #[error("{}", .0.status())]
    Cancelled(CaptureCancelled),
}

#[derive(Clone)]
pub enum CaptureRecognitionClient {
    ElevenLabs(Rc<ElevenLabsRealtimeClient>),
    Batch(Rc<BatchPreviewClient>),
    Local(Rc<LocalPreviewClient>),
}

impl CaptureRecognitionClient {
    async fn start(
        &self,
        language: &str,
        directory: Option<PathBuf>,
        enabled: bool,
        preview: Option<PreviewCallback>,
        committed: Option<CommittedCallback>,
    ) -> mluva_providers::Result<RecognitionSession> {
        match self {
            Self::ElevenLabs(client) => client
                .start(language, preview, committed)
                .await
                .map(RecognitionSession::Realtime),
            Self::Batch(client) => {
                let mut client = client.as_ref().clone();
                if let Some(directory) = directory {
                    client.directory = directory;
                }
                client.preview_enabled = enabled;
                Ok(RecognitionSession::Batch(client.start(language)))
            }
            Self::Local(client) => {
                let mut client = client.as_ref().clone();
                if let Some(directory) = directory {
                    client.directory = directory;
                }
                Ok(RecognitionSession::Batch(client.start(language)))
            }
        }
    }
}

enum RecognitionSession {
    Realtime(RealtimeSession),
    Batch(BatchPreviewSession),
}
impl RecognitionSession {
    fn snapshot(&self) -> RealtimePreview {
        match self {
            Self::Realtime(session) => session.snapshot(),
            Self::Batch(session) => session.snapshot(),
        }
    }
    fn is_healthy(&self) -> bool {
        match self {
            Self::Realtime(session) => session.is_healthy(),
            Self::Batch(session) => session.is_healthy(),
        }
    }
    fn bytes_sent(&self) -> usize {
        match self {
            Self::Realtime(session) => session.bytes_sent(),
            Self::Batch(session) => session.bytes_sent(),
        }
    }
    fn submit_audio(&self, frames: &[u8]) -> mluva_providers::Result<()> {
        match self {
            Self::Realtime(session) => session.submit_audio(frames).map(|_| ()),
            Self::Batch(session) => {
                session.submit_audio(frames);
                Ok(())
            }
        }
    }
    fn cancel(&self) {
        match self {
            Self::Realtime(session) => session.cancel(),
            Self::Batch(session) => session.cancel(),
        }
    }
    async fn cancel_and_wait(&self) {
        match self {
            Self::Realtime(session) => session.cancel_and_wait().await,
            Self::Batch(session) => session.cancel_and_wait().await,
        }
    }
    async fn finish(&self) -> mluva_providers::Result<RealtimeSessionResult> {
        match self {
            Self::Realtime(session) => session.finish().await,
            Self::Batch(session) => session.finish().await,
        }
    }
    fn set_preview_enabled(&self, enabled: bool) {
        if let Self::Batch(session) = self {
            session.set_preview_enabled(enabled);
        }
    }
}

struct State {
    audio_path: Option<PathBuf>,
    realtime: Option<Arc<RecognitionSession>>,
    cleanup: Option<Arc<SegmentCleanupSession>>,
    fallback_reason: Option<String>,
    model_identifier: Option<String>,
    started: Option<Instant>,
    microphone_requested: bool,
    cancelled: Option<CaptureCancelled>,
}

/// A session cannot be reused. Preparation, stop and cancellation serialize their
/// terminal cleanup; a cancellation request interrupts provider readiness but
/// always awaits any already-issued microphone operation before releasing it.
pub struct CaptureSession {
    pub identifier: String,
    pub options: CaptureOptions,
    pub preparation: TranscriptPreparationSnapshot,
    pub frozen_style: Option<SavedStyle>,
    workflow: Rc<DictationWorkflow>,
    recorder: CaptureRecorder,
    storage: Mutex<Option<CaptureStorage>>,
    realtime_client: Option<CaptureRecognitionClient>,
    preview_enabled: AtomicBool,
    preview_callback: std::sync::Mutex<Option<PreviewCallback>>,
    phase: AtomicU8,
    cancellation: CancellationToken,
    state: Mutex<State>,
}

impl CaptureSession {
    pub fn new(
        workflow: Rc<DictationWorkflow>,
        recorder: PipeWireRecorder,
        storage: CaptureStorage,
        realtime_client: Option<CaptureRecognitionClient>,
        mut options: CaptureOptions,
    ) -> WorkflowOutcome<Rc<Self>> {
        if !["dictation", "command", "scratchpad"].contains(&options.mode.as_str()) {
            return Err(WorkflowError::Invalid("Unsupported capture mode.".into()));
        }
        if options.incognito != matches!(storage, CaptureStorage::Incognito { .. }) {
            return Err(WorkflowError::Invalid(
                "Incognito capture requires memory-backed staging and crash cleanup.".into(),
            ));
        }
        if options.incognito && options.mode == "command" {
            return Err(WorkflowError::Invalid("Command mode is unavailable in Incognito because Codex durability cannot be guaranteed.".into()));
        }
        if workflow.config.rewrite_provider == "none" {
            options.use_cleanup = false;
            options.use_saved_style = false;
            if options.mode == "command" {
                return Err(WorkflowError::Invalid(
                    "Choose a rewriting provider to use Command mode.".into(),
                ));
            }
        }
        if options.incognito {
            options.use_cleanup = false;
            options.use_saved_style = false;
        }
        if options.mode != "dictation" {
            options.allow_auto_paste = false;
        }
        if options.mode != "command" {
            options.selected_text = None;
        }
        let frozen_style = if options.use_saved_style && options.mode != "command" {
            workflow.selected_style(
                options.application_identifier.as_deref(),
                options.style_identifier.as_deref(),
            )?
        } else {
            None
        };
        let preparation = workflow.freeze_transcript_preparation(
            &options.mode,
            options.application_identifier.as_deref(),
        )?;
        let recorder = CaptureRecorder::new(recorder)
            .map_err(|error| WorkflowError::Invalid(error.to_string()))?;
        Ok(Rc::new(Self {
            identifier: Uuid::new_v4().to_string(),
            preview_enabled: AtomicBool::new(options.preview_enabled),
            preview_callback: std::sync::Mutex::new(None),
            options,
            preparation,
            frozen_style,
            workflow,
            recorder,
            storage: Mutex::new(Some(storage)),
            realtime_client,
            phase: AtomicU8::new(CapturePhase::Preparing as u8),
            cancellation: CancellationToken::new(),
            state: Mutex::new(State {
                audio_path: None,
                realtime: None,
                cleanup: None,
                fallback_reason: None,
                model_identifier: None,
                started: None,
                microphone_requested: false,
                cancelled: None,
            }),
        }))
    }

    pub fn phase(&self) -> CapturePhase {
        match self.phase.load(Ordering::Acquire) {
            0 => CapturePhase::Preparing,
            1 => CapturePhase::Recording,
            2 => CapturePhase::Processing,
            3 => CapturePhase::Cancelling,
            4 => CapturePhase::Completed,
            5 => CapturePhase::Cancelled,
            _ => CapturePhase::Failed,
        }
    }
    fn set_phase(&self, phase: CapturePhase) {
        self.phase.store(phase as u8, Ordering::Release);
    }
    pub fn audio_level(&self) -> f64 {
        self.recorder.audio_level()
    }
    pub fn elapsed_seconds(&self) -> Option<f64> {
        self.state
            .try_lock()
            .ok()?
            .started
            .map(|started| started.elapsed().as_secs_f64())
    }
    pub fn preview(&self) -> Option<RealtimePreview> {
        self.state
            .try_lock()
            .ok()?
            .realtime
            .as_ref()
            .map(|session| session.snapshot())
    }
    pub fn config(&self) -> &mluva_core::config::AppConfig {
        &self.workflow.config
    }
    /// Install the owner-thread bridge before provider readiness. As in the
    /// release, compatible/local previews are polled; realtime also notifies.
    pub fn set_preview_callback(&self, callback: PreviewCallback) {
        *self.preview_callback.lock().unwrap() = Some(callback);
    }
    pub fn realtime_healthy(&self) -> bool {
        self.state.try_lock().is_ok_and(|state| {
            state
                .realtime
                .as_ref()
                .is_some_and(|session| session.is_healthy())
        })
    }

    pub fn set_preview_enabled(&self, enabled: bool) {
        self.preview_enabled.store(enabled, Ordering::Release);
        if let Ok(state) = self.state.try_lock()
            && let Some(session) = &state.realtime
        {
            session.set_preview_enabled(enabled && self.options.mode == "dictation");
        }
    }

    pub async fn prepare_and_start(&self) -> Result<CaptureReady, CaptureError> {
        let mut state = self.state.lock().await;
        if self.cancellation.is_cancelled() {
            return Err(CaptureError::Cancelled(
                self.cancel_locked(&mut state).await,
            ));
        }
        if self.phase() != CapturePhase::Preparing {
            return Err(CaptureError::Failed(
                "This capture is no longer preparing.".into(),
            ));
        }
        let result = self.prepare_locked(&mut state).await;
        if self.cancellation.is_cancelled() {
            return Err(CaptureError::Cancelled(
                self.cancel_locked(&mut state).await,
            ));
        }
        match result {
            Ok(ready) => Ok(ready),
            Err(message) => {
                if let Some(realtime) = state.realtime.take() {
                    realtime.cancel_and_wait().await;
                }
                let _ = self.recorder.cancel().await;
                let _ = self.recorder.close().await;
                self.set_phase(CapturePhase::Failed);
                Err(CaptureError::Failed(message))
            }
        }
    }

    async fn prepare_locked(&self, state: &mut State) -> Result<CaptureReady, String> {
        let storage = self
            .storage
            .lock()
            .await
            .take()
            .ok_or("This capture has no audio destination.")?;
        state.audio_path = Some(
            self.recorder
                .prepare_destination(storage, format!("{}.wav", self.identifier))
                .await
                .map_err(|error| error.to_string())?,
        );
        if self.cancellation.is_cancelled() {
            return Err(String::new());
        }
        if self.options.mode == "command" || self.options.use_cleanup || self.frozen_style.is_some()
        {
            let started = Instant::now();
            let resolved = tokio::select! { biased;
                _ = self.cancellation.cancelled() => return Err(String::new()),
                resolved = self.workflow.rewrite.resolve_model(self.workflow.config.codex_model.as_deref()) => resolved,
            };
            self.diagnostic(
                DiagnosticStage::CaptureReady,
                self.workflow.enhancement_provider(),
                if resolved.is_ok() {
                    DiagnosticOutcome::Completed
                } else {
                    DiagnosticOutcome::Failed
                },
                started.elapsed().as_secs_f64(),
            );
            state.model_identifier = Some(resolved.map_err(|error| {
                format!("Codex preparation failed before microphone capture: {error}")
            })?);
        }
        if self.options.use_cleanup
            && self.options.mode != "command"
            && self.workflow.config.rewrite_provider == "codex"
            && self.workflow.config.transcription_provider == "elevenlabs"
        {
            let parent = Arc::new(
                self.workflow
                    .rewrite
                    .spawn_codex()
                    .ok_or("Codex segment cleanup requires an isolated app-server client.")?,
            );
            let model = state
                .model_identifier
                .clone()
                .ok_or("Codex segment cleanup could not be prepared.")?;
            let preparing = self.preparation.clone();
            let cwd = self.workflow.cwd.clone();
            let instructions = self.workflow.cleanup_instructions.clone();
            let attempt_model = model.clone();
            let factory: SegmentAttemptFactory = Arc::new(move || {
                Ok(Arc::new(CodexSegmentCleanupAttempt {
                    client: parent.spawn(),
                    cwd: cwd.clone(),
                    model_identifier: attempt_model.clone(),
                    instructions: instructions.clone(),
                }))
            });
            state.cleanup = Some(Arc::new(SegmentCleanupSession::new(
                self.identifier.clone(),
                "codex-app-server".into(),
                model,
                Arc::new(move |raw| Ok(preparing.process(raw))),
                self.preparation.protected_vocabulary.clone(),
                factory,
                SegmentCleanupConfiguration::default(),
            )?));
        }
        let started = Instant::now();
        state.fallback_reason = match &self.realtime_client {
            None => Some("realtime-unavailable".into()),
            Some(client) => {
                let preview = self.preview_callback.lock().unwrap().take();
                let opened = tokio::select! { biased;
                    _ = self.cancellation.cancelled() => return Err(String::new()),
                    opened = client.start(
                        &self.workflow.config.language_code,
                        self.options.incognito.then(|| state.audio_path.as_ref().expect("prepared destination").parent().expect("capture directory").join("speech-previews")),
                        self.options.mode == "dictation" && self.preview_enabled.load(Ordering::Acquire),
                        preview,
                        state.cleanup.as_ref().map(|session| {
                            let session = session.clone();
                            Box::new(move |segment: mluva_providers::realtime::RealtimeCommittedSegment| {
                                session.accept_stable_segment(&segment.identifier, &segment.text); Ok(())
                            }) as CommittedCallback
                        }),
                    ) => opened,
                };
                match opened {
                    Ok(session) => {
                        state.realtime = Some(Arc::new(session));
                        None
                    }
                    Err(_) => Some("realtime-startup-failed".into()),
                }
            }
        };
        self.diagnostic(
            DiagnosticStage::CaptureReady,
            DiagnosticProvider::ElevenlabsScribeV2,
            if state.fallback_reason.is_some() {
                DiagnosticOutcome::SafeFallback
            } else {
                DiagnosticOutcome::Completed
            },
            started.elapsed().as_secs_f64(),
        );
        if self.cancellation.is_cancelled() {
            return Err(String::new());
        }
        let started = Instant::now();
        let callback = state.realtime.as_ref().map(|session| {
            let session = session.clone();
            Box::new(move |frames: &[u8], _level: f64| {
                session
                    .submit_audio(frames)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }) as mluva_audio::recorder::AudioChunkCallback
        });
        state.microphone_requested = true;
        let recorded = self
            .recorder
            .start(
                state.audio_path.clone().expect("prepared destination"),
                callback,
            )
            .await;
        self.diagnostic(
            DiagnosticStage::CaptureReady,
            DiagnosticProvider::Pipewire,
            if recorded.is_ok() {
                DiagnosticOutcome::Completed
            } else {
                DiagnosticOutcome::Failed
            },
            started.elapsed().as_secs_f64(),
        );
        recorded.map_err(|error| error.to_string())?;
        if let Some(session) = &state.realtime {
            session.set_preview_enabled(
                self.options.mode == "dictation" && self.preview_enabled.load(Ordering::Acquire),
            );
        }
        if self.cancellation.is_cancelled() {
            return Err(String::new());
        }
        state.started = Some(Instant::now());
        self.set_phase(CapturePhase::Recording);
        Ok(CaptureReady {
            fallback_reason: state.fallback_reason.clone(),
            model_identifier: state.model_identifier.clone(),
        })
    }

    pub fn request_cancel(&self) -> bool {
        loop {
            let phase = self.phase();
            if !matches!(phase, CapturePhase::Preparing | CapturePhase::Recording) {
                return phase == CapturePhase::Cancelling;
            }
            if self
                .phase
                .compare_exchange(
                    phase as u8,
                    CapturePhase::Cancelling as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                self.cancellation.cancel();
                return true;
            }
        }
    }

    pub async fn cancel(&self) -> Option<CaptureCancelled> {
        if !self.request_cancel() && self.phase() != CapturePhase::Cancelled {
            return None;
        }
        let mut state = self.state.lock().await;
        Some(self.cancel_locked(&mut state).await)
    }

    /// Application exit also interrupts processing before any later delivery.
    /// Unlike an ordinary Escape, exit closes the application-owned transports.
    pub async fn shutdown(&self) {
        self.request_shutdown();
        self.workflow.cancel().await;
        let mut state = self.state.lock().await;
        if !matches!(
            self.phase(),
            CapturePhase::Completed | CapturePhase::Failed | CapturePhase::Cancelled
        ) {
            self.cancel_locked(&mut state).await;
        }
        self.workflow.close().await;
    }

    pub fn request_shutdown(&self) {
        self.cancellation.cancel();
    }

    async fn cancel_locked(&self, state: &mut State) -> CaptureCancelled {
        if let Some(cancelled) = state.cancelled {
            return cancelled;
        }
        let before_ready = !state.microphone_requested;
        if let Some(started) = state.started {
            self.diagnostic(
                DiagnosticStage::Capture,
                DiagnosticProvider::Desktop,
                DiagnosticOutcome::Cancelled,
                started.elapsed().as_secs_f64(),
            );
        }
        let realtime = state.realtime.take();
        let cleanup = state.cleanup.take();
        if let Some(cleanup) = &cleanup {
            cleanup.cancel();
        }
        if let Some(realtime) = &realtime {
            realtime.cancel();
        }
        let _ = self.recorder.cancel().await;
        if let Some(realtime) = &realtime {
            realtime.cancel_and_wait().await;
        }
        if let Some(cleanup) = &cleanup {
            cleanup.wait_closed().await;
        }
        let cancelled = CaptureCancelled {
            before_ready,
            audio_was_streamed: realtime
                .as_ref()
                .is_some_and(|session| session.bytes_sent() > 0),
        };
        // A dropped Codex readiness request must not leave its app-server child
        // behind. Close is reusable; permanent cancellation is reserved for exit.
        if cancelled.before_ready {
            self.workflow.rewrite.close().await;
        }
        let _ = self.recorder.close().await;
        state.audio_path = None;
        state.cancelled = Some(cancelled);
        self.set_phase(CapturePhase::Cancelled);
        cancelled
    }

    pub fn begin_stop(&self) -> bool {
        self.phase
            .compare_exchange(
                CapturePhase::Recording as u8,
                CapturePhase::Processing as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub async fn complete(
        &self,
        target: Option<&dyn DeliveryTarget>,
        images: Vec<ImageInput>,
    ) -> WorkflowOutcome<WorkflowResult> {
        self.complete_with_images(target, std::future::ready(Ok(images)))
            .await
    }

    /// Finalize audio and recognition before waiting for an open picker. The
    /// caller then freezes its attached images on their owning desktop context.
    pub async fn complete_with_images<F>(
        &self,
        target: Option<&dyn DeliveryTarget>,
        images: F,
    ) -> WorkflowOutcome<WorkflowResult>
    where
        F: std::future::Future<Output = WorkflowOutcome<Vec<ImageInput>>>,
    {
        let mut state = self.state.lock().await;
        if self.phase() == CapturePhase::Recording {
            self.begin_stop();
        }
        if self.phase() != CapturePhase::Processing {
            return Err(WorkflowError::Invalid(
                "This capture is not ready to finish.".into(),
            ));
        }
        // Take the start timestamp so a second completion cannot dispatch twice.
        let started = state
            .started
            .take()
            .ok_or_else(|| WorkflowError::Invalid("This capture has already finished.".into()))?;
        self.diagnostic(
            DiagnosticStage::Capture,
            DiagnosticProvider::Desktop,
            DiagnosticOutcome::Completed,
            started.elapsed().as_secs_f64(),
        );
        let result = match self.complete_locked(&mut state, target, images).await {
            Err(error) if !matches!(error, WorkflowError::Failure(_)) => {
                let retained = state
                    .audio_path
                    .as_ref()
                    .filter(|path| path.exists())
                    .cloned();
                let retained = if self.options.incognito
                    || !self.options.audio_retention.should_retain(false)
                {
                    if let Some(path) = retained {
                        let _ = std::fs::remove_file(path);
                    }
                    None
                } else {
                    retained
                };
                Err(WorkflowFailure {
                    message: error.to_string(),
                    stage: "capture".into(),
                    history_entry: None,
                    retained_audio_path: retained,
                    output_text: String::new(),
                }
                .into())
            }
            result => result,
        };
        if let Some(realtime) = state.realtime.take() {
            realtime.cancel_and_wait().await;
        }
        if let Some(cleanup) = state.cleanup.take() {
            cleanup.cancel();
        }
        let _ = self.recorder.close().await;
        self.set_phase(if result.is_ok() {
            CapturePhase::Completed
        } else {
            CapturePhase::Failed
        });
        result
    }

    async fn complete_locked<F>(
        &self,
        state: &mut State,
        target: Option<&dyn DeliveryTarget>,
        images: F,
    ) -> WorkflowOutcome<WorkflowResult>
    where
        F: std::future::Future<Output = WorkflowOutcome<Vec<ImageInput>>>,
    {
        let audio_path = match self.recorder.stop().await {
            Ok(path) => path,
            Err(error) => {
                let retained = state
                    .audio_path
                    .as_ref()
                    .filter(|path| path.exists())
                    .cloned();
                let retained = if self.options.incognito
                    || !self.options.audio_retention.should_retain(false)
                {
                    if let Some(path) = retained {
                        let _ = std::fs::remove_file(path);
                    }
                    None
                } else {
                    retained
                };
                return Err(WorkflowFailure {
                    message: error.to_string(),
                    stage: "capture".into(),
                    history_entry: None,
                    retained_audio_path: retained,
                    output_text: String::new(),
                }
                .into());
            }
        };
        let mut transcription = None;
        let mut recognition_seconds = None;
        let mut segment_cleanup = None;
        if let Some(realtime) = &state.realtime {
            match realtime.finish().await {
                Ok(result) => {
                    if let Some(cleanup) = &state.cleanup {
                        let candidate = tokio::select! { biased;
                            _ = self.cancellation.cancelled() => {
                                cleanup.cancel_and_wait().await;
                                return Err(WorkflowError::Invalid("Capture stopped because Mluva is closing.".into()));
                            },
                            candidate = cleanup.stop_and_drain() => candidate,
                        };
                        if candidate.raw_text() == result.transcription.text {
                            segment_cleanup = Some(candidate);
                        } else {
                            cleanup.cancel();
                        }
                    }
                    transcription = Some(result.transcription);
                    recognition_seconds = Some(result.finalization_seconds);
                }
                Err(_) => {
                    state.fallback_reason = Some("realtime-stream-failed".into());
                    realtime.cancel_and_wait().await;
                }
            }
        }
        if transcription.is_none()
            && let Some(cleanup) = &state.cleanup
        {
            cleanup.cancel();
        }
        let images = tokio::select! { biased;
            _ = self.cancellation.cancelled() => return Err(WorkflowError::Invalid("Capture stopped because Mluva is closing.".into())),
            images = images => images?,
        };
        let completion = self.workflow.complete(
            &audio_path,
            Completion {
                mode: self.options.mode.clone(),
                segment_cleanup,
                use_codex_cleanup: self.options.use_cleanup,
                allow_auto_paste: self.options.allow_auto_paste,
                incognito: self.options.incognito,
                audio_retention_policy: self.options.audio_retention,
                selected_text: self.options.selected_text.clone(),
                session_identifier: Some(self.identifier.clone()),
                application_identifier: self.options.application_identifier.clone(),
                style_identifier: self.options.style_identifier.clone(),
                use_saved_style: self.options.use_saved_style,
                recognized_transcription: transcription,
                recognition_duration_seconds: recognition_seconds,
                recognition_used_batch_fallback: state.fallback_reason.is_some(),
                recognition_fallback_reason: state.fallback_reason.clone(),
                delivery_target: target,
                codex_model_identifier: state.model_identifier.clone(),
                transcript_preparation: Some(self.preparation.clone()),
                frozen_style: self.frozen_style.clone(),
                style_is_frozen: true,
                defer_delivery: self.options.defer_delivery,
                images,
            },
        );
        tokio::select! { biased;
            _ = self.cancellation.cancelled() => Err(WorkflowError::Invalid("Capture stopped because Mluva is closing.".into())),
            completed = completion => completed,
        }
    }

    fn diagnostic(
        &self,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        seconds: f64,
    ) {
        if let Some(store) = self
            .workflow
            .diagnostics
            .as_ref()
            .filter(|_| !self.options.incognito)
        {
            let _ = store.record(
                &self.identifier,
                &self.options.mode,
                stage,
                provider,
                outcome,
                seconds,
            );
        }
    }
}
