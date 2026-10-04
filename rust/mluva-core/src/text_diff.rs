//! Bounded word revisions, retaining unchanged interior words and source character offsets.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::text;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextChange {
    pub old_start: usize,
    pub old_end: usize,
    pub new_start: usize,
    pub new_end: usize,
}

struct Token {
    value: String,
    start: usize,
}

fn tokens(source: &[char], start: usize, end: usize) -> Vec<Token> {
    let category = |character| {
        if text::whitespace(character) {
            0
        } else if text::word_character(character) {
            1
        } else {
            2
        }
    };
    let mut result = Vec::new();
    let mut index = start;
    while index < end {
        let beginning = index;
        let kind = category(source[index]);
        index += 1;
        while index < end && category(source[index]) == kind {
            index += 1;
        }
        result.push(Token {
            value: source[beginning..index].iter().collect(),
            start: beginning,
        });
    }
    result
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Match {
    old: usize,
    new: usize,
    length: usize,
}

fn longest(
    before: &[Token],
    positions: &HashMap<&str, Vec<usize>>,
    old: std::ops::Range<usize>,
    new: std::ops::Range<usize>,
) -> Match {
    let mut best = Match {
        old: old.start,
        new: new.start,
        length: 0,
    };
    let mut previous = HashMap::new();
    for (index, token) in before.iter().enumerate().take(old.end).skip(old.start) {
        let mut next = HashMap::new();
        if let Some(candidates) = positions.get(token.value.as_str()) {
            for &position in candidates {
                if position < new.start {
                    continue;
                }
                if position >= new.end {
                    break;
                }
                let length = position
                    .checked_sub(1)
                    .and_then(|position| previous.get(&position).copied())
                    .unwrap_or(0)
                    + 1;
                next.insert(position, length);
                if length > best.length {
                    best = Match {
                        old: index + 1 - length,
                        new: position + 1 - length,
                        length,
                    };
                }
            }
        }
        previous = next;
    }
    best
}

fn matching_blocks(before: &[Token], after: &[Token]) -> Vec<Match> {
    let mut positions = HashMap::<&str, Vec<usize>>::new();
    for (index, token) in after.iter().enumerate() {
        positions.entry(&token.value).or_default().push(index);
    }
    let mut pending = vec![(0..before.len(), 0..after.len())];
    let mut matches = Vec::new();
    while let Some((old, new)) = pending.pop() {
        let found = longest(before, &positions, old.clone(), new.clone());
        if found.length == 0 {
            continue;
        }
        if old.start < found.old && new.start < found.new {
            pending.push((old.start..found.old, new.start..found.new));
        }
        let old_end = found.old + found.length;
        let new_end = found.new + found.length;
        if old_end < old.end && new_end < new.end {
            pending.push((old_end..old.end, new_end..new.end));
        }
        matches.push(found);
    }
    matches.sort();
    let mut merged: Vec<Match> = Vec::new();
    for found in matches {
        if let Some(previous) = merged.last_mut()
            && previous.old + previous.length == found.old
            && previous.new + previous.length == found.new
        {
            previous.length += found.length;
        } else {
            merged.push(found);
        }
    }
    merged.push(Match {
        old: before.len(),
        new: after.len(),
        length: 0,
    });
    merged
}

pub fn text_changes(previous: &str, current: &str) -> Vec<TextChange> {
    if previous == current {
        return Vec::new();
    }
    let old = previous.chars().collect::<Vec<_>>();
    let new = current.chars().collect::<Vec<_>>();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(before, after)| before == after)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(before, after)| before == after)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    let before = tokens(&old, prefix, old_end);
    let after = tokens(&new, prefix, new_end);
    if before.len() + after.len() > 2_000 {
        return vec![TextChange {
            old_start: prefix,
            old_end,
            new_start: prefix,
            new_end,
        }];
    }
    let old_offsets = before
        .iter()
        .map(|token| token.start)
        .chain([old_end])
        .collect::<Vec<_>>();
    let new_offsets = after
        .iter()
        .map(|token| token.start)
        .chain([new_end])
        .collect::<Vec<_>>();
    let (mut a, mut b) = (0, 0);
    let mut changes = Vec::new();
    for found in matching_blocks(&before, &after) {
        if a < found.old || b < found.new {
            changes.push(TextChange {
                old_start: old_offsets[a],
                old_end: old_offsets[found.old],
                new_start: new_offsets[b],
                new_end: new_offsets[found.new],
            });
        }
        a = found.old + found.length;
        b = found.new + found.length;
    }
    changes
}
