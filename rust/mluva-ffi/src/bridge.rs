//! One long-lived owner of the shared stores, provider sessions and agent route.
//!
//! The file layout under the shell's configuration and data directories matches the Linux
//! application's XDG layout, so settings, personalization and History use one format.
use mluva_core::{
    agent_voice::{DraftReason, Recipient, RoutePhase, SendOutcome, VoiceRoute, VoiceSettings},
    config::AppConfig,
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
    personalization::{DictionaryCaseBehavior, PersonalizationStore},
    vocabulary,
};
use mluva_providers::{
    Secret,
    claude_sessions::{self, ClaudeRegistry},
    credentials::validate_speech_key,
    elevenlabs::{ElevenLabsClient, SCRIBE_ENDPOINT},
    herdr,
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
}

pub struct Bridge {
    runtime: tokio::runtime::Runtime,
    emit: Emitter,
    stores: Mutex<Option<Stores>>,
    registry: Arc<Mutex<Option<ClaudeRegistry>>>,
    realtime: Arc<Mutex<HashMap<String, Arc<RealtimeSession>>>>,
    route: Arc<Mutex<VoiceRoute>>,
    closed: CancellationToken,
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

fn phase(phase: &RoutePhase) -> Value {
    let reason = |reason: &DraftReason| format!("{reason:?}").to_lowercase();
    match phase {
        RoutePhase::Recording => json!({"phase": "recording"}),
        RoutePhase::Finalizing => json!({"phase": "finalizing"}),
        RoutePhase::Grace { deadline_ms } => json!({"phase": "grace", "deadline_ms": deadline_ms}),
        RoutePhase::Draft(why) => json!({"phase": "draft", "reason": reason(why)}),
        RoutePhase::Submitting => json!({"phase": "submitting"}),
        RoutePhase::Sent => json!({"phase": "sent"}),
        RoutePhase::Unconfirmed => json!({"phase": "unconfirmed"}),
    }
}

fn draft_reason(name: &str) -> Result<DraftReason, String> {
    Ok(match name {
        "kept" => DraftReason::Kept,
        "edited" => DraftReason::Edited,
        "recipient_changed" => DraftReason::RecipientChanged,
        "unavailable" => DraftReason::Unavailable,
        "cancelled" => DraftReason::Cancelled,
        "interrupted" => DraftReason::Interrupted,
        _ => return Err("Unknown reason for keeping a draft.".into()),
    })
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
            registry: Arc::new(Mutex::new(ClaudeRegistry::from_environment())),
            realtime: Arc::default(),
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
            "route.begin" => {
                let recipient: Recipient = param(params, "recipient")?;
                lock(&self.route)
                    .begin(recipient, param(params, "now_ms")?)
                    .map(Value::from)
                    .map_err(text)
            }
            "route.release" => Ok(json!(lock(&self.route).release(param(params, "now_ms")?))),
            "route.recognized" => Ok(json!(lock(&self.route).recognized(
                &param::<String>(params, "id")?,
                &param::<String>(params, "text")?,
                optional(params, "history_id")?,
                param(params, "now_ms")?,
            ))),
            "route.disarm" => Ok(json!(lock(&self.route).disarm(draft_reason(&param::<
                String,
            >(
                params, "reason"
            )?)?))),
            "route.abandon" => {
                lock(&self.route)
                    .abandon_capture(draft_reason(&param::<String>(params, "reason")?)?);
                Ok(Value::Null)
            }
            "route.observe" => {
                let recipients: Vec<Recipient> = param(params, "recipients")?;
                lock(&self.route).observe(&recipients);
                Ok(self.route_state())
            }
            "route.state" => Ok(self.route_state()),
            "route.tick" => self.route_tick(param(params, "now_ms")?),
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
        // Tests and diagnostics may name Claude Code's directory; otherwise its own default.
        if let Some(claude) = optional::<PathBuf>(params, "claude_config_dir")? {
            *lock(&self.registry) = Some(ClaudeRegistry::new(claude, std::process::id()));
        }
        let history = HistoryStore::new(paths.data.join("history.sqlite3"));
        history.initialize().map_err(text)?;
        let conversations = ConversationStore::new(history.clone());
        conversations.initialize().map_err(text)?;
        let personalization = PersonalizationStore::new(paths.config.join("personalization.json"));
        *lock(&self.stores) = Some(Stores {
            paths,
            history,
            conversations,
            personalization,
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

    /// Herdr's panes in order, then Claude Code sessions Herdr does not show.
    fn discover(&self, params: &Value) -> Outcome {
        let explicit: Option<String> = optional(params, "socket")?;
        let home = lock(&self.stores)
            .as_ref()
            .map(|stores| stores.paths.herdr_config_home.clone())
            .ok_or("Mluva's local stores are not open.")?;
        let registry = self.registry.clone();
        self.task(async move {
            let paths = herdr::socket_candidates(&home, explicit.as_deref());
            let mut recipients = vec![];
            let (mut reached, mut guarded, mut errors) = (false, false, vec![]);
            for path in paths.into_iter().take(16) {
                match herdr::discover(&path).await {
                    Ok(found) => {
                        reached = true;
                        guarded |= found.guarded;
                        recipients.extend(found.recipients);
                    }
                    Err(error) => errors.push(error.to_string()),
                }
            }
            let sessions = tokio::task::spawn_blocking(move || {
                lock(&registry)
                    .as_mut()
                    .map(ClaudeRegistry::sessions)
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            let recipients = claude_sessions::merge(recipients, &sessions);
            let described: Vec<Value> = recipients
                .iter()
                .map(|r| {
                    json!({"recipient": r, "display_name": r.display_name(),
                        "draft_only": r.draft_only(), "draft_hint": r.draft_hint(),
                        "can_receive": r.can_receive(), "spoken_status": r.spoken_status()})
                })
                .collect();
            Ok(
                json!({"sessions": described, "herdr": reached, "guarded": guarded,
                "errors": errors}),
            )
        })
    }

    fn route_state(&self) -> Value {
        let route = lock(&self.route);
        match &route.attempt {
            None => Value::Null,
            Some(attempt) => {
                let mut state = phase(&attempt.phase);
                state["id"] = json!(attempt.id);
                state["recipient"] = json!(attempt.recipient);
                state["text"] = json!(attempt.text);
                state["history_id"] = json!(attempt.history_id);
                state
            }
        }
    }

    /// Submit once the cancel window has passed; the outcome arrives as a `route` event.
    fn route_tick(&self, now_ms: u64) -> Outcome {
        let submission = lock(&self.route).take_submission(now_ms);
        if let Some(submission) = submission {
            let route = self.route.clone();
            let emit = self.emit.clone();
            let cancel = self.closed.child_token();
            self.runtime.spawn(async move {
                let outcome = herdr::submit(&submission, &cancel).await;
                let received = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(now_ms, |d| d.as_millis() as u64);
                lock(&route).delivered(&submission.id, outcome, received);
                let outcome = match outcome {
                    SendOutcome::Accepted => "sent",
                    SendOutcome::Rejected => "rejected",
                    SendOutcome::Unconfirmed => "unconfirmed",
                };
                emit(json!({"type": "route", "id": submission.id, "outcome": outcome}));
            });
        }
        Ok(self.route_state())
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.closed.cancel();
        for session in lock(&self.realtime).drain().map(|(_, s)| s) {
            session.cancel();
        }
    }
}
