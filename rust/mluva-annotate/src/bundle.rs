//! Write one annotation bundle: rendered keyframes, a contact sheet, `summary.md` and
//! `annotation.json`. Every file is owner-only inside a directory the caller owns.
use crate::{
    keyframes,
    model::{Capture, Keyframe, Mark, Transcript, format_clock},
    render::{self, RenderError},
};
use mluva_core::annotations::{CONTACT_SHEET, keyframe_file};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub const SUMMARY: &str = "summary.md";
pub const DATA: &str = "annotation.json";
/// A keyframe's spoken clause is shortened past this many characters.
const MAX_SAID_CHARS: usize = 220;

#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub directory: PathBuf,
    pub summary: String,
    pub contact_sheet: PathBuf,
    pub frames: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("This recording has no marks to annotate.")]
    Empty,
    #[error("The screen was not captured, so no keyframes could be drawn.")]
    NoFrames,
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn shortened(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_SAID_CHARS {
        return text;
    }
    let cut: String = text.chars().take(MAX_SAID_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

/// Write an owner-only file atomically inside an existing directory.
fn write_private(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut file = tempfile::Builder::new()
        .prefix(".annotation-")
        .permissions(fs::Permissions::from_mode(0o600))
        .tempfile_in(parent)?;
    file.write_all(content)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

/// The header line and one line per keyframe, with absolute paths an agent can open.
pub fn summary(
    duration_ms: u64,
    keyframes: &[Keyframe],
    frames: &[PathBuf],
    contact_sheet: &Path,
) -> String {
    let mut out = format!(
        "[Screen annotation · {} · {} keyframe{} · contact sheet {}]\n",
        format_clock(duration_ms),
        keyframes.len(),
        if keyframes.len() == 1 { "" } else { "s" },
        contact_sheet.display()
    );
    for (keyframe, path) in keyframes.iter().zip(frames) {
        let said = keyframe
            .said
            .as_deref()
            .map(shortened)
            .filter(|said| !said.is_empty())
            .map(|said| format!(" “{said}”"))
            .unwrap_or_default();
        out.push_str(&format!(
            "- #{} ({}) [{}]{said} {}\n",
            keyframe.index,
            format_clock(keyframe.t),
            keyframe.event,
            path.display()
        ));
    }
    out
}

fn data(
    capture: &Capture,
    transcript: &Transcript,
    marks: &[Mark],
    keyframes: &[Keyframe],
    crops: &[[f32; 4]],
) -> serde_json::Value {
    let frames: Vec<_> = capture
        .frames
        .iter()
        .map(|frame| {
            json!({"t": frame.t, "screen": frame.screen, "reason": frame.reason,
                "width": frame.width, "height": frame.height, "windows": frame.windows})
        })
        .collect();
    let keyframes: Vec<_> = keyframes
        .iter()
        .zip(crops)
        .map(|(keyframe, crop)| {
            json!({"index": keyframe.index, "t": keyframe.t, "reason": keyframe.reason,
                "file": keyframe_file(keyframe.index),
                "screen": capture.frames[keyframe.frame].screen, "crop": crop,
                "event": keyframe.event, "said": keyframe.said,
                "marks": keyframe.marks.iter().filter_map(|&i| marks.get(i)).map(|m| m.id.clone()).collect::<Vec<_>>()})
        })
        .collect();
    json!({
        "schema": 1,
        "durationMs": capture.duration_ms,
        "screens": capture.screens,
        "timing": transcript.timing,
        "words": transcript.words,
        "strokes": capture.strokes,
        "marks": marks,
        "events": capture.events,
        "frames": frames,
        "keyframes": keyframes,
        "contactSheet": CONTACT_SHEET,
    })
}

/// Select, render and write. `directory` must already exist and belong to the caller; it is
/// kept owner-only. Captured source frames are read, never moved or deleted.
pub fn write(
    directory: &Path,
    capture: &Capture,
    transcript: &Transcript,
) -> Result<Written, BundleError> {
    if !capture.has_marks() {
        return Err(BundleError::Empty);
    }
    let (marks, keyframes) = keyframes::select(capture, transcript);
    if keyframes.is_empty() {
        return Err(BundleError::NoFrames);
    }
    // Nothing creates the directory: a bundle removed while it is written stays removed.
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let mut rendered = Vec::with_capacity(keyframes.len());
    let mut paths = Vec::with_capacity(keyframes.len());
    let mut crops = Vec::with_capacity(keyframes.len());
    for keyframe in &keyframes {
        let frame = &capture.frames[keyframe.frame];
        let Some(screen) = capture.screen(&frame.screen) else {
            return Err(BundleError::NoFrames);
        };
        let source = render::load(&frame.path)?;
        crops.push(render::crop(keyframe, &marks, frame, screen));
        let image = render::keyframe(&source, keyframe, &marks, &capture.strokes, frame, screen)?;
        let path = directory.join(keyframe_file(keyframe.index));
        write_private(&path, &render::png(&image)?)?;
        paths.push(path);
        rendered.push(image);
    }
    let sheet = directory.join(CONTACT_SHEET);
    write_private(&sheet, &render::png(&render::contact_sheet(&rendered)?)?)?;
    let summary = summary(capture.duration_ms, &keyframes, &paths, &sheet);
    let document = data(capture, transcript, &marks, &keyframes, &crops);
    let mut json = serde_json::to_vec_pretty(&document).expect("serializable annotation");
    json.push(b'\n');
    write_private(&directory.join(DATA), &json)?;
    write_private(&directory.join(SUMMARY), summary.as_bytes())?;
    Ok(Written {
        directory: directory.into(),
        summary,
        contact_sheet: sheet,
        frames: paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_has_one_header_and_one_line_per_keyframe() {
        let keyframe = |index, t, said: Option<&str>| Keyframe {
            index,
            t,
            reason: crate::model::KeyframeReason::Mark,
            frame: 0,
            event: "circled".into(),
            marks: vec![0],
            said: said.map(Into::into),
        };
        let text = summary(
            65_000,
            &[
                keyframe(1, 4_000, Some("This  button\nis wrong.")),
                keyframe(2, 61_000, None),
            ],
            &["/a/01.png".into(), "/a/02.png".into()],
            Path::new("/a/sheet.png"),
        );
        assert_eq!(
            text,
            "[Screen annotation · 1:05 · 2 keyframes · contact sheet /a/sheet.png]\n\
             - #1 (0:04) [circled] “This button is wrong.” /a/01.png\n\
             - #2 (1:01) [circled] /a/02.png\n"
        );
    }

    #[test]
    fn long_clauses_are_shortened_on_a_word_boundary() {
        let long = "word ".repeat(80);
        let short = shortened(&long);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= MAX_SAID_CHARS + 1);
        assert!(!short.contains("  "));
    }
}
