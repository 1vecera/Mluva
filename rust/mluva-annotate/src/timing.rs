//! Put the final transcript on the recording clock and split it between keyframes.
//!
//! `said_per_keyframe`, `ends_clause` and `word_count` are adapted from Scrawl's
//! `src/keyframes.rs` at f2ad4e8c75b6cc6171c38d85cd8755e4d9fe03fe.
//! Copyright (c) 2026 Kit Langton. MIT License; see `resources/scrawl-LICENSE`.
use crate::model::{Timing, Transcript, Word};

/// Speech that starts a new thought: after clause punctuation or a pause.
const CLAUSE_GAP_MS: u64 = 1_500;
/// Live preview words appear about this long after they were spoken.
const PREVIEW_LAG_MS: u64 = 700;

fn tokens(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// Times for the final words. Provider timings win; otherwise the preview's growth places each
/// word by its share of the text, earlier by the preview's lag; otherwise words spread evenly.
/// The final text is never changed, only timed.
pub fn transcript(
    text: &str,
    provider: Option<Vec<Word>>,
    preview: &[(u64, usize)],
    duration_ms: u64,
) -> Transcript {
    if let Some(words) = provider.filter(|words| !words.is_empty()) {
        return Transcript {
            text: text.into(),
            timing: Timing::Provider,
            words,
        };
    }
    let tokens = tokens(text);
    let n = tokens.len();
    let duration = duration_ms.max(1);
    let mut samples: Vec<(u64, usize)> = preview
        .iter()
        .copied()
        .filter(|(t, words)| *t <= duration && *words > 0)
        .collect();
    samples.sort_by_key(|(t, _)| *t);
    // A preview that never grew past a word carries no timing.
    let growth = samples.last().map_or(0, |(_, words)| *words);
    let timing = if growth >= 2 && n > 0 {
        Timing::Preview
    } else {
        Timing::Even
    };
    let at = |share: f64| -> u64 {
        match timing {
            Timing::Preview => {
                let target = share * growth as f64;
                let mut previous = (0_u64, 0_f64);
                for &(t, words) in &samples {
                    let words = words as f64;
                    if words >= target {
                        let span = (words - previous.1).max(1e-6);
                        let fraction = ((target - previous.1) / span).clamp(0.0, 1.0);
                        let t = previous.0 as f64 + fraction * (t - previous.0) as f64;
                        return (t as u64).saturating_sub(PREVIEW_LAG_MS);
                    }
                    previous = (t, words);
                }
                previous.0.saturating_sub(PREVIEW_LAG_MS)
            }
            _ => (share * duration as f64) as u64,
        }
    };
    let words = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let t0 = at(i as f64 / n as f64).min(duration);
            let t1 = at((i + 1) as f64 / n as f64).clamp(t0, duration);
            Word {
                t0,
                t1,
                text: (*token).into(),
            }
        })
        .collect();
    Transcript {
        text: text.into(),
        timing,
        words,
    }
}

fn ends_clause(text: &str, clause_words: usize) -> bool {
    let trimmed = text.trim_end_matches(['"', '\'', '”', '’', ')', ']']);
    trimmed.ends_with(['.', '!', '?', ';', ':']) || (clause_words >= 3 && trimmed.ends_with(','))
}

fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
        .max(1)
}

/// Each keyframe's text runs from the start of the clause in progress at its anchor until the
/// next keyframe's start, so the transcript is partitioned in order with nothing repeated or
/// dropped. Speech before the first keyframe belongs to it.
pub fn said_per_keyframe(
    times: &[u64],
    anchors: &[u64],
    transcript: &Transcript,
) -> Vec<Option<String>> {
    if times.is_empty() {
        return Vec::new();
    }
    let mut units: Vec<(u64, u64, &str)> = transcript
        .words
        .iter()
        .map(|w| (w.t0, w.t1, w.text.as_str()))
        .filter(|unit| !unit.2.trim().is_empty())
        .collect();
    units.sort_by_key(|unit| unit.0);
    let mut clauses: Vec<(u64, u64)> = Vec::new();
    let mut previous: Option<(u64, &str)> = None;
    let mut clause_words = 0_usize;
    for &(t0, t1, text) in &units {
        let new_clause = previous.is_none_or(|(end, prev)| {
            ends_clause(prev, clause_words) || t0.saturating_sub(end) >= CLAUSE_GAP_MS
        });
        match clauses.last_mut() {
            Some(clause) if !new_clause => {
                clause.1 = clause.1.max(t1);
                clause_words += word_count(text);
            }
            _ => {
                clauses.push((t0, t1));
                clause_words = word_count(text);
            }
        }
        previous = Some((t1, text));
    }
    let clause_start = |anchor: u64| {
        clauses
            .iter()
            .find(|(t0, t1)| *t0 <= anchor && anchor <= *t1)
            .map_or(anchor, |clause| clause.0)
    };
    let mut starts: Vec<u64> = Vec::with_capacity(times.len());
    for (i, &t) in times.iter().enumerate() {
        let anchor = anchors.get(i).copied().unwrap_or(t);
        let mut start = clause_start(anchor);
        if let Some(&prev) = starts.last()
            && start <= prev
        {
            // Two keyframes in one clause: the later one takes the words from its own moment.
            start = anchor.max(prev + 1);
        }
        starts.push(start);
    }
    let mut text = vec![String::new(); times.len()];
    for (t0, _, unit) in units {
        let unit = unit.trim();
        let mut index = starts
            .iter()
            .rposition(|&start| start <= t0.saturating_add(50))
            .unwrap_or(0);
        let attached = unit.starts_with([',', '.', '!', '?', ';', ':', '’', '\'', ')', ']', '”']);
        if attached && text[index].is_empty() && index > 0 {
            index -= 1;
        }
        let open = text[index].ends_with(['(', '[', '“']);
        if !text[index].is_empty() && !attached && !open {
            text[index].push(' ');
        }
        text[index].push_str(unit);
    }
    if text.iter().all(String::is_empty) && !transcript.text.trim().is_empty() {
        text[0] = transcript.text.trim().to_owned();
    }
    text.into_iter()
        .map(|text| (!text.is_empty()).then_some(text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_growth_places_words_and_never_changes_text() {
        let text = "first look here. then the second panel";
        let timed = transcript(text, None, &[(2_000, 2), (4_000, 3), (9_000, 7)], 10_000);
        assert_eq!(timed.timing, Timing::Preview);
        assert_eq!(timed.text, text);
        assert_eq!(timed.words.len(), 7);
        // Word 3 of 7 is reached at three sevenths of the preview's seven words.
        assert!(timed.words[3].t0 < 4_000 && timed.words[3].t0 > 2_000);
        assert!(timed.words.windows(2).all(|w| w[0].t0 <= w[1].t0));
    }

    #[test]
    fn falls_back_to_even_timing_and_prefers_provider_words() {
        let even = transcript("a b c d", None, &[], 4_000);
        assert_eq!(even.timing, Timing::Even);
        assert_eq!(even.words[2].t0, 2_000);
        let provider = vec![Word {
            t0: 5,
            t1: 9,
            text: "a".into(),
        }];
        assert_eq!(
            transcript("a", Some(provider.clone()), &[], 10).words,
            provider
        );
    }

    #[test]
    fn partitions_clauses_between_keyframes() {
        let timed = transcript(
            "This button is wrong. And this label overlaps.",
            None,
            &[],
            8_000,
        );
        let said = said_per_keyframe(&[1_000, 5_000], &[1_000, 5_000], &timed);
        assert_eq!(said[0].as_deref(), Some("This button is wrong."));
        assert_eq!(said[1].as_deref(), Some("And this label overlaps."));
    }
}
