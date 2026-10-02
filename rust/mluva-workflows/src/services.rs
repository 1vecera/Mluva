//! Application-owned local recovery and immutable capture service construction.

use crate::{
    capture::{CaptureOptions, CaptureRecognitionClient, CaptureSession},
    dictation::{DictationWorkflow, WorkflowError, WorkflowOutcome},
};
use chrono::Utc;
use mluva_audio::{capture::CaptureStorage, recorder::PipeWireRecorder};
use mluva_core::{
    config::{AppConfig, AppPaths, ConfigError},
    conversation::ConversationStore,
    database::{StoreError, StoreResult},
    diagnostics::DiagnosticsStore,
    history::{HistoryInput, HistoryStore},
    meeting::MeetingStore,
    personalization::PersonalizationStore,
    prompts::PromptStore,
    scratchpad::{ScratchpadDraft, ScratchpadDraftStore},
    screenshots::ScreenshotStore,
};
use mluva_providers::{
    Secret,
    batch_preview::BatchPreviewClient,
    credentials::CredentialStore,
    local_asr::OnnxOptions,
    local_preview::LocalPreviewClient,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions, SCRIBE_REALTIME_ENDPOINT},
    rewriting::RewriteClient,
    speech::SpeechClient,
};
use serde_json::{Map, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

const PRESENTATION: &[&str] = &[
    "welcome_completed",
    "history_sidebar_visible",
    "time_format",
    "widget_position",
    "widget_lines",
    "widget_opacity",
    "show_copy_action",
    "show_save_action",
    "smooth_scrolling",
    "scroll_duration_ms",
    "scroll_lookahead_lines",
    "review_timeout_seconds",
];
const LIVE: &[&str] = &[
    "live_rewrite_enabled",
    "live_rewrite_continuous",
    "live_rewrite_template",
    "live_rewrite_custom_instructions",
    "live_rewrite_min_characters",
    "live_rewrite_interval_seconds",
];

/// The parent supplies actual owner state. Presentation remains editable while
/// busy; Live changes have the released narrower recording exception.
pub struct SettingsActivity {
    pub preparing: bool,
    pub processing: bool,
    pub recording: bool,
    pub pending_incognito: bool,
    pub pending_mode: String,
    pub final_live: bool,
    pub rewriting: bool,
    pub live_schedule: bool,
}

impl Default for SettingsActivity {
    fn default() -> Self {
        Self {
            preparing: false,
            processing: false,
            recording: false,
            pending_incognito: false,
            pending_mode: "dictation".into(),
            final_live: false,
            rewriting: false,
            live_schedule: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsKind {
    Unchanged,
    Presentation { live_changed: bool },
    Providers,
}

/// Inline controls historically retain these privacy/title choices for the
/// running session even when their settings document cannot be written.
#[derive(Clone, Copy)]
pub enum InlineSavePolicy {
    Persistent,
    SessionIncognito,
    SessionTitles,
}

#[derive(Clone, Debug)]
pub struct SettingsUpdate {
    pub config: AppConfig,
    pub changed: BTreeSet<String>,
    pub kind: SettingsKind,
}

#[derive(Clone, Debug)]
pub enum ScratchpadRecovery {
    Empty,
    Restored(ScratchpadDraft),
    Malformed,
    DiscardedPrivate,
}
impl ScratchpadRecovery {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "",
            Self::Restored(_) => "Recovered an unresolved Scratchpad draft and its source audio.",
            Self::Malformed => {
                "The Scratchpad recovery document is malformed and was preserved. Repair it before saving another persistent Scratchpad draft."
            }
            Self::DiscardedPrivate => "Discarded invalid persisted Incognito recovery state.",
        }
    }
}

/// These stores, including the separate Meeting archive, open before capture
/// credentials. Desktop owners attach their surfaces to the same local stores.
pub struct ApplicationServices {
    pub paths: AppPaths,
    pub cwd: PathBuf,
    pub config_load_error: String,
    config: RefCell<AppConfig>,
    pub personalization: Rc<RefCell<PersonalizationStore>>,
    pub prompts: Rc<RefCell<PromptStore>>,
    pub history: HistoryStore,
    pub conversations: ConversationStore,
    pub screenshots: ScreenshotStore,
    pub meetings: Arc<Mutex<MeetingStore>>,
    pub scratchpad: Rc<RefCell<ScratchpadDraftStore>>,
    pub diagnostics: DiagnosticsStore,
    pub credentials: Arc<CredentialStore>,
}

impl ApplicationServices {
    pub fn from_environment() -> WorkflowOutcome<Rc<Self>> {
        let environ: BTreeMap<_, _> = [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_RUNTIME_DIR",
        ]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
        .collect();
        let paths = AppPaths::from_environ(&environ)
            .map_err(|error| WorkflowError::Invalid(error.to_string()))?;
        Self::open(paths)
    }

    pub fn open(paths: AppPaths) -> WorkflowOutcome<Rc<Self>> {
        let (config, config_load_error) = match AppConfig::load(&paths.config.join("config.json")) {
            Ok(config) => (config, String::new()),
            Err(_) => (AppConfig::default(), "config.json needs repair; using defaults. The file is preserved. Prompts remain editable.".into()),
        };
        let mut personalization =
            PersonalizationStore::new(paths.config.join("personalization.json"));
        let prompts = PromptStore::new(
            paths.config.join("prompts"),
            &config.live_rewrite_custom_instructions,
            &personalization.styles()?,
        )?;
        personalization.prompt_store = Some(prompts.clone());
        let cwd = paths.runtime.join("codex-workspace");
        fs::create_dir_all(&cwd)?;
        fs::set_permissions(&cwd, fs::Permissions::from_mode(0o700))?;
        let history = HistoryStore::new(paths.data.join("history.sqlite3"));
        history.initialize()?;
        let screenshots = ScreenshotStore::new(&history.database.path);
        if !config.incognito_mode {
            for capture in screenshots.pending_captures()? {
                let recovered = history.add(HistoryInput {
                    language_code: config.language_code.clone(),
                    delivery_outcome: "failed".into(),
                    ..HistoryInput::dictation("", "Screenshots from an interrupted narration.")
                })?;
                screenshots.bind_capture(&capture, &recovered.identifier)?;
            }
        }
        let conversations = ConversationStore::new(history.clone());
        conversations.initialize()?;
        let scratchpad = ScratchpadDraftStore::new(paths.data.join("scratchpad-draft.json"));
        let meetings = MeetingStore::new(paths.data.join("meetings/meetings.json"), None);
        let diagnostics = DiagnosticsStore::new(paths.data.join("diagnostics.sqlite3"), 5_000)?;
        diagnostics.initialize()?;
        let services = Rc::new(Self {
            paths,
            cwd,
            config_load_error,
            config: RefCell::new(config),
            personalization: Rc::new(RefCell::new(personalization)),
            prompts: Rc::new(RefCell::new(prompts)),
            history,
            conversations,
            screenshots,
            meetings: Arc::new(Mutex::new(meetings)),
            scratchpad: Rc::new(RefCell::new(scratchpad)),
            diagnostics,
            credentials: Arc::new(CredentialStore::new()),
        });
        services.prune_history(&BTreeSet::new())?;
        Ok(services)
    }

    pub fn config(&self) -> AppConfig {
        self.config.borrow().clone()
    }

    /// Refresh catalog identities from stored styles, retaining their immutable
    /// recovery instructions rather than turning prompt overrides into defaults.
    pub fn synchronize_style_prompts(&self) -> StoreResult<()> {
        let styles: Vec<_> = mluva_core::prompt_catalog::DEFAULTS
            .styles
            .iter()
            .chain(&self.personalization.borrow().state().custom_styles)
            .cloned()
            .collect();
        let prompts = {
            let mut prompts = self.prompts.borrow_mut();
            prompts.sync_styles(&styles)?;
            prompts.clone()
        };
        self.personalization.borrow_mut().prompt_store = Some(prompts);
        Ok(())
    }

    pub fn save_inline_config(
        &self,
        proposed: AppConfig,
        policy: InlineSavePolicy,
    ) -> Result<(), ConfigError> {
        proposed.validate()?;
        let saved = proposed.save(&self.paths.config.join("config.json"));
        if saved.is_ok() {
            self.config.replace(proposed);
        } else {
            let mut current = self.config.borrow_mut();
            match policy {
                InlineSavePolicy::Persistent => {}
                InlineSavePolicy::SessionIncognito => {
                    current.incognito_mode = proposed.incognito_mode
                }
                InlineSavePolicy::SessionTitles => {
                    current.automatic_titles = proposed.automatic_titles
                }
            }
        }
        saved
    }

    pub fn recover_scratchpad(&self) -> StoreResult<ScratchpadRecovery> {
        let mut store = self.scratchpad.borrow_mut();
        if store.persistence_error.is_some() {
            return Ok(ScratchpadRecovery::Malformed);
        }
        let Some(draft) = store.draft.clone() else {
            return Ok(ScratchpadRecovery::Empty);
        };
        if draft.incognito {
            store.clear(true)?;
            Ok(ScratchpadRecovery::DiscardedPrivate)
        } else {
            // Restoring the actual mode control invokes the same saved-default
            // handler in the release. A failed save still leaves the draft open.
            if self.config.borrow().default_mode != "scratchpad" {
                let _ = self.select_capture_mode("scratchpad", None);
            }
            Ok(ScratchpadRecovery::Restored(draft))
        }
    }

    /// The current identified application's profile owns its mode when enabled;
    /// otherwise this is the persisted default used by subsequent captures.
    pub fn select_capture_mode(
        &self,
        mode: &str,
        application: Option<&str>,
    ) -> WorkflowOutcome<()> {
        let mut config = self.config();
        if config.remember_per_application && application.is_some() {
            self.personalization
                .borrow_mut()
                .select_mode(mode, application, true)?;
        } else {
            config.default_mode = mode.into();
            config
                .save(&self.paths.config.join("config.json"))
                .map_err(|error| WorkflowError::Invalid(error.to_string()))?;
            self.config.replace(config);
        }
        Ok(())
    }

    /// Include retry, continuation and unresolved Command identities from their
    /// owners; the unresolved Scratchpad is always protected here.
    pub fn prune_history(&self, excluded: &BTreeSet<String>) -> StoreResult<usize> {
        let mut excluded = excluded.clone();
        if let Some(identifier) = self
            .scratchpad
            .borrow()
            .draft
            .as_ref()
            .and_then(|draft| draft.history_identifier.as_ref())
        {
            excluded.insert(identifier.clone());
        }
        let days = self
            .config
            .borrow()
            .history_retention_days
            .as_i64()
            .ok_or_else(|| StoreError::Invalid("History retention interval is too large".into()))?;
        self.history.prune_older_than(days, &excluded, Utc::now())
    }

    /// Save before publishing the new configuration. A rejected/write-failed
    /// update leaves both current settings and all active capture snapshots intact.
    /// The desktop applies the returned effects to its exact owners afterwards.
    pub fn apply_settings(
        &self,
        changes: &Map<String, Value>,
        activity: &SettingsActivity,
    ) -> Result<Option<SettingsUpdate>, ConfigError> {
        let previous = self.config();
        let mut changes = changes.clone();
        if changes.get("rewrite_provider").and_then(Value::as_str) == Some("none") {
            changes.insert("live_rewrite_enabled".into(), Value::Bool(false));
            changes.insert("automatic_titles".into(), Value::Bool(false));
        }
        if previous.rewrite_provider == "none"
            && changes.get("live_rewrite_enabled") == Some(&Value::Bool(true))
            && changes
                .get("rewrite_provider")
                .and_then(Value::as_str)
                .unwrap_or("none")
                == "none"
        {
            return Ok(None);
        }
        let mut document = serde_json::to_value(&previous).expect("serializable settings");
        let fields = document.as_object_mut().unwrap();
        let mut changed = BTreeSet::new();
        for (name, value) in changes {
            let Some(old) = fields.get(&name) else {
                return Err(ConfigError::Fields);
            };
            if *old != value {
                changed.insert(name.clone());
            }
            fields.insert(name, value);
        }
        let config: AppConfig =
            serde_json::from_value(document).map_err(|_| ConfigError::Fields)?;
        config.validate()?;
        let kind = if changed.is_empty() {
            return Ok(Some(SettingsUpdate {
                config,
                changed,
                kind: SettingsKind::Unchanged,
            }));
        } else if changed
            .iter()
            .all(|name| PRESENTATION.contains(&name.as_str()) || LIVE.contains(&name.as_str()))
        {
            let live_changed = changed.iter().any(|name| LIVE.contains(&name.as_str()));
            if live_changed
                && (activity.preparing
                    || activity.processing
                    || activity.final_live
                    || activity.rewriting
                    || (activity.recording
                        && (activity.pending_incognito || activity.pending_mode != "dictation")))
            {
                return Ok(None);
            }
            SettingsKind::Presentation { live_changed }
        } else {
            if activity.preparing
                || activity.processing
                || activity.recording
                || activity.rewriting
                || activity.live_schedule
            {
                return Ok(None);
            }
            SettingsKind::Providers
        };
        if config.save(&self.paths.config.join("config.json")).is_err() {
            return Ok(None);
        }
        self.config.replace(config.clone());
        Ok(Some(SettingsUpdate {
            config,
            changed,
            kind,
        }))
    }

    /// Retry creates an isolated readiness result. The parent installs it only
    /// if its frozen config still matches, and remains editable on failure.
    pub async fn capture_services(
        &self,
        binaries: NativeBinaries,
    ) -> WorkflowOutcome<CaptureServices> {
        let config = self.config();
        PipeWireRecorder::from_system(config.microphone_target.clone())
            .map_err(|error| WorkflowError::Invalid(error.to_string()))?;
        let speech_key = if config.transcription_provider == "elevenlabs" {
            Some(self.credentials.elevenlabs_api_key().await?)
        } else {
            None
        };
        let mut local =
            OnnxOptions::new(&self.paths.data, &config.local_model, binaries.asr_worker);
        local.device = config.local_device.clone();
        let speech = SpeechClient::from_config(&config, local.clone(), speech_key.clone())?;
        speech.close().await;
        Ok(CaptureServices {
            config,
            speech_key,
            local,
            cleanup_executable: binaries.audio_cleanup,
            data: self.paths.data.clone(),
            cwd: self.cwd.clone(),
            history: self.history.clone(),
            diagnostics: self.diagnostics.clone(),
            personalization: self.personalization.clone(),
            prompts: self.prompts.clone(),
        })
    }

    /// Meeting readiness is independent of the selected dictation provider. A
    /// missing key disables only explicit Meeting upload, leaving local review.
    pub async fn meeting_services(
        &self,
        binaries: &NativeBinaries,
    ) -> WorkflowOutcome<Arc<crate::meeting_services::MeetingServices>> {
        let executable =
            mluva_core::executables::find_executable("pw-record").ok_or_else(|| {
                WorkflowError::Invalid(
                    "pw-record is required. Install PipeWire tools for your distribution.".into(),
                )
            })?;
        let key = self.credentials.elevenlabs_api_key().await?;
        Ok(Arc::new(
            crate::meeting_services::MeetingServices::new(
                key,
                mluva_providers::elevenlabs::SCRIBE_ENDPOINT.into(),
                executable,
                binaries.audio_cleanup.clone(),
                self.meetings.clone(),
            )
            .with_diagnostics(self.diagnostics.clone()),
        ))
    }
}

/// Distribution supplies paths to its own native workers, never an interpreter.
#[derive(Clone)]
pub struct NativeBinaries {
    pub asr_worker: PathBuf,
    pub audio_cleanup: PathBuf,
}
impl NativeBinaries {
    pub fn beside_application() -> std::io::Result<Self> {
        let executable = std::env::current_exe()?;
        let directory = executable
            .parent()
            .ok_or_else(|| std::io::Error::other("Application needs a directory"))?;
        Ok(Self {
            asr_worker: directory.join("mluva-asr-worker"),
            audio_cleanup: directory.join("mluva-audio-cleanup"),
        })
    }
}

/// One immutable readiness configuration. Every recording creates its own
/// transport pair and snapshots current personalization/prompts before capture.
pub struct CaptureServices {
    pub config: AppConfig,
    speech_key: Option<Secret>,
    local: OnnxOptions,
    cleanup_executable: PathBuf,
    data: PathBuf,
    cwd: PathBuf,
    history: HistoryStore,
    diagnostics: DiagnosticsStore,
    personalization: Rc<RefCell<PersonalizationStore>>,
    prompts: Rc<RefCell<PromptStore>>,
}
impl CaptureServices {
    /// Recovery and recording each own fresh clients and the current local rules.
    pub fn workflow(&self) -> WorkflowOutcome<Rc<DictationWorkflow>> {
        let speech =
            SpeechClient::from_config(&self.config, self.local.clone(), self.speech_key.clone())?;
        let rewrite = RewriteClient::new(&self.config, None, None)?;
        let mut workflow = DictationWorkflow::new(
            self.config.clone(),
            speech,
            rewrite,
            self.history.clone(),
            self.cwd.clone(),
        );
        workflow.personalization = Some(self.personalization.borrow().clone());
        workflow.diagnostics = Some(self.diagnostics.clone());
        workflow.cleanup_instructions = self.prompts.borrow().read("cleanup")?.text;
        Ok(Rc::new(workflow))
    }

    pub fn launch(&self, options: CaptureOptions) -> WorkflowOutcome<Rc<CaptureSession>> {
        let recorder = PipeWireRecorder::from_system(self.config.microphone_target.clone())
            .map_err(|error| WorkflowError::Invalid(error.to_string()))?;
        let workflow = self.workflow()?;
        let previews = self
            .cwd
            .parent()
            .expect("runtime workspace")
            .join("speech-previews");
        let recognition = match self.config.transcription_provider.as_str() {
            "local" => CaptureRecognitionClient::Local(Rc::new(LocalPreviewClient::new(
                self.local.clone(),
                previews,
            ))),
            "litellm" => {
                let config = self.config.clone();
                let local = self.local.clone();
                CaptureRecognitionClient::Batch(Rc::new(BatchPreviewClient {
                    factory: Arc::new(move || {
                        let config = config.clone();
                        let local = local.clone();
                        Box::pin(async move {
                            SpeechClient::from_config(&config, local, None).map(Arc::new)
                        })
                    }),
                    directory: previews,
                    chunk_seconds: self.config.transcription_chunk_seconds as u32,
                    preview_enabled: self.config.live_rewrite_enabled,
                }))
            }
            _ => CaptureRecognitionClient::ElevenLabs(Rc::new(ElevenLabsRealtimeClient::new(
                self.speech_key
                    .clone()
                    .expect("credential checked during readiness"),
                SCRIBE_REALTIME_ENDPOINT,
                RealtimeOptions::default(),
            )?)),
        };
        let storage = if options.incognito {
            CaptureStorage::Incognito {
                cleanup_executable: self.cleanup_executable.clone(),
                memory_root: None,
            }
        } else {
            CaptureStorage::Persistent(self.data.join("recordings"))
        };
        CaptureSession::new(workflow, recorder, storage, Some(recognition), options)
    }
}
