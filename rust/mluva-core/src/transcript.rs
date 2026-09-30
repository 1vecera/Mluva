//! Deterministic spoken commands, retaining the recognition string as the immutable caller-owned source.

use std::sync::LazyLock;

use crate::text::{trim, whitespace, word_character};
use regex::{Captures, Regex};

const PARAGRAPH: &str = "\0mluva-paragraph\0";
const LINE: &str = "\0mluva-line\0";
const SCRATCH: &str = "\0mluva-scratch\0";

static COMMANDS: LazyLock<Vec<(Regex, String)>> = LazyLock::new(|| {
    [
        (r"new\s+paragraph", format!(" {PARAGRAPH} ")),
        (r"new\s+line", format!(" {LINE} ")),
        (r"scratch\s+that", format!(" {SCRATCH} ")),
        (r"question\s+mark", "?".into()),
        (r"exclamation\s+(?:mark|point)", "!".into()),
        (r"semi[ -]?colon", ";".into()),
        (r"full\s+stop", ".".into()),
        (r"period", ".".into()),
        (r"comma", ",".into()),
        (r"colon", ":".into()),
    ]
    .into_iter()
    .map(|(pattern, replacement)| {
        // Python's case-insensitive ASCII letters also admit dotted/dotless Turkish I.
        let pattern = pattern.replace('i', "[iİı]");
        (
            Regex::new(&format!("(?i){pattern}")).expect("fixed command expression"),
            replacement,
        )
    })
    .collect()
});
static BEFORE_PUNCTUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+([,.;:?!])").unwrap());
static PARAGRAPH_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]*\x00mluva-paragraph\x00[ \t]*").unwrap());
static LINE_SPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]*\x00mluva-line\x00[ \t]*").unwrap());
static BEFORE_NEWLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]+\n").unwrap());
static AFTER_NEWLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n[ \t]+").unwrap());
static EXTRA_LINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
static EXTRA_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]{2,}").unwrap());

/// Apply only explicit English structural phrases with the reference's Unicode word boundaries and spacing.
pub fn normalize_spoken_structure(raw: &str) -> String {
    let mut text = raw
        .split(whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    for (pattern, replacement) in COMMANDS.iter() {
        text = pattern
            .replace_all(&text, |captures: &Captures<'_>| {
                let matched = captures.get(0).unwrap();
                let before = text[..matched.start()].chars().next_back();
                let after = text[matched.end()..].chars().next();
                if before.is_some_and(word_character) || after.is_some_and(word_character) {
                    matched.as_str().to_owned()
                } else {
                    replacement.clone()
                }
            })
            .into_owned();
    }
    while let Some(position) = text.find(SCRATCH) {
        let prefix = &text[..position];
        let punctuation = prefix.rfind(['.', '?', '!']);
        let structural = [prefix.rfind(LINE), prefix.rfind(PARAGRAPH)]
            .into_iter()
            .flatten()
            .max();
        let retained = match (structural, punctuation) {
            (Some(line), boundary) if boundary.is_none_or(|punctuation| line > punctuation) => {
                line + if prefix[line..].starts_with(LINE) {
                    LINE.len()
                } else {
                    PARAGRAPH.len()
                }
            }
            (_, Some(punctuation)) => punctuation + 1,
            _ => 0,
        };
        text = format!(
            "{} {}",
            &prefix[..retained],
            &text[position + SCRATCH.len()..]
        );
    }
    text = BEFORE_PUNCTUATION.replace_all(&text, "$1").into_owned();
    let mut spaced = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        spaced.push(character);
        if let Some(next) = characters.peek()
            && ((",;:".contains(character) && !whitespace(*next))
                || (".?!".contains(character) && next.is_ascii_alphanumeric()))
        {
            spaced.push(' ');
        }
    }
    text = PARAGRAPH_SPACE.replace_all(&spaced, "\n\n").into_owned();
    text = LINE_SPACE.replace_all(&text, "\n").into_owned();
    text = BEFORE_NEWLINE.replace_all(&text, "\n").into_owned();
    text = AFTER_NEWLINE.replace_all(&text, "\n").into_owned();
    text = EXTRA_LINES.replace_all(&text, "\n\n").into_owned();
    text = EXTRA_SPACES.replace_all(&text, " ").into_owned();
    trim(&text).to_owned()
}
