//! Which part of a frame a keyframe keeps. Pure geometry, shared by every renderer.
use crate::model::{Frame, Keyframe, Mark, Screen};

/// Space kept around a window, and around ink so its stroke and halo stay inside.
const WINDOW_MARGIN: f32 = 16.0;
const INK_MARGIN: f32 = 12.0;

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
fn intersects(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
fn contains(a: [f32; 4], x: f32, y: f32) -> bool {
    a[0] <= x && x <= a[2] && a[1] <= y && y <= a[3]
}
fn area(a: [f32; 4]) -> f32 {
    (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0)
}
fn grown(a: [f32; 4], by: f32) -> [f32; 4] {
    [a[0] - by, a[1] - by, a[2] + by, a[3] + by]
}

/// The part of the frame to keep, in logical screen pixels `[x0, y0, x1, y1]`: whole windows, so
/// the agent sees the marked thing in context and no text is cut mid-word.
///
/// Marks crop to the window under their centre, the smallest one when windows overlap (a floating
/// window over a tiled one), widened to keep every mark. Marks over no window keep every window
/// they touch. Other keyframes crop to the active window. Without window geometry, or when marks
/// touch no window, the whole screen is kept.
pub fn crop(keyframe: &Keyframe, marks: &[Mark], frame: &Frame, screen: &Screen) -> [f32; 4] {
    let (w, h) = (screen.width.max(1) as f32, screen.height.max(1) as f32);
    let full = [0.0, 0.0, w, h];
    let ink = keyframe
        .marks
        .iter()
        .filter_map(|&i| marks.get(i))
        .filter(|mark| mark.screen == frame.screen)
        .map(|mark| grown(mark.region, INK_MARGIN))
        .reduce(union);
    let windows = frame.windows.iter().map(|window| window.rect);
    let kept = match ink {
        None => frame
            .windows
            .iter()
            .find(|window| window.active)
            .map(|window| window.rect),
        Some(ink) => {
            let (cx, cy) = ((ink[0] + ink[2]) / 2.0, (ink[1] + ink[3]) / 2.0);
            windows
                .clone()
                .filter(|rect| contains(*rect, cx, cy))
                .min_by(|a, b| area(*a).total_cmp(&area(*b)))
                .or_else(|| windows.filter(|rect| intersects(*rect, ink)).reduce(union))
                .map(|rect| union(rect, ink))
        }
    };
    kept.map(|rect| clamp(grown(rect, WINDOW_MARGIN), w, h))
        .unwrap_or(full)
}

/// Limit a box to `0..w × 0..h`.
fn clamp(c: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    let x0 = c[0].clamp(0.0, w - 1.0);
    let y0 = c[1].clamp(0.0, h - 1.0);
    [x0, y0, c[2].clamp(x0 + 1.0, w), c[3].clamp(y0 + 1.0, h)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FrameReason, KeyframeReason, Shape, WindowBox};

    fn screen() -> Screen {
        Screen {
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        }
    }
    fn frame(windows: &[([f32; 4], bool)]) -> Frame {
        Frame {
            t: 0,
            screen: "DP-1".into(),
            reason: FrameReason::Mark,
            path: "frame.jpg".into(),
            width: 1920,
            height: 1080,
            luma: vec![],
            windows: windows
                .iter()
                .map(|&(rect, active)| WindowBox { rect, active })
                .collect(),
        }
    }
    fn mark(region: [f32; 4]) -> Mark {
        Mark {
            id: "m1".into(),
            strokes: vec![0],
            t0: 0,
            t1: 0,
            screen: "DP-1".into(),
            region,
            shape: Shape::Circle,
            from: None,
            to: None,
            pin: None,
            description: "circled".into(),
        }
    }
    fn keyframe(marks: Vec<usize>) -> Keyframe {
        Keyframe {
            index: 1,
            t: 0,
            reason: KeyframeReason::Mark,
            frame: 0,
            event: String::new(),
            marks,
            said: None,
        }
    }
    // A tiled browser on the left half, a terminal on the right, a floating dialog over the browser.
    const BROWSER: [f32; 4] = [0.0, 30.0, 960.0, 1080.0];
    const TERMINAL: [f32; 4] = [960.0, 30.0, 1920.0, 1080.0];
    const DIALOG: [f32; 4] = [300.0, 300.0, 700.0, 600.0];

    #[test]
    fn marks_crop_to_their_whole_window_and_never_cut_it() {
        let frame = frame(&[(BROWSER, false), (TERMINAL, true)]);
        let kept = crop(
            &keyframe(vec![0]),
            &[mark([400.0, 500.0, 520.0, 560.0])],
            &frame,
            &screen(),
        );
        assert_eq!(kept, [0.0, 14.0, 976.0, 1080.0]);
    }

    #[test]
    fn a_floating_window_wins_and_marks_reaching_past_it_stay_inside() {
        let frame = frame(&[(BROWSER, false), (DIALOG, true)]);
        let inside = crop(
            &keyframe(vec![0]),
            &[mark([400.0, 400.0, 500.0, 450.0])],
            &frame,
            &screen(),
        );
        assert_eq!(inside, [284.0, 284.0, 716.0, 616.0]);
        let reaching = crop(
            &keyframe(vec![0]),
            &[mark([600.0, 400.0, 800.0, 450.0])],
            &frame,
            &screen(),
        );
        assert_eq!(reaching, [284.0, 284.0, 828.0, 616.0]);
    }

    #[test]
    fn marks_over_no_window_keep_every_window_they_touch() {
        let frame = frame(&[(BROWSER, false), (TERMINAL, false)]);
        // A mark in the top bar, centred above both windows, touching both.
        let kept = crop(
            &keyframe(vec![0]),
            &[mark([900.0, 0.0, 1000.0, 20.0])],
            &frame,
            &screen(),
        );
        assert_eq!(kept, [0.0, 0.0, 1920.0, 1080.0]);
        let alone = crop(
            &keyframe(vec![0]),
            &[mark([100.0, 2.0, 200.0, 10.0])],
            &self::frame(&[(DIALOG, false)]),
            &screen(),
        );
        assert_eq!(alone, [0.0, 0.0, 1920.0, 1080.0]);
    }

    #[test]
    fn without_window_geometry_the_whole_screen_is_kept() {
        let full = [0.0, 0.0, 1920.0, 1080.0];
        assert_eq!(
            crop(
                &keyframe(vec![0]),
                &[mark([10.0, 10.0, 50.0, 50.0])],
                &frame(&[]),
                &screen()
            ),
            full
        );
        assert_eq!(crop(&keyframe(vec![]), &[], &frame(&[]), &screen()), full);
    }

    #[test]
    fn other_keyframes_crop_to_the_active_window() {
        let frame = frame(&[(BROWSER, false), (TERMINAL, true)]);
        assert_eq!(
            crop(&keyframe(vec![]), &[], &frame, &screen()),
            [944.0, 14.0, 1920.0, 1080.0]
        );
    }
}
