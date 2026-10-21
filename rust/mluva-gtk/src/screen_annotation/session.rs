//! One annotated recording: overlays, temporary bindings, clean frames and focus events on the
//! recording clock, handed over as a frozen `Capture` when recording stops.
//!
//! Nothing is grabbed beyond one start frame until the first ink or pin, so a recording without
//! marks costs one frame on Hyprland and none elsewhere. Raw frames live only in a memory-backed
//! runtime directory, bounded in count and bytes.
use super::{
    hyprland,
    overlay::{Motion, Overlay},
};
use crate::async_runtime::DesktopRuntime;
use gtk::{gdk, prelude::*};
use mluva_annotate::{
    keyframes::screen_diff,
    model::{Capture, Frame, FrameReason, Screen, ScreenEvent, Stroke, StrokePoint, WindowBox},
    render,
};
use std::{
    cell::{Cell, RefCell},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
    time::Duration,
};
use tokio::io::AsyncBufReadExt;

/// Once drawing has started, a sample is taken this often and kept only if the screen changed.
const SAMPLE_EVERY: Duration = Duration::from_secs(2);
const SAMPLE_NOVELTY: f32 = 0.02;
/// A window or workspace change is captured once it has settled and no ink is on screen.
const SETTLE: Duration = Duration::from_millis(700);
const INK_RETRY: Duration = Duration::from_millis(500);
const INK_RETRIES: u8 = 6;
/// Raw frames kept at most, by count and by bytes. They live in memory, so the bound is firm.
const MAX_FRAMES: usize = 120;
const MAX_FRAME_BYTES: u64 = 192 * 1024 * 1024;
/// Frame grabs still running at Stop are waited for this long.
const COLLECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Strokes smaller than this, in logical pixels, are pointer drift rather than ink.
const MIN_STROKE: f32 = 14.0;
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
    /// While Shift is held: the stroke being drawn and its overlay line, once the pointer moved.
    ink: Option<Option<(usize, u64)>>,
    pins: u32,
    /// Shift-clicks whose pointer position is still being read.
    pending_pins: u32,
    sequence: u32,
    in_flight: usize,
    bytes: u64,
    stopped_at: Option<u64>,
    /// The first ink or pin started change sampling.
    armed: bool,
    /// `collect` has handed the frames to the bundle writer; nothing may change them now.
    sealed: bool,
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
    /// Ink and pins are accepted.
    ready: Cell<bool>,
    /// Bindings were sent to Hyprland and must be removed at Stop.
    binding: Cell<bool>,
    idle: Rc<tokio::sync::Notify>,
    sampler: RefCell<Option<glib::SourceId>>,
    settle: RefCell<Option<glib::SourceId>>,
    events: RefCell<Option<glib::JoinHandle<()>>>,
    watchdog: RefCell<Option<tokio::process::ChildStdin>>,
    message: Rc<dyn Fn(&str)>,
}

/// Whether frames may be written to `directory`: only to a memory-backed filesystem.
pub fn volatile(directory: &Path) -> bool {
    directory
        .canonicalize()
        .is_ok_and(|path| mluva_audio::volatile::memory_backed(&path))
}

/// Remove raw-frame directories left by a Mluva that was killed with its watchdog, except `keep`.
pub fn remove_stale_frames(runtime_directory: &Path, keep: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(runtime_directory) else {
        return;
    };
    let uid = unsafe { libc::getuid() };
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = entry
            .file_name()
            .to_string_lossy()
            .starts_with(FRAMES_PREFIX)
            && std::fs::symlink_metadata(&path)
                .is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == uid);
        if ours && Some(path.as_path()) != keep {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
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
        if !volatile(runtime_directory) {
            return Err("Screen annotation needs a memory-backed runtime directory (XDG_RUNTIME_DIR), so screen images never reach the disk; this recording continues without it.".into());
        }
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
            ready: Cell::new(false),
            binding: Cell::new(false),
            idle: Rc::new(tokio::sync::Notify::new()),
            sampler: RefCell::new(None),
            settle: RefCell::new(None),
            events: RefCell::new(None),
            watchdog: RefCell::new(None),
            message,
        }))
    }

    /// The private directory this recording's raw frames live in.
    pub fn frames_directory(&self) -> &Path {
        self.frames.path()
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
            // From here on the bindings may exist in Hyprland, so Stop always removes them.
            self.binding.set(true);
            hyprland::bind(&self.token).await?;
            if self.closed.get() {
                return Ok(());
            }
            self.watch_focus();
            self.ready.set(true);
            self.sample(FrameReason::Start, 0);
        } else {
            // Nothing binds Shift here; the first annotation action arms the session.
            self.ready.set(true);
        }
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

    /// Start change sampling at the first ink or pin.
    fn arm(self: &Rc<Self>) {
        if std::mem::replace(&mut self.state.borrow_mut().armed, true) {
            return;
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(SAMPLE_EVERY, move || {
            let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) else {
                return glib::ControlFlow::Break;
            };
            owner.sample(FrameReason::Sample, 0);
            glib::ControlFlow::Continue
        });
        self.sampler.replace(Some(source));
    }

    /// Shift went down or up.
    pub fn ink(self: &Rc<Self>, down: bool) {
        if self.closed.get() || !self.ready.get() {
            return;
        }
        let overlays = self.overlays.borrow().clone();
        if down {
            if self.state.borrow().ink.is_some() {
                return;
            }
            self.state.borrow_mut().ink = Some(None);
            self.arm();
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
        if let (Some((index, _)), Some(t)) = (stroke, self.now())
            && let Some(stroke) = self.state.borrow_mut().capture.strokes.get_mut(index)
        {
            stroke.t1 = stroke.t1.max(t);
        }
    }

    fn motion(self: &Rc<Self>, screen: &str, x: f64, y: f64) {
        let Some(t) = self.now() else {
            return;
        };
        let point = StrokePoint {
            x: x as f32,
            y: y as f32,
            t,
        };
        let Some(overlay) = self.overlay(screen) else {
            return;
        };
        let begun = {
            let mut state = self.state.borrow_mut();
            let Some(current) = state.ink else {
                return;
            };
            match current.filter(|&(i, _)| state.capture.strokes[i].screen == screen) {
                Some((index, _)) => {
                    let stroke = &mut state.capture.strokes[index];
                    stroke.points.push(point);
                    stroke.t1 = t;
                    None
                }
                None => {
                    // The first movement, or the pointer crossed to another monitor.
                    if let Some((previous, _)) = current {
                        state.capture.strokes[previous].t1 = t;
                    }
                    state.capture.strokes.push(Stroke {
                        t0: t,
                        t1: t,
                        screen: screen.into(),
                        pin: None,
                        points: vec![point],
                    });
                    let line = overlay.begin();
                    state.ink = Some(Some((state.capture.strokes.len() - 1, line)));
                    Some(line)
                }
            }
        };
        if let Some(line) = begun {
            // This stroke's ink stays hidden until the clean frame under it has been taken.
            let shown = overlay.clone();
            self.capture(
                screen,
                FrameReason::Mark,
                t,
                Box::new(move || shown.reveal(line)),
            );
        }
        overlay.point(x, y);
    }

    /// Shift-click: a numbered pin where the pointer is.
    pub fn pin(self: &Rc<Self>) {
        if self.closed.get() || !self.ready.get() {
            return;
        }
        let Some(t) = self.now() else {
            return;
        };
        self.arm();
        self.state.borrow_mut().pending_pins += 1;
        let weak = Rc::downgrade(self);
        let desktop = self.desktop;
        self.runtime.spawn(async move {
            let cursor = match desktop {
                Desktop::Hyprland => hyprland::cursor().await,
                Desktop::Layer => None,
            };
            let Some(owner) = weak.upgrade() else {
                return;
            };
            {
                let mut state = owner.state.borrow_mut();
                state.pending_pins -= 1;
                if state.pending_pins == 0 && state.in_flight == 0 {
                    owner.idle.notify_waiters();
                }
                // A Shift-click just before Stop still counts, until the frames are handed over.
                if state.sealed {
                    return;
                }
            }
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

    /// Make room under the frame bounds, evicting the oldest change sample. False when only
    /// mark, window and start frames remain and the bound is reached.
    fn room(state: &mut State) -> bool {
        while state.capture.frames.len() + state.in_flight >= MAX_FRAMES
            || state.bytes >= MAX_FRAME_BYTES
        {
            let Some(index) = state
                .capture
                .frames
                .iter()
                .position(|frame| frame.reason == FrameReason::Sample)
            else {
                return false;
            };
            let dropped = state.capture.frames.remove(index);
            state.bytes = state
                .bytes
                .saturating_sub(std::fs::metadata(&dropped.path).map_or(0, |m| m.len()));
            let _ = std::fs::remove_file(dropped.path);
        }
        true
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
            if state.sealed || !Self::room(&mut state) {
                // A stroke without its own frame uses the latest one before it.
                drop(state);
                done();
                return;
            }
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
            // Geometry only: every visible window's box, for cropping to whole windows.
            let windows = if grabbed && desktop == Desktop::Hyprland {
                hyprland::windows(&screen).await
            } else {
                vec![]
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
                Some(owner) => owner.captured(screen, reason, t, path, decoded, windows),
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
        windows: Vec<hyprland::Visible>,
    ) {
        let mut state = self.state.borrow_mut();
        state.in_flight = state.in_flight.saturating_sub(1);
        if state.in_flight == 0 {
            self.idle.notify_waiters();
        }
        // A late grab only ever removes its own new file once the frames are handed over.
        let Some((width, height, luma)) = decoded.filter(|_| !state.sealed) else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        // An unchanged screen reuses the clean frame already taken; strokes find it by time.
        if state
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
        let output = state.outputs.iter().find(|o| o.name == screen).cloned();
        let windows = output
            .map(|o| {
                windows
                    .iter()
                    .filter_map(|window| {
                        let [x0, y0, x1, y1] = window.rect;
                        let local = [
                            (x0 - o.x).max(0.0),
                            (y0 - o.y).max(0.0),
                            (x1 - o.x).min(o.width),
                            (y1 - o.y).min(o.height),
                        ];
                        (local[2] > local[0] && local[3] > local[1]).then(|| WindowBox {
                            rect: local.map(|v| v as f32),
                            active: window.active,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        state.bytes += std::fs::metadata(&path).map_or(0, |m| m.len());
        state.capture.frames.push(Frame {
            t,
            screen,
            reason,
            path,
            width,
            height,
            luma,
            windows,
        });
    }

    fn ink_visible(&self) -> bool {
        self.state.borrow().ink.is_some()
            || self.overlays.borrow().iter().any(|o| o.has_visible_ink())
    }

    /// Capture the focused output. Change samples skip a moment with ink on screen or a grab
    /// running; window changes wait for the ink to fade.
    fn sample(self: &Rc<Self>, reason: FrameReason, attempt: u8) {
        if reason != FrameReason::Start && self.ink_visible() {
            if reason == FrameReason::Window && attempt < INK_RETRIES {
                let weak = Rc::downgrade(self);
                glib::timeout_add_local_once(INK_RETRY, move || {
                    if let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.get()) {
                        owner.sample(reason, attempt + 1);
                    }
                });
            }
            return;
        }
        if reason == FrameReason::Sample && self.state.borrow().in_flight > 0 {
            return;
        }
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
            // Ink may have appeared while the outputs were read.
            if reason != FrameReason::Start && owner.ink_visible() {
                return;
            }
            let outputs = if outputs.is_empty() {
                owner.state.borrow().outputs.clone()
            } else {
                outputs
            };
            let Some(focused) = outputs
                .iter()
                .find(|output| output.focused)
                .or(outputs.first())
                .map(|output| output.name.clone())
            else {
                return;
            };
            owner.state.borrow_mut().outputs = outputs;
            if let Some(t) = owner.now() {
                owner.capture(&focused, reason, t, Box::new(|| {}));
            }
        });
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
                    hyprland::FocusEvent::Window { class } => ScreenEvent::Window { t, class },
                    hyprland::FocusEvent::Workspace { name } => ScreenEvent::Workspace { t, name },
                };
                if owner.state.borrow().capture.events.len() < 500 {
                    owner.state.borrow_mut().capture.events.push(event);
                }
                // Before the first mark a change costs nothing: no frame is taken.
                if !owner.state.borrow().armed {
                    continue;
                }
                if let Some(previous) = owner.settle.borrow_mut().take() {
                    previous.remove();
                }
                let settled = Rc::downgrade(&owner);
                let source = glib::timeout_add_local_once(SETTLE, move || {
                    if let Some(owner) = settled.upgrade() {
                        owner.settle.borrow_mut().take();
                        if !owner.closed.get() {
                            owner.sample(FrameReason::Window, 0);
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
        if self.binding.replace(false) {
            let token = self.token.clone();
            self.runtime.spawn(async move {
                hyprland::unbind(&token).await;
            });
        }
    }

    /// Whether anything was marked: drawn ink, a pin, or a Shift-click still being located.
    /// Known without waiting for frames, so a recording without marks is never delayed.
    pub fn has_marks(&self) -> bool {
        marked(&self.state.borrow())
    }

    /// Everything recorded, once grabs in progress have finished. After this, late grabs only
    /// delete their own files, so the frames handed over stay exactly as listed.
    pub async fn collect(&self) -> Capture {
        self.stop();
        let wait = async {
            loop {
                let notified = self.idle.notified();
                if self.state.borrow().in_flight == 0 && self.state.borrow().pending_pins == 0 {
                    break;
                }
                notified.await;
            }
        };
        let _ = tokio::time::timeout(COLLECT_TIMEOUT, wait).await;
        let mut state = self.state.borrow_mut();
        state.sealed = true;
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
        // Shift held for a capital while the pointer drifts a little is typing, not a mark.
        capture
            .strokes
            .retain(|stroke| stroke.pin.is_some() || drawn(stroke));
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

fn marked(state: &State) -> bool {
    state.pending_pins > 0
        || state
            .capture
            .strokes
            .iter()
            .any(|stroke| stroke.pin.is_some() || drawn(stroke))
}

fn drawn(stroke: &Stroke) -> bool {
    let b = mluva_annotate::sketch::bounds(&stroke.xy());
    (b[2] - b[0]).hypot(b[3] - b[1]) >= MIN_STROKE
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(points: &[(f32, f32)]) -> Stroke {
        Stroke {
            t0: 0,
            t1: 10,
            screen: "DP-1".into(),
            pin: None,
            points: points
                .iter()
                .map(|&(x, y)| StrokePoint { x, y, t: 0 })
                .collect(),
        }
    }

    #[test]
    fn pointer_drift_while_typing_a_capital_is_not_ink() {
        assert!(!drawn(&stroke(&[(100.0, 100.0)])));
        assert!(!drawn(&stroke(&[(100.0, 100.0), (108.0, 106.0)])));
        assert!(drawn(&stroke(&[(100.0, 100.0), (130.0, 100.0)])));
    }

    #[test]
    fn only_ink_pins_or_a_pin_being_located_mark_a_recording() {
        let mut state = State::default();
        assert!(!marked(&state));
        state
            .capture
            .strokes
            .push(stroke(&[(100.0, 100.0), (104.0, 103.0)]));
        assert!(!marked(&state), "drift while typing a capital adds nothing");
        state.pending_pins = 1;
        assert!(
            marked(&state),
            "a Shift-click just before Stop still counts"
        );
        state.pending_pins = 0;
        state
            .capture
            .strokes
            .push(stroke(&[(100.0, 100.0), (180.0, 100.0)]));
        assert!(marked(&state));
    }

    #[test]
    fn stale_frame_directories_go_but_others_and_the_current_one_stay() {
        let root = tempfile::tempdir().unwrap();
        let stale = root.path().join(format!("{FRAMES_PREFIX}old"));
        let current = root.path().join(format!("{FRAMES_PREFIX}current"));
        let other = root.path().join("other-directory");
        for directory in [&stale, &current, &other] {
            std::fs::create_dir(directory).unwrap();
            std::fs::write(directory.join("frame.jpg"), b"jpeg").unwrap();
        }
        let link = root.path().join(format!("{FRAMES_PREFIX}link"));
        std::os::unix::fs::symlink(&other, &link).unwrap();
        remove_stale_frames(root.path(), Some(&current));
        assert!(!stale.exists());
        assert!(current.join("frame.jpg").exists());
        assert!(other.join("frame.jpg").exists(), "a link is never followed");
    }

    #[test]
    fn frames_are_kept_only_on_memory_backed_filesystems() {
        let disk = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        assert!(!volatile(disk.path()));
        if Path::new("/dev/shm").is_dir() {
            assert!(volatile(Path::new("/dev/shm")));
        }
    }

    #[test]
    fn the_frame_bound_evicts_samples_first_and_never_mark_frames() {
        let directory = tempfile::tempdir().unwrap();
        let mut state = State::default();
        for (i, reason) in [FrameReason::Mark, FrameReason::Sample, FrameReason::Sample]
            .into_iter()
            .enumerate()
        {
            let path = directory.path().join(format!("{i}.jpg"));
            std::fs::write(&path, vec![0; 10]).unwrap();
            state.capture.frames.push(Frame {
                t: i as u64,
                screen: "DP-1".into(),
                reason,
                path,
                width: 1,
                height: 1,
                luma: vec![],
                windows: vec![],
            });
            state.bytes += 10;
        }
        state.in_flight = MAX_FRAMES - 3;
        assert!(AnnotationSession::room(&mut state));
        assert_eq!(state.capture.frames.len(), 2);
        assert_eq!(state.bytes, 20);
        assert!(!directory.path().join("1.jpg").exists());
        state.in_flight = MAX_FRAMES;
        assert!(!AnnotationSession::room(&mut state));
        assert_eq!(
            state
                .capture
                .frames
                .iter()
                .map(|f| f.reason)
                .collect::<Vec<_>>(),
            [FrameReason::Mark]
        );
        assert!(directory.path().join("0.jpg").exists());
    }
}
