//! Bounded local and generated labels that cannot replace source data or manual names.

use regex::Regex;
use std::sync::LazyLock;

use crate::text;

pub const MAX_TITLE_CHARACTERS: usize = 64;

static HESITATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:(?:um|uh|okay|so|well)[, .…]+)+").unwrap());
static SENTENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?](?:\s|$)").unwrap());

pub fn fallback_title(value: &str) -> String {
    let prepared = value
        .split(text::whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let prepared = HESITATION.replace(&prepared, "");
    let sentence = SENTENCE.splitn(&prepared, 2).next().unwrap_or("");
    let title: String = sentence
        .split(text::whitespace)
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TITLE_CHARACTERS)
        .collect();
    let title = title.trim_end_matches([' ', ',', ';', ':', '—', '-']);
    if title.is_empty() {
        "Untitled conversation".into()
    } else {
        title.into()
    }
}
