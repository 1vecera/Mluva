//! Restart-safe unresolved drafts with explicit Incognito and managed-audio boundaries.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::database::{StoreResult, invalid};
use crate::private_files::resolve_path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScratchpadDraft {
    pub identifier: String,
    pub history_identifier: Option<String>,
    pub created_at: String,
    pub raw_text: String,
    pub text: String,
    pub audio_path: Option<String>,
    #[serde(default)]
    pub incognito: bool,
    #[serde(default = "failure_retention")]
    pub audio_retention_policy: String,
    #[serde(default)]
    pub session_identifier: Option<String>,
}

fn failure_retention() -> String {
    "failures".into()
}

#[derive(Debug)]
pub struct ScratchpadDraftStore {
    pub path: PathBuf,
    pub draft: Option<ScratchpadDraft>,
    pub persistence_error: Option<String>,
}

impl ScratchpadDraftStore {
    /// Retire the old Notes review queue into local History. The durable marker
    /// and text commit together, so a crash before removing the JSON cannot
    /// duplicate a note or resurrect it after a later deletion.
    pub fn migrate_to_history(
        &mut self,
        history: &crate::history::HistoryStore,
    ) -> StoreResult<Option<String>> {
        if let Some(error) = &self.persistence_error {
            return Err(invalid(error));
        }
        let Some(draft) = self.draft.clone() else {
            return Ok(None);
        };
        if draft.incognito {
            self.clear(true)?;
            return Ok(None);
        }
        let mut connection = history.database.connect()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS legacy_notes_recovery (identifier TEXT PRIMARY KEY)",
        )?;
        let first = transaction.execute(
            "INSERT OR IGNORE INTO legacy_notes_recovery(identifier) VALUES (?)",
            [&draft.identifier],
        )? == 1;
        let mut recovered = None;
        if first {
            let identifier = draft.history_identifier.clone().unwrap_or_else(|| {
                uuid::Uuid::parse_str(&draft.identifier)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4())
                    .to_string()
            });
            // A missing existing owner means the note was deleted. Never restore it.
            if draft.history_identifier.is_none() {
                transaction.execute("INSERT OR IGNORE INTO transcription_history(identifier, created_at, raw_text, delivered_text, mode, language_code, delivery_outcome, retained_audio_path, audio_retention_policy) VALUES (?, ?, ?, ?, 'dictation', 'eng', 'recovered', ?, ?)",
                    params![identifier, draft.created_at, draft.raw_text, draft.raw_text, draft.audio_path, draft.audio_retention_policy])?;
            }
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM transcription_history WHERE identifier = ?)",
                [&identifier],
                |row| row.get(0),
            )?;
            if exists {
                // An editor save is newer than this retired review queue.
                transaction.execute("INSERT OR IGNORE INTO conversation_sources(history_identifier, text) VALUES (?, ?)", params![identifier, draft.text])?;
                recovered = Some(identifier);
            }
        }
        transaction.commit()?;
        self.clear(false)?;
        Ok(recovered)
    }

    pub fn new(path: impl AsRef<Path>) -> Self {
        let mut store = Self {
            path: path.as_ref().to_owned(),
            draft: None,
            persistence_error: None,
        };
        match fs::read(&store.path) {
            Ok(bytes) => match load_draft(&bytes) {
                Ok(draft) => store.draft = Some(draft),
                Err(error) => store.persistence_error = Some(error),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => store.persistence_error = Some(error.to_string()),
        }
        store
    }

    fn clear(&mut self, remove_audio: bool) -> StoreResult<()> {
        if remove_audio
            && let Some(audio) = self
                .draft
                .as_ref()
                .and_then(|draft| draft.audio_path.as_deref())
        {
            let path = resolve_path(Path::new(audio))?;
            let recordings = resolve_path(
                &self
                    .path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("recordings"),
            )?;
            if !path.starts_with(recordings) {
                return Err(invalid(
                    "Refusing to delete Scratchpad audio outside the managed recordings directory",
                ));
            }
            remove_if_present(&path)?;
        }
        self.erase_document()?;
        self.draft = None;
        Ok(())
    }

    fn erase_document(&self) -> StoreResult<()> {
        remove_if_present(&self.path)?;
        remove_if_present(&self.path.with_extension("tmp"))?;
        Ok(())
    }
}

fn load_draft(bytes: &[u8]) -> Result<ScratchpadDraft, String> {
    // Decode the complete document before its fields, as the released recovery
    // store does. A syntax error must not be obscured by an earlier unknown key.
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| recovery_parse_error(bytes, &error))?;
    if !value.is_object() {
        return Err("Scratchpad recovery document must be a JSON object.".into());
    }
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn recovery_parse_error(bytes: &[u8], error: &serde_json::Error) -> String {
    let message = error.to_string();
    if !message.starts_with("key must be a string at line ") {
        return message;
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
        return message;
    };
    let line_start = source
        .split_inclusive('\n')
        .take(error.line().saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let mut byte = (line_start + error.column().saturating_sub(1)).min(source.len());
    while !source.is_char_boundary(byte) {
        byte = byte.saturating_sub(1);
    }
    let position = source[..byte].chars().count();
    let column = source[line_start..byte].chars().count() + 1;
    format!(
        "Expecting property name enclosed in double quotes: line {} column {column} (char {position})",
        error.line()
    )
}

fn remove_if_present(path: &Path) -> StoreResult<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        result => result?,
    }
    Ok(())
}
