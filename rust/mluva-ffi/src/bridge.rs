//! One long-lived owner of the shared stores, provider sessions and agent route.
//!
//! The file layout under the shell's configuration and data directories matches the Linux
//! application's XDG layout, so settings, personalization and History use one format.
use crate::{agents, annotate};
use mluva_core::{
    agent_voice::{Submission, Target, VoiceRoute, VoiceSettings},
    annotations::{AnnotationSettings, AnnotationStore},
    config::AppConfig,
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
    personalization::{DictionaryCaseBehavior, PersonalizationStore},
    vocabulary,
};
use mluva_providers::{
    Secret,
    agent_targets::{self, Sources},
    claude_sessions::ClaudeRegistry,
    codex::threads::CodexThreads,
    credentials::validate_speech_key,
    elevenlabs::{ElevenLabsClient, SCRIBE_ENDPOINT},
    herdr::{self, CodexNames},
    realtime::{
        ElevenLabsRealtimeClient, RealtimeOptions, RealtimeSession, SCRIBE_REALTIME_ENDPOINT,
    },
};
use mluva_workflows::{
    dictation::with_appendix,
    preparation::{TranscriptPreparationSnapshot, freeze_transcript_preparation},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

/// Delivers one event to the shell. Called from runtime threads.
pub type Emitter = Arc<dyn Fn(Value) + Send + Sync>;

type Outcome = Result<Value, String>;

struct Paths {
    config: PathBuf,
    data: PathBuf,
    /// Where Herdr keeps its sockets: `~/.config` unless `XDG_CONFIG_HOME` says otherwise.
    herdr_config_home: PathBuf,
}

struct Stores {
    paths: Paths,
    history: HistoryStore,
    conversations: ConversationStore,
    personalization: PersonalizationStore,
    annotations: AnnotationStore,
    /// Bundles planned and waiting for the shell's images, by identifier.
    pending: HashMap<String, annotate::Pending>,
}

pub struct Bridge {
    runtime: tokio::runtime::Runtime,
    emit: Emitter,
    stores: Mutex<Option<Stores>>,
    realtime: Arc<Mutex<HashMap<String, Arc<RealtimeSession>>>>,
    /// Where agent discovery looks.
    reach: Mutex<Reach>,
    /// The targets of the latest discovery pass, with what their transports need.
    targets: Arc<Mutex<Vec<Target>>>,
    route: Arc<Mutex<VoiceRoute>>,
    closed: CancellationToken,
}

/// The sources agent discovery reads beyond Herdr.
struct Reach {
    /// Claude Code's own session registry.
    claude: Option<Arc<Mutex<ClaudeRegistry>>>,
    /// Live Codex threads; `None` when the shell leaves Codex out.
    codex: Option<Arc<tokio::sync::Mutex<CodexThreads>>>,
    /// Codex's session index, which names Codex threads in Herdr panes and outside them.
    codex_names: Option<Arc<Mutex<CodexNames>>>,
    /// Live Antigravity CLI sessions.
    antigravity: Option<Arc<tokio::sync::Mutex<mluva_providers::antigravity::AntigravitySessions>>>,
    /// Antigravity conversation titles from `conversation_summaries.db`.
    antigravity_titles: Option<Arc<Mutex<mluva_providers::antigravity::AntigravityTitles>>>,
    /// OpenCode session titles from `opencode.db`.
    opencode_titles: Option<Arc<Mutex<mluva_providers::opencode::OpenCodeTitles>>>,
    /// Desktop app surfaces, such as a new Claude chat.
    apps: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn param<T: DeserializeOwned>(params: &Value, name: &str) -> Result<T, String> {
    serde_json::from_value(params.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|_| format!("The request needs a valid `{name}`."))
}

fn optional<T: DeserializeOwned>(params: &Value, name: &str) -> Result<Option<T>, String> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => param(params, name).map(Some),
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// An owner-only directory, created when missing.
fn private_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Mluva's directories must be absolute paths.".into());
    }
    fs::create_dir_all(path).map_err(text)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(text)
}

impl Bridge {
    pub fn new(emit: Emitter) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("mluva-bridge")
            .enable_all()
            .build()
            .expect("the bridge runtime starts");
        Self {
            runtime,
            emit,
            stores: Mutex::new(None),
            realtime: Arc::default(),
            reach: Mutex::new(Reach {
                claude: ClaudeRegistry::from_environment().map(|r| Arc::new(Mutex::new(r))),
                codex: Some(Arc::default()),
                codex_names: herdr::codex_index_path()
                    .map(|index| Arc::new(Mutex::new(CodexNames::new(index)))),
                antigravity: Some(Arc::default()),
                antigravity_titles: Some(Arc::new(Mutex::new(
                    mluva_providers::antigravity::AntigravityTitles::new(
                        mluva_providers::antigravity::titles::antigravity_summaries_db_path(),
                    ),
                ))),
                opencode_titles: Some(Arc::new(Mutex::new(
                    mluva_providers::opencode::OpenCodeTitles::new(
                        mluva_providers::opencode::opencode_db_path(),
                    ),
                ))),
                apps: true,
            }),
            targets: Arc::default(),
            route: Arc::new(Mutex::new(VoiceRoute::default())),
            closed: CancellationToken::new(),
        }
    }

    /// Dispatch one method. Unknown methods and missing stores are errors, never panics.
    pub fn call(&self, method: &str, params: &Value) -> Outcome {
        match method {
            "open" => self.open(params),
            "key.validate" => validate_speech_key(&param::<String>(params, "key")?)
                .map(|()| Value::Null)
                .map_err(text),
            "dictation.process" => {
                let snapshot: TranscriptPreparationSnapshot = param(params, "snapshot")?;
                Ok(json!(snapshot.process(&param::<String>(params, "raw")?)))
            }
            "dictation.append" => Ok(json!(with_appendix(
                &param::<String>(params, "text")?,
                optional::<String>(params, "appendix")?.as_deref(),
            ))),
            // Whether a periodic sample shows a new view, by the planner's own measure.
            "annotate.diff" => Ok(json!(mluva_annotate::keyframes::screen_diff(
                &param::<Vec<f32>>(params, "a")?,
                &param::<Vec<f32>>(params, "b")?,
            ))),
            "scribe.transcribe" => self.transcribe(params),
            "realtime.start" => self.realtime_start(params),
            "realtime.finish" => self.realtime_finish(params),
            "realtime.cancel" => {
                let session = lock(&self.realtime).remove(&param::<String>(params, "session")?);
                if let Some(session) = session {
                    session.cancel();
                }
                Ok(Value::Null)
            }
            "agents.discover" => self.discover(params),
            "route.begin" => self.route_begin(params),
            "route.release" => Ok(json!(lock(&self.route).release(param(params, "now_ms")?))),
            "route.recognized" => {
                let recognized = lock(&self.route).recognized(
                    &param::<String>(params, "id")?,
                    &param::<String>(params, "text")?,
                    optional(params, "history_id")?,
                    param(params, "now_ms")?,
                );
                self.settle();
                Ok(json!(recognized))
            }
            "route.disarm" => {
                let reason = agents::draft_reason(&param::<String>(params, "reason")?)?;
                let disarmed = lock(&self.route).disarm(reason);
                self.settle();
                Ok(json!(disarmed))
            }
            "route.abandon" => {
                let reason = agents::draft_reason(&param::<String>(params, "reason")?)?;
                lock(&self.route).abandon_capture(reason);
                self.settle();
                Ok(Value::Null)
            }
            "route.state" => Ok(self.route_state()),
            "route.tick" => self.route_tick(param(params, "now_ms")?),
            // A second press during the cancel window sends at once, to the same target.
            "route.send_now" => {
                let submission = lock(&self.route).send_now(param(params, "now_ms")?);
                if let Some(submission) = submission {
                    self.deliver(submission);
                }
                Ok(self.route_state())
            }
            _ => self.with_stores(method, params),
        }
    }

    fn open(&self, params: &Value) -> Outcome {
        let paths = Paths {
            config: param(params, "config_dir")?,
            data: param(params, "data_dir")?,
            herdr_config_home: param(params, "herdr_config_home")?,
        };
        private_directory(&paths.config)?;
        private_directory(&paths.data)?;
        // Tests and diagnostics may name Claude Code's directory, otherwise its own default,
        // and may leave out Codex threads or app surfaces.
        {
            let mut reach = lock(&self.reach);
            if let Some(claude) = optional::<PathBuf>(params, "claude_config_dir")? {
                reach.claude = Some(Arc::new(Mutex::new(ClaudeRegistry::new(
                    claude,
                    std::process::id(),
                ))));
            }
            if optional::<bool>(params, "codex")? == Some(false) {
                reach.codex = None;
                reach.codex_names = None;
            }
            if let Some(apps) = optional::<bool>(params, "apps")? {
                reach.apps = apps;
            }
        }
        let history = HistoryStore::new(paths.data.join("history.sqlite3"));
        history.initialize().map_err(text)?;
        let conversations = ConversationStore::new(history.clone());
        conversations.initialize().map_err(text)?;
        let personalization = PersonalizationStore::new(paths.config.join("personalization.json"));
        let config = AppConfig::load(&paths.config.join("config.json")).unwrap_or_default();
        annotate::recover(&history, &config)?;
        let annotations = AnnotationStore::new(&history.database.path);
        *lock(&self.stores) = Some(Stores {
            paths,
            history,
            conversations,
            personalization,
            annotations,
            pending: HashMap::new(),
        });
        Ok(Value::Null)
    }

    fn with_stores(&self, method: &str, params: &Value) -> Outcome {
        let mut guard = lock(&self.stores);
        let stores = guard
            .as_mut()
            .ok_or_else(|| format!("Mluva's local stores are not open for `{method}`."))?;
        let config_path = stores.paths.config.join("config.json");
        match method {
            "config.load" => Ok(match AppConfig::load(&config_path) {
                Ok(config) => json!({"config": config, "error": null}),
                Err(_) => json!({"config": AppConfig::default(),
                    "error": "config.json needs repair; using defaults. The file is preserved."}),
            }),
            "config.save" => {
                let config: AppConfig = param(params, "config")?;
                config
                    .save(&config_path)
                    .map(|()| Value::Null)
                    .map_err(text)
            }
            "voice.load" => VoiceSettings::load(&stores.paths.config.join("agent-voice.json"))
                .map(|settings| json!(settings))
                .map_err(text),
            "voice.save" => param::<VoiceSettings>(params, "settings")?
                .save(
                    &stores.paths.config.join("agent-voice.json"),
                    &param::<String>(params, "recording_key")?,
                )
                .map(|()| Value::Null)
                .map_err(text),
            "history.add" => {
                let input: HistoryInput = param(params, "input")?;
                stores.history.add(input).map(|e| json!(e)).map_err(text)
            }
            "history.recent" => stores
                .history
                .recent(param(params, "limit")?)
                .map(|e| json!(e))
                .map_err(text),
            "history.search" => stores
                .conversations
                .search(
                    &param::<String>(params, "query")?,
                    param(params, "limit")?,
                    None,
                )
                .map(|e| json!(e))
                .map_err(text),
            "history.find" => stores
                .history
                .find(&param::<String>(params, "id")?)
                .map(|e| json!(e))
                .map_err(text),
            "history.title" => stores
                .history
                .update_title(
                    &param::<String>(params, "id")?,
                    optional::<String>(params, "title")?
                        .as_deref()
                        .filter(|t| !t.trim().is_empty()),
                )
                .map(|e| json!(e))
                .map_err(text),
            "history.delete" => stores
                .history
                .delete(&param::<String>(params, "id")?)
                .map(|()| Value::Null)
                .map_err(text),
            "history.source" => {
                let entry = stores
                    .history
                    .find(&param::<String>(params, "id")?)
                    .map_err(text)?;
                stores
                    .conversations
                    .source_text(&entry, true)
                    .map(Value::from)
                    .map_err(text)
            }
            "history.save_text" => stores
                .conversations
                .save_text(
                    &param::<String>(params, "id")?,
                    &param::<String>(params, "text")?,
                    None,
                )
                .map(|()| Value::Null)
                .map_err(text),
            "history.export" => {
                let entry = stores
                    .history
                    .find(&param::<String>(params, "id")?)
                    .map_err(text)?;
                let source = stores
                    .conversations
                    .source_text(&entry, true)
                    .map_err(text)?;
                let replies = stores
                    .conversations
                    .replies(&entry.identifier)
                    .map_err(text)?;
                let directory: PathBuf = param(params, "directory")?;
                stores
                    .history
                    .export(
                        &entry,
                        &directory,
                        &param::<String>(params, "format")?,
                        &replies,
                        Some(&source),
                    )
                    .map(|path| json!(path))
                    .map_err(text)
            }
            "personalization.state" => Ok(stores.personalization.state().document()),
            "personalization.save_replacement" => stores
                .personalization
                .save_dictionary_replacement(
                    &param::<String>(params, "spoken")?,
                    &param::<String>(params, "written")?,
                    optional::<String>(params, "application")?.as_deref(),
                    optional::<DictionaryCaseBehavior>(params, "case_behavior")?
                        .unwrap_or_default(),
                )
                .map(|rule| json!(rule))
                .map_err(text),
            "personalization.delete_replacement" => stores
                .personalization
                .delete_dictionary_replacement(&param::<String>(params, "id")?)
                .map(|()| Value::Null)
                .map_err(text),
            "personalization.save_snippet" => stores
                .personalization
                .save_snippet(
                    &param::<String>(params, "trigger")?,
                    &param::<String>(params, "expansion")?,
                    optional::<String>(params, "typed")?.as_deref(),
                    optional::<String>(params, "application")?.as_deref(),
                )
                .map(|snippet| json!(snippet))
                .map_err(text),
            "personalization.delete_snippet" => stores
                .personalization
                .delete_snippet(&param::<String>(params, "id")?)
                .map(|()| Value::Null)
                .map_err(text),
            "vocabulary.suggestions" => {
                let entries = stores
                    .history
                    .recent(param(params, "limit")?)
                    .map_err(text)?;
                let state = stores.personalization.state();
                Ok(json!(vocabulary::suggestions(
                    &entries,
                    &state.dictionary,
                    &state.dismissed_vocabulary_suggestion_identifiers,
                )))
            }
            "vocabulary.dismiss" => stores
                .personalization
                .dismiss_vocabulary_suggestion(&param::<String>(params, "id")?)
                .map(|()| Value::Null)
                .map_err(text),
            "annotate.settings" => {
                AnnotationSettings::load(&annotate::settings_path(&stores.paths.config))
                    .map(|settings| json!(settings))
                    .map_err(text)
            }
            "annotate.save_settings" => {
                let path = annotate::settings_path(&stores.paths.config);
                let mut settings = AnnotationSettings::load(&path).map_err(text)?;
                if params.get("enabled").is_some() {
                    settings.enabled = param(params, "enabled")?;
                }
                if params.get("modifier").is_some() {
                    settings.modifier = param(params, "modifier")?;
                }
                settings.save(&path).map(|()| Value::Null).map_err(text)
            }
            "annotate.plan" => annotate::plan(
                &stores.annotations,
                &mut stores.pending,
                serde_json::from_value(params.clone())
                    .map_err(|_| "The request needs a valid screen annotation.".to_owned())?,
            ),
            "annotate.finish" => annotate::finish(
                &stores.annotations,
                &mut stores.pending,
                &param::<String>(params, "identifier")?,
            ),
            "annotate.discard" => annotate::discard(
                &stores.annotations,
                &mut stores.pending,
                &param::<String>(params, "identifier")?,
            ),
            "annotate.bind" => stores
                .annotations
                .bind_capture(
                    &param::<String>(params, "owner")?,
                    &param::<String>(params, "conversation")?,
                )
                .map(|()| Value::Null)
                .map_err(text),
            "annotate.discard_owner" => stores
                .annotations
                .delete_owner(&param::<String>(params, "owner")?, true)
                .map(|()| Value::Null)
                .map_err(text),
            "annotate.recent" => {
                annotate::recent(&stores.annotations, &param::<String>(params, "id")?)
            }
            "dictation.freeze" => {
                let config = AppConfig::load(&config_path).unwrap_or_default();
                freeze_transcript_preparation(
                    &config,
                    Some(&stores.personalization),
                    "dictation",
                    optional::<String>(params, "application")?.as_deref(),
                )
                .map(|snapshot| json!(snapshot))
                .map_err(text)
            }
            _ => Err(format!("Unknown method `{method}`.")),
        }
    }

    /// Start background work; its result arrives as a `task` event with the same identifier.
    fn task<F>(&self, work: F) -> Outcome
    where
        F: Future<Output = Outcome> + Send + 'static,
    {
        let id = uuid::Uuid::new_v4().to_string();
        let emit = self.emit.clone();
        let closed = self.closed.clone();
        let task = id.clone();
        self.runtime.spawn(async move {
            let outcome = tokio::select! {
                _ = closed.cancelled() => Err("Mluva is closing.".to_owned()),
                outcome = work => outcome,
            };
            emit(match outcome {
                Ok(result) => json!({"type": "task", "task": task, "ok": result}),
                Err(error) => json!({"type": "task", "task": task, "error": error}),
            });
        });
        Ok(json!({"task": id}))
    }

    fn transcribe(&self, params: &Value) -> Outcome {
        let key = Secret::new(param::<String>(params, "key")?);
        let path: PathBuf = param(params, "path")?;
        let language: String = param(params, "language")?;
        let model: String = optional(params, "model")?.unwrap_or_else(|| "scribe_v2".into());
        self.task(async move {
            let client = ElevenLabsClient::new(key, SCRIBE_ENDPOINT, Duration::from_secs(120))
                .map_err(text)?;
            let result = client
                .transcribe(&path, &language, &model)
                .await
                .map_err(text)?;
            Ok(json!(result))
        })
    }

    fn realtime_start(&self, params: &Value) -> Outcome {
        let key = Secret::new(param::<String>(params, "key")?);
        let language: String = param(params, "language")?;
        let session_id = uuid::Uuid::new_v4().to_string();
        let client = ElevenLabsRealtimeClient::new(
            key,
            SCRIBE_REALTIME_ENDPOINT,
            RealtimeOptions::default(),
        )
        .map_err(text)?;
        let emit = self.emit.clone();
        let preview_session = session_id.clone();
        let preview = Box::new(move |preview: mluva_providers::realtime::RealtimePreview| {
            emit(
                json!({"type": "realtime.preview", "session": preview_session,
                "committed": preview.committed_text, "volatile": preview.volatile_text}),
            );
            Ok(())
        });
        let sessions = self.realtime.clone();
        let started = session_id.clone();
        self.task(async move {
            let session = client
                .start(&language, Some(preview), None)
                .await
                .map_err(text)?;
            lock(&sessions).insert(started.clone(), Arc::new(session));
            Ok(json!({"session": started}))
        })
    }

    fn realtime_finish(&self, params: &Value) -> Outcome {
        let id: String = param(params, "session")?;
        let session = lock(&self.realtime)
            .remove(&id)
            .ok_or("The realtime session is no longer running.")?;
        self.task(async move {
            let result = session.finish().await.map_err(text)?;
            Ok(json!({"transcription": result.transcription,
                "finalization_seconds": result.finalization_seconds}))
        })
    }

    pub fn realtime_audio(&self, session: &str, frames: &[u8]) -> bool {
        let session = lock(&self.realtime).get(session).cloned();
        session.is_some_and(|session| session.submit_audio(frames).unwrap_or(false))
    }

    /// Every session Mluva can see, in the shared order: Herdr's panes, then Claude Code
    /// sessions and Codex threads Herdr does not show, then app surfaces. The pending attempt
    /// follows its session through the pass.
    fn discover(&self, params: &Value) -> Outcome {
        let explicit: Option<String> = optional(params, "socket")?;
        let home = lock(&self.stores)
            .as_ref()
            .map(|stores| stores.paths.herdr_config_home.clone())
            .ok_or("Mluva's local stores are not open.")?;
        let sources = {
            let reach = lock(&self.reach);
            Sources {
                herdr: herdr::socket_candidates(&home, explicit.as_deref()),
                claude: reach.claude.clone(),
                codex: reach.codex.clone(),
                codex_names: reach.codex_names.clone(),
                antigravity: reach.antigravity.clone(),
                antigravity_titles: reach.antigravity_titles.clone(),
                opencode_titles: reach.opencode_titles.clone(),
                apps: reach.apps,
            }
        };
        let (targets, route, history) = (self.targets.clone(), self.route.clone(), self.history());
        let cancel = self.closed.child_token();
        self.task(async move {
            let found = agent_targets::discover(sources, &cancel)
                .await
                .ok_or("Mluva is closing.")?;
            {
                let mut route = lock(&route);
                route.observe(&found.targets);
                if let (Some(history), Some(attempt)) = (&history, &route.attempt) {
                    agents::record_outcome(history, attempt);
                }
            }
            let described: Vec<Value> = found.targets.iter().map(agents::describe).collect();
            let summary = found.summary();
            *lock(&targets) = found.targets;
            Ok(
                json!({"sessions": described, "herdr": found.herdr, "guarded": found.guarded,
                "summary": summary, "errors": found.errors}),
            )
        })
    }

    /// Begin recording for the target the shell was shown, as the latest pass found it now.
    fn route_begin(&self, params: &Value) -> Outcome {
        let chosen: Target = param(params, "target")?;
        let current = lock(&self.targets)
            .iter()
            .find(|target| target.same_instance(&chosen))
            .cloned()
            .ok_or("That session is no longer running. Choose it again on the wheel.")?;
        lock(&self.route)
            .begin(current, param(params, "now_ms")?)
            .map(Value::from)
            .map_err(text)
    }

    fn route_state(&self) -> Value {
        lock(&self.route)
            .attempt
            .as_ref()
            .map_or(Value::Null, agents::attempt)
    }

    /// History, when the stores are open.
    fn history(&self) -> Option<HistoryStore> {
        lock(&self.stores)
            .as_ref()
            .map(|stores| stores.history.clone())
    }

    /// Record what became of the current attempt's text in its History row.
    fn settle(&self) {
        if let (Some(history), Some(attempt)) = (self.history(), &lock(&self.route).attempt) {
            agents::record_outcome(&history, attempt);
        }
    }

    /// Deliver once the cancel window has passed; the outcome arrives as a `route` event.
    fn route_tick(&self, now_ms: u64) -> Outcome {
        let submission = lock(&self.route).take_submission(now_ms);
        match submission {
            Some(submission) => self.deliver(submission),
            // A window that ran out unseen keeps the draft.
            None => self.settle(),
        }
        Ok(self.route_state())
    }

    /// Deliver a message whose automatic-send permission was already consumed, through the
    /// target's own transport. A queued message is followed until the agent takes it.
    fn deliver(&self, submission: Submission) {
        let (route, emit, history) = (self.route.clone(), self.emit.clone(), self.history());
        let cancel = self.closed.child_token();
        // The row this message belongs to, so a later "taken" reaches it even after another
        // dictation has started.
        let row = lock(&self.route)
            .attempt
            .as_ref()
            .filter(|attempt| attempt.id == submission.id)
            .and_then(|attempt| attempt.history_id.clone());
        let settle = move |route: &Mutex<VoiceRoute>, taken_elsewhere: bool| {
            let Some(history) = &history else {
                return;
            };
            match (&lock(route).attempt, &row) {
                (Some(attempt), _) if !taken_elsewhere => agents::record_outcome(history, attempt),
                (_, Some(row)) if taken_elsewhere => agents::record_taken(history, row),
                _ => {}
            }
        };
        self.runtime.spawn(async move {
            let receipt = agent_targets::deliver(&submission, &cancel).await;
            lock(&route).delivered(&submission.id, receipt.outcome, now_ms());
            settle(&route, false);
            emit(json!({"type": "route", "id": submission.id,
                "outcome": agents::outcome_name(receipt.outcome)}));
            let Some(taken) = receipt.taken else {
                return;
            };
            let seen = tokio::select! {
                _ = cancel.cancelled() => false,
                seen = taken => seen,
            };
            if !seen {
                return;
            }
            let current = lock(&route).taken(&submission.id);
            settle(&route, !current);
            emit(json!({"type": "route", "id": submission.id, "outcome": "taken"}));
        });
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.closed.cancel();
        for session in lock(&self.realtime).drain().map(|(_, s)| s) {
            session.cancel();
        }
    }
}
