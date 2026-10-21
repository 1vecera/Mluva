//! Classify ink into circles, underlines, lines, arrows, taps and pins.
//!
//! Adapted from Scrawl's `src/sketch.rs` at f2ad4e8c75b6cc6171c38d85cd8755e4d9fe03fe.
//! Copyright (c) 2026 Kit Langton. MIT License; see `resources/scrawl-LICENSE`.
//! Mluva works in logical pixels per screen instead of Scrawl's normalized frame coordinates.
use crate::model::{Mark, Shape, Stroke};
use std::f32::consts::PI;

/// A pause longer than this between strokes starts a new mark.
pub const MARK_GAP_MS: u64 = 1_200;
/// Strokes one mark may group.
pub const MAX_MARK_STROKES: usize = 6;

pub fn bounds(points: &[[f32; 2]]) -> [f32; 4] {
    let Some(&[x, y]) = points.first() else {
        return [0.0; 4];
    };
    points.iter().fold([x, y, x, y], |b, p| {
        [
            b[0].min(p[0]),
            b[1].min(p[1]),
            b[2].max(p[0]),
            b[3].max(p[1]),
        ]
    })
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// One stroke's shape and what a mark needs from it.
struct Ink {
    shape: Shape,
    /// What the stroke covers: a circle's loop without its lead-in or tail, else the whole stroke.
    region: [f32; 4],
    /// A one-stroke arrow's shaft start and tip.
    ends: Option<([f32; 2], [f32; 2])>,
}

fn ink_of(stroke: &Stroke) -> Ink {
    let points = stroke.xy();
    let region = bounds(&points);
    let plain = |shape| Ink {
        shape,
        region,
        ends: None,
    };
    if stroke.pin.is_some() {
        return plain(Shape::Pin);
    }
    let Some(&first) = points.first() else {
        return plain(Shape::Tap);
    };
    let b = region;
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    let len: f32 = points.windows(2).map(|p| distance(p[0], p[1])).sum();
    let last = *points.last().unwrap();
    let chord = distance(first, last);
    let shoelace = points
        .windows(2)
        .map(|p| p[0][0] * p[1][1] - p[1][0] * p[0][1])
        .sum::<f32>()
        + last[0] * first[1]
        - first[0] * last[1];
    let loop_box = || loop_in(&points).map(|(i, j)| bounds(&points[i..=j]));
    if w.hypot(h) < 14.0 {
        plain(Shape::Tap)
    } else if encloses(chord, 0.35, len, b, 0.5 * shoelace.abs()) {
        // A short tail or lead-in can still end close enough to count; what was circled is the
        // loop inside it. A stroke that goes round more than once keeps its whole box.
        Ink {
            shape: Shape::Circle,
            region: (len <= 1.5 * round(b))
                .then(loop_box)
                .flatten()
                .unwrap_or(region),
            ends: None,
        }
    } else if chord > 0.88 * len {
        plain(if w > 2.5 * h {
            Shape::Underline
        } else {
            Shape::Line
        })
    } else if let Some(region) = loop_box() {
        Ink {
            shape: Shape::Circle,
            region,
            ends: None,
        }
    } else if let Some(tip) = arrow_tip(&points, w.max(h)) {
        Ink {
            shape: Shape::Arrow,
            region,
            ends: Some((points[0], points[tip])),
        }
    } else {
        plain(Shape::Mark)
    }
}

/// The most points `loop_in` tries as a loop's ends.
const MAX_LOOP_POINTS: usize = 512;

/// Ramanujan's approximation of the perimeter of the ellipse filling box `b`.
fn round(b: [f32; 4]) -> f32 {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    PI / 2.0 * (3.0 * (w + h) - ((3.0 * w + h) * (w + 3.0 * h)).sqrt())
}

/// Whether a path goes round something: it ends within `max_gap` of its long side of where it
/// began, is longer than twice its long side, fills a third of its box and is not too flat.
/// Flatter loops, to 5:1, count when they also close well within their short side and go round
/// at most three times, so a scribble over a line of text does not.
fn encloses(gap: f32, max_gap: f32, len: f32, b: [f32; 4], area: f32) -> bool {
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    let (big, small) = (w.max(h), w.min(h));
    gap < max_gap * big
        && len > 2.0 * big
        && small >= 18.0
        && area > 0.32 * w * h
        && (small > 0.42 * big || (small > 0.2 * big && gap < 0.6 * small && len <= 3.5 * round(b)))
}

/// The loop inside a stroke that circles something once before or after a lead-in, tail or
/// overshoot: the stretch `i..=j`, at least 0.6 of the stroke's length, whose ends meet within
/// 0.15 of its size and that `encloses` its box. Of several, the one whose ends meet closest.
/// Ends are tried at every `step`th point; each stretch is judged by prefix sums over every point.
fn loop_in(points: &[[f32; 2]]) -> Option<(usize, usize)> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let mut run = Vec::with_capacity(n);
    let mut twice_area = Vec::with_capacity(n);
    run.push(0.0f64);
    twice_area.push(0.0f64);
    for p in points.windows(2) {
        run.push(run.last().unwrap() + distance(p[0], p[1]) as f64);
        twice_area.push(
            twice_area.last().unwrap() + p[0][0] as f64 * p[1][1] as f64
                - p[1][0] as f64 * p[0][1] as f64,
        );
    }
    let total = run[n - 1];
    let step = n.div_ceil(MAX_LOOP_POINTS);
    let mut ends: Vec<usize> = (0..n).step_by(step).collect();
    if ends.last() != Some(&(n - 1)) {
        ends.push(n - 1);
    }
    let mut best: Option<(f32, usize, usize)> = None;
    for (k, &i) in ends.iter().enumerate() {
        let a = points[i];
        let mut b = [a[0], a[1], a[0], a[1]];
        for &j in &ends[k + 1..] {
            let z = points[j];
            b = [
                b[0].min(z[0]),
                b[1].min(z[1]),
                b[2].max(z[0]),
                b[3].max(z[1]),
            ];
            let len = run[j] - run[i];
            if len < 0.6 * total {
                continue;
            }
            let gap = distance(a, z);
            let closeness = gap / (b[2] - b[0]).max(b[3] - b[1]);
            if best.is_some_and(|(c, ..)| closeness >= c) {
                continue;
            }
            let len = len as f32;
            let twice = twice_area[j] - twice_area[i] + z[0] as f64 * a[1] as f64
                - a[0] as f64 * z[1] as f64;
            if len <= 1.5 * round(b) && encloses(gap, 0.15, len, b, 0.5 * twice.abs() as f32) {
                best = Some((closeness, i, j));
            }
        }
    }
    best.map(|(_, i, j)| (i, j))
}

/// How far `p` is from the line through `a` and `b`.
fn off_line(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]) * (a[1] - p[1]) - (a[0] - p[0]) * (b[1] - a[1])).abs() / distance(a, b)
}

/// The tip of an arrow drawn in one stroke (`big` its box's long side): a straight shaft from the
/// first point to the tip, where the stroke turns back into one or two straight barbs that end
/// behind the tip, and the head reaches both sides of the shaft. Check marks, V's, zigzags and
/// underlines that hook back to one side fail.
fn arrow_tip(points: &[[f32; 2]], big: f32) -> Option<usize> {
    let start = points[0];
    let mut tip = 0;
    let mut turned = false;
    for (i, &p) in points.iter().enumerate() {
        let (d, far) = (distance(start, p), distance(start, points[tip]));
        if d >= far {
            tip = i;
        } else if far >= 0.5 * big && d < 0.85 * far {
            turned = true;
            break;
        }
    }
    let t = points[tip];
    let shaft = distance(start, t);
    if !turned
        || points[..tip]
            .iter()
            .any(|&p| off_line(p, start, t) > 0.1 * shaft)
    {
        return None;
    }
    // The direction the shaft arrives in, over its last 30%.
    let arc: f32 = points[..=tip]
        .windows(2)
        .map(|p| distance(p[0], p[1]))
        .sum();
    let mut run = 0.0;
    let back = (0..tip)
        .find(|&i| {
            run += distance(points[i], points[i + 1]);
            run >= 0.7 * arc
        })
        .unwrap_or(0);
    let along = distance(points[back], t);
    if along < 1.0 {
        return None;
    }
    let dir = [
        (t[0] - points[back][0]) / along,
        (t[1] - points[back][1]) / along,
    ];
    // Barbs: runs away from the tip by at least `slack`, each from the nearest point before it.
    let slack = 0.15 * shaft;
    let mut barbs: Vec<(usize, usize)> = Vec::new();
    let (mut out, mut near, mut rising) = (tip, tip, true);
    for (i, &p) in points.iter().enumerate().skip(tip + 1) {
        if (p[0] - t[0]) * dir[0] + (p[1] - t[1]) * dir[1] > slack {
            return None;
        }
        let d = distance(t, p);
        if rising {
            if d >= distance(t, points[out]) {
                out = i;
            } else if d < distance(t, points[out]) - slack {
                barbs.push((near, out));
                (near, rising) = (i, false);
            }
        } else if d <= distance(t, points[near]) {
            near = i;
        } else if d > distance(t, points[near]) + slack {
            (out, rising) = (i, true);
        }
    }
    if rising && distance(t, points[out]) >= slack {
        barbs.push((near, out));
    }
    let cos = |deg: f32| deg.to_radians().cos();
    let mut measured = Vec::with_capacity(barbs.len());
    for (from, end) in barbs {
        let (a, z) = (points[from], points[end]);
        let span = distance(a, z);
        if points[from..=end]
            .iter()
            .any(|&p| off_line(p, a, z) > 0.15 * span)
        {
            return None;
        }
        let e = [z[0] - t[0], z[1] - t[1]];
        let d = e[0].hypot(e[1]);
        let c = -(e[0] * dir[0] + e[1] * dir[1]) / d;
        if d > 0.9 * shaft || c < cos(75.0) {
            return None;
        }
        measured.push((d, c));
    }
    let head = match measured[..] {
        [(d, c)] => d <= 0.6 * shaft && c <= cos(15.0),
        [(_, c0), (_, c1)] => c0.max(c1) <= cos(5.0),
        _ => false,
    };
    let side = |p: [f32; 2]| dir[0] * (p[1] - t[1]) - dir[1] * (p[0] - t[0]);
    let reach = |sign: f32| {
        points[tip..]
            .iter()
            .map(|&p| sign * side(p))
            .fold(0.0, f32::max)
    };
    let (left, right) = (reach(1.0), reach(-1.0));
    (head && left.min(right) > 0.3 * left.max(right)).then_some(tip)
}

/// Group strokes drawn in quick succession on one screen into marks and classify each.
pub fn marks_of(strokes: &[Stroke]) -> Vec<Mark> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, stroke) in strokes.iter().enumerate() {
        if stroke.points.is_empty() {
            continue;
        }
        if let Some(group) = groups.last_mut() {
            let prev = &strokes[*group.last().unwrap()];
            if prev.pin.is_none()
                && stroke.pin.is_none()
                && prev.screen == stroke.screen
                && stroke.t0.saturating_sub(prev.t1) <= MARK_GAP_MS
                && group.len() < MAX_MARK_STROKES
            {
                group.push(i);
                continue;
            }
        }
        groups.push(vec![i]);
    }
    groups
        .into_iter()
        .enumerate()
        .map(|(i, group)| mark_of(strokes, i, group))
        .collect()
}

fn mark_of(strokes: &[Stroke], index: usize, group: Vec<usize>) -> Mark {
    let first = &strokes[group[0]];
    let all: Vec<_> = group.iter().flat_map(|&j| strokes[j].xy()).collect();
    let mut mark = Mark {
        id: format!("m{}", index + 1),
        t0: first.t0,
        t1: group
            .iter()
            .map(|&j| strokes[j].t1)
            .max()
            .unwrap_or(first.t1),
        screen: first.screen.clone(),
        region: bounds(&all),
        shape: Shape::Mark,
        from: None,
        to: None,
        pin: first.pin,
        strokes: group.clone(),
        description: String::new(),
    };
    if first.pin.is_some() {
        let [x, y] = first.xy().first().copied().unwrap_or([0.0; 2]);
        mark.region = [x, y, x, y];
        mark.shape = Shape::Pin;
    } else {
        let mut sized: Vec<_> = group
            .iter()
            .map(|&j| {
                let points = strokes[j].xy();
                let b = bounds(&points);
                let size = (b[2] - b[0]) + (b[3] - b[1]);
                (ink_of(&strokes[j]), b, points, size)
            })
            .collect();
        sized.sort_by(|a, b| b.3.total_cmp(&a.3));
        let (ink, _, points, _) = &sized[0];
        let shape = ink.shape;
        let rest = &sized[1..];
        let a = points.first().copied().unwrap_or([0.0; 2]);
        let z = points.last().copied().unwrap_or(a);
        // A line with one or two short strokes at one end is an arrow drawn in parts.
        if matches!(shape, Shape::Line | Shape::Underline) && (1..=2).contains(&rest.len()) {
            let len = distance(a, z);
            let near = |p| {
                rest.iter().all(|r| {
                    distance([(r.1[0] + r.1[2]) / 2.0, (r.1[1] + r.1[3]) / 2.0], p) < 0.35 * len
                })
            };
            if near(z) || near(a) {
                mark.shape = Shape::Arrow;
                mark.from = Some(if near(z) { a } else { z });
                mark.to = Some(if near(z) { z } else { a });
            }
        }
        if mark.shape != Shape::Arrow
            && (rest.iter().all(|r| r.0.shape == shape) || shape == Shape::Circle)
        {
            match shape {
                Shape::Circle => {
                    mark.shape = Shape::Circle;
                    // What was circled: the loops, without any lead-in or tail.
                    mark.region = if rest.iter().all(|r| r.0.shape == Shape::Circle) {
                        bounds(
                            &sized
                                .iter()
                                .flat_map(|r| {
                                    let [x0, y0, x1, y1] = r.0.region;
                                    [[x0, y0], [x1, y1]]
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        ink.region
                    };
                }
                Shape::Line if rest.is_empty() => {
                    mark.shape = Shape::Line;
                    mark.from = Some(a);
                    mark.to = Some(z);
                }
                Shape::Arrow if rest.is_empty() => {
                    mark.shape = Shape::Arrow;
                    mark.from = ink.ends.map(|e| e.0);
                    mark.to = ink.ends.map(|e| e.1);
                }
                Shape::Tap | Shape::Underline => mark.shape = shape,
                _ => {}
            }
        }
    }
    mark.description = describe_mark(&mark);
    mark
}

/// A short phrase for the summary; coordinates stay in the JSON.
pub fn describe_mark(mark: &Mark) -> String {
    let many = if mark.strokes.len() > 1 {
        format!(" ({} strokes)", mark.strokes.len())
    } else {
        String::new()
    };
    match mark.shape {
        Shape::Circle => format!("circled{many}"),
        Shape::Underline => format!("underlined{many}"),
        Shape::Line => "drew a line".into(),
        Shape::Arrow => "drew an arrow".into(),
        Shape::Pin => format!("pin {}", mark.pin.unwrap_or(1)),
        Shape::Tap => format!("tapped{many}"),
        Shape::Mark => format!("marked{many}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StrokePoint;

    fn stroke(points: &[[f32; 2]], t0: u64) -> Stroke {
        Stroke {
            t0,
            t1: t0 + 10 * points.len() as u64,
            screen: "DP-1".into(),
            pin: None,
            points: points
                .iter()
                .enumerate()
                .map(|(i, p)| StrokePoint {
                    x: p[0],
                    y: p[1],
                    t: t0 + 10 * i as u64,
                })
                .collect(),
        }
    }
    fn circle(cx: f32, cy: f32, r: f32) -> Vec<[f32; 2]> {
        (0..=64)
            .map(|i| {
                let a = i as f32 / 64.0 * 2.0 * PI;
                [cx + r * a.cos(), cy + r * a.sin()]
            })
            .collect()
    }

    #[test]
    fn classifies_common_marks() {
        let marks = marks_of(&[
            stroke(&circle(400.0, 300.0, 80.0), 0),
            stroke(&[[100.0, 500.0], [300.0, 502.0], [500.0, 504.0]], 5_000),
            stroke(
                &[
                    [100.0, 100.0],
                    [200.0, 200.0],
                    [300.0, 300.0],
                    [260.0, 300.0],
                    [220.0, 300.0],
                    [300.0, 300.0],
                    [300.0, 260.0],
                    [300.0, 220.0],
                ],
                10_000,
            ),
            stroke(&[[50.0, 50.0], [52.0, 51.0]], 15_000),
        ]);
        let shapes: Vec<_> = marks.iter().map(|m| m.shape).collect();
        assert_eq!(
            shapes,
            [Shape::Circle, Shape::Underline, Shape::Arrow, Shape::Tap]
        );
        assert_eq!(marks[0].description, "circled");
        assert!(marks[0].region[0] > 300.0 && marks[0].region[2] < 500.0);
    }

    #[test]
    fn groups_quick_strokes_on_one_screen_and_keeps_pins_apart() {
        let mut pin = stroke(&[[10.0, 10.0]], 600);
        pin.pin = Some(1);
        let mut other_screen = stroke(&[[0.0, 0.0], [100.0, 0.0]], 700);
        other_screen.screen = "HDMI-A-1".into();
        let marks = marks_of(&[
            stroke(&[[0.0, 0.0], [100.0, 0.0]], 0),
            stroke(&[[0.0, 20.0], [100.0, 20.0]], 300),
            pin,
            other_screen,
        ]);
        assert_eq!(marks.len(), 3);
        assert_eq!(marks[0].strokes, [0, 1]);
        assert_eq!(marks[1].shape, Shape::Pin);
        assert_eq!(marks[1].description, "pin 1");
        assert_eq!(marks[2].screen, "HDMI-A-1");
    }
}
