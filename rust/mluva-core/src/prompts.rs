//! Local overrides with lossless recovery baselines and optimistic concurrency.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::database::{StoreResult, invalid};
use crate::private_files::atomic_write_private_durable;
use crate::prompt_catalog::{DEFAULTS, Prompt, SavedStyle};
use crate::text;

pub const MAX_PROMPT_CHARACTERS: usize = 8000;
const CONFLICT: &str =
    "This file changed locally. Your draft is kept. Copy it, then cancel and reopen to reload.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptState {
    pub text: String,
    pub token: Option<Vec<u8>>,
    pub overridden: bool,
    pub error: String,
}

#[derive(Clone, Debug)]
pub struct PromptStore {
    pub directory: PathBuf,
    catalog: Vec<Prompt>,
}

impl PromptStore {
    pub fn new(
        directory: impl AsRef<Path>,
        custom_live: &str,
        styles: &[SavedStyle],
    ) -> StoreResult<Self> {
        let mut store = Self {
            directory: directory.as_ref().to_owned(),
            catalog: DEFAULTS.prompts.clone(),
        };
        if !custom_live.is_empty() {
            let prompt = store
                .catalog
                .iter_mut()
                .find(|prompt| prompt.identifier == "live-custom")
                .unwrap();
            prompt.default = custom_live.into();
            prompt.built_in = false;
        }
        store.sync_styles(styles)?;
        Ok(store)
    }

    pub fn catalog(&self) -> &[Prompt] {
        &self.catalog
    }

    pub fn prompt(&self, identifier: &str) -> StoreResult<&Prompt> {
        self.catalog
            .iter()
            .find(|prompt| prompt.identifier == identifier)
            .ok_or_else(|| invalid("Unknown prompt identifier"))
    }

    pub fn sync_styles(&mut self, styles: &[SavedStyle]) -> StoreResult<()> {
        let prompts = styles
            .iter()
            .map(|style| {
                let uuid = uuid::Uuid::parse_str(&style.identifier)
                    .map_err(|_| invalid("Invalid saved style identifier"))?;
                Ok(Prompt {
                    identifier: format!("style-{uuid}"),
                    name: format!("Style · {}", style.name),
                    purpose: "Saved rewrite and capture output style.".into(),
                    default: style.instructions.clone(),
                    built_in: style.is_built_in,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;
        self.catalog
            .retain(|prompt| !prompt.identifier.starts_with("style-"));
        for prompt in prompts {
            if let Some(existing) = self
                .catalog
                .iter_mut()
                .find(|existing| existing.identifier == prompt.identifier)
            {
                *existing = prompt;
            } else {
                self.catalog.push(prompt);
            }
        }
        Ok(())
    }

    pub fn path(&self, identifier: &str) -> StoreResult<PathBuf> {
        self.prompt(identifier)?;
        Ok(self.directory.join(format!("{identifier}.md")))
    }

    pub fn read(&self, identifier: &str) -> StoreResult<PromptState> {
        let prompt = self.prompt(identifier)?;
        let path = self.path(identifier)?;
        let filename = path.file_name().unwrap().to_string_lossy();
        let baseline = |token, overridden, error| PromptState {
            text: prompt.default.clone(),
            token,
            overridden,
            error,
        };
        let raw = match fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(baseline(None, false, String::new()));
            }
            Err(error) => {
                let description = error.to_string();
                let description = description
                    .split(" (os error ")
                    .next()
                    .unwrap_or(&description);
                return Ok(baseline(
                    None,
                    true,
                    format!("Cannot read {filename}: {description}. Using the baseline."),
                ));
            }
        };
        match std::str::from_utf8(&raw) {
            Ok(text) => match self.validate(identifier, text) {
                Ok(()) => Ok(PromptState {
                    text: text.into(),
                    token: Some(raw),
                    overridden: true,
                    error: String::new(),
                }),
                Err(error) => Ok(baseline(
                    Some(raw),
                    true,
                    format!("{filename}: {error}. Using the baseline until repaired."),
                )),
            },
            Err(error) => {
                let reason = utf8_diagnostic(&raw, error);
                Ok(baseline(
                    Some(raw),
                    true,
                    format!("{filename}: {reason}. Using the baseline until repaired."),
                ))
            }
        }
    }

    pub fn snapshot(&self) -> StoreResult<BTreeMap<String, String>> {
        self.catalog
            .iter()
            .map(|prompt| {
                Ok((
                    prompt.identifier.clone(),
                    self.read(&prompt.identifier)?.text,
                ))
            })
            .collect()
    }

    pub fn validate(&self, identifier: &str, value: &str) -> StoreResult<()> {
        self.path(identifier)?;
        if value.chars().count() > MAX_PROMPT_CHARACTERS {
            return Err(invalid("Use at most 8,000 characters"));
        }
        if text::trim(value).is_empty() && identifier != "live-custom" {
            return Err(invalid("The prompt cannot be empty"));
        }
        if value
            .chars()
            .any(|character| character < ' ' && !['\n', '\r', '\t'].contains(&character))
        {
            return Err(invalid("Remove control characters"));
        }
        Ok(())
    }

    pub fn save(&self, identifier: &str, value: &str, expected: Option<&[u8]>) -> StoreResult<()> {
        self.validate(identifier, value)?;
        self.check_conflict(identifier, expected)?;
        atomic_write_private_durable(&self.path(identifier)?, value.as_bytes())?;
        Ok(())
    }

    pub fn reset(&self, identifier: &str, expected: Option<&[u8]>) -> StoreResult<()> {
        self.check_conflict(identifier, expected)?;
        match fs::remove_file(self.path(identifier)?) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            result => result?,
        }
        Ok(())
    }

    fn check_conflict(&self, identifier: &str, expected: Option<&[u8]>) -> StoreResult<()> {
        let actual = match fs::read(self.path(identifier)?) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if actual.as_deref() != expected {
            return Err(invalid(CONFLICT));
        }
        Ok(())
    }
}

fn utf8_diagnostic(raw: &[u8], error: std::str::Utf8Error) -> String {
    let start = error.valid_up_to();
    let reason = if error.error_len().is_none() {
        "unexpected end of data"
    } else if !(0xc2..=0xf4).contains(&raw[start]) {
        "invalid start byte"
    } else {
        "invalid continuation byte"
    };
    let length = error.error_len().unwrap_or(raw.len() - start);
    if length == 1 {
        format!(
            "'utf-8' codec can't decode byte 0x{:02x} in position {start}: {reason}",
            raw[start]
        )
    } else {
        format!(
            "'utf-8' codec can't decode bytes in position {start}-{}: {reason}",
            start + length - 1
        )
    }
}
