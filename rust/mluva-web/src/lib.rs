//! A bounded, authenticated audio inbox with desktop history and clipboard delivery.

pub mod auth;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::{HistoryEntry, HistoryInput, HistoryStore},
};
use mluva_providers::elevenlabs::ElevenLabsClient;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::{
    collections::BTreeMap, io::Write, path::PathBuf, process::Stdio, sync::Arc, time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::{Mutex, Semaphore},
};

pub const MAX_UPLOAD: usize = 20 * 1024 * 1024;
const PREFIX: &str = "mluva-web:";

pub struct AppState {
    pub access: Arc<auth::Access>,
    pub origin: String,
    pub config: AppConfig,
    pub paths: AppPaths,
    pub speech: Arc<ElevenLabsClient>,
    pub clipboard: PathBuf,
    pub ffmpeg: PathBuf,
    jobs: Mutex<BTreeMap<String, Job>>,
    busy: Arc<Semaphore>,
}

#[derive(Clone, Serialize)]
pub struct Job {
    pub identifier: String,
    pub phase: String,
    pub text: String,
    pub copied: bool,
    pub message: String,
}

impl AppState {
    pub fn new(
        access: Arc<auth::Access>,
        origin: String,
        config: AppConfig,
        paths: AppPaths,
        speech: Arc<ElevenLabsClient>,
        clipboard: PathBuf,
        ffmpeg: PathBuf,
    ) -> Self {
        Self {
            access,
            origin,
            config,
            paths,
            speech,
            clipboard,
            ffmpeg,
            jobs: Mutex::new(BTreeMap::new()),
            busy: Arc::new(Semaphore::new(1)),
        }
    }

    fn history(&self) -> HistoryStore {
        HistoryStore::new(self.paths.data.join("history.sqlite3"))
    }

    fn completed(&self, identifier: &str) -> Result<Option<HistoryEntry>, String> {
        if self.config.incognito_mode {
            return Ok(None);
        }
        let connection = Connection::open(self.paths.data.join("history.sqlite3"))
            .map_err(|_| "Mluva history is unavailable.")?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|_| "Mluva history is unavailable.")?;
        let found: Option<String> = connection.query_row(
            "SELECT identifier FROM transcription_history WHERE application_identifier = ? AND raw_text != '' ORDER BY created_at DESC LIMIT 1",
            [format!("{PREFIX}{identifier}")], |row| row.get(0),
        ).optional().map_err(|_| "Mluva history is unavailable.")?;
        found
            .map(|id| {
                self.history()
                    .find(&id)
                    .map_err(|_| "Mluva history is unavailable.".into())
            })
            .transpose()
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [("Content-Type", "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/manifest.webmanifest",
            get(|| async {
                (
                    [("Content-Type", "application/manifest+json")],
                    include_str!("../web/manifest.webmanifest"),
                )
            }),
        )
        .route(
            "/sw.js",
            get(|| async {
                (
                    [("Content-Type", "text/javascript; charset=utf-8")],
                    include_str!("../web/sw.js"),
                )
            }),
        )
        .route(
            "/icons/mluva-192.png",
            get(|| async {
                (
                    [("Content-Type", "image/png")],
                    include_bytes!("../../../docs/brand/png/mluva-mark-192.png").as_slice(),
                )
            }),
        )
        .route(
            "/icons/mluva-512.png",
            get(|| async {
                (
                    [("Content-Type", "image/png")],
                    include_bytes!("../../../docs/brand/png/mluva-mark-512.png").as_slice(),
                )
            }),
        )
        .route("/api/recordings", get(recordings))
        .route("/api/recordings/{identifier}", post(upload).get(job))
        .route("/api/recordings/{identifier}/copy", post(copy))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .layer(DefaultBodyLimit::max(MAX_UPLOAD))
        .layer(middleware::from_fn_with_state(state.clone(), perimeter))
        .with_state(state)
}

async fn perimeter(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let token = request
        .headers()
        .get("cf-access-jwt-assertion")
        .and_then(|value| value.to_str().ok());
    let mut response = if !match token {
        Some(token) => state.access.accepts(token).await,
        None => false,
    } {
        error(
            StatusCode::UNAUTHORIZED,
            "Open the Cloudflare-protected Mluva URL and sign in.",
        )
    } else if request.method() != Method::GET
        && request
            .headers()
            .get("origin")
            .and_then(|value| value.to_str().ok())
            != Some(state.origin.as_str())
    {
        error(
            StatusCode::FORBIDDEN,
            "This request did not come from the Mluva page.",
        )
    } else {
        next.run(request).await
    };
    for (key, value) in [
        ("cache-control", "private, no-store"),
        ("x-robots-tag", "noindex, nofollow"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; worker-src 'self'; style-src 'unsafe-inline'; media-src 'self' blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        ),
        ("permissions-policy", "microphone=(self), camera=()"),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    response
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"message": message}))).into_response()
}

fn recording_id(value: &str) -> Result<String, &'static str> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| "Invalid recording ID.")
}

fn receipt(identifier: String, entry: HistoryEntry) -> Job {
    Job {
        identifier,
        phase: "completed".into(),
        text: entry.delivered_text,
        copied: entry.delivery_outcome == "copied",
        message: String::new(),
    }
}

async fn upload(
    State(state): State<Arc<AppState>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
    audio: Bytes,
) -> Response {
    let identifier = match recording_id(&identifier) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    // Serialize admission, including checking the durable receipt. Retries cannot pay twice.
    let mut jobs = state.jobs.lock().await;
    if let Some(job) = jobs.get(&identifier).filter(|job| job.phase != "failed") {
        return Json(job).into_response();
    }
    match state.completed(&identifier) {
        Ok(Some(entry)) => return Json(receipt(identifier, entry)).into_response(),
        Err(message) => return error(StatusCode::INTERNAL_SERVER_ERROR, &message),
        Ok(None) => {}
    }
    if audio.is_empty() {
        return error(StatusCode::BAD_REQUEST, "The recording is empty.");
    }
    let mime = headers
        .get("content-type")
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    let format = match mime.split(';').next().unwrap_or("").trim() {
        "audio/webm" => "matroska",
        "audio/mp4" | "video/mp4" => "mov",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        _ => {
            return error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Unsupported recording format.",
            );
        }
    };
    let Ok(permit) = state.busy.clone().try_acquire_owned() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Another recording is processing. Keep this recording and retry shortly.",
        );
    };
    let job = Job {
        identifier: identifier.clone(),
        phase: "processing".into(),
        text: String::new(),
        copied: false,
        message: String::new(),
    };
    // Bounded volatile receipts; durable completed recordings remain in desktop history.
    if jobs.len() >= 50 {
        jobs.retain(|_, job| job.phase == "processing");
    }
    jobs.insert(identifier.clone(), job.clone());
    drop(jobs);
    tokio::spawn(async move {
        let result = process(&state, &identifier, audio, format).await;
        let finished = match result {
            Ok(job) => job,
            Err(message) => Job {
                identifier: identifier.clone(),
                phase: "failed".into(),
                text: String::new(),
                copied: false,
                message,
            },
        };
        state.jobs.lock().await.insert(identifier, finished);
        drop(permit);
    });
    (StatusCode::ACCEPTED, Json(job)).into_response()
}

async fn process(
    state: &AppState,
    identifier: &str,
    audio: Bytes,
    format: &str,
) -> Result<Job, String> {
    // Share native Incognito's memory-only staging and independent crash cleanup.
    let mut volatile = if state.config.incognito_mode {
        let cleanup = state.paths.data.join("app/bin/mluva-audio-cleanup");
        Some(
            mluva_audio::volatile::VolatileAudioStore::open(&cleanup).map_err(
                |_| "Incognito needs /dev/shm and the installed Mluva audio cleanup helper.",
            )?,
        )
    } else {
        None
    };
    let temporary_root = match volatile.as_mut() {
        Some(store) => store
            .directory()
            .map_err(|_| "Incognito audio cleanup is unavailable.")?
            .to_owned(),
        None => std::env::temp_dir(),
    };
    let mut upload =
        tempfile::NamedTempFile::new_in(&temporary_root).map_err(|_| "Could not receive audio.")?;
    upload
        .write_all(&audio)
        .map_err(|_| "Could not receive audio.")?;
    let wav = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(&temporary_root)
        .map_err(|_| "Could not prepare audio.")?;
    let converted = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(&state.ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-y",
                "-protocol_whitelist",
                "file,pipe",
                "-f",
            ])
            .arg(format)
            .args(["-i"])
            .arg(upload.path())
            .args([
                "-vn",
                "-t",
                "601",
                "-ar",
                "16000",
                "-ac",
                "1",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(wav.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| "Audio conversion timed out. Download the recording and retry.")?
    .map_err(|_| "Could not convert the recording.")?;
    if !converted.success() {
        return Err("The audio could not be read. Download the recording before retrying.".into());
    }
    if wav
        .as_file()
        .metadata()
        .map_err(|_| "Could not read the converted audio.")?
        .len()
        > 19_200_128
    {
        return Err("Recordings are limited to 10 minutes. Download this audio and split it before retrying.".into());
    }
    let recognition = state
        .speech
        .transcribe(wav.path(), &state.config.language_code, "scribe_v2")
        .await;
    let (text, language, transcription_id, failed) = match recognition {
        Ok(result) if !result.text.is_empty() => (
            result.text,
            result.language_code,
            result.transcription_id,
            false,
        ),
        _ => (
            String::new(),
            state.config.language_code.clone(),
            None,
            true,
        ),
    };
    let copied = !failed && state.config.auto_copy_dictation && clipboard(state, &text).await;
    if !state.config.incognito_mode {
        let retained = if state.config.audio_retention_policy.should_retain(copied) {
            let destination = state
                .paths
                .data
                .join("recordings")
                .join(format!("web-{}.wav", uuid::Uuid::new_v4()));
            mluva_core::private_files::atomic_write_private(
                &destination,
                &std::fs::read(wav.path()).map_err(|_| "Could not retain recovery audio.")?,
            )
            .map_err(|_| "Could not retain recovery audio.")?;
            Some(destination.to_string_lossy().into_owned())
        } else {
            None
        };
        let saved = state.history().add(HistoryInput {
            raw_text: text.clone(),
            delivered_text: text.clone(),
            language_code: language,
            transcription_id,
            application_identifier: Some(format!("{PREFIX}{identifier}")),
            recognition_route: Some("scribe-v2-batch".into()),
            retained_audio_path: retained.clone(),
            audio_retention_policy: Some(
                match state.config.audio_retention_policy {
                    mluva_core::config::AudioRetentionPolicy::Never => "never",
                    mluva_core::config::AudioRetentionPolicy::Failures => "failures",
                    mluva_core::config::AudioRetentionPolicy::Always => "always",
                }
                .into(),
            ),
            mode: "dictation".into(),
            delivery_outcome: if copied {
                "copied"
            } else if failed {
                "failed"
            } else {
                "ready"
            }
            .into(),
            ..HistoryInput::default()
        });
        if saved.is_err() {
            if let Some(path) = retained {
                let _ = std::fs::remove_file(path);
            }
            // Recognition remains available on the device even if local persistence fails.
            return Ok(Job { identifier: identifier.into(), phase: if failed { "failed" } else { "completed" }.into(), text, copied,
                message: "Mluva history could not be saved. Keep the text or download the recording on this device.".into() });
        }
    }
    Ok(Job {
        identifier: identifier.into(),
        phase: if failed { "failed" } else { "completed" }.into(),
        text,
        copied,
        message: if failed {
            "Transcription failed. Your recording is still on this device; retry or download it."
        } else if !copied {
            "Text is ready. PC clipboard copying is disabled or unavailable."
        } else {
            "Copied to the PC clipboard."
        }
        .into(),
    })
}

async fn clipboard(state: &AppState, text: &str) -> bool {
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let mut child = Command::new(&state.clipboard)
            .args(["--type", "text/plain"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("Clipboard input unavailable"))?;
        stdin.write_all(text.as_bytes()).await?;
        drop(stdin);
        child.wait().await
    })
    .await;
    matches!(result, Ok(Ok(status)) if status.success())
}

async fn job(State(state): State<Arc<AppState>>, Path(identifier): Path<String>) -> Response {
    let identifier = match recording_id(&identifier) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    if let Some(job) = state.jobs.lock().await.get(&identifier) {
        return Json(job).into_response();
    }
    match state.completed(&identifier) {
        Ok(Some(entry)) => Json(receipt(identifier, entry)).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "Recording not found."),
        Err(message) => error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

async fn recordings(State(state): State<Arc<AppState>>) -> Response {
    if state.config.incognito_mode {
        return Json(Vec::<Job>::new()).into_response();
    }
    match state.history().recent(200) {
        Ok(entries) => Json(
            entries
                .into_iter()
                .filter_map(|entry| {
                    let identifier = entry
                        .application_identifier
                        .as_deref()?
                        .strip_prefix(PREFIX)?
                        .to_owned();
                    if entry.raw_text.is_empty() {
                        return None;
                    }
                    Some(receipt(identifier, entry))
                })
                .take(30)
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(_) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Mluva history is unavailable.",
        ),
    }
}

async fn copy(State(state): State<Arc<AppState>>, Path(identifier): Path<String>) -> Response {
    let identifier = match recording_id(&identifier) {
        Ok(id) => id,
        Err(message) => return error(StatusCode::BAD_REQUEST, message),
    };
    let entry = match state.completed(&identifier) {
        Ok(entry) => entry,
        Err(message) => return error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    };
    let text = match entry.as_ref() {
        Some(entry) => entry.delivered_text.clone(),
        None => state
            .jobs
            .lock()
            .await
            .get(&identifier)
            .map(|job| job.text.clone())
            .unwrap_or_default(),
    };
    if text.is_empty() {
        return error(StatusCode::NOT_FOUND, "Completed recording not found.");
    }
    let copied = clipboard(&state, &text).await;
    if copied {
        // Copy is read-only with respect to History; native edits may occur while
        // the desktop command runs, and must never be overwritten by a receipt.
        if let Some(job) = state.jobs.lock().await.get_mut(&identifier) {
            job.copied = true;
        }
    }
    Json(serde_json::json!({"copied": copied})).into_response()
}
