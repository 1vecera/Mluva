//! Restrained, source-preserving Markdown styling with Unicode character offsets.

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::text;

pub const MAX_WORD_WRAP_RUN: usize = 1_024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkdownSpan {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

impl MarkdownSpan {
    fn new(start: usize, end: usize, style: &str) -> Self {
        Self {
            start,
            end,
            style: style.into(),
        }
    }
}

pub fn needs_character_wrapping(source: &str) -> bool {
    let (mut run, mut paragraph, mut joiner) = (0, 0, false);
    for character in source.chars() {
        if matches!(character, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
            (run, paragraph, joiner) = (0, 0, false);
            continue;
        }
        paragraph += 1;
        joiner |= matches!(character, '\u{2060}' | '\u{feff}');
        if joiner && paragraph > MAX_WORD_WRAP_RUN {
            return true;
        }
        run = if matches!(character, ' ' | '\t') {
            0
        } else {
            run + 1
        };
        if run > MAX_WORD_WRAP_RUN {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy)]
struct Opener {
    marker: (char, usize),
    start: usize,
    end: usize,
}

fn pair(
    spans: &mut Vec<MarkdownSpan>,
    offset: usize,
    open: (usize, usize),
    close: (usize, usize),
    styles: &[&str],
) {
    spans.push(MarkdownSpan::new(
        offset + open.0,
        offset + open.1,
        "syntax",
    ));
    for style in styles {
        spans.push(MarkdownSpan::new(offset + open.1, offset + close.0, style));
    }
    spans.push(MarkdownSpan::new(
        offset + close.0,
        offset + close.1,
        "syntax",
    ));
}

fn inline_spans(source: &[char], offset: usize, spans: &mut Vec<MarkdownSpan>) {
    let mut ticks: BTreeMap<usize, VecDeque<(usize, usize)>> = BTreeMap::new();
    let mut index = 0;
    while index < source.len() {
        if source[index] != '`' {
            index += 1;
            continue;
        }
        let start = index;
        while index < source.len() && source[index] == '`' {
            index += 1;
        }
        ticks
            .entry(index - start)
            .or_default()
            .push_back((start, index));
    }
    let mut openers = Vec::<Opener>::new();
    let mut matching: BTreeMap<(char, usize), Vec<Opener>> = BTreeMap::new();
    index = 0;
    while index < source.len() {
        let character = source[index];
        if character == '\\'
            && source
                .get(index + 1)
                .is_some_and(|next| "\\`*_{}[]()#+.!>~-".contains(*next))
        {
            spans.push(MarkdownSpan::new(
                offset + index,
                offset + index + 1,
                "syntax",
            ));
            index += 2;
            continue;
        }
        if !matches!(character, '`' | '*' | '_') {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while source.get(end) == Some(&character) {
            end += 1;
        }
        let length = end - index;
        if character == '`' {
            let candidates = ticks.entry(length).or_default();
            while candidates.front().is_some_and(|&(start, _)| start <= index) {
                candidates.pop_front();
            }
            if let Some(close) = candidates.pop_front() {
                pair(spans, offset, (index, end), close, &["code"]);
                index = close.1;
            } else {
                index = end;
            }
            continue;
        }
        if length > 3 {
            index = end;
            continue;
        }
        let marker = (character, length);
        let before = index
            .checked_sub(1)
            .map(|position| source[position])
            .unwrap_or(' ');
        let after = source.get(end).copied().unwrap_or(' ');
        let mut can_open = !text::whitespace(after);
        let mut can_close = !text::whitespace(before);
        if length == 1 {
            can_open &= !text::word_character(before);
            can_close &= !text::word_character(after);
        }
        if can_close && matching.get(&marker).is_some_and(|stack| !stack.is_empty()) {
            let opener = *matching.get(&marker).unwrap().last().unwrap();
            while let Some(removed) = openers.pop() {
                matching.get_mut(&removed.marker).unwrap().pop();
                if removed.start == opener.start {
                    break;
                }
            }
            let styles: &[&str] = match length {
                3 => &["strong", "em"],
                2 => &["strong"],
                _ => &["em"],
            };
            pair(
                spans,
                offset,
                (opener.start, opener.end),
                (index, end),
                styles,
            );
        } else if can_open {
            let opener = Opener {
                marker,
                start: index,
                end,
            };
            openers.push(opener);
            matching.entry(marker).or_default().push(opener);
        }
        index = end;
    }
}

fn fence(content: &[char]) -> Option<(char, usize, &[char])> {
    let indent = content
        .iter()
        .take_while(|&&character| character == ' ')
        .count();
    if indent > 3 {
        return None;
    }
    let character = *content.get(indent)?;
    if !matches!(character, '`' | '~') {
        return None;
    }
    let length = content[indent..]
        .iter()
        .take_while(|&&next| next == character)
        .count();
    (length >= 3).then_some((character, length, &content[indent + length..]))
}

fn block_prefix(content: &[char]) -> Option<(usize, String)> {
    let indent = content
        .iter()
        .take_while(|&&character| character == ' ')
        .count();
    if indent <= 3 {
        let hashes = content[indent..]
            .iter()
            .take_while(|&&character| character == '#')
            .count();
        if (1..=6).contains(&hashes) {
            let mut end = indent + hashes;
            let start = end;
            while content
                .get(end)
                .is_some_and(|character| matches!(character, ' ' | '\t'))
            {
                end += 1;
            }
            if end > start
                && content
                    .get(end)
                    .is_some_and(|&character| !text::whitespace(character))
            {
                return Some((end, format!("h{hashes}")));
            }
        }
        if content.get(indent) == Some(&'>') {
            let mut end = indent + 1;
            if content
                .get(end)
                .is_some_and(|character| matches!(character, ' ' | '\t'))
            {
                end += 1;
            }
            return Some((end, "quote".into()));
        }
    }
    let indent = content
        .iter()
        .take_while(|&&character| matches!(character, ' ' | '\t'))
        .count();
    let mut end = indent;
    if content
        .get(end)
        .is_some_and(|character| matches!(character, '-' | '+' | '*'))
    {
        end += 1;
    } else {
        while content
            .get(end)
            .is_some_and(|&character| text::decimal(character))
        {
            end += 1;
        }
        if end == indent
            || !content
                .get(end)
                .is_some_and(|character| matches!(character, '.' | ')'))
        {
            return None;
        }
        end += 1;
    }
    let start = end;
    while content
        .get(end)
        .is_some_and(|character| matches!(character, ' ' | '\t'))
    {
        end += 1;
    }
    (end > start
        && content
            .get(end)
            .is_some_and(|&character| !text::whitespace(character)))
    .then_some((0, "list".into()))
}

fn line_separator(character: char) -> bool {
    matches!(
        character,
        '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

pub fn markdown_spans(source: &str) -> Vec<MarkdownSpan> {
    let characters = source.chars().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut offset = 0;
    let mut active_fence = None;
    while offset < characters.len() {
        let mut line_end = offset;
        while line_end < characters.len() && !line_separator(characters[line_end]) {
            line_end += 1;
        }
        if line_end < characters.len() {
            if characters[line_end] == '\r' && characters.get(line_end + 1) == Some(&'\n') {
                line_end += 1;
            }
            line_end += 1;
        }
        let mut content_end = line_end;
        while content_end > offset
            && matches!(
                characters[content_end - 1],
                '\r' | '\n' | '\u{2028}' | '\u{2029}'
            )
        {
            content_end -= 1;
        }
        let content = &characters[offset..content_end];
        let marker = fence(content);
        if let Some((character, length)) = active_fence {
            if marker.is_some_and(|(next, count, tail)| {
                next == character
                    && count >= length
                    && tail.iter().all(|&character| text::whitespace(character))
            }) {
                spans.push(MarkdownSpan::new(offset, content_end, "syntax"));
                active_fence = None;
            } else {
                spans.push(MarkdownSpan::new(offset, content_end, "code"));
            }
        } else if let Some((character, length, _)) = marker {
            active_fence = Some((character, length));
            spans.push(MarkdownSpan::new(offset, line_end, "syntax"));
        } else {
            let mut start = 0;
            if let Some((prefix, style)) = block_prefix(content) {
                start = prefix;
                if prefix > 0 {
                    spans.push(MarkdownSpan::new(offset, offset + prefix, "syntax"));
                }
                spans.push(MarkdownSpan::new(offset + prefix, content_end, &style));
            }
            inline_spans(&content[start..], offset + start, &mut spans);
        }
        offset = line_end;
    }
    spans
}

/// Reading and measurement use a projection; editing continues to own the unchanged source.
pub fn visible_markdown(source: &str, spans: &[MarkdownSpan]) -> (String, Vec<MarkdownSpan>) {
    let source = source.chars().collect::<Vec<_>>();
    let mut hidden = vec![false; source.len()];
    for span in spans.iter().filter(|span| span.style == "syntax") {
        let start = span.start.min(hidden.len());
        let end = span.end.min(hidden.len()).max(start);
        hidden[start..end].fill(true);
    }
    let mut offsets = Vec::with_capacity(source.len() + 1);
    offsets.push(0);
    let mut visible = String::new();
    for (character, hidden) in source.iter().zip(hidden) {
        offsets.push(offsets.last().unwrap() + usize::from(!hidden));
        if !hidden {
            visible.push(*character);
        }
    }
    let styles = spans
        .iter()
        .filter(|span| {
            span.style != "syntax"
                && span.end < offsets.len()
                && span.start <= span.end
                && offsets[span.start] != offsets[span.end]
        })
        .map(|span| MarkdownSpan::new(offsets[span.start], offsets[span.end], &span.style))
        .collect();
    (visible, styles)
}
