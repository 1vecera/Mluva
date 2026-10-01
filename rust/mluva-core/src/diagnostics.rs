//! Bounded, content-free local timings compatible with the released event ledger.

use crate::{
    config::{AppConfig, AudioRetentionPolicy},
    database::{StoreResult, invalid, timestamp},
    private_files::atomic_write_private,
};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticStage {
    CaptureReady,
    Capture,
    Recognition,
    Enhancement,
    Delivery,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticProvider {
    Pipewire,
    ElevenlabsScribeV2,
    CodexAppServer,
    Desktop,
    Local,
    Litellm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticOutcome {
    Completed,
    Failed,
    RawFallback,
    SafeFallback,
    Deferred,
    Cancelled,
}

impl DiagnosticStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CaptureReady => "capture-ready",
            Self::Capture => "capture",
            Self::Recognition => "recognition",
            Self::Enhancement => "enhancement",
            Self::Delivery => "delivery",
        }
    }
}
impl DiagnosticProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pipewire => "pipewire",
            Self::ElevenlabsScribeV2 => "elevenlabs-scribe-v2",
            Self::CodexAppServer => "codex-app-server",
            Self::Desktop => "desktop",
            Self::Local => "local",
            Self::Litellm => "litellm",
        }
    }
}
impl DiagnosticOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::RawFallback => "raw-fallback",
            Self::SafeFallback => "safe-fallback",
            Self::Deferred => "deferred",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub created_at: String,
    pub session_identifier: String,
    pub mode: String,
    pub stage: String,
    pub provider: String,
    pub outcome: String,
    pub duration_ms: i64,
}

#[derive(Clone, Debug)]
pub struct DiagnosticsStore {
    pub path: PathBuf,
    maximum_events: i64,
}
impl DiagnosticsStore {
    pub fn new(path: impl AsRef<Path>, maximum_events: i64) -> StoreResult<Self> {
        if maximum_events <= 0 {
            return Err(invalid("maximum_events must be positive"));
        }
        Ok(Self {
            path: path.as_ref().into(),
            maximum_events,
        })
    }
    pub fn initialize(&self) -> StoreResult<()> {
        if let Some(parent) = self.path.parent() {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)?;
        }
        Connection::open(&self.path)?.execute_batch(
            "CREATE TABLE IF NOT EXISTS diagnostic_events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL, session_identifier TEXT NOT NULL,
                mode TEXT NOT NULL, stage TEXT NOT NULL, provider TEXT NOT NULL,
                outcome TEXT NOT NULL, duration_ms INTEGER NOT NULL
            );",
        )?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    pub fn record(
        &self,
        session_identifier: &str,
        mode: &str,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        duration_seconds: f64,
    ) -> StoreResult<()> {
        Uuid::parse_str(session_identifier)
            .map_err(|_| invalid("Invalid diagnostic session identifier"))?;
        if !["dictation", "command", "scratchpad", "meeting"].contains(&mode) {
            return Err(invalid(format!("Unsupported diagnostic mode: {mode}")));
        }
        if !duration_seconds.is_finite() || !(0.0..=86_400.0).contains(&duration_seconds) {
            return Err(invalid(
                "Diagnostic duration must be between zero and 24 hours",
            ));
        }
        let mut connection = Connection::open(&self.path)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO diagnostic_events (created_at, session_identifier, mode, stage, provider, outcome, duration_ms) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![timestamp(), session_identifier, mode, stage.as_str(), provider.as_str(), outcome.as_str(), (duration_seconds * 1_000.0).round_ties_even() as i64],
        )?;
        transaction.execute(
            "DELETE FROM diagnostic_events WHERE sequence NOT IN (SELECT sequence FROM diagnostic_events ORDER BY sequence DESC LIMIT ?)",
            [self.maximum_events],
        )?;
        transaction.commit()?;
        Ok(())
    }
    pub fn recent(&self, limit: i64) -> StoreResult<Vec<DiagnosticEvent>> {
        let connection = Connection::open(&self.path)?;
        let mut statement = connection.prepare("SELECT created_at, session_identifier, mode, stage, provider, outcome, duration_ms FROM diagnostic_events ORDER BY sequence DESC LIMIT ?")?;
        let rows = statement.query_map([limit.clamp(0, self.maximum_events)], |row| {
            Ok(DiagnosticEvent {
                created_at: row.get(0)?,
                session_identifier: row.get(1)?,
                mode: row.get(2)?,
                stage: row.get(3)?,
                provider: row.get(4)?,
                outcome: row.get(5)?,
                duration_ms: row.get(6)?,
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }
    pub fn export(&self, directory: &Path, config: &AppConfig) -> StoreResult<PathBuf> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let now = Utc::now();
        let generated_at = now.to_rfc3339_opts(
            if now.timestamp_subsec_micros() == 0 {
                SecondsFormat::Secs
            } else {
                SecondsFormat::Micros
            },
            false,
        );
        let path = directory.join(format!(
            "mluva-diagnostics-{}.json",
            now.format("%Y%m%dT%H%M%S%6fZ")
        ));
        static LANGUAGE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\A[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})?\z").unwrap());
        let policy = match config.audio_retention_policy {
            AudioRetentionPolicy::Never => "never",
            AudioRetentionPolicy::Failures => "failures",
            AudioRetentionPolicy::Always => "always",
        };
        let mut events = self.recent(1_000)?;
        events.reverse();
        let payload = json!({
            "schema": "mluva-diagnostics-v1", "generated_at": generated_at,
            "configuration": {
                "language_code": if LANGUAGE.is_match(&config.language_code) { &config.language_code } else { "custom" },
                "transcription_model": if config.transcription_model == "scribe_v2" { "scribe_v2" } else { "custom" },
                "codex_model_configured": config.codex_model.is_some(), "microphone_target_configured": config.microphone_target.is_some(),
                "system_audio_target_configured": config.system_audio_target.is_some(), "default_mode": config.default_mode,
                "global_recording_key": config.global_recording_key, "auto_paste": config.auto_paste,
                "spoken_commands_enabled": config.spoken_commands_enabled, "remember_per_application": config.remember_per_application,
                "audio_retention_policy": policy, "incognito_mode": config.incognito_mode, "history_retention_days": config.history_retention_days,
            },
            "events": events,
            "excluded": ["audio", "audio paths", "transcript text", "selected text", "clipboard content", "application identity", "window titles", "credentials", "raw exception messages"],
        });
        let mut bytes = serde_json::to_vec_pretty(&payload)?;
        bytes.push(b'\n');
        atomic_write_private(&path, &bytes)?;
        Ok(path)
    }
}
