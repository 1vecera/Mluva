//! Restart-safe unresolved drafts with explicit Incognito and managed-audio boundaries.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::database::{StoreResult, invalid};
use crate::private_files::{atomic_write_private, resolve_path};

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

    pub fn save(&mut self, draft: ScratchpadDraft, persist: bool) -> StoreResult<()> {
        if let Some(error) = &self.persistence_error {
            return Err(invalid(format!(
                "Scratchpad changes are disabled until the malformed recovery document is repaired: {error}"
            )));
        }
        if !persist || draft.incognito {
            self.erase_document()?;
        } else {
            let mut content = serde_json::to_vec_pretty(&draft)?;
            content.push(b'\n');
            atomic_write_private(&self.path, &content)?;
        }
        self.draft = Some(draft);
        Ok(())
    }

    pub fn clear(&mut self, remove_audio: bool) -> StoreResult<()> {
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
