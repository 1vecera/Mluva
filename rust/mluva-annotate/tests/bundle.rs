//! A bundle written from synthetic frames: rendered keyframes, contact sheet, summary and data,
//! all owner-only, and nothing recreated once its directory is gone.
use cairo::{Context, Format, ImageSurface};
use mluva_annotate::{
    bundle,
    model::{Capture, Frame, FrameReason, Screen, Stroke, StrokePoint},
    render, timing,
};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

/// A 1920×1080 frame of one flat colour with a dark block, saved as a private JPEG or PNG.
fn frame(directory: &Path, name: &str, shade: f64, t: u64) -> Frame {
    let surface = ImageSurface::create(Format::Rgb24, 1920, 1080).unwrap();
    let cr = Context::new(&surface).unwrap();
    cr.set_source_rgb(shade, shade, shade);
    cr.paint().unwrap();
    cr.set_source_rgb(0.1, 0.2, 0.3);
    cr.rectangle(800.0, 400.0, 300.0, 120.0);
    cr.fill().unwrap();
    drop(cr);
    let path = directory.join(name);
    fs::write(&path, render::png(&surface).unwrap()).unwrap();
    let mut loaded = render::load(&path).unwrap();
    Frame {
        t,
        screen: "DP-1".into(),
        reason: FrameReason::Mark,
        path,
        width: 1920,
        height: 1080,
        luma: render::luma(&mut loaded),
        windows: vec![],
    }
}

fn circle(t0: u64, cx: f32, cy: f32) -> Stroke {
    let points: Vec<_> = (0..=48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            StrokePoint {
                x: cx + 120.0 * a.cos(),
                y: cy + 70.0 * a.sin(),
                t: t0 + 10 * i,
            }
        })
        .collect();
    Stroke {
        t0,
        t1: t0 + 480,
        screen: "DP-1".into(),
        pin: None,
        points,
    }
}

fn capture(directory: &Path) -> Capture {
    let mut pin = Stroke {
        t0: 9_000,
        t1: 9_000,
        screen: "DP-1".into(),
        pin: Some(1),
        points: vec![StrokePoint {
            x: 300.0,
            y: 200.0,
            t: 9_000,
        }],
    };
    pin.points[0].t = 9_000;
    Capture {
        duration_ms: 12_000,
        screens: vec![Screen {
            name: "DP-1".into(),
            x: 0,
            y: 0,
            width: 960,
            height: 540,
            scale: 2.0,
        }],
        strokes: vec![circle(2_000, 475.0, 230.0), pin],
        frames: vec![
            frame(directory, "a.png", 0.95, 2_000),
            frame(directory, "b.png", 0.30, 9_000),
        ],
        events: vec![],
        preview: vec![],
    }
}

#[test]
fn a_bundle_renders_private_keyframes_a_contact_sheet_and_a_summary() {
    let root = tempfile::tempdir().unwrap();
    let sources = root.path().join("sources");
    fs::create_dir(&sources).unwrap();
    let directory = root.path().join("bundle");
    fs::create_dir(&directory).unwrap();
    let capture = capture(&sources);
    let transcript = timing::transcript(
        "Circle this panel. Then pin the corner.",
        None,
        &[],
        capture.duration_ms,
    );
    let written = bundle::write(&directory, &capture, &transcript).unwrap();
    assert_eq!(written.frames.len(), 2);
    let lines: Vec<_> = written.summary.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("[Screen annotation · 0:12 · 2 keyframes · contact sheet /"));
    assert!(lines[1].starts_with("- #1 (0:02) [circled] “Circle this panel.” /"));
    assert!(lines[2].starts_with("- #2 (0:09) [pin 1] “Then pin the corner.” /"));
    assert!(lines[1].ends_with("/frames/frame-01.png"));
    assert_eq!(
        fs::read_to_string(directory.join("summary.md")).unwrap(),
        written.summary
    );
    for path in [&directory, &directory.join("frames")] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for path in written
        .frames
        .iter()
        .chain([&written.contact_sheet, &directory.join("annotation.json")])
    {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // A HiDPI frame is cropped around the circle and kept within the size agents read well.
    let first = render::load(&written.frames[0]).unwrap();
    assert!(first.width() <= 1600 && first.height() < 1080);
    let sheet = render::load(&written.contact_sheet).unwrap();
    assert!(sheet.width() <= 1600);
    let data: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("annotation.json")).unwrap()).unwrap();
    assert_eq!(data["keyframes"][0]["marks"][0], "m1");
    assert_eq!(data["marks"][1]["shape"], "pin");
    assert_eq!(data["timing"], "even");
    assert!(fs::read_dir(&directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')
    }));
}

#[test]
fn a_removed_bundle_directory_is_never_recreated() {
    let root = tempfile::tempdir().unwrap();
    let capture = capture(root.path());
    let transcript = timing::transcript("Look here.", None, &[], capture.duration_ms);
    let missing = root.path().join("removed-bundle");
    assert!(bundle::write(&missing, &capture, &transcript).is_err());
    assert!(!missing.exists());
}
