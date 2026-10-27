//! Tide windows for the harbour planner: when a berth can take a vessel of a given draught.

use std::time::Duration;

use crate::tide::{Reading, Table};

/// The least water a vessel needs under its keel, in metres.
const CLEARANCE: f64 = 0.6;

/// A span of time in which a berth has enough water.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub opens: Duration,
    pub closes: Duration,
}

impl Window {
    pub fn length(&self) -> Duration {
        self.closes.saturating_sub(self.opens)
    }
}

/// Every window in `table` deep enough for `draught`, merged where they touch.
pub fn windows(table: &Table, draught: f64) -> Vec<Window> {
    let needed = draught + CLEARANCE;
    let mut found: Vec<Window> = vec![];
    for pair in table.readings().windows(2) {
        let [a, b] = pair else { continue };
        if a.depth < needed && b.depth < needed {
            continue;
        }
        let opens = crossing(a, b, needed).unwrap_or(a.at);
        let closes = crossing(b, a, needed).unwrap_or(b.at);
        match found.last_mut() {
            Some(last) if last.closes >= opens => last.closes = closes,
            _ => found.push(Window { opens, closes }),
        }
    }
    found
}

/// When the depth between two readings passes `level`, by linear interpolation.
fn crossing(from: &Reading, to: &Reading, level: f64) -> Option<Duration> {
    let rise = to.depth - from.depth;
    if rise.abs() < f64::EPSILON || (level - from.depth) / rise < 0. {
        return None;
    }
    let share = ((level - from.depth) / rise).clamp(0., 1.);
    let span = to.at.as_secs_f64() - from.at.as_secs_f64();
    Some(Duration::from_secs_f64(from.at.as_secs_f64() + share * span))
}
