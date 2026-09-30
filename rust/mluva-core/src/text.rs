//! Text primitives shared by persisted settings and source-preserving document operations.

use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct WordRanges {
    ranges: Vec<(u32, u32)>,
}

static WORD_RANGES: LazyLock<WordRanges> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/word-ranges.json"))
        .expect("frozen Unicode word properties")
});

/// Keep the released recognizer's Unicode alphanumeric boundaries stable across regex-library updates.
pub fn word_character(character: char) -> bool {
    if character.is_ascii() {
        return character.is_ascii_alphanumeric() || character == '_';
    }
    let code = u32::from(character);
    let ranges = &WORD_RANGES.ranges;
    let index = ranges.partition_point(|(_, last)| *last < code);
    ranges
        .get(index)
        .is_some_and(|(first, last)| (*first..=*last).contains(&code))
}

/// The reference also treats the four Unicode information separators as whitespace.
pub fn whitespace(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

pub fn trim(value: &str) -> &str {
    value.trim_matches(whitespace)
}
