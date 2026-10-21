//! Choose the moments an agent should see: every mark, then window changes, settled screen
//! changes and spoken cues, each only when it shows something new.
//!
//! The scoring, novelty and spoken-cue rules are adapted from Scrawl's `src/keyframes.rs` at
//! f2ad4e8c75b6cc6171c38d85cd8755e4d9fe03fe. Copyright (c) 2026 Kit Langton. MIT License;
//! see `resources/scrawl-LICENSE`. Scrawl selects from every video frame; Mluva selects from the
//! sparse frames it captured while recording.
use crate::{
    model::{
        BASE_KEYFRAMES, Capture, Frame, Keyframe, KeyframeReason, MAX_KEYFRAMES, Mark, ScreenEvent,
        Timing, Transcript, keyframe_budget,
    },
    sketch::marks_of,
    timing::said_per_keyframe,
};

pub const THUMB_WIDTH: usize = 64;
pub const THUMB_HEIGHT: usize = 36;
const THUMB_PIXELS: usize = THUMB_WIDTH * THUMB_HEIGHT;
/// Two keyframes closer than this repeat one moment.
const FRAME_GAP_MS: u64 = 750;
/// A cue, change or window frame must differ from every chosen keyframe by this much.
const NOVELTY: f32 = 0.03;
/// A screen that changed by more than this between samples is a new view.
const SCENE_CHANGE: f32 = 0.08;
/// Marks share a keyframe while their screen changed by less than this.
const SAME_SCREEN: f32 = 0.05;
/// Marks share a keyframe only when drawn as one gesture: each begun within this long of the
/// previous one ending. Separate gestures keep separate keyframes, so each is matched to the
/// words said while it was drawn. (Scrawl groups every mark on an unchanged screen.)
const GESTURE_GAP_MS: u64 = 2_500;
/// A group of marks closes past this span or this many strokes, so one frame stays legible.
const GROUP_MS: u64 = 20_000;
const GROUP_STROKES: usize = 12;
/// A window change is shown by the first frame this soon after it.
const SETTLE_WINDOW_MS: u64 = 3_000;

/// Grey thumbnail of Cairo ARGB32 (little-endian BGRA) pixels, sampled at a fixed grid.
pub fn luma(data: &[u8], stride: usize, width: usize, height: usize) -> Vec<f32> {
    let mut thumbnail = vec![0.0; THUMB_PIXELS];
    if width == 0 || height == 0 {
        return thumbnail;
    }
    for y in 0..THUMB_HEIGHT {
        let sy = ((2 * y + 1) * height / (2 * THUMB_HEIGHT)).min(height - 1);
        for x in 0..THUMB_WIDTH {
            let sx = ((2 * x + 1) * width / (2 * THUMB_WIDTH)).min(width - 1);
            let offset = sy * stride + sx * 4;
            if let Some(pixel) = data.get(offset..offset + 3) {
                thumbnail[y * THUMB_WIDTH + x] = (0.0722 * pixel[0] as f32
                    + 0.7152 * pixel[1] as f32
                    + 0.2126 * pixel[2] as f32)
                    / 255.0;
            }
        }
    }
    thumbnail
}

fn thumbnail_diff(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != THUMB_PIXELS || b.len() != THUMB_PIXELS {
        return 1.0;
    }
    (a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / THUMB_PIXELS as f32).clamp(0.0, 1.0)
}

/// Global difference, or the fourth-most-changed of 8×6 blocks, so a moving pointer never
/// counts as a new view while a real change across a region does.
pub fn screen_diff(a: &[f32], b: &[f32]) -> f32 {
    let global = thumbnail_diff(a, b);
    if a.len() != THUMB_PIXELS || b.len() != THUMB_PIXELS {
        return global;
    }
    const COLS: usize = 8;
    const ROWS: usize = 6;
    const BW: usize = THUMB_WIDTH / COLS;
    const BH: usize = THUMB_HEIGHT / ROWS;
    let mut top = [0.0_f32; 4];
    for by in 0..ROWS {
        for bx in 0..COLS {
            let mut sum = 0.0_f32;
            for y in (by * BH)..((by + 1) * BH) {
                for x in (bx * BW)..((bx + 1) * BW) {
                    let i = y * THUMB_WIDTH + x;
                    sum += (a[i] - b[i]).abs();
                }
            }
            let block = sum / (BW * BH) as f32;
            if block > top[3] {
                top[3] = block;
                top.sort_by(|x, y| y.total_cmp(x));
            }
        }
    }
    global.max((top[3] * 1.4).clamp(0.0, 1.0))
}

/// Words that point at the screen, in English and Czech. Strong ones are deliberate cues; the
/// rest are as often filler, so they rank with ordinary changes.
fn spoken_cue(word: &str) -> Option<(f32, bool)> {
    let word = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    let (score, strong) = match word.as_str() {
        "now" | "look" | "watch" | "teď" | "koukni" | "podívej" | "sleduj" => (58.0, true),
        "here" | "tady" | "tu" | "zde" => (56.0, true),
        "see" | "there" | "this" | "these" | "tohle" | "tenhle" | "tahle" | "tam" => (44.0, false),
        "that" | "those" | "tamto" | "tyhle" => (42.0, false),
        _ => return None,
    };
    Some((score, strong))
}

struct Candidate {
    frame: usize,
    span: (u64, u64),
    score: f32,
    reason: KeyframeReason,
    marks: Vec<usize>,
    event: String,
}

/// The frame of `screen` that best shows the moment `t`: the first one at or after it within
/// `after_ms`, else the latest one before it, else any frame on that screen.
fn frame_near(
    frames: &[Frame],
    order: &[usize],
    screen: &str,
    t: u64,
    after_ms: u64,
) -> Option<usize> {
    let on: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| frames[i].screen == screen)
        .collect();
    on.iter()
        .copied()
        .find(|&i| frames[i].t >= t && frames[i].t <= t + after_ms)
        .or_else(|| on.iter().copied().rev().find(|&i| frames[i].t <= t))
        .or_else(|| on.first().copied())
}

/// Marks in drawing order, grouped while drawn as one gesture on one unchanged screen with no
/// window change between them; then neighbours merge, most alike first, until they fit `budget`.
/// Each mark's frame is the clean one taken as it began.
fn group_marks(
    capture: &Capture,
    order: &[usize],
    marks: &[Mark],
    budget: usize,
) -> Vec<(Vec<usize>, usize)> {
    let frames = &capture.frames;
    let at: Vec<Option<usize>> = marks
        .iter()
        .map(|mark| frame_near(frames, order, &mark.screen, mark.t0, 2_000))
        .collect();
    let diff = |a: Option<usize>, b: Option<usize>| match (a, b) {
        (Some(a), Some(b)) => screen_diff(&frames[a].luma, &frames[b].luma),
        _ => 1.0,
    };
    let changed_between = |from: u64, to: u64| {
        capture
            .events
            .iter()
            .any(|event| event.t() > from && event.t() < to)
    };
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, mark) in marks.iter().enumerate() {
        if let Some(group) = groups.last_mut() {
            let last = *group.last().unwrap();
            let first = &marks[group[0]];
            let strokes: usize = group.iter().map(|&j| marks[j].strokes.len()).sum();
            if marks[last].screen == mark.screen
                && !changed_between(marks[last].t1, mark.t0)
                && mark.t0.saturating_sub(marks[last].t1) <= GESTURE_GAP_MS
                && mark.t1.saturating_sub(first.t0) <= GROUP_MS
                && strokes + mark.strokes.len() <= GROUP_STROKES
                && diff(at[last], at[i]) < SAME_SCREEN
            {
                group.push(i);
                continue;
            }
        }
        groups.push(vec![i]);
    }
    while groups.len() > budget.max(1) {
        // Prefer merging neighbours on one screen; their ink can share a frame.
        let pick = (0..groups.len() - 1)
            .map(|k| {
                let (a, b) = (*groups[k].last().unwrap(), groups[k + 1][0]);
                let same = marks[a].screen == marks[b].screen;
                (k, !same, diff(at[a], at[b]))
            })
            .min_by(|x, y| x.1.cmp(&y.1).then(x.2.total_cmp(&y.2)))
            .map(|(k, ..)| k)
            .unwrap();
        let next = groups.remove(pick + 1);
        groups[pick].extend(next);
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let last = *group.last().unwrap();
            at[last].map(|frame| (group, frame))
        })
        .collect()
}

fn describe_event(event: &ScreenEvent) -> String {
    match event {
        ScreenEvent::Window { class, .. } if !class.is_empty() => format!("switched to {class}"),
        ScreenEvent::Window { .. } => "switched window".into(),
        ScreenEvent::Workspace { name, .. } => format!("workspace {name}"),
    }
}

/// Select keyframes, in time order and numbered from 1, with the speech each one covers.
pub fn select(capture: &Capture, transcript: &Transcript) -> (Vec<Mark>, Vec<Keyframe>) {
    let marks = marks_of(&capture.strokes);
    let frames = &capture.frames;
    if frames.is_empty() {
        return (marks, Vec::new());
    }
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by_key(|&i| (frames[i].t, i));
    let budget = keyframe_budget(capture.duration_ms);
    let mut candidates: Vec<Candidate> = group_marks(capture, &order, &marks, MAX_KEYFRAMES)
        .into_iter()
        .map(|(group, frame)| {
            let (first, last) = (&marks[group[0]], &marks[*group.last().unwrap()]);
            let pins_only = group.iter().all(|&i| marks[i].pin.is_some());
            Candidate {
                frame,
                span: (first.t0, last.t1),
                score: 100.0,
                reason: if pins_only {
                    KeyframeReason::Pin
                } else {
                    KeyframeReason::Mark
                },
                event: group
                    .iter()
                    .map(|&i| marks[i].description.clone())
                    .collect::<Vec<_>>()
                    .join("; "),
                marks: group,
            }
        })
        .collect();
    for event in &capture.events {
        let t = event.t();
        if let Some(&frame) = order
            .iter()
            .find(|&&i| frames[i].t >= t && frames[i].t <= t + SETTLE_WINDOW_MS)
        {
            candidates.push(Candidate {
                frame,
                span: (t, frames[frame].t),
                score: 70.0,
                reason: KeyframeReason::Window,
                marks: Vec::new(),
                event: describe_event(event),
            });
        }
    }
    // A run of changing samples on one screen is one transition; its last frame is the view.
    for screen in capture.screens.iter().map(|s| s.name.as_str()) {
        let on: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| frames[i].screen == screen)
            .collect();
        let mut transition: Option<(usize, u64, f32)> = None;
        let push = |candidates: &mut Vec<Candidate>, (frame, start, diff): (usize, u64, f32)| {
            candidates.push(Candidate {
                frame,
                span: (start, frames[frame].t),
                score: 40.0 + 40.0 * diff,
                reason: KeyframeReason::SceneChange,
                marks: Vec::new(),
                event: "the screen changed".into(),
            })
        };
        for pair in on.windows(2) {
            let diff = screen_diff(&frames[pair[0]].luma, &frames[pair[1]].luma);
            if diff > SCENE_CHANGE {
                let start = transition.map_or(frames[pair[0]].t, |(_, t, _)| t);
                let max = transition.map_or(diff, |(_, _, d)| d.max(diff));
                transition = Some((pair[1], start, max));
            } else if let Some(done) = transition.take() {
                push(&mut candidates, done);
            }
        }
        if let Some(done) = transition {
            push(&mut candidates, done);
        }
    }
    // A pointing word takes what was on screen as it was said; evenly spread words are too
    // imprecise to point at anything.
    if transcript.timing != Timing::Even {
        for word in &transcript.words {
            let Some((score, strong)) = spoken_cue(&word.text) else {
                continue;
            };
            let t = word.t0 + 150;
            let Some(&frame) = order
                .iter()
                .rev()
                .find(|&&i| frames[i].t <= t)
                .or_else(|| order.first())
            else {
                continue;
            };
            let cue = word
                .text
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            candidates.push(Candidate {
                frame,
                span: (word.t0, word.t0),
                score: score + if strong { 6.0 } else { 1.0 },
                reason: KeyframeReason::Speech,
                marks: Vec::new(),
                event: format!("said “{cue}”"),
            });
        }
    }
    // The view before anything happened is a moment of its own when nothing else shows it.
    let first = order[0];
    candidates.push(Candidate {
        frame: first,
        span: (frames[first].t, frames[first].t),
        score: 30.0,
        reason: KeyframeReason::Initial,
        marks: Vec::new(),
        event: "the screen at the start".into(),
    });
    candidates.sort_by(|a, b| {
        a.marks
            .is_empty()
            .cmp(&b.marks.is_empty())
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| frames[a.frame].t.cmp(&frames[b.frame].t))
    });
    let admissible = |candidate: &Candidate, selected: &[Candidate]| {
        if !candidate.marks.is_empty() {
            return true;
        }
        let t = frames[candidate.frame].t;
        if selected.iter().any(|other| {
            other.frame == candidate.frame
                || frames[other.frame].t.abs_diff(t) < FRAME_GAP_MS
                || (other.marks.len() > 1 && other.span.0 <= t && t <= other.span.1)
        }) {
            return false;
        }
        selected.iter().all(|other| {
            screen_diff(&frames[other.frame].luma, &frames[candidate.frame].luma) >= NOVELTY
        })
    };
    let mut selected: Vec<Candidate> = Vec::new();
    let mut rest = Vec::new();
    for candidate in candidates {
        if !candidate.marks.is_empty() && selected.len() < MAX_KEYFRAMES {
            selected.push(candidate);
        } else {
            rest.push(candidate);
        }
    }
    // Past the base, keyframes are only for marks.
    for candidate in rest {
        if selected.len() >= budget.clamp(1, BASE_KEYFRAMES) {
            break;
        }
        if admissible(&candidate, &selected) {
            selected.push(candidate);
        }
    }
    selected.sort_by_key(|c| (frames[c.frame].t, c.span.0));
    let times: Vec<u64> = selected.iter().map(|c| frames[c.frame].t).collect();
    let anchors: Vec<u64> = selected
        .iter()
        .map(|c| c.span.0.min(frames[c.frame].t))
        .collect();
    let said = said_per_keyframe(&times, &anchors, transcript);
    let keyframes = selected
        .into_iter()
        .zip(said)
        .enumerate()
        .map(|(i, (candidate, said))| Keyframe {
            index: i + 1,
            t: frames[candidate.frame].t,
            reason: candidate.reason,
            frame: candidate.frame,
            event: candidate.event,
            marks: candidate.marks,
            said,
        })
        .collect();
    (marks, keyframes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{FrameReason, Screen, Stroke, StrokePoint},
        timing::transcript,
    };

    fn frame(t: u64, shade: f32, reason: FrameReason) -> Frame {
        Frame {
            t,
            screen: "DP-1".into(),
            reason,
            path: format!("frame-{t}.png").into(),
            width: 1920,
            height: 1080,
            luma: vec![shade; THUMB_PIXELS],
            window: None,
        }
    }
    fn line(t0: u64, y: f32) -> Stroke {
        Stroke {
            t0,
            t1: t0 + 400,
            screen: "DP-1".into(),
            pin: None,
            points: (0..20)
                .map(|i| StrokePoint {
                    x: 100.0 + 20.0 * i as f32,
                    y,
                    t: t0 + 20 * i,
                })
                .collect(),
        }
    }
    fn capture(frames: Vec<Frame>, strokes: Vec<Stroke>, duration_ms: u64) -> Capture {
        Capture {
            duration_ms,
            screens: vec![Screen {
                name: "DP-1".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.0,
            }],
            strokes,
            frames,
            events: vec![],
            preview: vec![],
        }
    }

    #[test]
    fn keeps_every_mark_and_fills_with_distinct_moments_only() {
        let frames = vec![
            frame(0, 0.1, FrameReason::Start),
            frame(2_000, 0.1, FrameReason::Sample),
            frame(4_000, 0.5, FrameReason::Sample),
            frame(4_500, 0.5, FrameReason::Mark),
            frame(6_000, 0.5, FrameReason::Sample),
            frame(9_000, 0.9, FrameReason::Sample),
            frame(12_400, 0.9, FrameReason::Mark),
        ];
        let strokes = vec![line(4_000, 300.0), line(12_000, 600.0)];
        let timed = transcript("one two three four five six", None, &[], 14_000);
        let (marks, keyframes) = select(&capture(frames, strokes, 14_000), &timed);
        assert_eq!(marks.len(), 2);
        let reasons: Vec<_> = keyframes.iter().map(|k| k.reason).collect();
        // The start and both marks. Unchanged samples, and changes whose screen a mark's frame
        // already shows, are not repeated.
        assert_eq!(
            reasons,
            [
                KeyframeReason::Initial,
                KeyframeReason::Mark,
                KeyframeReason::Mark
            ]
        );
        assert!(keyframes.windows(2).all(|k| k[0].t < k[1].t));
        assert_eq!(keyframes[0].index, 1);
        let said: String = keyframes
            .iter()
            .filter_map(|k| k.said.clone())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(said, "one two three four five six");
    }

    #[test]
    fn more_marks_than_the_limit_merge_into_twelve_frames() {
        let mut frames = Vec::new();
        let mut strokes = Vec::new();
        for i in 0..16_u64 {
            let t = i * 3_000;
            frames.push(frame(t + 450, i as f32 / 16.0, FrameReason::Mark));
            strokes.push(line(t, 100.0 + i as f32));
        }
        let timed = transcript("", None, &[], 50_000);
        let (marks, keyframes) = select(&capture(frames, strokes, 50_000), &timed);
        assert_eq!(marks.len(), 16);
        assert_eq!(keyframes.len(), MAX_KEYFRAMES);
        let shown: usize = keyframes.iter().map(|k| k.marks.len()).sum();
        assert_eq!(shown, 16);
    }

    #[test]
    fn separate_gestures_on_one_screen_keep_their_own_keyframes_and_words() {
        // One unchanged screen: a two-stroke gesture, then a separate mark five seconds later.
        let frames = vec![
            frame(1_000, 0.4, FrameReason::Mark),
            frame(2_000, 0.4, FrameReason::Mark),
            frame(7_000, 0.4, FrameReason::Mark),
        ];
        let strokes = vec![line(1_000, 100.0), line(2_000, 140.0), line(7_000, 500.0)];
        let timed = transcript(
            "This header is wrong. And this footer overlaps.",
            None,
            &[(3_000, 4), (9_000, 8)],
            10_000,
        );
        let (marks, keyframes) = select(&capture(frames, strokes, 10_000), &timed);
        assert_eq!(marks.len(), 2, "quick strokes form one mark");
        let groups: Vec<_> = keyframes.iter().map(|k| k.marks.clone()).collect();
        assert_eq!(groups, [vec![0], vec![1]]);
        assert_eq!(
            keyframes[0].t, 1_000,
            "a mark uses the frame taken as it began"
        );
        assert_eq!(keyframes[0].said.as_deref(), Some("This header is wrong."));
        assert_eq!(
            keyframes[1].said.as_deref(),
            Some("And this footer overlaps.")
        );
    }

    #[test]
    fn czech_and_english_cues_point_at_the_screen() {
        assert!(spoken_cue("Tady,").is_some_and(|(_, strong)| strong));
        assert!(spoken_cue("look").is_some());
        assert!(spoken_cue("button").is_none());
    }
}
