//! Actual GTK buffer/tag/layout observations; run only on the disposable desktop.

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use adw::prelude::*;
use mluva_gtk::markdown_view::MarkdownTextView;
use mluva_gtk::theme::ThemeController;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Reference {
    gtk: [u32; 3],
    pango: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    before: String,
    after: String,
    reading: Observation,
    editing: Observation,
    replaced: Observation,
    after_reading: Observation,
}
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Interval {
    start: usize,
    end: usize,
    tags: Vec<String>,
}
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Observation {
    source: String,
    visible: String,
    intervals: Vec<Interval>,
    heights: Vec<i32>,
    wrap: String,
    limited: bool,
    tooltip: Option<String>,
    insert: i32,
    bound: i32,
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(8));
    }
}

fn observation(view: &MarkdownTextView) -> Observation {
    let source = view.text();
    let buffer = view.buffer();
    let mut intervals: Vec<Interval> = Vec::new();
    for index in 0..source.chars().count() {
        let mut tags = buffer
            .iter_at_offset(index as i32)
            .tags()
            .iter()
            .filter_map(|tag| tag.name().map(String::from))
            .collect::<Vec<_>>();
        tags.sort();
        if let Some(previous) = intervals.last_mut()
            && previous.tags == tags
        {
            previous.end = index + 1;
        } else {
            intervals.push(Interval {
                start: index,
                end: index + 1,
                tags,
            });
        }
    }
    Observation {
        source,
        visible: buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .into(),
        intervals,
        heights: [320, 680, 1040]
            .into_iter()
            .map(|width| view.document_height(width))
            .collect(),
        wrap: if view.wrap_mode() == gtk::WrapMode::Char {
            "char"
        } else {
            "word-char"
        }
        .into(),
        limited: view.formatting_limited(),
        tooltip: view.tooltip_text().map(String::from),
        insert: buffer.iter_at_mark(&buffer.get_insert()).offset(),
        bound: buffer.iter_at_mark(&buffer.selection_bound()).offset(),
    }
}

fn assert_observation(view: &MarkdownTextView, expected: &Observation, index: usize, stage: &str) {
    let actual = observation(view);
    let actual_json = serde_json::to_value(&actual).unwrap();
    let expected_json = serde_json::to_value(expected).unwrap();
    let differences = expected_json
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| actual_json[*key] != expected_json[*key])
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        differences.is_empty(),
        "case {index} {stage}: fields {differences:?}; heights {:?} != {:?}; tooltip {:?} != {:?}",
        actual.heights,
        expected.heights,
        actual.tooltip,
        expected.tooltip
    );
}

#[test]
#[ignore = "requires a disposable GTK display, private accessibility/session buses and XDG directories"]
fn document_reading_editing_and_revisions_match_released_native_widget_observations() {
    let root = std::env::var_os("OFFSCREEN_SESSION_ROOT")
        .map(PathBuf::from)
        .expect("private desktop helper required");
    assert!(
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    let portals = PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap())
        .join("xdg-desktop-portal/portals.conf");
    std::fs::create_dir_all(portals.parent().unwrap()).unwrap();
    std::fs::write(portals, "[preferred]\ndefault=gtk\n").unwrap();
    adw::init().unwrap();
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let reference: Reference =
        serde_json::from_str(include_str!("fixtures/document-widget-cases.json")).unwrap();
    assert_eq!(
        [
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ],
        reference.gtk,
        "recollect the immutable release's widget observations when the system renderer changes"
    );
    assert_eq!(gtk::pango::version_string().as_str(), reference.pango);
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .unwrap();
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let outside = gtk::Button::with_label("Outside editor");
    body.append(&outside);
    window.set_content(Some(&body));
    window.present();
    for (index, case) in reference.cases.into_iter().enumerate() {
        eprintln!("widget_case={index}");
        let view = MarkdownTextView::new(&case.before, true, true);
        body.append(&view);
        settle();
        let buffer = view.buffer();
        let length = case.before.chars().count() as i32;
        buffer.select_range(
            &buffer.iter_at_offset(2.min(length)),
            &buffer.iter_at_offset(6.min(length)),
        );
        outside.grab_focus();
        settle();
        assert_observation(&view, &case.reading, index, "reading");
        view.grab_focus();
        settle();
        assert_observation(&view, &case.editing, index, "editing");
        view.replace_text(&case.after);
        settle();
        assert_observation(&view, &case.replaced, index, "revision");
        outside.grab_focus();
        settle();
        assert_observation(&view, &case.after_reading, index, "reading after revision");
        body.remove(&view);
    }
    window.destroy();
    drop(_theme);

    // Exercise the real native theme lifecycle on an isolated synthetic palette.
    let theme_directory = root.join("state/theme-lifecycle");
    std::fs::create_dir_all(&theme_directory).unwrap();
    let palette_path = theme_directory.join("colors.toml");
    let palette = |mode: &str| {
        format!(
            "background=\"#151c1a\"\nforeground=\"#edf5ef\"\naccent=\"#88dabb\"\nmode=\"{mode}\"\n"
        )
    };
    let manager = adw::StyleManager::default();
    let previous = manager.color_scheme();
    std::fs::write(&palette_path, palette("dark")).unwrap();
    let theme = ThemeController::apply(
        &theme_directory,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .unwrap();
    assert_eq!(manager.color_scheme(), adw::ColorScheme::ForceDark);
    let wait_for_scheme = |expected| {
        let deadline = Instant::now() + Duration::from_secs(4);
        while manager.color_scheme() != expected && Instant::now() < deadline {
            settle();
        }
        assert_eq!(manager.color_scheme(), expected);
    };
    std::fs::write(&palette_path, palette("light")).unwrap();
    wait_for_scheme(adw::ColorScheme::ForceLight);
    // The previous scheme is also light; first apply dark to observe removal restoring it.
    std::fs::write(&palette_path, palette("dark")).unwrap();
    wait_for_scheme(adw::ColorScheme::ForceDark);
    std::fs::remove_file(&palette_path).unwrap();
    wait_for_scheme(previous);
    std::fs::write(&palette_path, palette("dark")).unwrap();
    wait_for_scheme(adw::ColorScheme::ForceDark);
    drop(theme);
    assert_eq!(manager.color_scheme(), previous);
}
