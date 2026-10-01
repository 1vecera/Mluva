//! Capture-ordered terminal cleanup values consumed by recording completion.
//!
//! The asynchronous segment scheduler owns candidate validation. Completion binds
//! its terminal snapshot to the exact session, model and recognition string.

use mluva_core::text;
use serde::{Deserialize, Serialize};

pub const MAX_SEGMENT_CHARACTERS: usize = 8_000;
pub const MAX_RESPONSE_CHARACTERS: usize = 8_000;

pub fn cleanup_prompt(text: &str, instructions: &str) -> String {
    format!(
        "{instructions} Preserve every fact, number, name, URL, path, identifier, command, and negation. Return only the cleaned text.\n\nDICTATION:\n{text}"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentCleanupFailure {
    Cancelled,
    InputTooLarge,
    MalformedOutput,
    OutputTooLarge,
    Processing,
    Provider,
    Safety,
    SkippedCapacity,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentCleanupTerminalSegment {
    pub identifier: String,
    pub sequence: i64,
    pub raw_text: String,
    pub selected_text: String,
    pub failure: Option<SegmentCleanupFailure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SegmentCleanupTerminalSnapshot {
    pub session_identifier: String,
    pub provider_identifier: String,
    pub model_identifier: String,
    pub segments: Vec<SegmentCleanupTerminalSegment>,
    pub stop_drain_seconds: f64,
}

impl SegmentCleanupTerminalSnapshot {
    pub fn raw_text(&self) -> String {
        join_segments(
            self.segments
                .iter()
                .map(|segment| segment.raw_text.as_str()),
        )
    }
    pub fn selected_text(&self) -> String {
        join_segments(
            self.segments
                .iter()
                .map(|segment| segment.selected_text.as_str()),
        )
    }
    pub fn successful_segments(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| segment.failure.is_none())
            .count()
    }
    pub fn failed_segments(&self) -> usize {
        self.segments.len() - self.successful_segments()
    }
}

fn join_segments<'a>(segments: impl Iterator<Item = &'a str>) -> String {
    segments
        .map(text::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
