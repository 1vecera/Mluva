//! Render the production session wheel in synthetic states for private visual review.
//!
//! Run inside the off-screen desktop helper with `light`, `dark` or `nord`, then the output
//! directory and optionally `overlay`. The wheel sizes itself to the helper's screen, so set
//! its screen to the display being judged. X11 has no layer-shell, so `overlay` applies the
//! overlay styling to a screen-sized window. An overlay is see-through, so its captures keep
//! their alpha: composite them over a desktop to judge them. Every session name, pane, socket,
//! process and session ID is a fixture.
//!
//! Besides the captures the gallery audits the wheel and exits with a failure if a name leaves
//! the screen, two names overlap, or a status or a different choice moves any name or key.
//!
//! A last argument of `motion` renders the wheel's motion instead, frame by frame on a stepped
//! 60 fps clock: opening with 6 and with 20 sessions, a short hop, a hop across the dial, a new
//! choice mid-swing, a blocked choice and a page turn at 30 sessions. It writes the frames to
//! `motion/` and how long each took to snapshot and render to `motion-timings.json`.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use mluva_core::agent_voice::{
    AgentKind, AgentStatus, AppLink, ClaudeInbox, CodexThread, HerdrPane, Host, Target,
};
use mluva_gtk::agent_session_wheel::{KEY_REACH, SessionWheel};
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

/// A Claude Code session with its own inbox, found in Claude Code's registry.
fn inboxed(index: usize, title: &str, status: AgentStatus, host: Host) -> Target {
    Target {
        inbox: Some(ClaudeInbox {
            entry: format!("/fixture/claude/sessions/{}.json", 50_000 + index),
            socket: format!("/run/user/fixture/cc-socks/{}.sock", 50_000 + index),
            proc_start: "123456".into(),
        }),
        host,
        ..native(index, title, status)
    }
}

/// A live Codex thread outside Herdr, held open by a TUI or by Codex Desktop's server.
fn codex_thread(index: usize, title: &str, kind: AgentKind, status: AgentStatus) -> Target {
    let thread = format!("01900000-0000-7000-8000-0000000{index:05}");
    Target {
        kind,
        name: format!("codex-{}", &thread[thread.len() - 8..]),
        // Codex Desktop holds its threads in its own bundled server.
        host: if kind == AgentKind::CodexDesktop {
            Host::App
        } else {
            Host::Terminal
        },
        title: title.into(),
        project: "/fixture/project".into(),
        status,
        session_id: thread.clone(),
        pid: 60_000 + index as u32,
        codex: Some(CodexThread {
            lock: format!("/fixture/codex/thread-writer-locks/{thread}.lock"),
            holder: 60_000 + index as u32,
        }),
        ..Target::default()
    }
}

/// Every kind of target and every delivery level on one wheel, in discovery order: Herdr's
/// panes, Claude Code sessions outside Herdr, live Codex threads, then a new Claude chat.
fn universal() -> Vec<Target> {
    let mut codex_pane = session(2, AgentStatus::Working);
    codex_pane.title = "Release checklist".into();
    let mut claude_pane = session(1, AgentStatus::Idle);
    claude_pane.title = "Shortcut portal fix".into();
    let mut unguarded_codex = unmanaged(4, AgentStatus::Idle);
    unguarded_codex.kind = AgentKind::CodexCli;
    unguarded_codex.title = "Onboarding copy".into();
    unguarded_codex.session_id.clear();
    unguarded_codex.pid = 0;
    vec![
        claude_pane,
        codex_pane,
        unguarded_codex,
        inboxed(
            0,
            "Cold start profile",
            AgentStatus::Idle,
            Host::Tmux("work:@3.%7".into()),
        ),
        inboxed(1, "Přehled zkoušek", AgentStatus::Blocked, Host::Terminal),
        inboxed(2, "Migration notes", AgentStatus::Working, Host::Background),
        Target {
            kind: AgentKind::ClaudeDesktop,
            ..inboxed(3, "Inbox triage 🙂", AgentStatus::Idle, Host::App)
        },
        native(4, "Parakeet benchmark", AgentStatus::Idle),
        codex_thread(
            5,
            "Narration port",
            AgentKind::CodexCli,
            AgentStatus::Unknown,
        ),
        codex_thread(
            6,
            "Widget breathing",
            AgentKind::CodexDesktop,
            AgentStatus::Idle,
        ),
        Target {
            kind: AgentKind::ClaudeDesktop,
            name: "claude-new-chat".into(),
            title: "New chat".into(),
            status: AgentStatus::Idle,
            app: Some(AppLink::ClaudeNewChat),
            ..Target::default()
        },
    ]
}

/// One capture's fixture inputs: each target as the wheel received it.
fn describe(entries: &[Target], selected: Option<usize>, page: usize) -> serde_json::Value {
    serde_json::json!({
        "page": page,
        "chosen": selected,
        "targets": entries.iter().map(|t| serde_json::json!({
            "name": t.display_name(),
            "kind": t.kind.label(),
            "place": t.place(),
            "status": t.status,
            "route": t.route().as_str(),
            "delivery": t.delivery(),
            "receives_now": t.can_receive(),
        })).collect::<Vec<_>>(),
    })
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

/// A wheel for the gallery, styled as the overlay when asked, not yet shown.
fn wheel() -> SessionWheel {
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
    wheel
}

fn show(entries: &[Target], selected: Option<usize>, page: usize) -> SessionWheel {
    let wheel = wheel();
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

/// Every shown name's key letter, in window coordinates: it must stay beside its tick whatever
/// caption follows the name.
fn key_boxes(wheel: &SessionWheel) -> Vec<gtk::graphene::Rect> {
    let mut keys = vec![];
    for choice in wheel.choices.iter().filter(|choice| choice.is_visible()) {
        let mut child = choice.first_child().and_then(|body| body.first_child());
        while let Some(widget) = child {
            if widget.has_css_class("ml-wheel-key") {
                keys.push(
                    widget
                        .compute_bounds(&wheel.window)
                        .expect("key inside the window"),
                );
            }
            child = widget.next_sibling();
        }
    }
    keys
}

/// How far each shown key's centre is from the outer end of its own tick, in pixels.
fn key_distances(wheel: &SessionWheel) -> Vec<f64> {
    let placements = wheel.placements();
    name_boxes(wheel)
        .iter()
        .zip(key_boxes(wheel))
        .zip(&placements)
        .map(|((name, key), placement)| {
            // The ring centre in window coordinates, from where this name was placed.
            let (cx, cy) = (
                f64::from(name.x()) - placement.x,
                f64::from(name.y()) - placement.y,
            );
            let (tx, ty) = (cx + placement.tick.0, cy + placement.tick.1);
            let (kx, ky) = (
                f64::from(key.x() + key.width() / 2.),
                f64::from(key.y() + key.height() / 2.),
            );
            (kx - tx).hypot(ky - ty)
        })
        .collect()
}

/// Every control the wheel shows, by name, in window coordinates: the names, then the legend's
/// entries and the pager when it shows.
fn controls(wheel: &SessionWheel) -> Vec<(String, gtk::Widget)> {
    let names = wheel
        .choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| choice.is_visible())
        .map(|(index, choice)| (format!("name {index}"), choice.clone().upcast()));
    let legend = [
        ("record", wheel.record.clone().upcast::<gtk::Widget>()),
        ("transcript", wheel.transcript.clone().upcast()),
        ("cancel", wheel.cancel.clone().upcast()),
        ("pager", wheel.pager.clone().upcast()),
    ]
    .into_iter()
    .filter(|(_, widget)| widget.is_visible())
    .map(|(name, widget)| (name.to_owned(), widget));
    names.chain(legend).collect()
}

/// Every name and legend entry is on screen, none overlaps another, and none is squeezed below
/// its natural width, which would let its text run over its neighbour. Returns what is wrong.
fn audit(wheel: &SessionWheel, label: &str) -> Vec<String> {
    let (width, height) = (wheel.window.width() as f32, wheel.window.height() as f32);
    let controls = controls(wheel);
    let boxes: Vec<gtk::graphene::Rect> = controls
        .iter()
        .map(|(_, widget)| {
            widget
                .compute_bounds(&wheel.window)
                .expect("control inside the window")
        })
        .collect();
    let mut problems = vec![];
    let distances = key_distances(wheel);
    if let Some((index, worst)) = distances
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))
    {
        println!("{label}: farthest key from its tick: {worst:.0} px (name {index})");
        if worst > KEY_REACH {
            problems.push(format!(
                "{label}: key of name {index} is {worst:.0} px from its tick"
            ));
        }
    }
    for (index, ((name, widget), one)) in controls.iter().zip(&boxes).enumerate() {
        if one.x() < 0.
            || one.y() < 0.
            || one.x() + one.width() > width
            || one.y() + one.height() > height
        {
            problems.push(format!(
                "{label}: {name} leaves the {width}x{height} screen"
            ));
        }
        let (_, natural, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
        if one.width() + 0.5 < natural as f32 {
            problems.push(format!(
                "{label}: {name} is squeezed to {} of its {natural} px",
                one.width()
            ));
        }
        for ((other, _), two) in controls.iter().zip(&boxes).skip(index + 1) {
            if one
                .intersection(two)
                .is_some_and(|common| common.width() > 0.5 && common.height() > 0.5)
            {
                problems.push(format!(
                    "{label}: {name} and {other} overlap: ({},{} {}x{}) ({},{} {}x{})",
                    one.x(),
                    one.y(),
                    one.width(),
                    one.height(),
                    two.x(),
                    two.y(),
                    two.width(),
                    two.height(),
                ));
            }
        }
    }
    problems
}

/// Render one frame: the seconds the snapshot and render took, and the frame written as PNG.
fn frame(window: &adw::Window, path: &Path) -> f64 {
    let (width, height) = (window.width(), window.height());
    let started = std::time::Instant::now();
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node().expect("rendered wheel");
    let renderer = window
        .native()
        .and_then(|n| n.renderer())
        .expect("renderer");
    let bounds = gtk::graphene::Rect::new(0., 0., width as f32, height as f32);
    let texture = renderer.render_texture(&node, Some(&bounds));
    let seconds = started.elapsed().as_secs_f64();
    texture.save_to_png(path).expect("write frame");
    seconds
}

/// The motion reel: each scene stepped at 60 fps, every frame rendered and timed.
fn motion(output: &Path) -> glib::ExitCode {
    const STEP: f64 = 1. / 60.;
    let frames = output.join("motion");
    std::fs::create_dir_all(&frames).unwrap();
    let mut index = 0;
    let mut timings: Vec<(String, f64)> = vec![];
    // Every frame's scene and the hand's bearing, for a plot of the hand against time.
    let mut hands: Vec<serde_json::Value> = vec![];
    let mut play = |wheel: &SessionWheel, scene: &str, count: usize| {
        for _ in 0..count {
            let hand = wheel.step_motion(STEP);
            // Let the frame clock paint the step; the stepped clock alone moves the dial.
            settle(1);
            let seconds = frame(&wheel.window, &frames.join(format!("{index:05}.png")));
            timings.push((scene.to_owned(), seconds));
            hands.push(serde_json::json!({"frame": index, "scene": scene, "hand": hand}));
            index += 1;
        }
    };
    // Open a wheel as the application does: filled before it maps, then shown.
    let open = |entries: &[Target], selected: usize| {
        let wheel = wheel();
        wheel.step_motion(0.);
        wheel.update(entries, Some(entries[selected].clone()), "F12", "F9");
        wheel.window.present();
        // Wait until it is mapped and laid out; the stepped clock holds the motion at its start.
        for _ in 0..300 {
            settle(1);
            if wheel.window.is_mapped() && wheel.window.width() > 0 {
                break;
            }
        }
        settle(100);
        wheel
    };
    // How long each new choice takes before the swing's first frame can be drawn.
    let updates = std::cell::RefCell::new(vec![]);
    let choose = |wheel: &SessionWheel, entries: &[Target], selected: usize| {
        let started = std::time::Instant::now();
        wheel.update(entries, Some(entries[selected].clone()), "F12", "F9");
        updates
            .borrow_mut()
            .push(started.elapsed().as_secs_f64() * 1000.);
        glib::MainContext::default().iteration(false);
    };

    let six = sessions(6);
    let wheel = open(&six, 0);
    play(&wheel, "open-6", 45);
    wheel.window.destroy();
    settle(2);

    let twenty = sessions(20);
    let wheel = open(&twenty, 7);
    play(&wheel, "open-20", 45);
    choose(&wheel, &twenty, 8);
    play(&wheel, "short-hop", 30);
    choose(&wheel, &twenty, 17);
    play(&wheel, "across", 40);
    choose(&wheel, &twenty, 3);
    play(&wheel, "retarget", 5);
    choose(&wheel, &twenty, 1);
    play(&wheel, "retarget", 35);
    choose(&wheel, &twenty, 10);
    play(&wheel, "blocked", 45);
    wheel.window.destroy();
    settle(2);

    let thirty = sessions(30);
    let wheel = open(&thirty, 2);
    play(&wheel, "open-30", 36);
    wheel.page.set(1);
    choose(&wheel, &thirty, 2);
    play(&wheel, "page-turn", 30);
    wheel.window.destroy();

    let mut scenes: Vec<serde_json::Value> = vec![];
    let mut names: Vec<&str> = timings.iter().map(|(scene, _)| scene.as_str()).collect();
    names.dedup();
    for name in names {
        let mut ms: Vec<f64> = timings
            .iter()
            .filter(|(scene, _)| scene == name)
            .map(|(_, seconds)| seconds * 1000.)
            .collect();
        ms.sort_by(f64::total_cmp);
        scenes.push(serde_json::json!({
            "scene": name,
            "frames": ms.len(),
            "median_ms": ms[ms.len() / 2],
            "p95_ms": ms[(ms.len() * 95 / 100).min(ms.len() - 1)],
            "max_ms": ms[ms.len() - 1],
        }));
    }
    std::fs::write(
        output.join("motion-timings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "fps": 60,
            "frames": index,
            "measures": "snapshot of the whole window and render to a texture with the active GSK renderer, per frame",
            "scenes": scenes,
            "choice_update_ms": *updates.borrow(),
            "hands": hands,
        }))
        .unwrap(),
    )
    .unwrap();
    println!("motion frames: {index}");
    glib::ExitCode::SUCCESS
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
    if std::env::args().nth(4).as_deref() == Some("motion") {
        return motion(&output);
    }

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
        // Every kind and delivery level together, each level chosen in turn.
        ("universal-send-herdr", universal(), Some(0), 0),
        ("universal-queue-tmux", universal(), Some(3), 0),
        ("universal-queue-waiting", universal(), Some(4), 0),
        ("universal-queue-desktop", universal(), Some(6), 0),
        ("universal-draft-only", universal(), Some(7), 0),
        ("universal-send-codex", universal(), Some(8), 0),
        ("universal-send-codex-desktop", universal(), Some(9), 0),
        ("universal-prefill-new-chat", universal(), Some(10), 0),
    ];
    let mut problems = vec![];
    let mut index = serde_json::Map::new();
    for (name, entries, selected, page) in scenarios {
        let wheel = show(&entries, selected, page);
        problems.extend(audit(&wheel, name));
        let file = format!("{palette}-{name}.png");
        capture(&wheel.window, &output.join(&file));
        index.insert(file, describe(&entries, selected, page));
        wheel.window.destroy();
        settle(2);
    }
    // A refresh that changes a status, or a different choice, moves no name and no key.
    let mut entries = sessions(20);
    let wheel = show(&entries, Some(7), 0);
    let before = (wheel.placements(), name_boxes(&wheel), key_boxes(&wheel));
    entries[3].status = AgentStatus::Blocked;
    entries[5].status = AgentStatus::Idle;
    entries[9].status = AgentStatus::Working;
    wheel.update(&entries, Some(entries[12].clone()), "F12", "F9");
    settle(100);
    let after = (wheel.placements(), name_boxes(&wheel), key_boxes(&wheel));
    if before != after {
        problems.push("a status refresh or a new choice moved a name or a key".into());
    }
    println!(
        "layout independent of status and choice: {}",
        before == after
    );
    wheel.window.destroy();
    settle(2);
    std::fs::write(
        output.join(format!("{palette}-index.json")),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();
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
