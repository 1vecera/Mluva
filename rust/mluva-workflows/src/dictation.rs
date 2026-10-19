//! One finalized recording, from batch or committed realtime recognition to recovery.

use crate::preparation::{
    TranscriptPreparationSnapshot, freeze_transcript_preparation, milliseconds,
};
use mluva_core::{
    config::{AppConfig, AudioRetentionPolicy},
    database::{StoreError, StoreResult},
    delivery::{DeliveryOptions, DeliveryReceipt, TargetResult, deliver_text},
    diagnostics::{DiagnosticOutcome, DiagnosticProvider, DiagnosticStage, DiagnosticsStore},
    history::{HistoryEntry, HistoryInput, HistoryStore, RECOGNITION_FALLBACKS, RetryRecognition},
    personalization::{PersonalizationStore, snippet_variables},
    screenshots::{ImageInput, validate_images},
    text,
    transcript::normalize_spoken_structure,
};
use mluva_providers::{ProviderError, TranscriptionResult, speech::SpeechClient};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::Instant,
};
use uuid::Uuid;

/// A caller-owned capture snapshot. Ordinary Dictation has no selected-text getter.
pub trait DeliveryTarget {
    fn application_identifier(&self) -> Option<&str>;
    fn restore(&self) -> TargetResult<bool>;
    fn confirm_insertion(&self, inserted_text: &str) -> TargetResult<Option<bool>>;
    fn insert_text(&self, inserted_text: &str) -> TargetResult<Option<bool>>;
}

pub struct Completion<'a> {
    pub allow_auto_paste: bool,
    pub incognito: bool,
    pub audio_retention_policy: AudioRetentionPolicy,
    pub session_identifier: Option<String>,
    pub application_identifier: Option<String>,
    pub recognized_transcription: Option<TranscriptionResult>,
    pub recognition_duration_seconds: Option<f64>,
    pub recognition_used_batch_fallback: bool,
    pub recognition_fallback_reason: Option<String>,
    pub delivery_target: Option<&'a dyn DeliveryTarget>,
    pub transcript_preparation: Option<TranscriptPreparationSnapshot>,
    pub defer_delivery: bool,
    pub images: Vec<ImageInput>,
}
impl Default for Completion<'_> {
    fn default() -> Self {
        Self {
            allow_auto_paste: false,
            incognito: false,
            audio_retention_policy: AudioRetentionPolicy::Failures,
            session_identifier: None,
            application_identifier: None,
            recognized_transcription: None,
            recognition_duration_seconds: None,
            recognition_used_batch_fallback: false,
            recognition_fallback_reason: None,
            delivery_target: None,
            transcript_preparation: None,
            defer_delivery: false,
            images: vec![],
        }
    }
}

#[derive(Debug)]
pub struct WorkflowResult {
    pub transcription: TranscriptionResult,
    pub output_text: String,
    pub delivery: DeliveryReceipt,
    pub history_entry: Option<HistoryEntry>,
    pub retained_audio_path: Option<PathBuf>,
    pub incognito: bool,
    pub mode: String,
    pub recognition_ms: i64,
    pub enhancement_ms: i64,
    pub delivery_ms: i64,
    pub session_identifier: String,
    pub recognition_fallback: bool,
    pub recognition_route: String,
    pub recognition_fallback_reason: Option<String>,
}

#[derive(Debug)]
pub struct WorkflowFailure {
    pub message: String,
    pub stage: String,
    pub history_entry: Option<HistoryEntry>,
    pub retained_audio_path: Option<PathBuf>,
    pub output_text: String,
}
impl fmt::Display for WorkflowFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for WorkflowFailure {}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Failure(#[from] Box<WorkflowFailure>),
}
impl From<WorkflowFailure> for WorkflowError {
    fn from(failure: WorkflowFailure) -> Self {
        Self::Failure(Box::new(failure))
    }
}
pub type WorkflowOutcome<T> = Result<T, WorkflowError>;

#[derive(Default)]
struct Progress {
    recognition_route: Option<String>,
    recognition_fallback_reason: Option<String>,
    recognition_ms: Option<i64>,
    enhancement_ms: Option<i64>,
    delivery_ms: Option<i64>,
}

/// One application-owned speech transport and compatible private stores. The caller
/// freezes deterministic capture preparation before recording, then supplies it here.
/// GTK capture targets stay on their owning main context; callers must not move a
/// target-bearing completion onto a provider worker thread.
pub struct DictationWorkflow {
    pub config: AppConfig,
    pub speech: SpeechClient,
    pub history: HistoryStore,
    pub cwd: PathBuf,
    pub personalization: Option<PersonalizationStore>,
    pub diagnostics: Option<DiagnosticsStore>,
}
impl DictationWorkflow {
    pub fn new(
        config: AppConfig,
        speech: SpeechClient,
        history: HistoryStore,
        cwd: PathBuf,
    ) -> Self {
        Self {
            config,
            speech,
            history,
            cwd,
            personalization: None,
            diagnostics: None,
        }
    }
    pub fn recognition_provider(&self) -> DiagnosticProvider {
        match self.config.transcription_provider.as_str() {
            "litellm" => DiagnosticProvider::Litellm,
            "local" => DiagnosticProvider::Local,
            _ => DiagnosticProvider::ElevenlabsScribeV2,
        }
    }

    pub fn freeze_transcript_preparation(
        &self,
        mode: &str,
        application: Option<&str>,
    ) -> StoreResult<TranscriptPreparationSnapshot> {
        freeze_transcript_preparation(
            &self.config,
            self.personalization.as_ref(),
            mode,
            application,
        )
    }

    pub async fn complete(
        &self,
        audio_path: &Path,
        mut request: Completion<'_>,
    ) -> WorkflowOutcome<WorkflowResult> {
        validate_images(&request.images)?;
        if request.incognito && !request.images.is_empty() {
            return Err(invalid("Screenshots are unavailable in Incognito."));
        }
        let session = request
            .session_identifier
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let preparation = match &request.transcript_preparation {
            Some(preparation) => preparation.clone(),
            None => self.freeze_transcript_preparation(
                "dictation",
                request.application_identifier.as_deref(),
            )?,
        };
        let recognition_started = Instant::now();
        let mut progress = Progress::default();
        let mut used_fallback = request.recognition_used_batch_fallback;
        let recognition: WorkflowOutcome<(TranscriptionResult, f64)> = async {
            let (route, reason) = resolve_recognition_metadata(
                request.recognized_transcription.is_some(),
                used_fallback,
                request.recognition_fallback_reason.as_deref(),
            )?;
            progress.recognition_route = Some(route.into());
            progress.recognition_fallback_reason = reason;
            if self.config.transcription_provider != "elevenlabs" {
                progress.recognition_route = Some(
                    if self.config.transcription_provider == "local" {
                        "managed-local"
                    } else {
                        "litellm-batch"
                    }
                    .into(),
                );
                progress.recognition_fallback_reason = None;
                used_fallback = false;
            }
            match request.recognized_transcription.take() {
                Some(transcription) => {
                    let seconds = request
                        .recognition_duration_seconds
                        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                        .ok_or_else(|| {
                            invalid(
                                "Realtime recognition duration must be a finite nonnegative value.",
                            )
                        })?;
                    Ok((transcription, seconds))
                }
                None => {
                    let transcription = self
                        .speech
                        .transcribe(
                            audio_path,
                            &self.config.language_code,
                            &self.config.transcription_model,
                        )
                        .await?;
                    Ok((transcription, recognition_started.elapsed().as_secs_f64()))
                }
            }
        }
        .await;
        let (transcription, recognition_seconds) = match recognition {
            Ok(value) => value,
            Err(error) => {
                let seconds = recognition_started.elapsed().as_secs_f64();
                progress.recognition_ms = Some(milliseconds(seconds));
                self.record_diagnostic(
                    &session,
                    &request,
                    DiagnosticStage::Recognition,
                    self.recognition_provider(),
                    DiagnosticOutcome::Failed,
                    seconds,
                );
                return Err(self
                    .record_failure(
                        &error,
                        "recognition",
                        audio_path,
                        &request,
                        None,
                        "",
                        progress,
                    )
                    .into());
            }
        };
        let recognition_ms = milliseconds(recognition_seconds);
        progress.recognition_ms = Some(recognition_ms);
        self.record_diagnostic(
            &session,
            &request,
            DiagnosticStage::Recognition,
            self.recognition_provider(),
            if used_fallback {
                DiagnosticOutcome::SafeFallback
            } else {
                DiagnosticOutcome::Completed
            },
            recognition_seconds,
        );
        if text::trim(&transcription.text).is_empty() {
            let retain = !request.incognito && request.audio_retention_policy.should_retain(true);
            if !retain {
                remove_audio(audio_path)?;
            }
            return Ok(WorkflowResult {
                transcription,
                output_text: String::new(),
                delivery: ready("No speech detected. Ready when you are."),
                history_entry: None,
                retained_audio_path: retain.then(|| audio_path.into()),
                incognito: request.incognito,
                mode: "dictation".into(),
                recognition_ms,
                enhancement_ms: 0,
                delivery_ms: 0,
                session_identifier: session,
                recognition_fallback: used_fallback,
                recognition_route: progress.recognition_route.unwrap(),
                recognition_fallback_reason: progress.recognition_fallback_reason,
            });
        }
        let enhancement_started = Instant::now();
        let output_text = preparation.process(&transcription.text);
        let enhancement_seconds = enhancement_started.elapsed().as_secs_f64();
        let enhancement_ms = milliseconds(enhancement_seconds);
        self.record_diagnostic(
            &session,
            &request,
            DiagnosticStage::Enhancement,
            DiagnosticProvider::Local,
            DiagnosticOutcome::Completed,
            enhancement_seconds,
        );
        progress.enhancement_ms = Some(enhancement_ms);
        let delivery_started = Instant::now();
        let mut warnings = Vec::new();
        let delivery = self.deliver(&output_text, &request, &mut warnings);
        let delivery_seconds = delivery_started.elapsed().as_secs_f64();
        let delivery_ms = milliseconds(delivery_seconds);
        let mut delivery = match delivery {
            Ok(delivery) => delivery,
            Err(error) => {
                progress.delivery_ms = Some(delivery_ms);
                self.record_diagnostic(
                    &session,
                    &request,
                    DiagnosticStage::Delivery,
                    DiagnosticProvider::Desktop,
                    DiagnosticOutcome::Failed,
                    delivery_seconds,
                );
                return Err(self
                    .record_failure(
                        &error,
                        "delivery",
                        audio_path,
                        &request,
                        Some(&transcription),
                        &output_text,
                        progress,
                    )
                    .into());
            }
        };
        self.record_diagnostic(
            &session,
            &request,
            DiagnosticStage::Delivery,
            DiagnosticProvider::Desktop,
            if request.defer_delivery {
                DiagnosticOutcome::Deferred
            } else if delivery.paste_dispatched && !delivery.pasted {
                DiagnosticOutcome::SafeFallback
            } else {
                DiagnosticOutcome::Completed
            },
            delivery_seconds,
        );
        if used_fallback {
            warnings.insert(
                0,
                fallback_guidance(progress.recognition_fallback_reason.as_deref()).into(),
            );
        }
        if !warnings.is_empty() {
            delivery
                .guidance
                .push_str(&format!(" {}", warnings.join(" ")));
        }
        let should_retain =
            !request.incognito && request.audio_retention_policy.should_retain(true);
        let mut retained_audio_path = should_retain.then(|| audio_path.to_owned());
        let mut entry = None;
        let mut history_error = None;
        let recognition_route = progress.recognition_route.clone().unwrap();
        let recognition_fallback_reason = progress.recognition_fallback_reason.clone();
        if !request.incognito {
            let outcome = delivery.history_outcome();
            progress.delivery_ms = Some(delivery_ms);
            match self.history.add(history_input(
                &request,
                &self.config,
                Some(&transcription),
                &output_text,
                outcome,
                retained_audio_path.as_deref(),
                progress,
            )) {
                Ok(saved) => entry = Some(saved),
                Err(error) => {
                    history_error = Some(error);
                    retained_audio_path = None;
                }
            }
        }
        if retained_audio_path.is_none() {
            remove_audio(audio_path)?;
        }
        if let Some(error) = history_error {
            delivery
                .guidance
                .push_str(&format!(" Local history could not be saved: {error}"));
        }
        Ok(WorkflowResult {
            transcription,
            output_text,
            delivery,
            history_entry: entry,
            retained_audio_path,
            incognito: request.incognito,
            mode: "dictation".into(),
            recognition_ms,
            enhancement_ms,
            delivery_ms,
            session_identifier: session,
            recognition_fallback: used_fallback,
            recognition_route,
            recognition_fallback_reason,
        })
    }

    fn deliver(
        &self,
        output: &str,
        request: &Completion<'_>,
        warnings: &mut Vec<String>,
    ) -> Result<DeliveryReceipt, mluva_core::delivery::DeliveryError> {
        if request.defer_delivery {
            return Ok(ready("Dictation ready for delivery."));
        }
        let mut auto_paste = request.allow_auto_paste;
        if auto_paste
            && !request
                .delivery_target
                .is_some_and(|target| target.restore().unwrap_or(false))
        {
            auto_paste = false;
            warnings.push(
                "The captured text target could not be restored, so no paste was attempted.".into(),
            );
        }
        if let Some(target) = request.delivery_target.filter(|_| auto_paste) {
            let mut confirm = || target.confirm_insertion(output);
            let mut insert = |text: &str| target.insert_text(text);
            let mut authorize = || target.restore();
            deliver_text(
                output,
                true,
                DeliveryOptions {
                    confirm_paste: Some(&mut confirm),
                    insert_directly: Some(&mut insert),
                    authorize_keyboard_paste: Some(&mut authorize),
                    application_identifier: target.application_identifier(),
                    ..Default::default()
                },
            )
        } else {
            deliver_text(output, false, DeliveryOptions::default())
        }
    }

    fn record_diagnostic(
        &self,
        session: &str,
        request: &Completion<'_>,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        seconds: f64,
    ) {
        if let Some(store) = self.diagnostics.as_ref().filter(|_| !request.incognito) {
            let _ = store.record(session, "dictation", stage, provider, outcome, seconds);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_failure(
        &self,
        error: &dyn fmt::Display,
        stage: &str,
        audio_path: &Path,
        request: &Completion<'_>,
        transcription: Option<&TranscriptionResult>,
        output: &str,
        progress: Progress,
    ) -> WorkflowFailure {
        if request.incognito || !request.audio_retention_policy.should_retain(false) {
            let _ = remove_audio(audio_path);
        }
        let retained_audio_path = audio_path.exists().then(|| audio_path.to_owned());
        let mut message = format!(
            "{}{} failed: {error}",
            stage[..1].to_uppercase(),
            &stage[1..]
        );
        let history_entry = if request.incognito {
            None
        } else {
            match self.history.add(history_input(
                request,
                &self.config,
                transcription,
                output,
                &format!("{stage}-failed"),
                retained_audio_path.as_deref(),
                progress,
            )) {
                Ok(entry) => Some(entry),
                Err(error) => {
                    message.push_str(&format!(
                        ". Local failure history could not be saved: {error}"
                    ));
                    None
                }
            }
        };
        WorkflowFailure {
            message,
            stage: stage.into(),
            history_entry,
            retained_audio_path,
            output_text: output.into(),
        }
    }

    /// Retry only managed failure audio and persist a preview; no desktop delivery.
    pub async fn retry_recognition(&self, identifier: &str) -> WorkflowOutcome<HistoryEntry> {
        let entry = self.history.find(identifier)?;
        let path = self.history.managed_retained_audio(identifier)?;
        let started = Instant::now();
        let transcription = self
            .speech
            .transcribe(
                &path,
                &entry.language_code,
                &self.config.transcription_model,
            )
            .await?;
        let recognition_ms = milliseconds(started.elapsed().as_secs_f64());
        let policy = entry
            .audio_retention_policy
            .as_deref()
            .unwrap_or("failures");
        if !["never", "failures", "always"].contains(&policy) {
            return Err(invalid(format!(
                "Unsupported audio retention policy: {policy}"
            )));
        }
        let mut prepared = if self.config.spoken_commands_enabled && entry.mode != "command" {
            normalize_spoken_structure(&transcription.text)
        } else {
            transcription.text.clone()
        };
        if let Some(store) = &self.personalization {
            prepared = store.process_transcript(
                &prepared,
                entry.application_identifier.as_deref(),
                &snippet_variables(),
            )?;
        }
        let route = match self.config.transcription_provider.as_str() {
            "local" => "managed-local-retry",
            "litellm" => "litellm-batch-retry",
            _ => "scribe-v2-batch-retry",
        };
        Ok(self.history.mark_retry_ready(
            identifier,
            RetryRecognition {
                raw_text: &transcription.text,
                delivered_text: &prepared,
                language_code: &transcription.language_code,
                transcription_id: transcription.transcription_id.as_deref(),
                retain_audio: policy != "never",
                recognition_ms: Some(recognition_ms),
                recognition_route: route,
            },
        )?)
    }

    pub async fn close(&self) {
        self.speech.close().await;
    }
    pub async fn cancel(&self) {
        self.speech.cancel().await;
    }
}

fn history_input(
    request: &Completion<'_>,
    config: &AppConfig,
    transcription: Option<&TranscriptionResult>,
    output: &str,
    outcome: &str,
    retained: Option<&Path>,
    progress: Progress,
) -> HistoryInput {
    HistoryInput {
        raw_text: transcription.map_or_else(String::new, |result| result.text.clone()),
        delivered_text: output.into(),
        mode: "dictation".into(),
        language_code: transcription.map_or_else(
            || config.language_code.clone(),
            |result| result.language_code.clone(),
        ),
        transcription_id: transcription.and_then(|result| result.transcription_id.clone()),
        delivery_outcome: outcome.into(),
        retained_audio_path: retained.map(|path| path.to_string_lossy().into_owned()),
        audio_retention_policy: Some(
            match request.audio_retention_policy {
                AudioRetentionPolicy::Never => "never",
                AudioRetentionPolicy::Failures => "failures",
                AudioRetentionPolicy::Always => "always",
            }
            .into(),
        ),
        application_identifier: request.application_identifier.clone(),
        recognition_route: progress.recognition_route,
        recognition_fallback_reason: progress.recognition_fallback_reason,
        recognition_ms: progress.recognition_ms,
        enhancement_ms: progress.enhancement_ms,
        delivery_ms: progress.delivery_ms,
        ..HistoryInput::dictation("", "")
    }
}
fn resolve_recognition_metadata(
    realtime: bool,
    fallback: bool,
    reason: Option<&str>,
) -> WorkflowOutcome<(&'static str, Option<String>)> {
    if realtime {
        if fallback {
            return Err(invalid(
                "A committed realtime result cannot also be marked as a batch fallback.",
            ));
        }
        if reason.is_some() {
            return Err(invalid(
                "A committed realtime result cannot have a batch fallback reason.",
            ));
        }
        return Ok(("scribe-v2-realtime", None));
    }
    if !fallback {
        if reason.is_some() {
            return Err(invalid(
                "A recognition fallback reason requires a batch fallback.",
            ));
        }
        return Ok(("scribe-v2-batch", None));
    }
    let reason = reason.unwrap_or("realtime-unavailable");
    if !RECOGNITION_FALLBACKS.contains(&reason) {
        return Err(invalid(format!(
            "Unsupported recognition fallback reason: {reason}"
        )));
    }
    Ok(("scribe-v2-batch", Some(reason.into())))
}
fn fallback_guidance(reason: Option<&str>) -> &'static str {
    match reason {
        Some("realtime-startup-failed") => {
            "Realtime recognition could not start; this capture was completed with batch Scribe v2."
        }
        Some("realtime-stream-failed") => {
            "Realtime recognition did not produce committed text; this capture was completed with batch Scribe v2."
        }
        _ => {
            "Realtime recognition was unavailable; this capture was completed with batch Scribe v2."
        }
    }
}
fn ready(guidance: &str) -> DeliveryReceipt {
    DeliveryReceipt {
        copied: false,
        pasted: false,
        guidance: guidance.into(),
        paste_dispatched: false,
        paste_confirmed: None,
    }
}
fn remove_audio(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
fn invalid(message: impl Into<String>) -> WorkflowError {
    WorkflowError::Invalid(message.into())
}
