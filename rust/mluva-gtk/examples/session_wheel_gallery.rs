//! Render the production session wheel in synthetic states for private visual review.
//!
//! Run inside the off-screen desktop helper with `light`, `dark` or `nord`, then the output
//! directory and optionally `overlay`. The wheel sizes itself to the helper's screen, so set
//! its screen to the display being judged. X11 has no layer-shell, so `overlay` applies the
//! overlay styling to a screen-sized window. The room is opaque, so a capture needs no desktop
//! behind it. Every session name, pane, socket, process and session ID is a fixture.
//!
//! Besides the captures the gallery audits the wheel and exits with a failure if a name leaves
//! the screen, two names overlap, or a status or a different choice moves any name.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use mluva_core::agent_voice::{AgentKind, AgentStatus, HerdrPane, Target};
use mluva_gtk::agent_session_wheel::SessionWheel;
use mluva_gtk::theme::ThemeController;

const NORD: &str = "mode = \"dark\"\naccent = \"#81a1c1\"\nselection = \"#434c5e\"\nmuted = \"#4c566a\"\nbackground = \"#2e3440\"\nforeground = \"#d8dee9\"\nred = \"#bf616a\"\nyellow = \"#ebcb8b\"\ngreen = \"#a3be8c\"\n";

/// Session titles as people name them: short task meanings, a few long legacy or user names
/// shown whole, duplicates, Czech, CJK and emoji.
const TITLES: [&str; 30] = [
    "Release checklist",
    "Shortcut portal fix",
    "api-tests",
    "Onboarding copy",
    "api-tests",
    "Cold start profile",
    "Přehled zkoušek",
    "Investigate why the clipboard owner changes after suspend and resume on Omarchy",
    "修复标题渲染",
    "Migration notes",
    "Rebuild the settings page as one continuous scroll with section navigation",
    "docs",
    "Inbox triage 🙂",
    "Parakeet benchmark",
    "Narration port",
    "Chief of staff",
    "PR 112 review",
    "segmentation",
    "Widget breathing",
    "Release notes",
    "Retention audit",
    "Hosted sync",
    "Czech settings",
    "Fedora build check",
    "Model cache cleanup",
    "First-word latency",
    "Notices update",
    "Launch video",
    "Support reply",
    "Theme tokens",
];

fn pane(index: usize, guard: &str) -> Option<HerdrPane> {
    Some(HerdrPane {
        socket: "/run/user/fixture/herdr.sock".into(),
        server_epoch: "fixture".into(),
        pane_id: format!("w{}:p{}", index / 8 + 1, index % 8 + 1),
        guard: guard.into(),
    })
}

fn session(index: usize, status: AgentStatus) -> Target {
    Target {
        herdr: pane(index, &"g".repeat(64)),
        name: format!("agent-{}", index + 1),
        title: TITLES[index].into(),
        kind: if index % 3 == 1 {
            AgentKind::ClaudeCode
        } else {
            AgentKind::CodexCli
        },
        project: "/fixture/project".into(),
        status,
        ..Target::default()
    }
}

/// A Claude Code session Herdr lists but cannot send to, such as one typed into a pane.
fn unmanaged(index: usize, status: AgentStatus) -> Target {
    Target {
        herdr: pane(index, ""),
        kind: AgentKind::ClaudeCode,
        session_id: format!("fixture-session-{index}"),
        pid: 40_000 + index as u32,
        ..session(index, status)
    }
}

/// A Claude Code session found only in its own registry: no Herdr pane or socket.
fn native(index: usize, title: &str, status: AgentStatus) -> Target {
    Target {
        herdr: None,
        name: format!("project-{:02x}", 0x3a + index),
        title: title.into(),
        kind: AgentKind::ClaudeCode,
        project: "/fixture/project".into(),
        status,
        session_id: format!("fixture-native-{index}"),
        pid: 50_000 + index as u32,
        ..Target::default()
    }
}

/// Herdr-guarded, Herdr-unmanaged and registry-only sessions together, in that order.
fn mixed() -> Vec<Target> {
    vec![
        session(0, AgentStatus::Idle),
        session(1, AgentStatus::Working),
        unmanaged(3, AgentStatus::Idle),
        session(2, AgentStatus::Idle),
        unmanaged(9, AgentStatus::Working),
        native(0, "Cold start profile", AgentStatus::Idle),
        native(1, "", AgentStatus::Working),
    ]
}

/// Herdr is absent: Claude Code's registry alone, two sessions sharing a title.
fn native_only() -> Vec<Target> {
    vec![
        native(0, "Release checklist", AgentStatus::Idle),
        native(1, "Shortcut portal fix", AgentStatus::Working),
        native(2, "api-tests", AgentStatus::Idle),
        native(3, "api-tests", AgentStatus::Blocked),
        native(4, "", AgentStatus::Idle),
    ]
}

fn sessions(count: usize) -> Vec<Target> {
    (0..count)
        .map(|i| {
            let status = match i {
                5 | 13 => AgentStatus::Working,
                10 => AgentStatus::Blocked,
                _ => AgentStatus::Idle,
            };
            session(i, status)
        })
        .collect()
}

fn settle(frames: usize) {
    let context = glib::MainContext::default();
    for _ in 0..frames {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(16);
        while std::time::Instant::now() < deadline {
            context.iteration(false);
        }
    }
}

fn capture(window: &adw::Window, path: &Path) {
    let (width, height) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node().expect("rendered wheel");
    let renderer = window
        .native()
        .and_then(|n| n.renderer())
        .expect("renderer");
    let bounds = gtk::graphene::Rect::new(0., 0., width as f32, height as f32);
    renderer
        .render_texture(&node, Some(&bounds))
        .save_to_png(path)
        .expect("write capture");
    println!("{} {width}x{height}", path.display());
}

fn show(entries: &[Target], selected: Option<usize>, page: usize) -> SessionWheel {
    let wheel = SessionWheel::new();
    if std::env::args().nth(3).as_deref() == Some("overlay") {
        let screen = gtk::gdk::Display::default()
            .and_then(|display| display.monitors().item(0))
            .and_downcast::<gtk::gdk::Monitor>()
            .expect("private screen")
            .geometry();
        // The compositor would size the overlay to its output; here the window takes the screen.
        wheel.window.add_css_class("ml-wheel-overlay");
        wheel.window.set_resizable(true);
        wheel
            .window
            .set_default_size(screen.width(), screen.height());
    }
    wheel.page.set(page);
    wheel.window.present();
    wheel.update(entries, selected.map(|i| entries[i].clone()), "F12", "F9");
    // The hand sweeps in on a spring; wait for it and the fade to come to rest.
    settle(100);
    wheel
}

/// Every shown name's box in window coordinates.
fn name_boxes(wheel: &SessionWheel) -> Vec<gtk::graphene::Rect> {
    wheel
        .choices
        .iter()
        .filter(|choice| choice.is_visible())
        .map(|choice| {
            choice
                .compute_bounds(&wheel.window)
                .expect("name inside the window")
        })
        .collect()
}

/// Every name is on screen and no two overlap. Returns what is wrong, if anything.
fn audit(wheel: &SessionWheel, label: &str) -> Vec<String> {
    let (width, height) = (wheel.window.width() as f32, wheel.window.height() as f32);
    let boxes = name_boxes(wheel);
    let mut problems = vec![];
    for (index, one) in boxes.iter().enumerate() {
        if one.x() < 0.
            || one.y() < 0.
            || one.x() + one.width() > width
            || one.y() + one.height() > height
        {
            problems.push(format!(
                "{label}: name {index} leaves the {width}x{height} screen"
            ));
        }
        for (other, two) in boxes.iter().enumerate().skip(index + 1) {
            if one
                .intersection(two)
                .is_some_and(|common| common.width() > 0.5 && common.height() > 0.5)
            {
                problems.push(format!(
                    "{label}: names {index} and {other} overlap: ({},{} {}x{}) ({},{} {}x{}); placed {:?} {:?}",
                    one.x(), one.y(), one.width(), one.height(), two.x(), two.y(), two.width(), two.height(),
                    wheel.placements()[index], wheel.placements()[other]
                ));
            }
        }
    }
    problems
}

fn main() -> glib::ExitCode {
    let Some(root) = std::env::var_os("OFFSCREEN_SESSION_ROOT").map(PathBuf::from) else {
        eprintln!("Run this gallery inside the private off-screen desktop helper.");
        return glib::ExitCode::FAILURE;
    };
    let palette = std::env::args().nth(1).unwrap_or_else(|| "light".into());
    let output = std::env::args()
        .nth(2)
        .map_or_else(|| root.join("wheel-gallery"), PathBuf::from);
    std::fs::create_dir_all(&output).unwrap();
    gtk::init().unwrap();
    adw::init().unwrap();
    let theme_dir = root.join("state/omarchy/current/theme");
    std::fs::create_dir_all(&theme_dir).unwrap();
    match palette.as_str() {
        "nord" => std::fs::write(theme_dir.join("colors.toml"), NORD).unwrap(),
        "dark" => adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark),
        _ => adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight),
    }
    let _theme = ThemeController::apply(
        &theme_dir,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .expect("bundled fonts");

    let thirty = sessions(30);
    let mut missing = sessions(3);
    let mut gone = session(7, AgentStatus::Unknown);
    gone.herdr.as_mut().unwrap().socket = "/run/user/fixture/other.sock".into();
    missing.push(gone);
    let mut unnamed = sessions(4);
    unnamed[1].title.clear();
    unnamed[3].title.clear();
    unnamed[3].name = unnamed[3].locator();
    // Twenty with the last one found outside Herdr, as the design prototype shows it.
    let mut twenty_draft = sessions(20);
    twenty_draft[19] = unmanaged(19, AgentStatus::Idle);
    let scenarios: Vec<(&str, Vec<Target>, Option<usize>, usize)> = vec![
        ("one", sessions(1), Some(0), 0),
        ("three", sessions(3), Some(1), 0),
        ("six", sessions(6), Some(0), 0),
        ("twenty", sessions(20), Some(7), 0),
        ("twenty-draft", twenty_draft, Some(7), 0),
        ("twenty-blocked", sessions(20), Some(10), 0),
        ("missing-selected", missing, Some(3), 0),
        ("unnamed-fallback", unnamed, Some(0), 0),
        ("paged-elsewhere", thirty.clone(), Some(27), 0),
        ("paged-second", thirty, Some(27), 1),
        ("unselected", sessions(4), None, 0),
        ("empty", vec![], None, 0),
        ("mixed", mixed(), Some(1), 0),
        ("mixed-draft-selected", mixed(), Some(2), 0),
        ("native-only", native_only(), None, 0),
        ("native-only-draft-selected", native_only(), Some(2), 0),
    ];
    let mut problems = vec![];
    for (name, entries, selected, page) in scenarios {
        let wheel = show(&entries, selected, page);
        problems.extend(audit(&wheel, name));
        capture(&wheel.window, &output.join(format!("{palette}-{name}.png")));
        wheel.window.destroy();
        settle(2);
    }
    // A refresh that changes a status, or a different choice, moves no name.
    let mut entries = sessions(20);
    let wheel = show(&entries, Some(7), 0);
    let before = (wheel.placements(), name_boxes(&wheel));
    entries[3].status = AgentStatus::Blocked;
    entries[5].status = AgentStatus::Idle;
    entries[9].status = AgentStatus::Working;
    wheel.update(&entries, Some(entries[12].clone()), "F12", "F9");
    settle(100);
    let after = (wheel.placements(), name_boxes(&wheel));
    if before != after {
        problems.push("a status refresh or a new choice moved a name".into());
    }
    println!(
        "layout independent of status and choice: {}",
        before == after
    );
    wheel.window.destroy();
    settle(2);
    if !problems.is_empty() {
        for problem in &problems {
            eprintln!("{problem}");
        }
        return glib::ExitCode::FAILURE;
    }
    // Pointer hover on a choice, then keyboard focus on a choice, on Record and on Cancel.
    let entries = sessions(6);
    let wheel = show(&entries, Some(0), 0);
    wheel.choices[3].set_state_flags(gtk::StateFlags::PRELIGHT, false);
    settle(20);
    capture(&wheel.window, &output.join(format!("{palette}-hover.png")));
    wheel.choices[3].unset_state_flags(gtk::StateFlags::PRELIGHT);
    wheel.window.set_focus_visible(true);
    wheel.choices[4].grab_focus();
    settle(20);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-focus-choice.png")),
    );
    wheel.record.grab_focus();
    settle(20);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-focus-record.png")),
    );
    wheel.cancel.grab_focus();
    settle(20);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-focus-cancel.png")),
    );
    wheel.window.destroy();
    settle(2);
    // The hand swings to a new choice with the light trailing it; capture it in flight and at rest.
    let wheel = show(&entries, Some(0), 0);
    wheel.update(&entries, Some(entries[4].clone()), "F12", "F9");
    settle(4);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-swing-mid.png")),
    );
    settle(100);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-swing-end.png")),
    );
    glib::ExitCode::SUCCESS
}
