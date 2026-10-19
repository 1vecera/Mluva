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
    scratchpad::ScratchpadDraftStore,
    screenshots::ScreenshotStore,
};
use mluva_providers::{
    Secret,
    batch_preview::BatchPreviewClient,
    credentials::CredentialStore,
    local_asr::OnnxOptions,
    local_preview::LocalPreviewClient,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions, SCRIBE_REALTIME_ENDPOINT},
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
];
/// The parent supplies actual owner state. Presentation remains editable while busy.
#[derive(Default)]
pub struct SettingsActivity {
    pub preparing: bool,
    pub processing: bool,
    pub recording: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsKind {
    Unchanged,
    Presentation,
    Providers,
}

/// Incognito stays enabled for the session even if its settings cannot be saved.
#[derive(Clone, Copy)]
pub enum InlineSavePolicy {
    Persistent,
    SessionIncognito,
}

#[derive(Clone, Debug)]
pub struct SettingsUpdate {
    pub config: AppConfig,
    pub changed: BTreeSet<String>,
    pub kind: SettingsKind,
}

/// These stores, including the separate Meeting archive, open before capture
/// credentials. Desktop owners attach their surfaces to the same local stores.
pub struct ApplicationServices {
    pub paths: AppPaths,
    pub cwd: PathBuf,
    pub config_load_error: String,
    config: RefCell<AppConfig>,
    pub personalization: Rc<RefCell<PersonalizationStore>>,
    pub history: HistoryStore,
    pub conversations: ConversationStore,
    pub screenshots: ScreenshotStore,
    pub meetings: Arc<Mutex<MeetingStore>>,
    pub recovery_message: String,
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
            Err(_) => (AppConfig::default(), "config.json needs repair; using defaults. The file is preserved. Your saved data is unchanged.".into()),
        };
        let personalization = PersonalizationStore::new(paths.config.join("personalization.json"));
        let cwd = paths.runtime.join("workspace");
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
        let mut legacy_notes = ScratchpadDraftStore::new(paths.data.join("scratchpad-draft.json"));
        let (recovered, recovery_message) = match legacy_notes.migrate_to_history(&history) {
            Ok(Some(id)) => (Some(id), "Your unfinished Notes draft is now in local History. It has not been sent or synced.".into()),
            Ok(None) => (None, String::new()),
            Err(error) => (None, format!("Your old Notes recovery file is preserved but could not be opened: {error}")),
        };
        let meetings = MeetingStore::new(paths.data.join("meetings/meetings.json"), None);
        let diagnostics = DiagnosticsStore::new(paths.data.join("diagnostics.sqlite3"), 5_000)?;
        diagnostics.initialize()?;
        let services = Rc::new(Self {
            paths,
            cwd,
            config_load_error,
            config: RefCell::new(config),
            personalization: Rc::new(RefCell::new(personalization)),
            history,
            conversations,
            screenshots,
            meetings: Arc::new(Mutex::new(meetings)),
            recovery_message,
            diagnostics,
            credentials: Arc::new(CredentialStore::new()),
        });
        services.prune_history(&recovered.into_iter().collect())?;
        Ok(services)
    }

    pub fn config(&self) -> AppConfig {
        self.config.borrow().clone()
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
            }
        }
        saved
    }

    /// Protect entries owned by active retries or continued recordings.
    pub fn prune_history(&self, excluded: &BTreeSet<String>) -> StoreResult<usize> {
        let days = self
            .config
            .borrow()
            .history_retention_days
            .as_i64()
            .ok_or_else(|| StoreError::Invalid("History retention interval is too large".into()))?;
        self.history.prune_older_than(days, excluded, Utc::now())
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
        let changes = changes.clone();
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
            .all(|name| PRESENTATION.contains(&name.as_str()))
        {
            SettingsKind::Presentation
        } else {
            if activity.preparing || activity.processing || activity.recording {
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
    pub screenshot_editor: PathBuf,
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
            screenshot_editor: directory.join("mluva-screenshot-editor"),
        })
    }
}

/// One immutable readiness configuration. Every recording creates its own
/// transport pair and snapshots current personalization/prompts before capture.
#[derive(Clone)]
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
}
impl CaptureServices {
    /// Recovery and recording each own fresh clients and the current local rules.
    pub fn workflow(&self) -> WorkflowOutcome<Rc<DictationWorkflow>> {
        let speech =
            SpeechClient::from_config(&self.config, self.local.clone(), self.speech_key.clone())?;
        let mut workflow = DictationWorkflow::new(
            self.config.clone(),
            speech,
            self.history.clone(),
            self.cwd.clone(),
        );
        workflow.personalization = Some(self.personalization.borrow().clone());
        workflow.diagnostics = Some(self.diagnostics.clone());
        Ok(Rc::new(workflow))
    }

    pub fn launch(&self, options: CaptureOptions) -> WorkflowOutcome<Rc<CaptureSession>> {
        self.launch_input(options, false)
    }
    pub fn launch_phone(&self, options: CaptureOptions) -> WorkflowOutcome<Rc<CaptureSession>> {
        self.launch_input(options, true)
    }
    fn launch_input(
        &self,
        options: CaptureOptions,
        phone: bool,
    ) -> WorkflowOutcome<Rc<CaptureSession>> {
        let recorder = if phone {
            mluva_audio::capture::CaptureRecorder::phone()
        } else {
            PipeWireRecorder::from_system(self.config.microphone_target.clone())
                .and_then(mluva_audio::capture::CaptureRecorder::new)
        }
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
                    preview_enabled: true,
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
        CaptureSession::with_recorder(workflow, recorder, storage, Some(recognition), options)
    }
}
