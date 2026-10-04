//! Text primitives shared by persisted settings and source-preserving document operations.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct WordRanges {
    ranges: Vec<(u32, u32)>,
}

static WORD_RANGES: LazyLock<WordRanges> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/word-ranges.json"))
        .expect("frozen Unicode word properties")
});

#[derive(Deserialize)]
struct CaseProperties {
    alphabetic: Vec<(u32, u32)>,
    uppercase: Vec<(u32, u32)>,
    lowercase: Vec<(u32, u32)>,
    cased: Vec<(u32, u32)>,
    case_ignorable: Vec<(u32, u32)>,
    decimal: Vec<(u32, u32)>,
    upper_mapping: BTreeMap<u32, String>,
    title_mapping: BTreeMap<u32, String>,
    lower_mapping: BTreeMap<u32, String>,
    ignore_case_groups: Vec<String>,
}

static CASE_PROPERTIES: LazyLock<CaseProperties> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/case-properties.json"))
        .expect("frozen Unicode case properties")
});

static IGNORE_CASE_LITERALS: LazyLock<BTreeMap<char, String>> = LazyLock::new(|| {
    let mut result = BTreeMap::new();
    for group in &CASE_PROPERTIES.ignore_case_groups {
        let expression = format!(
            "[{}]",
            group
                .chars()
                .map(|character| regex::escape(&character.to_string()))
                .collect::<String>()
        );
        for character in group.chars() {
            result.insert(character, expression.clone());
        }
    }
    result
});

pub(crate) fn ignore_case_literal(character: char) -> String {
    IGNORE_CASE_LITERALS
        .get(&character)
        .cloned()
        .unwrap_or_else(|| regex::escape(&character.to_string()))
}

fn in_ranges(character: char, ranges: &[(u32, u32)]) -> bool {
    let code = u32::from(character);
    let index = ranges.partition_point(|(_, last)| *last < code);
    ranges
        .get(index)
        .is_some_and(|(first, last)| (*first..=*last).contains(&code))
}

pub fn alphabetic(character: char) -> bool {
    in_ranges(character, &CASE_PROPERTIES.alphabetic)
}

pub fn uppercase(character: char) -> bool {
    in_ranges(character, &CASE_PROPERTIES.uppercase)
}

pub fn lowercase(character: char) -> bool {
    in_ranges(character, &CASE_PROPERTIES.lowercase)
}

pub fn decimal(character: char) -> bool {
    in_ranges(character, &CASE_PROPERTIES.decimal)
}

pub(crate) fn decimal_value(character: char) -> Option<u32> {
    let code = u32::from(character);
    let index = CASE_PROPERTIES
        .decimal
        .partition_point(|(_, last)| *last < code);
    CASE_PROPERTIES
        .decimal
        .get(index)
        .filter(|(first, last)| (*first..=*last).contains(&code))
        .map(|(first, _)| (code - first) % 10)
}

pub(crate) fn decimal_expression() -> String {
    let mut result = String::from("[");
    for &(first, last) in &CASE_PROPERTIES.decimal {
        result.push_str(&format!(r"\u{{{first:X}}}-\u{{{last:X}}}"));
    }
    result.push(']');
    result
}

/// Whole-string casing keeps expansions such as sharp S and the final Greek sigma.
pub fn upper(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if let Some(mapped) = CASE_PROPERTIES.upper_mapping.get(&u32::from(character)) {
            result.push_str(mapped);
        } else {
            result.push(character);
        }
    }
    result
}

pub fn lower(value: &str) -> String {
    lower_or_title(value, false)
}

/// Released title casing starts a word after each uncased character, including `_`.
pub fn title(value: &str) -> String {
    lower_or_title(value, true)
}

fn lower_or_title(value: &str, title: bool) -> String {
    let characters: Vec<_> = value.chars().collect();
    // Compute both contexts in linear time, including long combining-mark sequences.
    let mut following_cased = vec![false; characters.len()];
    let mut next_cased = false;
    for (index, &character) in characters.iter().enumerate().rev() {
        following_cased[index] = next_cased;
        if !in_ranges(character, &CASE_PROPERTIES.case_ignorable) {
            next_cased = in_ranges(character, &CASE_PROPERTIES.cased);
        }
    }
    let mut preceding_cased = false;
    let mut previous_cased = false;
    let mut result = String::with_capacity(value.len());
    for (index, &character) in characters.iter().enumerate() {
        if title && !previous_cased {
            if let Some(mapped) = CASE_PROPERTIES.title_mapping.get(&u32::from(character)) {
                result.push_str(mapped);
            } else {
                result.push(character);
            }
        } else if character == 'Σ' {
            result.push(if preceding_cased && !following_cased[index] {
                'ς'
            } else {
                'σ'
            });
        } else if let Some(mapped) = CASE_PROPERTIES.lower_mapping.get(&u32::from(character)) {
            result.push_str(mapped);
        } else {
            result.push(character);
        }
        if !in_ranges(character, &CASE_PROPERTIES.case_ignorable) {
            preceding_cased = in_ranges(character, &CASE_PROPERTIES.cased);
        }
        previous_cased = in_ranges(character, &CASE_PROPERTIES.cased);
    }
    result
}

/// Keep the released recognizer's Unicode alphanumeric boundaries stable across regex-library updates.
pub fn word_character(character: char) -> bool {
    if character.is_ascii() {
        return character.is_ascii_alphanumeric() || character == '_';
    }
    in_ranges(character, &WORD_RANGES.ranges)
}

/// The reference also treats the four Unicode information separators as whitespace.
pub fn whitespace(character: char) -> bool {
    character.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&character)
}

pub fn trim(value: &str) -> &str {
    value.trim_matches(whitespace)
}
