//! Record the production session wheel for the launch film: one lossless 60 fps take of the
//! see-through dial opening over a busy synthetic desktop, then the hand and its light trace
//! moving between sessions.
//!
//! Copy this file into `rust/mluva-gtk/examples/`, build it there and run it through
//! `launch-video/launch/capture/wheel_take.sh` (see `launch-video/launch/README.md`). It runs in a
//! private headless sway, so the wheel is the same layer-shell overlay the app opens on Hyprland
//! and the compositor blends its translucent wash over the real windows behind it.
//!
//! The wheel's motion runs on its own stepped clock (`SessionWheel::step_motion`, the production
//! hook for frame-by-frame captures): every frame advances it exactly 1/60 s, the driver waits
//! for GTK to paint that frame and for the compositor to receive it, and grabs the composited
//! output with `grim`. However slowly the software renderer draws, the take plays at real speed.
//! Every session name, pane, socket, process and session ID below is a fixture.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use adw::prelude::*;
use mluva_core::agent_voice::{
    AgentKind, AgentStatus, AppLink, ClaudeInbox, CodexThread, HerdrPane, Host, Target,
};
use mluva_gtk::agent_session_wheel::SessionWheel;
use mluva_gtk::theme::ThemeController;

const STEP: f64 = 1. / 60.;

fn seconds(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn frames(seconds: f64) -> usize {
    (seconds * 60.).round() as usize
}

fn herdr(pane: &str) -> Option<HerdrPane> {
    Some(HerdrPane {
        socket: "/run/user/fixture/herdr.sock".into(),
        server_epoch: "fixture".into(),
        pane_id: pane.into(),
        guard: "g".repeat(64),
    })
}

fn base(kind: AgentKind, title: &str, index: u32, status: AgentStatus) -> Target {
    Target {
        kind,
        name: format!("fixture-{index}"),
        title: title.into(),
        project: "/fixture/project".into(),
        status,
        session_id: format!("fixture-session-{index}"),
        pid: 60_000 + index,
        ..Target::default()
    }
}

/// A Claude Code session whose registry entry names its inbox.
fn inboxed(kind: AgentKind, title: &str, index: u32, host: Host) -> Target {
    Target {
        inbox: Some(ClaudeInbox {
            entry: format!("/fixture/claude/sessions/{}.json", 60_000 + index),
            socket: format!("/run/user/fixture/cc-socks/{}.sock", 60_000 + index),
            proc_start: "123456".into(),
        }),
        host,
        ..base(kind, title, index, AgentStatus::Idle)
    }
}

/// A live Codex thread outside Herdr, held open by a TUI or by Codex Desktop's server.
fn codex_thread(kind: AgentKind, title: &str, index: u32, host: Host) -> Target {
    let thread = format!("01900000-0000-7000-8000-0000000{index:05}");
    Target {
        name: format!("codex-{}", &thread[thread.len() - 8..]),
        session_id: thread.clone(),
        codex: Some(CodexThread {
            lock: format!("/fixture/codex/thread-writer-locks/{thread}.lock"),
            holder: 60_000 + index,
        }),
        host,
        ..base(kind, title, index, AgentStatus::Idle)
    }
}

/// A working desk in discovery order: Herdr's panes, Claude Code sessions outside Herdr, live
/// Codex threads, then a new Claude chat. The delivery take's disposable sessions carry the same
/// names: Codex "Release notes" and Claude Code "Portal test", both in tmux.
fn sessions() -> Vec<Target> {
    vec![
        Target {
            herdr: herdr("w1:p1"),
            ..base(
                AgentKind::ClaudeCode,
                "Cold start profile",
                1,
                AgentStatus::Idle,
            )
        },
        Target {
            herdr: herdr("w1:p2"),
            ..base(AgentKind::CodexCli, "api-tests", 2, AgentStatus::Working)
        },
        inboxed(
            AgentKind::ClaudeCode,
            "Portal test",
            3,
            Host::Tmux("work:@0.%1".into()),
        ),
        inboxed(AgentKind::ClaudeCode, "Onboarding copy", 4, Host::Terminal),
        inboxed(AgentKind::ClaudeDesktop, "Inbox triage", 5, Host::App),
        codex_thread(
            AgentKind::CodexCli,
            "Release notes",
            6,
            Host::Tmux("work:@0.%0".into()),
        ),
        codex_thread(AgentKind::CodexDesktop, "Widget breathing", 7, Host::App),
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

fn main() -> glib::ExitCode {
    let (Some(output), Ok(_)) = (
        std::env::args().nth(1).map(PathBuf::from),
        std::env::var("WAYLAND_DISPLAY"),
    ) else {
        eprintln!("Run this take inside launch-video/launch/capture/wheel_take.sh.");
        return glib::ExitCode::FAILURE;
    };
    let theme = std::env::var("MLUVA_TAKE_THEME").unwrap_or_else(|_| "vantablack".into());
    // Claude Code in Herdr, Claude Desktop, Claude Code in tmux, Codex in tmux.
    let order: Vec<usize> = std::env::var("MLUVA_TAKE_ORDER")
        .unwrap_or_else(|_| "0,4,2,5".into())
        .split(',')
        .map(|v| v.trim().parse().expect("MLUVA_TAKE_ORDER lists indices"))
        .collect();
    let lead = frames(seconds("MLUVA_TAKE_LEAD", 0.6));
    let first_hold = frames(seconds("MLUVA_TAKE_FIRST_HOLD", 1.5));
    let hold = frames(seconds("MLUVA_TAKE_HOLD", 1.4));
    let tail = frames(seconds("MLUVA_TAKE_TAIL", 2.0));

    gtk::init().unwrap();
    adw::init().unwrap();
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap();
    let theme_dir = state.join("omarchy/current/theme");
    std::fs::create_dir_all(&theme_dir).unwrap();
    std::fs::copy(
        format!("/usr/share/omarchy/themes/{theme}/colors.toml"),
        theme_dir.join("colors.toml"),
    )
    .expect("installed Omarchy theme");
    let _theme = ThemeController::apply(
        &theme_dir,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .expect("bundled fonts");

    let entries = sessions();
    let wheel = SessionWheel::new();
    if !wheel.overlay {
        eprintln!("The compositor offers no layer-shell: the wheel would not be the overlay.");
        return glib::ExitCode::FAILURE;
    }
    let frames_dir = output.join("frames");
    std::fs::create_dir_all(&frames_dir).unwrap();
    let context = glib::MainContext::default();
    let display = gtk::gdk::Display::default().unwrap();
    let grab = |frame: usize| {
        display.sync();
        let png = frames_dir.join(format!("{frame:05}.png"));
        let status = Command::new("grim")
            .args(["-t", "png", "-l", "1"])
            .arg(&png)
            .status()
            .expect("grim");
        assert!(status.success(), "grab frame {frame}");
    };
    let settle = |ms: u64| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        while std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    };

    // The desktop before the agent key: one grab per frame of the lead.
    for frame in 0..lead {
        settle(5);
        grab(frame);
    }
    // Opened as the application opens it: filled before it maps, then shown. The stepped clock
    // holds the opening at its first instant until the first step.
    wheel.step_motion(0.);
    wheel.update(&entries, Some(entries[order[0]].clone()), "F12", "F9");
    wheel.window.present();
    for _ in 0..600 {
        settle(5);
        if wheel.window.is_mapped() && wheel.window.width() > 0 {
            break;
        }
    }
    settle(400);
    let painted = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = painted.clone();
    wheel
        .window
        .frame_clock()
        .expect("realized wheel")
        .connect_after_paint(move |_| flag.set(true));

    let mut moves = vec![(lead, order[0])];
    let mut at = lead + first_hold;
    for index in &order[1..] {
        moves.push((at, *index));
        at += hold;
    }
    let total = at - hold + tail;
    let mut timeline = vec![];
    let mut hands = vec![];
    let mut next = moves.iter().peekable();
    for frame in lead..total {
        let mut event = None;
        while let Some((when, index)) = next.peek() {
            if *when > frame {
                break;
            }
            if frame > lead {
                wheel.update(&entries, Some(entries[*index].clone()), "F12", "F9");
            }
            event = Some(if frame == lead { "open" } else { "select" });
            timeline.push(serde_json::json!({
                "frame": frame,
                "event": event,
                "selected": index,
                "name": entries[*index].display_name(),
                "kind": entries[*index].kind.label(),
                "place": entries[*index].place(),
            }));
            next.next();
        }
        painted.set(false);
        let hand = wheel.step_motion(if frame == lead { 0. } else { STEP });
        let started = std::time::Instant::now();
        while !painted.get() {
            context.iteration(false);
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "frame {frame} never painted"
            );
        }
        settle(8);
        grab(frame);
        hands.push(serde_json::json!({"frame": frame, "hand": hand, "event": event}));
    }

    let take = output.join("wheel-take.mkv");
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-y", "-v", "error", "-framerate", "60", "-i"])
        .arg(frames_dir.join("%05d.png"))
        .args(["-c:v", "ffv1", "-level", "3", "-pix_fmt", "bgr0"])
        .arg(&take)
        .status()
        .expect("ffmpeg");
    let monitor = display
        .monitors()
        .item(0)
        .and_downcast::<gtk::gdk::Monitor>()
        .unwrap();
    let fixtures: Vec<_> = entries
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.display_name(),
                "kind": t.kind.label(),
                "place": t.place(),
                "route": t.route().as_str(),
                "delivery": t.delivery().verb(),
            })
        })
        .collect();
    let record = serde_json::json!({
        "boundary": "Production SessionWheel (mluva-gtk) as a wlr-layer-shell overlay in a private headless sway with the pixman renderer; fixture sessions; no Herdr, agent, input device or network",
        "logical_screen": [monitor.geometry().width(), monitor.geometry().height()],
        "scale": monitor.scale_factor(),
        "theme": theme,
        "agent_key": "F12",
        "fixtures": fixtures,
        "frames": total,
        "fps": 60,
        "clock": "SessionWheel::step_motion: the wheel's own stepped clock, 1/60 s per frame from the opening; grim screencopy of the composited output after each paint",
        "lead_frames_without_wheel": lead,
        "timeline": timeline,
        "hand_bearing_per_frame": hands,
        "ffmpeg_status": status.code(),
    });
    let mut file = std::fs::File::create(output.join("wheel-take.json")).unwrap();
    writeln!(file, "{}", serde_json::to_string_pretty(&record).unwrap()).unwrap();
    println!("{}", take.display());
    if status.success() {
        glib::ExitCode::SUCCESS
    } else {
        glib::ExitCode::FAILURE
    }
}
