//! Backward-compatible history, recovery provenance and privacy-aware cleanup.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::conversation::Rewrite;
use crate::database::{Database, StoreError, StoreResult, invalid, timestamp};
use crate::private_files::{atomic_write_private, resolve_path};
use crate::screenshots::ScreenshotStore;
use crate::text::trim;

pub const RECOGNITION_ROUTES: [&str; 9] = [
    "scribe-v2-realtime",
    "scribe-v2-batch",
    "scribe-v2-batch-retry",
    "litellm-batch",
    "managed-local",
    "managed-local-retry",
    "voxtype-local",
    "litellm-batch-retry",
    "voxtype-local-retry",
];
pub const RECOGNITION_FALLBACKS: [&str; 3] = [
    "realtime-unavailable",
    "realtime-startup-failed",
    "realtime-stream-failed",
];
pub const ENHANCEMENT_CONTEXTS: [&str; 3] = ["selected-text", "style-instructions", "screenshots"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub identifier: String,
    pub created_at: String,
    pub raw_text: String,
    pub delivered_text: String,
    pub mode: String,
    pub language_code: String,
    pub transcription_id: Option<String>,
    pub delivery_outcome: String,
    pub title: Option<String>,
    pub retained_audio_path: Option<String>,
    pub audio_retention_policy: Option<String>,
    pub application_identifier: Option<String>,
    pub correction_source_text: Option<String>,
    pub recognition_route: Option<String>,
    pub recognition_fallback_reason: Option<String>,
    pub enhancement_provider_id: Option<String>,
    pub enhancement_model_identifier: Option<String>,
    pub enhancement_context_sources: Vec<String>,
    pub enhancement_outcome: Option<String>,
    pub recognition_ms: Option<i64>,
    pub enhancement_ms: Option<i64>,
    pub delivery_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryInput {
    pub raw_text: String,
    pub delivered_text: String,
    pub mode: String,
    pub language_code: String,
    pub transcription_id: Option<String>,
    pub delivery_outcome: String,
    pub retained_audio_path: Option<String>,
    pub audio_retention_policy: Option<String>,
    pub application_identifier: Option<String>,
    pub recognition_route: Option<String>,
    pub recognition_fallback_reason: Option<String>,
    pub enhancement_provider_id: Option<String>,
    pub enhancement_model_identifier: Option<String>,
    pub enhancement_context_sources: Vec<String>,
    pub enhancement_outcome: Option<String>,
    pub recognition_ms: Option<i64>,
    pub enhancement_ms: Option<i64>,
    pub delivery_ms: Option<i64>,
}

impl HistoryInput {
    pub fn dictation(raw: impl Into<String>, delivered: impl Into<String>) -> Self {
        Self {
            raw_text: raw.into(),
            delivered_text: delivered.into(),
            mode: "dictation".into(),
            language_code: "eng".into(),
            delivery_outcome: "copied".into(),
            ..Self::default()
        }
    }

    fn validate(&self) -> StoreResult<()> {
        if self
            .recognition_route
            .as_deref()
            .is_some_and(|route| !RECOGNITION_ROUTES.contains(&route))
        {
            return Err(invalid("Unsupported recognition route"));
        }
        if self
            .recognition_fallback_reason
            .as_deref()
            .is_some_and(|reason| !RECOGNITION_FALLBACKS.contains(&reason))
        {
            return Err(invalid("Unsupported recognition fallback reason"));
        }
        if self.recognition_fallback_reason.is_some()
            && self.recognition_route.as_deref() != Some("scribe-v2-batch")
        {
            return Err(invalid(
                "A realtime fallback reason requires the Scribe v2 batch route",
            ));
        }
        match self.enhancement_provider_id.as_deref() {
            None => {
                if self.enhancement_model_identifier.is_some()
                    || !self.enhancement_context_sources.is_empty()
                    || self.enhancement_outcome.is_some()
                {
                    return Err(invalid("Enhancement metadata requires a provider ID"));
                }
            }
            Some(provider) => {
                if !["codex-app-server", "litellm"].contains(&provider) {
                    return Err(invalid("Unsupported enhancement provider"));
                }
                let model = self.enhancement_model_identifier.as_deref().unwrap_or("");
                if model.is_empty()
                    || model != trim(model)
                    || model.chars().count() > 200
                    || model.chars().any(|character| character < ' ')
                {
                    return Err(invalid(
                        "Enhancement model must be one bounded concrete identifier",
                    ));
                }
                let unique: BTreeSet<_> = self.enhancement_context_sources.iter().collect();
                if unique.len() != self.enhancement_context_sources.len()
                    || unique
                        .iter()
                        .any(|source| !ENHANCEMENT_CONTEXTS.contains(&source.as_str()))
                {
                    return Err(invalid(
                        "Enhancement context sources must be unique reviewed values",
                    ));
                }
                if !self.enhancement_outcome.as_deref().is_some_and(|outcome| {
                    ["completed", "raw-fallback", "safe-fallback", "failed"].contains(&outcome)
                }) {
                    return Err(invalid(
                        "Enhancement outcome must be one reviewed terminal value",
                    ));
                }
            }
        }
        for (label, value) in [
            ("recognition_ms", self.recognition_ms),
            ("enhancement_ms", self.enhancement_ms),
            ("delivery_ms", self.delivery_ms),
        ] {
            validate_timing(label, value)?;
        }
        Ok(())
    }
}

pub struct RetryRecognition<'a> {
    pub raw_text: &'a str,
    pub delivered_text: &'a str,
    pub language_code: &'a str,
    pub transcription_id: Option<&'a str>,
    pub retain_audio: bool,
    pub recognition_ms: Option<i64>,
    pub recognition_route: &'a str,
}

#[derive(Clone, Debug)]
pub struct HistoryStore {
    pub database: Database,
}

impl HistoryStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            database: Database::new(path),
        }
    }

    pub fn initialize(&self) -> StoreResult<()> {
        let directory = self.database.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let mut connection = self.database.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("schema.sql"))?;
        let columns = {
            let mut statement = transaction.prepare("PRAGMA table_info(transcription_history)")?;
            statement
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?
        };
        for (name, kind) in [
            ("title", "TEXT"),
            ("title_revision", "INTEGER NOT NULL DEFAULT 0"),
            ("retained_audio_path", "TEXT"),
            ("audio_retention_policy", "TEXT"),
            ("application_identifier", "TEXT"),
            ("correction_source_text", "TEXT"),
            ("recognition_route", "TEXT"),
            ("recognition_fallback_reason", "TEXT"),
            ("enhancement_provider_id", "TEXT"),
            ("enhancement_model_identifier", "TEXT"),
            ("enhancement_context_sources", "TEXT"),
            ("enhancement_outcome", "TEXT"),
            ("recognition_ms", "INTEGER"),
            ("enhancement_ms", "INTEGER"),
            ("delivery_ms", "INTEGER"),
        ] {
            if !columns.contains(name) {
                transaction.execute(
                    &format!("ALTER TABLE transcription_history ADD COLUMN {name} {kind}"),
                    [],
                )?;
            }
        }
        transaction.commit()?;
        fs::set_permissions(&self.database.path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    pub fn add(&self, input: HistoryInput) -> StoreResult<HistoryEntry> {
        input.validate()?;
        let identifier = uuid::Uuid::new_v4().to_string();
        let created = timestamp();
        self.database.connect()?.execute(
            "INSERT INTO transcription_history (identifier, created_at, raw_text, delivered_text, mode, language_code,
             transcription_id, delivery_outcome, retained_audio_path, audio_retention_policy, application_identifier,
             recognition_route, recognition_fallback_reason, enhancement_provider_id, enhancement_model_identifier,
             enhancement_context_sources, enhancement_outcome, recognition_ms, enhancement_ms, delivery_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![identifier, created, input.raw_text, input.delivered_text, input.mode, input.language_code,
                input.transcription_id, input.delivery_outcome, input.retained_audio_path, input.audio_retention_policy,
                input.application_identifier, input.recognition_route, input.recognition_fallback_reason,
                input.enhancement_provider_id, input.enhancement_model_identifier,
                if input.enhancement_context_sources.is_empty() { None } else { Some(input.enhancement_context_sources.join(",")) },
                input.enhancement_outcome, input.recognition_ms, input.enhancement_ms, input.delivery_ms],
        )?;
        self.find(&identifier)
    }

    pub fn recent(&self, limit: i64) -> StoreResult<Vec<HistoryEntry>> {
        let connection = self.database.connect()?;
        let mut statement = connection
            .prepare("SELECT * FROM transcription_history ORDER BY created_at DESC LIMIT ?")?;
        Ok(statement
            .query_map([limit], decode_entry)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn find(&self, identifier: &str) -> StoreResult<HistoryEntry> {
        self.database
            .connect()?
            .query_row(
                "SELECT * FROM transcription_history WHERE identifier = ?",
                [identifier],
                decode_entry,
            )
            .optional()?
            .ok_or(StoreError::NotFound)
    }

    pub fn update_title(&self, identifier: &str, title: Option<&str>) -> StoreResult<HistoryEntry> {
        self.database.connect()?.execute("UPDATE transcription_history SET title = ?, title_revision = title_revision + 1 WHERE identifier = ?", params![title, identifier])?;
        self.find(identifier)
    }

    pub fn save_generated_title(
        &self,
        identifier: &str,
        title: &str,
        expected: Option<&str>,
    ) -> StoreResult<bool> {
        Ok(self.database.connect()?.execute("UPDATE transcription_history SET title = ? WHERE identifier = ? AND title IS ? AND title_revision = 0", params![title, identifier, expected])? == 1)
    }

    pub fn restore_raw(&self, identifier: &str) -> StoreResult<HistoryEntry> {
        self.database.connect()?.execute("UPDATE transcription_history SET delivered_text = raw_text, correction_source_text = NULL,
             delivery_outcome = 'restored-raw', enhancement_provider_id = NULL, enhancement_model_identifier = NULL,
             enhancement_context_sources = NULL, enhancement_outcome = NULL, enhancement_ms = NULL, delivery_ms = NULL WHERE identifier = ?", [identifier])?;
        self.find(identifier)
    }

    pub fn reprocess(
        &self,
        identifier: &str,
        delivered: &str,
        enhancement_ms: i64,
    ) -> StoreResult<HistoryEntry> {
        let delivered = trim(delivered);
        if delivered.is_empty() {
            return Err(invalid("Reprocessed text cannot be empty"));
        }
        validate_timing("enhancement_ms", Some(enhancement_ms))?;
        self.database.connect()?.execute("UPDATE transcription_history SET delivered_text = ?, correction_source_text = NULL,
             delivery_outcome = 'pending-preview', enhancement_provider_id = NULL, enhancement_model_identifier = NULL,
             enhancement_context_sources = NULL, enhancement_outcome = NULL, enhancement_ms = ?, delivery_ms = NULL WHERE identifier = ?", params![delivered, enhancement_ms, identifier])?;
        self.find(identifier)
    }

    pub fn correct_delivered_text(
        &self,
        identifier: &str,
        delivered: &str,
    ) -> StoreResult<HistoryEntry> {
        let delivered = trim(delivered);
        if delivered.is_empty() {
            return Err(invalid("Corrected text cannot be empty"));
        }
        let entry = self.find(identifier)?;
        if delivered == entry.delivered_text {
            return Ok(entry);
        }
        let original = entry
            .correction_source_text
            .as_deref()
            .filter(|text| !text.is_empty())
            .unwrap_or(&entry.delivered_text);
        self.database.connect()?.execute("UPDATE transcription_history SET delivered_text = ?, correction_source_text = ?, delivery_outcome = 'pending-preview' WHERE identifier = ?", params![delivered, original, identifier])?;
        self.find(identifier)
    }

    pub fn mark_delivered(
        &self,
        identifier: &str,
        delivered: &str,
        outcome: &str,
        retain_audio: bool,
        delivery_ms: Option<i64>,
    ) -> StoreResult<HistoryEntry> {
        let entry = self.find(identifier)?;
        validate_timing("delivery_ms", delivery_ms)?;
        if !retain_audio {
            self.delete_retained_audio(&entry)?;
        }
        self.database.connect()?.execute(
            "UPDATE transcription_history SET delivered_text = ?, delivery_outcome = ?,
             retained_audio_path = CASE WHEN ? THEN retained_audio_path ELSE NULL END,
             delivery_ms = COALESCE(?, delivery_ms) WHERE identifier = ?",
            params![delivered, outcome, retain_audio, delivery_ms, identifier],
        )?;
        self.find(identifier)
    }

    pub fn mark_retry_ready(
        &self,
        identifier: &str,
        retry: RetryRecognition<'_>,
    ) -> StoreResult<HistoryEntry> {
        let entry = self.find(identifier)?;
        validate_timing("recognition_ms", retry.recognition_ms)?;
        self.database.connect()?.execute("UPDATE transcription_history SET raw_text = ?, delivered_text = ?, language_code = ?,
             transcription_id = ?, correction_source_text = NULL, recognition_route = ?, recognition_fallback_reason = NULL,
             enhancement_provider_id = NULL, enhancement_model_identifier = NULL, enhancement_context_sources = NULL,
             enhancement_outcome = NULL, recognition_ms = ?, enhancement_ms = NULL, delivery_ms = NULL, delivery_outcome = 'retry-ready'
             WHERE identifier = ?", params![retry.raw_text, retry.delivered_text, retry.language_code, retry.transcription_id,
                retry.recognition_route, retry.recognition_ms, identifier])?;
        if !retry.retain_audio {
            self.delete_retained_audio(&entry)?;
            self.database.connect()?.execute(
                "UPDATE transcription_history SET retained_audio_path = NULL WHERE identifier = ?",
                [identifier],
            )?;
        }
        self.find(identifier)
    }

    pub fn managed_retained_audio(&self, identifier: &str) -> StoreResult<PathBuf> {
        let entry = self.find(identifier)?;
        let path = self.validated_audio_path(
            entry
                .retained_audio_path
                .as_deref()
                .ok_or_else(|| invalid("History entry has no retained audio"))?,
        )?;
        if !path.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Retained audio no longer exists",
            )
            .into());
        }
        Ok(path)
    }

    pub fn recording_conversation(&self, identifier: &str) -> StoreResult<HistoryEntry> {
        let parent: Option<String> = self.database.connect()?.query_row("SELECT history_identifier FROM recording_continuations WHERE segment_identifier = ?", [identifier], |row| row.get(0)).optional()?;
        self.find(parent.as_deref().unwrap_or(identifier))
    }

    pub fn continuations(&self, identifier: &str) -> StoreResult<Vec<HistoryEntry>> {
        let connection = self.database.connect()?;
        let mut statement = connection.prepare("SELECT h.* FROM recording_continuations c JOIN transcription_history h ON h.identifier = c.segment_identifier WHERE c.history_identifier = ? ORDER BY h.created_at")?;
        Ok(statement
            .query_map([identifier], decode_entry)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn delete(&self, identifier: &str) -> StoreResult<()> {
        let entry = self.find(identifier)?;
        for segment in self.continuations(identifier)? {
            self.delete(&segment.identifier)?;
        }
        self.delete_retained_audio(&entry)?;
        ScreenshotStore::new(&self.database.path).delete_owner(identifier, false)?;
        self.database.connect()?.execute(
            "DELETE FROM transcription_history WHERE identifier = ?",
            [identifier],
        )?;
        Ok(())
    }

    pub fn prune_older_than(
        &self,
        retention_days: i64,
        excluded: &BTreeSet<String>,
        now: DateTime<Utc>,
    ) -> StoreResult<usize> {
        if retention_days <= 0 {
            return Ok(0);
        }
        let cutoff = now
            .checked_sub_signed(
                Duration::try_days(retention_days)
                    .ok_or_else(|| invalid("History retention interval is too large"))?,
            )
            .ok_or_else(|| invalid("History retention interval is too large"))?;
        let connection = self.database.connect()?;
        let mut statement = connection.prepare(
            "SELECT segment_identifier, history_identifier FROM recording_continuations",
        )?;
        let segments: BTreeMap<String, String> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut groups: BTreeMap<String, Vec<HistoryEntry>> = BTreeMap::new();
        for entry in self.recent(i64::from(i32::MAX))? {
            groups
                .entry(
                    segments
                        .get(&entry.identifier)
                        .unwrap_or(&entry.identifier)
                        .clone(),
                )
                .or_default()
                .push(entry);
        }
        let mut removed = 0;
        for group in groups.values() {
            if group.iter().any(|item| excluded.contains(&item.identifier)) {
                continue;
            }
            let times = group
                .iter()
                .map(|entry| parse_timestamp(&entry.created_at))
                .collect::<StoreResult<Vec<_>>>()?;
            if times.iter().max().is_some_and(|latest| *latest >= cutoff) {
                continue;
            }
            for entry in group {
                ScreenshotStore::new(&self.database.path).delete_owner(&entry.identifier, false)?;
                match self.delete_retained_audio(entry) {
                    Err(StoreError::Invalid(_)) => {}
                    result => result?,
                }
            }
            let mut connection = self.database.connect()?;
            let transaction = connection.transaction()?;
            for item in group {
                transaction.execute(
                    "DELETE FROM transcription_history WHERE identifier = ?",
                    [&item.identifier],
                )?;
            }
            transaction.commit()?;
            removed += group.len();
        }
        Ok(removed)
    }

    pub fn export(
        &self,
        entry: &HistoryEntry,
        directory: &Path,
        format: &str,
        rewrites: &[Rewrite],
        source_text: Option<&str>,
    ) -> StoreResult<PathBuf> {
        let segments = self.continuations(&entry.identifier)?;
        let stamp: String = entry
            .created_at
            .chars()
            .take(19)
            .filter(|character| *character != ':')
            .collect();
        let prefix: String = entry.identifier.chars().take(8).collect();
        let (extension, content) = match format {
            "json" => {
                let mut value = serde_json::to_value(entry)?;
                let object = value.as_object_mut().unwrap();
                object.insert(
                    "working_source".into(),
                    serde_json::to_value(source_text.unwrap_or(&entry.raw_text))?,
                );
                object.insert("rewrites".into(), serde_json::to_value(rewrites)?);
                object.insert(
                    "recording_segments".into(),
                    serde_json::to_value(&segments)?,
                );
                ("json", serde_json::to_string_pretty(&value)? + "\n")
            }
            "markdown" => {
                let text = self.markdown_export(entry, rewrites, source_text, &segments);
                ("md", text)
            }
            _ => return Err(invalid("Unsupported history export format")),
        };
        let path = directory.join(format!("mluva-{stamp}-{prefix}.{extension}"));
        atomic_write_private(&path, content.as_bytes())?;
        Ok(path)
    }

    fn markdown_export(
        &self,
        entry: &HistoryEntry,
        rewrites: &[Rewrite],
        source_text: Option<&str>,
        segments: &[HistoryEntry],
    ) -> String {
        let nonempty = |value: Option<&str>, fallback: &str| {
            value
                .filter(|text| !text.is_empty())
                .unwrap_or(fallback)
                .to_owned()
        };
        let duration = |value: Option<i64>| {
            value
                .map(|value| format!("{value} ms"))
                .unwrap_or_else(|| "Not recorded".into())
        };
        let mut text = format!(
            "# {}\n\n- Created: {}\n- Mode: {}\n- Language: {}\n- Delivery: {}\n- Recognition: {}\n- Recognition fallback: {}\n- Enhancement provider: {}\n- Enhancement model: {}\n- Enhancement context: {}\n- Enhancement outcome: {}\n- Recognition latency: {}\n- Enhancement latency: {}\n- Delivery latency: {}\n- Application: {}\n\n## Delivered text\n\n{}\n\n## Raw transcript\n\n{}\n",
            nonempty(entry.title.as_deref(), "Mluva transcript"),
            entry.created_at,
            entry.mode,
            entry.language_code,
            entry.delivery_outcome,
            nonempty(entry.recognition_route.as_deref(), "Legacy/unknown"),
            nonempty(entry.recognition_fallback_reason.as_deref(), "None"),
            nonempty(entry.enhancement_provider_id.as_deref(), "None"),
            nonempty(entry.enhancement_model_identifier.as_deref(), "None"),
            if entry.enhancement_context_sources.is_empty() {
                "None".into()
            } else {
                entry.enhancement_context_sources.join(", ")
            },
            nonempty(entry.enhancement_outcome.as_deref(), "None"),
            duration(entry.recognition_ms),
            duration(entry.enhancement_ms),
            duration(entry.delivery_ms),
            nonempty(entry.application_identifier.as_deref(), "Not captured"),
            entry.delivered_text,
            entry.raw_text
        );
        if let Some(source) = source_text
            && source != entry.raw_text
        {
            text.push_str(&format!("\n## Edited source\n\n{source}\n"));
        }
        for segment in segments {
            text.push_str(&format!(
                "\n## Continued recording · {}\n\n{}\n",
                segment.created_at, segment.raw_text
            ));
        }
        for reply in rewrites {
            text.push_str(&format!(
                "\n## Rewrite · {}\n\n### Instruction\n\n{}\n\n### Result\n\n{}\n\nModel: {}\n",
                reply.created_at, reply.instruction, reply.text, reply.model
            ));
        }
        text
    }

    fn delete_retained_audio(&self, entry: &HistoryEntry) -> StoreResult<()> {
        if let Some(path) = entry.retained_audio_path.as_deref() {
            match fs::remove_file(self.validated_audio_path(path)?) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                result => result?,
            }
        }
        Ok(())
    }

    fn validated_audio_path(&self, retained: &str) -> StoreResult<PathBuf> {
        let audio = resolve_path(Path::new(retained))?;
        let directory = resolve_path(
            &self
                .database
                .path
                .parent()
                .unwrap_or(Path::new("."))
                .join("recordings"),
        )?;
        if !audio.starts_with(directory) {
            return Err(invalid(
                "Refusing to access history audio outside the managed recordings directory",
            ));
        }
        Ok(audio)
    }
}

fn validate_timing(label: &str, value: Option<i64>) -> StoreResult<()> {
    if value.is_some_and(|value| !(0..=86_400_000).contains(&value)) {
        return Err(invalid(format!(
            "{label} must be an integer from zero through 24 hours"
        )));
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> StoreResult<DateTime<Utc>> {
    if let Ok(stamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(stamp.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
        .map(|value| value.and_utc())
        .map_err(|_| invalid("History contains an invalid timestamp"))
}

pub(crate) fn decode_entry(row: &Row<'_>) -> rusqlite::Result<HistoryEntry> {
    let context: Option<String> = row.get("enhancement_context_sources")?;
    Ok(HistoryEntry {
        identifier: row.get("identifier")?,
        created_at: row.get("created_at")?,
        raw_text: row.get("raw_text")?,
        delivered_text: row.get("delivered_text")?,
        mode: row.get("mode")?,
        language_code: row.get("language_code")?,
        transcription_id: row.get("transcription_id")?,
        delivery_outcome: row.get("delivery_outcome")?,
        title: row.get("title")?,
        retained_audio_path: row.get("retained_audio_path")?,
        audio_retention_policy: row.get("audio_retention_policy")?,
        application_identifier: row.get("application_identifier")?,
        correction_source_text: row.get("correction_source_text")?,
        recognition_route: row.get("recognition_route")?,
        recognition_fallback_reason: row.get("recognition_fallback_reason")?,
        enhancement_provider_id: row.get("enhancement_provider_id")?,
        enhancement_model_identifier: row.get("enhancement_model_identifier")?,
        enhancement_context_sources: context
            .filter(|value| !value.is_empty())
            .map(|value| value.split(',').map(str::to_owned).collect())
            .unwrap_or_default(),
        enhancement_outcome: row.get("enhancement_outcome")?,
        recognition_ms: row.get("recognition_ms")?,
        enhancement_ms: row.get("enhancement_ms")?,
        delivery_ms: row.get("delivery_ms")?,
    })
}
