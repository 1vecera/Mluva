//! One annotated recording: overlays, temporary bindings, clean frames and focus events on the
//! recording clock, handed over as a frozen `Capture` when recording stops.
use super::{
    hyprland,
    overlay::{Motion, Overlay},
};
use crate::async_runtime::DesktopRuntime;
use gtk::{gdk, prelude::*};
use mluva_annotate::{
    keyframes::screen_diff,
    model::{Capture, Frame, FrameReason, Screen, ScreenEvent, Stroke, StrokePoint},
    render,
};
use std::{
    cell::{Cell, RefCell},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
    time::Duration,
};
use tokio::io::AsyncBufReadExt;

/// A sample is taken this often when nothing else happened, and kept only if the screen changed.
const SAMPLE_EVERY: Duration = Duration::from_secs(2);
const SAMPLE_NOVELTY: f32 = 0.02;
/// A window or workspace change is captured once it has settled.
const SETTLE: Duration = Duration::from_millis(700);
/// Captured frames kept in the private runtime directory at most.
const MAX_FRAMES: usize = 60;
/// Frame grabs still running at Stop are waited for this long.
const COLLECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const FRAMES_PREFIX: &str = "mluva-annotation-";

/// Milliseconds since the recording started, or None once it has none.
pub type Clock = Rc<dyn Fn() -> Option<u64>>;

/// Hyprland binds Shift and reports pointer, windows and focus. Any other layer-shell
/// compositor gets the overlays and frame grabs, driven only by Mluva's application actions.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Desktop {
    Hyprland,
    Layer,
}

#[derive(Default)]
struct State {
    capture: Capture,
    outputs: Vec<hyprland::Output>,
    /// While Shift is held: the stroke being drawn, once the pointer has moved.
    ink: Option<Option<usize>>,
    pins: u32,
    sequence: u32,
    in_flight: usize,
    stopped_at: Option<u64>,
}

pub struct AnnotationSession {
    pub owner: String,
    desktop: Desktop,
    runtime: Rc<DesktopRuntime>,
    clock: Clock,
    token: String,
    frames: tempfile::TempDir,
    cleanup: PathBuf,
    overlays: RefCell<Vec<Rc<Overlay>>>,
    state: RefCell<State>,
    closed: Cell<bool>,
    bound: Cell<bool>,
    idle: Rc<tokio::sync::Notify>,
    sampler: RefCell<Option<glib::SourceId>>,
    settle: RefCell<Option<glib::SourceId>>,
    events: RefCell<Option<glib::JoinHandle<()>>>,
    watchdog: RefCell<Option<tokio::process::ChildStdin>>,
    message: Rc<dyn Fn(&str)>,
}

impl AnnotationSession {
    /// Prepare a session for one recording. `activate` installs it on the desktop.
    pub fn new(
        runtime: Rc<DesktopRuntime>,
        owner: &str,
        runtime_directory: &Path,
        cleanup: &Path,
        clock: Clock,
        message: Rc<dyn Fn(&str)>,
    ) -> Result<Rc<Self>, String> {
        let wayland = gdk::Display::default()
            .is_some_and(|display| display.type_().name().starts_with("GdkWayland"));
        if !wayland {
            return Err(
                "Screen annotation needs a Wayland desktop; this recording continues without it."
                    .into(),
            );
        }
        let desktop = if hyprland::available() {
            Desktop::Hyprland
        } else {
            Desktop::Layer
        };
        std::fs::create_dir_all(runtime_directory).map_err(|_| unavailable())?;
        let frames = tempfile::Builder::new()
            .prefix(FRAMES_PREFIX)
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(runtime_directory)
            .map_err(|_| unavailable())?;
        Ok(Rc::new(Self {
            owner: owner.into(),
            desktop,
            runtime,
            clock,
            token: glib::uuid_string_random().replace('-', ""),
            frames,
            cleanup: cleanup.into(),
            overlays: RefCell::new(vec![]),
            state: RefCell::new(State::default()),
            closed: Cell::new(false),
            bound: Cell::new(false),
            idle: Rc::new(tokio::sync::Notify::new()),
            sampler: RefCell::new(None),
            settle: RefCell::new(None),
            events: RefCell::new(None),
            watchdog: RefCell::new(None),
            message,
        }))
    }

    /// Start crash cleanup, map the overlays and bind Shift. Each step re-checks Stop.
    pub async fn activate(self: Rc<Self>) {
        if let Err(message) = self.install().await {
            self.stop();
            (self.message)(&message);
        }
    }

    async fn install(self: &Rc<Self>) -> Result<(), String> {
        self.start_watchdog().await?;
        let display = gdk::Display::default().ok_or_else(unavailable)?;
        let monitors = display.monitors();
        let outputs = match self.desktop {
            Desktop::Hyprland => hyprland::outputs().await,
            Desktop::Layer => monitor_outputs(&monitors),
        };
        if outputs.is_empty() || self.closed.get() {
            return Err(unavailable());
        }
        self.state.borrow_mut().outputs = outputs.clone();
        let weak = Rc::downgrade(self);
        let motion: Motion = Rc::new(move |screen, x, y| {
            if let Some(owner) = weak.upgrade() {
                owner.motion(screen, x, y);
            }
        });
        for monitor in monitors.iter::<gdk::Monitor>().flatten() {
            let Some(name) = monitor.connector() else {
                continue;
            };
            if !outputs.iter().any(|output| output.name == name.as_str()) {
                continue;
            }
            match Overlay::new(&monitor, &name, motion.clone()) {
                Some(overlay) => self.overlays.borrow_mut().push(overlay),
                None => return Err(unavailable()),
            }
        }
        if self.overlays.borrow().is_empty() || self.closed.get() {
            return Err(unavailable());
        }
        if self.desktop == Desktop::Hyprland {
            hyprland::bind(&self.token).await?;
            if self.closed.get() {
                hyprland::unbind(&self.token).await;
                return Ok(());
            }
            self.watch_focus();
        }
        self.bound.set(true);
        self.start_sampling();
        self.sample(FrameReason::Start);
        Ok(())
    }

    /// The watchdog removes the bindings and frames if Mluva exits without doing so.
    async fn start_watchdog(&self) -> Result<(), String> {
        let mut command = tokio::process::Command::new(&self.cleanup);
        command
            .arg("--annotation-cleanup")
            .arg(self.frames.path())
            .arg(&self.token)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for name in ["PATH", "XDG_RUNTIME_DIR", "HYPRLAND_INSTANCE_SIGNATURE"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        // SAFETY: setsid is async-signal-safe; the watchdog must outlive Mluva's process group.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|_| unavailable())?;
        let stdout = child.stdout.take().ok_or_else(unavailable)?;
        let mut line = String::new();
        let ready = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::io::BufReader::new(stdout).read_line(&mut line),
        )
        .await;
        if !matches!(ready, Ok(Ok(_))) || line != "ready\n" {
            let _ = child.start_kill();
            return Err(unavailable());
        }
        self.watchdog.replace(child.stdin.take());
        // The watchdog runs in its own session; Tokio reaps it after it exits.
        drop(child);
        Ok(())
    }

    fn overlay(&self, screen: &str) -> Option<Rc<Overlay>> {
        self.overlays
            .borrow()
            .iter()
            .find(|overlay| overlay.screen == screen)
            .cloned()
    }

    fn now(&self) -> Option<u64> {
        (self.clock)()
    }

    /// Shift went down or up.
    pub fn ink(self: &Rc<Self>, down: bool) {
        if self.closed.get() || !self.bound.get() {
            return;
        }
        let overlays = self.overlays.borrow().clone();
        if down {
            if self.state.borrow().ink.is_some() {
                return;
            }
            self.state.borrow_mut().ink = Some(None);
            for overlay in &overlays {
                overlay.hide_ink();
                overlay.set_drawing(true);
            }
            return;
        }
        let Some(stroke) = self.state.borrow_mut().ink.take() else {
            return;
        };
        for overlay in &overlays {
            overlay.set_drawing(false);
            overlay.end();
        }
        if let (Some(index), Some(t)) = (stroke, self.now())
            && let Some(stroke) = self.state.borrow_mut().capture.strokes.get_mut(index)
        {
            stroke.t1 = stroke.t1.max(t);
        }
    }

    fn motion(self: &Rc<Self>, screen: &str, x: f64, y: f64) {
        let Some(t) = self.now() else {
            return;
        };
        let begin = {
            let mut state = self.state.borrow_mut();
            let Some(current) = state.ink else {
                return;
            };
            let same = current.filter(|&i| state.capture.strokes[i].screen == screen);
            match same {
                Some(index) => {
                    let stroke = &mut state.capture.strokes[index];
                    stroke.points.push(StrokePoint {
                        x: x as f32,
                        y: y as f32,
                        t,
                    });
                    stroke.t1 = t;
                    false
                }
                None => {
                    // The first movement, or the pointer crossed to another monitor.
                    if let Some(previous) = current {
                        state.capture.strokes[previous].t1 = t;
                    }
                    state.capture.strokes.push(Stroke {
                        t0: t,
                        t1: t,
                        screen: screen.into(),
                        pin: None,
                        points: vec![StrokePoint {
                            x: x as f32,
                            y: y as f32,
                            t,
                        }],
                    });
                    state.ink = Some(Some(state.capture.strokes.len() - 1));
                    true
                }
            }
        };
        let Some(overlay) = self.overlay(screen) else {
            return;
        };
        if begin {
            overlay.begin();
            // Ink stays hidden until the clean frame under it has been taken.
            let shown = overlay.clone();
            self.capture(
                screen,
                FrameReason::Mark,
                t,
                Box::new(move || shown.reveal()),
            );
        }
        overlay.point(x, y);
    }

    /// Shift-click: a numbered pin where the pointer is.
    pub fn pin(self: &Rc<Self>) {
        if self.closed.get() || !self.bound.get() {
            return;
        }
        let Some(t) = self.now() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let desktop = self.desktop;
        self.runtime.spawn(async move {
            let cursor = match desktop {
                Desktop::Hyprland => hyprland::cursor().await,
                Desktop::Layer => None,
            };
            let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                return;
            };
            let outputs = owner.state.borrow().outputs.clone();
            // Without compositor IPC, the pin goes where the pointer last moved over an overlay.
            let located = match cursor {
                Some((x, y)) => outputs
                    .iter()
                    .find(|output| output.contains(x, y))
                    .map(|output| (output.clone(), x - output.x, y - output.y)),
                None => owner.overlays.borrow().iter().find_map(|overlay| {
                    let (x, y) = overlay.pointer.get()?;
                    let output = outputs.iter().find(|o| o.name == overlay.screen)?;
                    Some((output.clone(), x, y))
                }),
            };
            let Some((output, lx, ly)) = located else {
                return;
            };
            {
                let mut state = owner.state.borrow_mut();
                state.pins += 1;
                let number = state.pins;
                state.capture.strokes.push(Stroke {
                    t0: t,
                    t1: t,
                    screen: output.name.clone(),
                    pin: Some(number),
                    points: vec![StrokePoint {
                        x: lx as f32,
                        y: ly as f32,
                        t,
                    }],
                });
            }
            let overlay = owner.overlay(&output.name);
            owner.capture(
                &output.name,
                FrameReason::Mark,
                t,
                Box::new(move || {
                    if let Some(overlay) = overlay {
                        overlay.pulse(lx, ly);
                    }
                }),
            );
        });
    }

    /// The live preview's length, for aligning providers that return no word times.
    pub fn preview(&self, words: usize) {
        if self.closed.get() {
            return;
        }
        if let Some(t) = self.now() {
            let mut state = self.state.borrow_mut();
            if state
                .capture
                .preview
                .last()
                .is_none_or(|(_, last)| *last != words)
            {
                state.capture.preview.push((t, words));
            }
        }
    }

    fn capture(
        self: &Rc<Self>,
        screen: &str,
        reason: FrameReason,
        t: u64,
        done: Box<dyn FnOnce()>,
    ) {
        let path = {
            let mut state = self.state.borrow_mut();
            state.sequence += 1;
            state.in_flight += 1;
            self.frames
                .path()
                .join(format!("frame-{:04}.jpg", state.sequence))
        };
        let screen = screen.to_owned();
        let weak = Rc::downgrade(self);
        let desktop = self.desktop;
        self.runtime.spawn(async move {
            let grabbed = hyprland::grab(&screen, &path).await;
            let window = if grabbed && reason != FrameReason::Mark && desktop == Desktop::Hyprland {
                hyprland::active_window().await
            } else {
                None
            };
            let decoded = if grabbed {
                let source = path.clone();
                tokio::task::spawn_blocking(move || {
                    let mut surface = render::load(&source).ok()?;
                    let luma = render::luma(&mut surface);
                    Some((surface.width() as u32, surface.height() as u32, luma))
                })
                .await
                .ok()
                .flatten()
            } else {
                None
            };
            done();
            match weak.upgrade() {
                Some(owner) => owner.captured(screen, reason, t, path, decoded, window),
                None => {
                    let _ = std::fs::remove_file(&path);
                }
            }
        });
    }

    fn captured(
        &self,
        screen: String,
        reason: FrameReason,
        t: u64,
        path: PathBuf,
        decoded: Option<(u32, u32, Vec<f32>)>,
        window: Option<[f64; 4]>,
    ) {
        let mut state = self.state.borrow_mut();
        state.in_flight = state.in_flight.saturating_sub(1);
        if state.in_flight == 0 {
            self.idle.notify_waiters();
        }
        let Some((width, height, luma)) = decoded else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        let output = state.outputs.iter().find(|o| o.name == screen).cloned();
        let window = window
            .zip(output.as_ref())
            .and_then(|([x0, y0, x1, y1], o)| {
                let local = [
                    (x0 - o.x).max(0.0),
                    (y0 - o.y).max(0.0),
                    (x1 - o.x).min(o.width),
                    (y1 - o.y).min(o.height),
                ];
                (local[2] > local[0] && local[3] > local[1]).then(|| local.map(|v| v as f32))
            });
        if reason == FrameReason::Sample
            && state
                .capture
                .frames
                .iter()
                .rev()
                .find(|frame| frame.screen == screen)
                .is_some_and(|last| screen_diff(&last.luma, &luma) < SAMPLE_NOVELTY)
        {
            let _ = std::fs::remove_file(&path);
            return;
        }
        state.capture.frames.push(Frame {
            t,
            screen,
            reason,
            path,
            width,
            height,
            luma,
            window,
        });
        if state.capture.frames.len() > MAX_FRAMES
            && let Some(index) = state
                .capture
                .frames
                .iter()
                .position(|frame| frame.reason == FrameReason::Sample)
        {
            let dropped = state.capture.frames.remove(index);
            let _ = std::fs::remove_file(dropped.path);
        }
    }

    /// Capture the focused output, unless ink is on screen or a grab is running.
    fn sample(self: &Rc<Self>, reason: FrameReason) {
        let weak = Rc::downgrade(self);
        let desktop = self.desktop;
        self.runtime.spawn(async move {
            let outputs = match desktop {
                Desktop::Hyprland => hyprland::outputs().await,
                Desktop::Layer => vec![],
            };
            let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                return;
            };
            let outputs = if outputs.is_empty() {
                owner.state.borrow().outputs.clone()
            } else {
                outputs
            };
            if reason == FrameReason::Sample
                && (owner.state.borrow().ink.is_some()
                    || owner.state.borrow().in_flight > 0
                    || owner.overlays.borrow().iter().any(|o| o.has_visible_ink()))
            {
                return;
            }
            let Some(focused) = outputs
                .iter()
                .find(|output| output.focused)
                .or(outputs.first())
                .map(|output| output.name.clone())
            else {
                return;
            };
            if !outputs.is_empty() {
                owner.state.borrow_mut().outputs = outputs;
            }
            if let Some(t) = owner.now() {
                owner.capture(&focused, reason, t, Box::new(|| {}));
            }
        });
    }

    fn start_sampling(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(SAMPLE_EVERY, move || {
            let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                return glib::ControlFlow::Break;
            };
            owner.sample(FrameReason::Sample);
            glib::ControlFlow::Continue
        });
        self.sampler.replace(Some(source));
    }

    fn watch_focus(self: &Rc<Self>) {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        self.runtime.spawn_background(hyprland::watch(sender));
        let weak = Rc::downgrade(self);
        let task = self.runtime.spawn(async move {
            while let Some((_, event)) = receiver.recv().await {
                let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                    return;
                };
                let Some(t) = owner.now() else {
                    continue;
                };
                let event = match event {
                    hyprland::FocusEvent::Window { class, title } => {
                        ScreenEvent::Window { t, class, title }
                    }
                    hyprland::FocusEvent::Workspace { name } => ScreenEvent::Workspace { t, name },
                };
                if owner.state.borrow().capture.events.len() < 500 {
                    owner.state.borrow_mut().capture.events.push(event);
                }
                if let Some(previous) = owner.settle.borrow_mut().take() {
                    previous.remove();
                }
                let settled = Rc::downgrade(&owner);
                let source = glib::timeout_add_local_once(SETTLE, move || {
                    if let Some(owner) = settled.upgrade() {
                        owner.settle.borrow_mut().take();
                        if !owner.closed.get() {
                            owner.sample(FrameReason::Window);
                        }
                    }
                });
                owner.settle.replace(Some(source));
            }
        });
        self.events.replace(Some(task));
    }

    /// Recording stopped or was cancelled: remove the bindings and overlays at once. Frames
    /// already being grabbed still finish; `collect` waits for them.
    pub fn stop(&self) {
        if self.closed.replace(true) {
            return;
        }
        if let Some(source) = self.sampler.borrow_mut().take() {
            source.remove();
        }
        if let Some(source) = self.settle.borrow_mut().take() {
            source.remove();
        }
        if let Some(task) = self.events.borrow_mut().take() {
            task.abort();
        }
        {
            let mut state = self.state.borrow_mut();
            state.ink = None;
            state.stopped_at = self.now();
        }
        for overlay in self.overlays.borrow_mut().drain(..) {
            overlay.close();
        }
        if self.bound.replace(false) && self.desktop == Desktop::Hyprland {
            let token = self.token.clone();
            self.runtime.spawn(async move {
                hyprland::unbind(&token).await;
            });
        }
    }

    /// Everything recorded, once grabs in progress have finished.
    pub async fn collect(&self) -> Capture {
        self.stop();
        let wait = async {
            loop {
                let notified = self.idle.notified();
                if self.state.borrow().in_flight == 0 {
                    break;
                }
                notified.await;
            }
        };
        let _ = tokio::time::timeout(COLLECT_TIMEOUT, wait).await;
        let state = self.state.borrow();
        let mut capture = state.capture.clone();
        capture.duration_ms = state
            .stopped_at
            .unwrap_or_else(|| capture.strokes.iter().map(|s| s.t1).max().unwrap_or(0));
        capture.screens = state
            .outputs
            .iter()
            .map(|output| Screen {
                name: output.name.clone(),
                x: output.x as i32,
                y: output.y as i32,
                width: output.width as i32,
                height: output.height as i32,
                scale: output.scale,
            })
            .collect();
        capture.frames.retain(|frame| frame.path.is_file());
        capture
    }
}

impl Drop for AnnotationSession {
    fn drop(&mut self) {
        self.stop();
        // EOF tells the watchdog to remove this session's bindings again and its frames.
        self.watchdog.borrow_mut().take();
    }
}

/// Outputs as GTK sees them, for compositors without Hyprland's IPC. The first is treated as
/// focused.
fn monitor_outputs(monitors: &gtk::gio::ListModel) -> Vec<hyprland::Output> {
    monitors
        .iter::<gdk::Monitor>()
        .flatten()
        .enumerate()
        .filter_map(|(index, monitor)| {
            let geometry = monitor.geometry();
            Some(hyprland::Output {
                name: monitor.connector()?.into(),
                x: geometry.x() as f64,
                y: geometry.y() as f64,
                width: geometry.width() as f64,
                height: geometry.height() as f64,
                scale: monitor.scale(),
                focused: index == 0,
            })
        })
        .collect()
}

fn unavailable() -> String {
    "Screen annotation could not start; this recording continues without it.".into()
}

/// The watchdog half: wait for Mluva to close the pipe or exit, then remove this session's
/// Hyprland bindings and its private frame directory.
pub fn run_cleanup(directory: &Path, token: &str) -> i32 {
    use std::io::{Read, Write};
    use std::os::unix::fs::MetadataExt;
    let Ok(metadata) = std::fs::symlink_metadata(directory) else {
        return 1;
    };
    let valid = metadata.is_dir()
        && metadata.uid() == unsafe { libc::getuid() }
        && metadata.mode() & 0o777 == 0o700
        && directory
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(FRAMES_PREFIX))
        && directory.canonicalize().is_ok_and(|path| path == directory)
        && hyprland::valid_token(token);
    if !valid {
        return 2;
    }
    let identity = (metadata.dev(), metadata.ino());
    {
        let mut stdout = std::io::stdout().lock();
        if stdout
            .write_all(b"ready\n")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            return 1;
        }
    }
    let mut stdin = std::io::stdin().lock();
    let mut buffer = [0; 1024];
    loop {
        match stdin.read(&mut buffer) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    // Only Hyprland has bindings to remove.
    let hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
    if let Some(mut child) = hyprland
        .then(|| {
            std::process::Command::new("hyprctl")
                .arg("eval")
                .arg(format!("do\n{}\nend", hyprland::remove_script(token)))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        })
        .flatten()
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while matches!(child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
    if std::fs::symlink_metadata(directory)
        .is_ok_and(|current| current.is_dir() && (current.dev(), current.ino()) == identity)
    {
        let _ = std::fs::remove_dir_all(directory);
    }
    0
}
