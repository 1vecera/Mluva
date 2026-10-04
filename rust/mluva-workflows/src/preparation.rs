//! Deterministic rules and variable values frozen before audio capture.

use mluva_core::{
    config::AppConfig,
    database::StoreResult,
    history::{HistoryEntry, HistoryStore},
    personalization::{
        DictionaryReplacement, PersonalizationStore, Snippet, personalize_transcript,
        snippet_variables,
    },
    transcript::normalize_spoken_structure,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptPreparationSnapshot {
    pub mode: String,
    pub spoken_commands_enabled: bool,
    pub dictionary_replacements: Vec<DictionaryReplacement>,
    pub snippets: Vec<Snippet>,
    pub variables: BTreeMap<String, String>,
    pub protected_vocabulary: Vec<String>,
}

impl TranscriptPreparationSnapshot {
    pub fn structured_text(&self, raw_text: &str) -> String {
        if self.spoken_commands_enabled && self.mode != "command" {
            normalize_spoken_structure(raw_text)
        } else {
            raw_text.into()
        }
    }

    pub fn process(&self, raw_text: &str) -> String {
        personalize_transcript(
            &self.structured_text(raw_text),
            &self.dictionary_replacements,
            &self.snippets,
            &self.variables,
        )
    }
}

pub fn freeze_transcript_preparation(
    config: &AppConfig,
    personalization: Option<&PersonalizationStore>,
    mode: &str,
    application_identifier: Option<&str>,
) -> StoreResult<TranscriptPreparationSnapshot> {
    let (dictionary_replacements, snippets, protected_vocabulary) = match personalization {
        Some(store) => (
            store.scoped_dictionary(application_identifier)?,
            store.scoped_snippets(application_identifier)?,
            store.scoped_recognition_context(application_identifier)?,
        ),
        None => (vec![], vec![], vec![]),
    };
    Ok(TranscriptPreparationSnapshot {
        mode: mode.into(),
        spoken_commands_enabled: config.spoken_commands_enabled,
        dictionary_replacements,
        snippets,
        variables: snippet_variables(),
        protected_vocabulary,
    })
}

/// Explicit History reprocessing uses current rules and never a provider or clipboard.
pub fn reprocess_history_entry(
    config: &AppConfig,
    history: &HistoryStore,
    personalization: Option<&PersonalizationStore>,
    identifier: &str,
) -> StoreResult<HistoryEntry> {
    let entry = history.find(identifier)?;
    let preparation = freeze_transcript_preparation(
        config,
        personalization,
        &entry.mode,
        entry.application_identifier.as_deref(),
    )?;
    let started = Instant::now();
    let delivered = preparation.process(&entry.raw_text);
    history.reprocess(
        identifier,
        &delivered,
        milliseconds(started.elapsed().as_secs_f64()),
    )
}

pub(crate) fn milliseconds(seconds: f64) -> i64 {
    (seconds * 1_000.0).round_ties_even() as i64
}
