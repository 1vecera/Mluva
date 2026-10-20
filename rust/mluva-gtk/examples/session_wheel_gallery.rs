//! Render the production session wheel in synthetic states for private visual review.
//!
//! Run inside the off-screen desktop helper with `light`, `dark` or `nord`.
//! Every session name, pane and socket is a fixture.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use mluva_core::agent_voice::{AgentStatus, Recipient};
use mluva_gtk::agent_session_wheel::SessionWheel;
use mluva_gtk::theme::ThemeController;

const NORD: &str = "mode = \"dark\"\naccent = \"#81a1c1\"\nselection = \"#434c5e\"\nmuted = \"#4c566a\"\nbackground = \"#2e3440\"\nforeground = \"#d8dee9\"\nred = \"#bf616a\"\nyellow = \"#ebcb8b\"\ngreen = \"#a3be8c\"\n";

fn session(name: &str, pane: &str, status: AgentStatus) -> Recipient {
    Recipient {
        socket: "/run/user/fixture/herdr.sock".into(),
        server_epoch: "fixture".into(),
        pane_id: pane.into(),
        guard: "g".repeat(64),
        name: name.into(),
        kind: "codex".into(),
        project: "/fixture/project".into(),
        status,
    }
}

fn sessions(names: &[&str]) -> Vec<Recipient> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| session(name, &format!("w1:p{}", i + 1), AgentStatus::Idle))
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

    let nine = sessions(&[
        "Chief of staff",
        "api-tests",
        "release notes",
        "design review",
        "api-tests",
        "build fixer",
        "inbox triage",
        "translator",
        "data cleanup",
    ]);
    let mut mixed = sessions(&[
        "Chief of staff",
        "long-session-name-for-regression",
        "api-tests",
        "api-tests",
        "Documentation site implementer",
        "ops",
    ]);
    mixed[2].status = AgentStatus::Working;
    mixed[4].status = AgentStatus::Blocked;
    let mut missing = sessions(&["Chief of staff", "api-tests", "release notes"]);
    let mut gone = session("design review", "w2:p4", AgentStatus::Unknown);
    gone.socket = "/run/user/fixture/other.sock".into();
    missing.push(gone.clone());
    let scenarios: Vec<(&str, Vec<Recipient>, Option<usize>, usize)> = vec![
        ("six", nine[..6].to_vec(), Some(0), 0),
        ("paged", nine.clone(), Some(1), 0),
        ("paged-second", nine.clone(), Some(7), 1),
        ("paged-elsewhere", nine.clone(), Some(7), 0),
        ("one", nine[..1].to_vec(), Some(0), 0),
        ("two", nine[..2].to_vec(), Some(1), 0),
        ("three", nine[..3].to_vec(), Some(2), 0),
        ("mixed", mixed.clone(), Some(1), 0),
        ("blocked-selected", mixed, Some(4), 0),
        ("missing-selected", missing, Some(3), 0),
        ("unselected", nine[..4].to_vec(), None, 0),
        ("empty", vec![], None, 0),
    ];
    for (name, entries, selected, page) in scenarios {
        let wheel = SessionWheel::new();
        wheel.page.set(page);
        wheel.update(&entries, selected.map(|i| entries[i].clone()), "F12", "F9");
        wheel.window.present();
        settle(30);
        capture(&wheel.window, &output.join(format!("{palette}-{name}.png")));
        wheel.window.destroy();
        settle(2);
    }
    // Pointer hover on one choice and keyboard focus on the hub, then on a choice.
    let entries = nine[..6].to_vec();
    let wheel = SessionWheel::new();
    wheel.update(&entries, Some(entries[0].clone()), "F12", "F9");
    wheel.window.present();
    settle(30);
    wheel.choices[3].set_state_flags(gtk::StateFlags::PRELIGHT, false);
    wheel.window.set_focus_visible(true);
    wheel.record.grab_focus();
    settle(10);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-hover-focus.png")),
    );
    wheel.choices[3].unset_state_flags(gtk::StateFlags::PRELIGHT);
    wheel.choices[1].grab_focus();
    settle(10);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-focus-choice.png")),
    );
    wheel.window.destroy();
    settle(2);
    // The needle swings to a new choice; capture it in flight and at rest.
    let entries = nine[..6].to_vec();
    let wheel = SessionWheel::new();
    wheel.update(&entries, Some(entries[0].clone()), "F12", "F9");
    wheel.window.present();
    settle(30);
    wheel.update(&entries, Some(entries[4].clone()), "F12", "F9");
    settle(5);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-swing-mid.png")),
    );
    settle(20);
    capture(
        &wheel.window,
        &output.join(format!("{palette}-swing-end.png")),
    );
    glib::ExitCode::SUCCESS
}
