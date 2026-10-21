//! What one annotated recording captured, on the recording clock (milliseconds since capture
//! started). Coordinates are logical pixels local to the screen they were drawn on.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Keyframes any recording may fill from ordinary moments.
pub const BASE_KEYFRAMES: usize = 6;
/// Keyframes a recording may use at most, marks included.
pub const MAX_KEYFRAMES: usize = 12;

/// `BASE_KEYFRAMES` up to 40 s, then one more per 20 s, up to `MAX_KEYFRAMES` at 2:20.
pub fn keyframe_budget(duration_ms: u64) -> usize {
    (BASE_KEYFRAMES + duration_ms.saturating_sub(20_000) as usize / 20_000).min(MAX_KEYFRAMES)
}

/// `M:SS`, as the badges and summary show it.
pub fn format_clock(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// One compositor output, in layout coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    pub t: u64,
}

/// One Shift-held pointer path, or a Shift-click pin when `pin` is set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub t0: u64,
    pub t1: u64,
    pub screen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<u32>,
    pub points: Vec<StrokePoint>,
}
impl Stroke {
    pub fn xy(&self) -> Vec<[f32; 2]> {
        self.points.iter().map(|p| [p.x, p.y]).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Circle,
    Underline,
    Line,
    Arrow,
    Tap,
    Pin,
    Mark,
}

/// Strokes drawn together, classified by what they look like.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub id: String,
    pub strokes: Vec<usize>,
    pub t0: u64,
    pub t1: u64,
    pub screen: String,
    /// Bounding box `[x0, y0, x1, y1]` in logical pixels on `screen`.
    pub region: [f32; 4],
    pub shape: Shape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<u32>,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameReason {
    /// The first view after capture started.
    Start,
    /// A periodic sample, kept only when the screen changed.
    Sample,
    /// The screen as a stroke or pin was finished.
    Mark,
    /// The settled view after the active window or workspace changed.
    Window,
}

/// A captured screen image. `path` is a private PNG owned by the capture until the bundle
/// is written; `luma` is a small grey thumbnail used to compare screens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub t: u64,
    pub screen: String,
    pub reason: FrameReason,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub luma: Vec<f32>,
    /// The active window's box on this screen, logical pixels, when the compositor reported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScreenEvent {
    Window {
        t: u64,
        class: String,
        title: String,
    },
    Workspace {
        t: u64,
        name: String,
    },
}
impl ScreenEvent {
    pub fn t(&self) -> u64 {
        match self {
            Self::Window { t, .. } | Self::Workspace { t, .. } => *t,
        }
    }
}

/// Everything recorded while the user annotated, frozen when recording stops.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub duration_ms: u64,
    pub screens: Vec<Screen>,
    pub strokes: Vec<Stroke>,
    pub frames: Vec<Frame>,
    pub events: Vec<ScreenEvent>,
    /// The recording's live preview length over time: `(t, words shown)`. Providers that
    /// return no word timing are aligned against it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preview: Vec<(u64, usize)>,
}
impl Capture {
    /// Only drawn marks or pins make a recording an annotated one.
    pub fn has_marks(&self) -> bool {
        self.strokes.iter().any(|stroke| !stroke.points.is_empty())
    }
    pub fn screen(&self, name: &str) -> Option<&Screen> {
        self.screens.iter().find(|screen| screen.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Word {
    pub t0: u64,
    pub t1: u64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Timing {
    /// Word times from the speech provider.
    Provider,
    /// Word times interpolated from when the live preview grew.
    Preview,
    /// Words spread evenly over the recording; spoken cues are ignored.
    Even,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub text: String,
    pub timing: Timing,
    pub words: Vec<Word>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyframeReason {
    Mark,
    Pin,
    Window,
    SceneChange,
    Speech,
    Initial,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub index: usize,
    pub t: u64,
    pub reason: KeyframeReason,
    /// Index into `Capture::frames`.
    pub frame: usize,
    pub event: String,
    pub marks: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}
