//! Record the production session wheel for the Mluva showreel: one lossless 60 fps take of the
//! overlay wheel on the private X11 screen while the choice steps through fixture sessions once
//! per beat.
//!
//! Copy this file into `rust/mluva-gtk/examples/` of a checkout that has the target model, build
//! it there and run it through `dev/run-isolated.sh` at the screen size and scale to record (see
//! `launch-video/showreel/README.md`). The wheel window takes the whole private screen, as the
//! overlay does on Omarchy, and nothing else is on that display. Run it with `take_clock.so`
//! preloaded: the driver steps the process's monotonic clock one 1/60 s frame at a time, waits for
//! GTK to paint that frame and grabs the screen with FFmpeg's `x11grab` as a PNG, so the wheel's
//! own spring and fades are recorded frame by frame at their real speed, however slowly the
//! software renderer draws.
//! Every session name, pane, socket, process and session ID below is a fixture.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use adw::prelude::*;
use mluva_core::agent_target::{AppLink, ClaudeInbox, CodexThread, Host};
use mluva_core::agent_voice::{AgentKind, AgentStatus, HerdrPane, Target};
use mluva_gtk::agent_session_wheel::SessionWheel;
use mluva_gtk::theme::ThemeController;

/// The score's beat: the choice moves once per beat.
const BEAT: f64 = 60.0 / 128.0;
/// Recording before the wheel opens, and after the last move.
const LEAD: f64 = 0.8;
const HOLD: f64 = 1.6;

fn herdr(pane: &str) -> Option<HerdrPane> {
    Some(HerdrPane {
        socket: "/run/user/fixture/herdr.sock".into(),
        server_epoch: "fixture".into(),
        pane_id: pane.into(),
        guard: "g".repeat(64),
    })
}

fn target(kind: AgentKind, title: &str, index: u32) -> Target {
    Target {
        kind,
        name: format!("fixture-{index}"),
        title: title.into(),
        project: "/fixture/project".into(),
        status: AgentStatus::Idle,
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
        ..target(kind, title, index)
    }
}

/// A live Codex thread outside Herdr, held open by a TUI or by Codex Desktop's server.
fn codex_thread(kind: AgentKind, title: &str, index: u32, host: Host) -> Target {
    let thread = format!("01900000-0000-7000-8000-0000000{index:05}");
    Target {
        name: format!("codex-{}", &thread[..8]),
        session_id: thread.clone(),
        codex: Some(CodexThread {
            lock: format!("/fixture/codex/thread-writer-locks/{thread}.lock"),
            holder: 60_000 + index,
        }),
        host,
        ..target(kind, title, index)
    }
}

/// One session of every kind and route the reel names, in discovery order: Herdr's panes,
/// Claude Code sessions outside Herdr, live Codex threads, then a new Claude chat.
fn sessions() -> Vec<Target> {
    vec![
        Target {
            herdr: herdr("w1:p1"),
            ..target(AgentKind::ClaudeCode, "Cold start profile", 1)
        },
        Target {
            herdr: herdr("w1:p2"),
            ..target(AgentKind::CodexCli, "api-tests", 2)
        },
        // The same names as the disposable sessions in the delivery take.
        inboxed(
            AgentKind::ClaudeCode,
            "Portal test",
            3,
            Host::Tmux("work:@0.%1".into()),
        ),
        inboxed(AgentKind::ClaudeCode, "Onboarding copy", 4, Host::Terminal),
        inboxed(AgentKind::ClaudeDesktop, "Inbox triage", 5, Host::App),
        // Found in Claude Code's registry without an inbox, such as a `--bare` session.
        target(AgentKind::ClaudeCode, "Parakeet benchmark", 6),
        codex_thread(AgentKind::CodexCli, "Release notes", 7, Host::Terminal),
        codex_thread(AgentKind::CodexDesktop, "Widget breathing", 8, Host::App),
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

/// The capture clock from `take_clock.so`: once set, the process's monotonic time is whatever
/// the driver says, so every animation advances exactly one frame per step.
fn clock_setter() -> Option<extern "C" fn(i64)> {
    unsafe extern "C" {
        fn dlsym(
            handle: *mut std::ffi::c_void,
            symbol: *const std::ffi::c_char,
        ) -> *mut std::ffi::c_void;
    }
    let symbol = unsafe { dlsym(std::ptr::null_mut(), c"mluva_take_clock_set".as_ptr()) };
    (!symbol.is_null()).then(|| unsafe {
        std::mem::transmute::<*mut std::ffi::c_void, extern "C" fn(i64)>(symbol)
    })
}

fn monotonic_ns() -> i64 {
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
    }
    let mut ts = Timespec { sec: 0, nsec: 0 };
    unsafe { clock_gettime(1, &mut ts) };
    ts.sec * 1_000_000_000 + ts.nsec
}

/// GDK's frame interval, so a step always lands on the frame clock's next frame.
const FRAME_NS: i64 = 16_667_000;

fn main() -> glib::ExitCode {
    let (Some(root), Some(output), Some(display)) = (
        std::env::var_os("OFFSCREEN_SESSION_ROOT").map(PathBuf::from),
        std::env::var_os("OFFSCREEN_ARTIFACT_DIR").map(PathBuf::from),
        std::env::var("DISPLAY").ok(),
    ) else {
        eprintln!("Run this take inside dev/run-isolated.sh.");
        return glib::ExitCode::FAILURE;
    };
    let theme = std::env::var("MLUVA_TAKE_THEME").unwrap_or_else(|_| "vantablack".into());
    let order: Vec<usize> = std::env::var("MLUVA_TAKE_ORDER")
        .unwrap_or_else(|_| "0,2,4,8,1,5,3,6".into())
        .split(',')
        .map(|v| v.trim().parse().expect("MLUVA_TAKE_ORDER lists indices"))
        .collect();
    gtk::init().unwrap();
    adw::init().unwrap();
    let theme_dir = root.join("state/omarchy/current/theme");
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

    let monitor = gtk::gdk::Display::default()
        .and_then(|d| d.monitors().item(0))
        .and_downcast::<gtk::gdk::Monitor>()
        .expect("private screen");
    let geometry = monitor.geometry();
    let scale = monitor.scale_factor();
    let (width, height) = (geometry.width() * scale, geometry.height() * scale);

    let entries = sessions();
    let wheel = SessionWheel::new();
    wheel.window.add_css_class("ml-wheel-overlay");
    wheel.window.set_resizable(true);
    wheel
        .window
        .set_default_size(geometry.width(), geometry.height());

    let set_clock = clock_setter().expect("preload take_clock.so");
    let frames_dir = output.join("frames");
    std::fs::create_dir_all(&frames_dir).unwrap();
    let painted = std::rc::Rc::new(std::cell::Cell::new(false));
    let paint_times = std::rc::Rc::new(std::cell::RefCell::new(Vec::<i64>::new()));
    let context = glib::MainContext::default();
    let origin = monotonic_ns();
    set_clock(origin);

    let lead = (LEAD * 60.0).round() as usize;
    let last_move = lead + ((order.len() - 1) as f64 * BEAT * 60.0).round() as usize;
    let total = last_move + (HOLD * 60.0).round() as usize;
    let mut timeline = vec![];
    let mut moves = order.iter().enumerate().skip(1).peekable();
    let grab = |frame: usize| {
        gtk::gdk::Display::default().unwrap().sync();
        let png = frames_dir.join(format!("{frame:05}.png"));
        let status = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-f",
                "x11grab",
                "-draw_mouse",
                "0",
            ])
            .args(["-video_size", &format!("{width}x{height}")])
            .args(["-i", &format!("{display}+0,0"), "-frames:v", "1"])
            .arg(&png)
            .env_remove("LD_PRELOAD")
            .status()
            .expect("ffmpeg");
        assert!(status.success(), "grab frame {frame}");
    };
    // Before the wheel opens the screen is empty: one grab per 1/60 s.
    for frame in 0..lead {
        set_clock(origin + frame as i64 * FRAME_NS);
        for _ in 0..50 {
            context.iteration(false);
        }
        grab(frame);
    }
    // Then the clock moves in 1 ms ticks and every frame GDK paints is grabbed: the take holds
    // exactly the frames the app drew, at the frame times its own frame clock chose.
    let mut now = origin + lead as i64 * FRAME_NS;
    set_clock(now);
    wheel.window.present();
    wheel.update(&entries, Some(entries[order[0]].clone()), "F12", "F9");
    let flag = painted.clone();
    let times = paint_times.clone();
    wheel
        .window
        .frame_clock()
        .expect("realized wheel")
        .connect_after_paint(move |clock| {
            flag.set(true);
            times.borrow_mut().push(clock.frame_time());
        });
    timeline.push(serde_json::json!({"frame": lead, "event": "open", "selected": order[0]}));
    let mut frame = lead;
    let mut idle_ticks = 0;
    while frame < total {
        while let Some((step, index)) = moves.peek() {
            if frame < lead + (*step as f64 * BEAT * 60.0).round() as usize {
                break;
            }
            wheel.update(&entries, Some(entries[**index].clone()), "F12", "F9");
            timeline
                .push(serde_json::json!({"frame": frame, "event": "select", "selected": index}));
            moves.next();
        }
        wheel.window.queue_draw();
        for _ in 0..200 {
            context.iteration(false);
            if painted.get() {
                break;
            }
        }
        if painted.replace(false) {
            grab(frame);
            frame += 1;
            idle_ticks = 0;
        } else {
            idle_ticks += 1;
            assert!(idle_ticks < 200, "the frame clock stopped painting");
        }
        now += 1_000_000;
        set_clock(now);
    }
    let status = Command::new("ffmpeg")
        .args(["-nostdin", "-y", "-v", "error", "-framerate", "60", "-i"])
        .arg(frames_dir.join("%05d.png"))
        .args(["-c:v", "ffv1", "-level", "3"])
        .arg(output.join("wheel-take.mkv"))
        .env_remove("LD_PRELOAD")
        .status()
        .expect("ffmpeg");
    let take = output.join("wheel-take.mkv");

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
        "boundary": "Production SessionWheel (mluva-gtk) on a private Xvfb screen; fixture sessions, no Herdr, agent or network",
        "screen": [width, height],
        "scale": scale,
        "theme": theme,
        "beat_seconds": BEAT,
        "agent_key": "F12",
        "fixtures": fixtures,
        "frames": total,
        "fps": 60,
        "clock": "monotonic clock stepped 1/60 s per frame (take_clock.so); one x11grab PNG per painted frame",
        "timeline": timeline,
        "paint_frame_times_us_from_origin": paint_times.borrow().iter().map(|t| t - origin / 1000).collect::<Vec<_>>(),
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
