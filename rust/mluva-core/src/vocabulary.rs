//! Review-only suggestions derived from explicit corrections, never automatic learning.

use std::collections::{BTreeMap, BTreeSet};

use caseless::default_case_fold_str as fold;
use serde::{Deserialize, Serialize};
use unicode_normalization::{UnicodeNormalization, char::canonical_combining_class};

use crate::history::HistoryEntry;
use crate::personalization::DictionaryReplacement;
use crate::text;

pub const MAX_CORRECTION_CHARACTERS: usize = 80;
pub const MAX_CORRECTION_WORDS: usize = 4;
pub const MAX_SUGGESTION_HISTORY_ENTRIES: usize = 1_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VocabularySuggestion {
    pub identifier: String,
    pub spoken: String,
    pub written: String,
    pub application_identifier: Option<String>,
    pub occurrences: usize,
}

struct Word<'a> {
    value: &'a str,
    start: usize,
    end: usize,
}

fn words(source: &str) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut scan = source.char_indices().peekable();
    while let Some((start, character)) = scan.next() {
        if text::whitespace(character) {
            continue;
        }
        while scan
            .peek()
            .is_some_and(|(_, character)| !text::whitespace(*character))
        {
            scan.next();
        }
        let end = scan
            .peek()
            .map(|(position, _)| *position)
            .unwrap_or(source.len());
        let leading =
            source[start..end].trim_start_matches(['"', '“', '”', '‘', '’', '(', '[', '{']);
        let trimmed = leading.trim_end_matches([
            '"', '“', '”', '‘', '’', ')', ']', '}', ',', '.', '!', '?', ';', ':',
        ]);
        if !trimmed.is_empty() {
            let start = end - leading.len();
            words.push(Word {
                value: trimmed,
                start,
                end: start + trimmed.len(),
            });
        }
    }
    words
}

pub fn focused_correction(source: &str, corrected: &str) -> Option<(String, String)> {
    let source_words = words(source);
    let corrected_words = words(corrected);
    if source_words.is_empty() || corrected_words.is_empty() {
        return None;
    }
    let prefix = source_words
        .iter()
        .zip(&corrected_words)
        .take_while(|(left, right)| left.value == right.value)
        .count();
    let suffix = source_words[prefix..]
        .iter()
        .rev()
        .zip(corrected_words[prefix..].iter().rev())
        .take_while(|(left, right)| left.value == right.value)
        .count();
    let source_changed = &source_words[prefix..source_words.len() - suffix];
    let corrected_changed = &corrected_words[prefix..corrected_words.len() - suffix];
    if source_changed.is_empty()
        || corrected_changed.is_empty()
        || source_changed.len() > MAX_CORRECTION_WORDS
        || corrected_changed.len() > MAX_CORRECTION_WORDS
    {
        return None;
    }
    if prefix + suffix == 0 && source_words.len().max(corrected_words.len()) > 3 {
        return None;
    }
    let spoken = &source[source_changed.first()?.start..source_changed.last()?.end];
    let written = &corrected[corrected_changed.first()?.start..corrected_changed.last()?.end];
    if spoken == written
        || spoken.chars().count() > MAX_CORRECTION_CHARACTERS
        || written.chars().count() > MAX_CORRECTION_CHARACTERS
    {
        return None;
    }
    Some((spoken.into(), written.into()))
}

fn normalized_spoken(value: &str) -> String {
    fold(value)
        .nfd()
        .filter(|&character| canonical_combining_class(character) == 0)
        .collect()
}

fn covered_by_dictionary(
    suggestion: &VocabularySuggestion,
    dictionary: &[DictionaryReplacement],
) -> bool {
    let key = normalized_spoken(&suggestion.spoken);
    for rule in dictionary {
        if normalized_spoken(&rule.spoken) != key {
            continue;
        }
        if suggestion.application_identifier.is_none() {
            return rule.application_identifier.is_none();
        }
        if rule.application_identifier.is_none()
            || rule.application_identifier == suggestion.application_identifier
        {
            return true;
        }
    }
    false
}

pub fn suggestions(
    entries: &[HistoryEntry],
    dictionary: &[DictionaryReplacement],
    dismissed: &BTreeSet<String>,
) -> Vec<VocabularySuggestion> {
    let mut grouped = BTreeMap::<String, VocabularySuggestion>::new();
    for entry in entries {
        let Some(source) = &entry.correction_source_text else {
            continue;
        };
        let Some((spoken, written)) = focused_correction(source, &entry.delivered_text) else {
            continue;
        };
        let identifier = [
            entry
                .application_identifier
                .as_deref()
                .unwrap_or_default()
                .to_owned(),
            normalized_spoken(&spoken),
            written.nfc().collect(),
        ]
        .join("\u{1f}");
        if dismissed.contains(&identifier) {
            continue;
        }
        let occurrences = grouped
            .get(&identifier)
            .map(|suggestion| suggestion.occurrences + 1)
            .unwrap_or(1);
        grouped.insert(
            identifier.clone(),
            VocabularySuggestion {
                identifier,
                spoken,
                written,
                application_identifier: entry.application_identifier.clone(),
                occurrences,
            },
        );
    }
    let mut visible = grouped
        .into_values()
        .filter(|suggestion| !covered_by_dictionary(suggestion, dictionary))
        .collect::<Vec<_>>();
    visible.sort_by_cached_key(|suggestion| {
        (
            std::cmp::Reverse(suggestion.occurrences),
            fold(&suggestion.spoken),
            fold(&suggestion.written),
            suggestion.identifier.clone(),
        )
    });
    visible
}
