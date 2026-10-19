//! Native publisher → real bridge → unchanged QML, observed through Qt IPC and pixels.
//! The baseline is collected from untouched released processes, not this implementation.
use gio::prelude::*;
use glib::variant::ToVariant;
use mluva_gtk::overlay_state::{OverlayPublisher, OverlayState};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

const NAME: &str = "com.mluva.Linux";
const GEOMETRY: &[&str] = &["x", "y", "width", "height", "viewportTop", "viewportHeight"];
fn number(value: &Value, key: &str) -> f64 {
    value[key]
        .as_f64()
        .unwrap_or_else(|| panic!("missing numeric {key}: {value}"))
}
fn flag(value: &Value, key: &str) -> bool {
    value[key]
        .as_bool()
        .unwrap_or_else(|| panic!("missing boolean {key}: {value}"))
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing text {key}: {value}"))
}
fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        if Instant::now() >= end {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn output(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn saved(path: impl AsRef<Path>, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn same_fields(before: &Value, after: &Value, keys: &[&str]) {
    for key in keys {
        assert_eq!(before[*key], after[*key], "{key}");
    }
}
fn reference() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/released-omarchy-widget.json")).unwrap();
    assert_eq!(
        value["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    value
}
fn static_state(value: &Value) -> Value {
    let mut value = value.clone();
    // These depend on animation phase and measured arrival times. Their bounds,
    // intermediate frames, direction and reduced-motion behavior are checked below.
    for key in [
        "identifier",
        "options",
        "menuOpen",
        "copyEnabled",
        "copyVisible",
        "reviewDuration",
        "dotRadius",
        "dotPhase",
        "lookAhead",
        "textY",
        "targetY",
        "scrollTarget",
        "revisionProgress",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}

struct Process(Child);
impl Process {
    fn start(command: &mut Command) -> Self {
        Self(command.process_group(0).spawn().unwrap())
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        // Only this private child's process group, including its bridge/capture children.
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

struct Widget {
    root: PathBuf,
    process: Process,
    publisher: Rc<RefCell<OverlayPublisher>>,
    connection: gio::DBusConnection,
    _actions: gio::SimpleActionGroup,
    export: Option<gio::ActionGroupExportId>,
    owns_name: bool,
    states: Vec<Value>,
}
impl Widget {
    fn new(name: &str) -> Self {
        let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
            .canonicalize()
            .unwrap();
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_STATE_HOME",
            "XAUTHORITY",
        ] {
            assert!(
                PathBuf::from(std::env::var_os(key).unwrap())
                    .canonicalize()
                    .unwrap()
                    .starts_with(&private)
            );
        }
        assert_ne!(
            fs::read_link("/proc/self/ns/net")
                .unwrap()
                .to_str()
                .unwrap(),
            std::env::var("MLUVA_HOST_NET_NS").unwrap()
        );
        for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
            assert!(!Path::new(path).exists());
        }
        assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
        assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
        assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let root = private.join(name);
        fs::create_dir(&root).unwrap();
        let source = repository.join("linux/quickshell/mluva.dictation");
        output(
            Command::new("cp")
                .arg("-r")
                .arg(&source)
                .arg(root.join("mluva.dictation")),
        );
        fs::write(
            root.join("shell.qml"),
            include_str!("../../../linux/tests/shell_overlay_fixture.qml")
                .replace("\"../quickshell/mluva.dictation\"", "\"./mluva.dictation\""),
        )
        .unwrap();
        for module in ["Commons", "Ui"] {
            output(
                Command::new("cp")
                    .arg("-r")
                    .arg(Path::new("/usr/share/omarchy/shell").join(module))
                    .arg(root.join(module)),
            );
            for entry in fs::read_dir(root.join(module)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|extension| extension == "qml") {
                    let source = fs::read_to_string(&path).unwrap();
                    fs::write(
                        path,
                        source.replace(
                            "Quickshell.env(\"HOME\")",
                            "Quickshell.env(\"OFFSCREEN_SESSION_ROOT\")",
                        ),
                    )
                    .unwrap();
                }
            }
        }
        let tools = root.join("bin");
        fs::create_dir(&tools).unwrap();
        fs::write(
            tools.join("hyprctl"),
            "#!/bin/sh\nprintf '{\"int\":0}\\n'\n",
        )
        .unwrap();
        fs::set_permissions(tools.join("hyprctl"), fs::Permissions::from_mode(0o700)).unwrap();
        let connection = gio::DBusConnection::for_address_sync(
            &std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
            gio::Cancellable::NONE,
        )
        .unwrap();
        let publisher = Rc::new(RefCell::new(OverlayPublisher::new(connection.clone())));
        let actions = gio::SimpleActionGroup::new();
        let status = gio::SimpleAction::new("status", None);
        let replay = publisher.clone();
        status.connect_activate(move |_, _| {
            assert!(replay.borrow().replay());
        });
        actions.add_action(&status);
        let export = connection
            .export_action_group("/com/mluva/Linux", &actions)
            .unwrap();
        let claimed = connection
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "RequestName",
                Some(&(NAME, 4_u32).to_variant()),
                None,
                gio::DBusCallFlags::NO_AUTO_START,
                2000,
                gio::Cancellable::NONE,
            )
            .unwrap();
        assert_eq!(claimed.child_get::<u32>(0), 1);
        let log = fs::File::create(root.join("quickshell.log")).unwrap();
        let process = Process::start(
            Command::new("quickshell")
                .args(["--no-color", "-p"])
                .arg(root.join("shell.qml"))
                .env("MLUVA_SHELL_COMMAND", repository.join("linux/mluva-shell"))
                .env(
                    "PATH",
                    format!("{}:{}", tools.display(), std::env::var("PATH").unwrap()),
                )
                .stdout(log.try_clone().unwrap())
                .stderr(log),
        );
        Self {
            root,
            process,
            publisher,
            connection,
            _actions: actions,
            export: Some(export),
            owns_name: true,
            states: vec![],
        }
    }
    fn publish(&self, specification: Value) {
        let state: OverlayState = serde_json::from_value(specification).unwrap();
        assert!(self.publisher.borrow_mut().publish(&state));
    }
    fn clear(&self) {
        assert!(self.publisher.borrow_mut().clear());
    }
    fn frames(&self) -> Vec<Value> {
        let content = fs::read_to_string(self.root.join("quickshell.log")).unwrap();
        let Some((complete, _)) = content.rsplit_once('\n') else {
            return vec![];
        };
        complete
            .lines()
            .filter_map(|line| {
                line.split_once("MLUVA_SNAPSHOT ")
                    .map(|(_, value)| serde_json::from_str(value).unwrap())
            })
            .collect()
    }
    fn observe(&mut self, phase: &str, preview: Option<&str>) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut previous = Value::Null;
        let mut seen = self.frames().len();
        loop {
            assert!(
                self.process.0.try_wait().unwrap().is_none(),
                "Quickshell exited; inspect {}",
                self.root.display()
            );
            let frames = self.frames();
            if frames.len() > seen {
                let state = frames.last().unwrap();
                let mut stable = state.clone();
                stable.as_object_mut().unwrap().remove("dotRadius");
                stable.as_object_mut().unwrap().remove("dotPhase");
                if state["phase"] == phase
                    && stable == previous
                    && preview.is_none_or(|value| state["preview"] == value)
                {
                    self.states.push(state.clone());
                    return state.clone();
                }
                previous = stable;
            }
            seen = frames.len();
            assert!(
                Instant::now() < deadline,
                "Widget did not settle in {phase}: {previous}"
            );
            pump(Duration::from_millis(10));
        }
    }
    fn ipc(&self, arguments: &[&str]) -> String {
        output(
            Command::new("quickshell")
                .args([
                    "ipc",
                    "--pid",
                    &self.process.0.id().to_string(),
                    "call",
                    "fixture",
                ])
                .args(arguments),
        )
    }
    fn queried(&self, arguments: &[&str]) -> Value {
        serde_json::from_str(&self.ipc(arguments)).unwrap()
    }
    fn focus_editor(&self) {
        let windows = output(Command::new("xdotool").args([
            "search",
            "--onlyvisible",
            "--name",
            "^Mluva overlay fixture$",
        ]));
        let identifiers = windows.lines().collect::<Vec<_>>();
        assert_eq!(identifiers.len(), 1);
        output(Command::new("xdotool").args(["windowfocus", "--sync", identifiers[0]]));
    }
    fn picture(&self, name: &str) {
        output(
            Command::new("import")
                .args(["-window", "root"])
                .arg(self.root.join(name)),
        );
    }
    fn motion(&self, preview: &str, duration: f64, mut preferences: Value) -> Vec<Value> {
        let before = self.frames().len();
        preferences["phase"] = json!("recording");
        preferences["preview"] = json!(preview);
        self.publish(preferences);
        pump(Duration::from_secs_f64(duration));
        self.frames()
            .into_iter()
            .skip(before)
            .filter(|frame| {
                let Some(start) = frame["previewStart"].as_u64() else {
                    return false;
                };
                frame["preview"] == preview.chars().skip(start as usize).collect::<String>()
            })
            .collect()
    }
    fn release(&mut self) {
        if self.owns_name {
            self.connection
                .call_sync(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "ReleaseName",
                    Some(&(NAME,).to_variant()),
                    None,
                    gio::DBusCallFlags::NO_AUTO_START,
                    2000,
                    gio::Cancellable::NONE,
                )
                .unwrap();
            self.owns_name = false;
        }
    }
}
impl Drop for Widget {
    fn drop(&mut self) {
        self.release();
        if let Some(export) = self.export.take() {
            self.connection.unexport_action_group(export);
        }
        let _ = self.connection.close_sync(gio::Cancellable::NONE);
    }
}

#[test]
#[ignore = "requires guarded private X11/bus/network/devices, Omarchy QML and the native source bridge"]
fn released_widget_controls_geometry_and_live_motion() {
    // The complete released snapshots below own fixed geometry, text, controls,
    // theme and focus expectations. Additional assertions cover motion and
    // asynchronous effects that those stable snapshots cannot demonstrate.
    let mut widget = Widget::new("native-widget");
    widget.observe("idle", None);
    widget.focus_editor();
    widget.observe("idle", None);
    widget.ipc(&["theme", "false"]);
    widget.publish(json!({"phase":"recording","preview":"One recorder across monitor bars"}));
    widget.observe("recording", None);
    for (primary, secondary) in [
        ("true", "true"),
        ("false", "true"),
        ("true", "true"),
        ("true", "false"),
    ] {
        widget.ipc(&["bars", primary, secondary]);
        widget.observe("recording", Some("One recorder across monitor bars"));
    }
    widget.clear();
    widget.observe("idle", None);
    for phase in ["preparing", "recording", "processing", "error"] {
        let preview = if phase == "recording" {
            format!(
                "{}LATEST WORDS: Žluťoučký kůň",
                "Earlier words ".repeat(100)
            )
        } else {
            String::new()
        };
        widget.publish(json!({"phase":phase,"elapsed_seconds":73,"level":0.4,"preview":preview}));
        let state = widget.observe(phase, None);
        if phase == "recording" {
            assert!(number(&state, "textY") < 0.0);
            assert!((number(&state, "textY") - number(&state, "targetY")).abs() < 0.1);
        }
        widget.picture(&format!("{phase}.png"));
    }
    for preset in ["bottom-left", "bottom-right", "bottom-center"] {
        widget.publish(
            json!({"phase":"recording","preview":"Preset check","widget_position":preset}),
        );
        widget.observe("recording", Some("Preset check"));
    }
    let samples = widget.queried(&["motionSamples"]);
    widget.publish(json!({"phase":"idle"}));
    widget.observe("idle", None);
    let short_text = text(&samples, "shortText");
    let near_text = text(&samples, "nearEdge");
    let wrapped_text = text(&samples, "wrapped");
    assert!(!near_text.is_empty() && wrapped_text.starts_with(near_text));
    widget.publish(json!({"phase":"recording","preview":short_text}));
    let short = widget.observe("recording", Some(short_text));
    assert_eq!(short["lineCount"], 4);
    assert_eq!(short["textY"], 0);
    let near = widget.motion(near_text, 1.4, json!({}));
    assert_eq!(near.last().unwrap()["lineCount"], 5);
    assert!(number(near.last().unwrap(), "lookAhead") > 0.0);
    let wrapped = widget.motion(wrapped_text, 1.4, json!({}));
    assert_eq!(wrapped.last().unwrap()["lineCount"], 6);
    for (initial, frames) in [(&short, &near), (near.last().unwrap(), &wrapped)] {
        let target = number(frames.last().unwrap(), "targetY");
        assert!(target < number(initial, "textY"));
        assert!(
            frames
                .iter()
                .any(|frame| number(frame, "textY") > target + 0.1
                    && number(frame, "textY") < number(initial, "textY") - 0.1)
        );
        assert!((number(frames.last().unwrap(), "textY") - target).abs() < 0.1);
        assert!(
            frames
                .iter()
                .all(|frame| frame["height"] == short["height"])
        );
        for (before, after) in std::iter::once(initial).chain(frames.iter()).zip(frames) {
            assert!(
                (number(after, "textY") - number(before, "textY")).abs()
                    < number(&short, "lineHeight") * 0.65
            );
        }
    }
    same_fields(&widget.queried(&["position"]), &short, &["x", "y"]);
    saved(
        widget.root.join("scroll-motion.json"),
        &json!({"before":short,"near_edge":near,"wrap":wrapped}),
    );
    widget.picture("five-lines.png");
    let long = wrapped_text.repeat(3);
    widget.publish(json!({"phase":"recording","preview":long}));
    let before = widget.observe("recording", Some(&long));
    let contraction = widget.motion(wrapped_text, 1.4, json!({}));
    saved(
        widget.root.join("preview-contraction.json"),
        &json!({"before":before,"frames":contraction}),
    );
    assert!(!contraction.is_empty());
    for (previous, frame) in std::iter::once(&before)
        .chain(contraction.iter())
        .zip(&contraction)
    {
        assert!(
            flag(frame, "visible") && flag(frame, "headerVisible") && flag(frame, "timerVisible")
        );
        same_fields(&before, frame, GEOMETRY);
        assert!(number(frame, "scrollTarget") <= number(previous, "scrollTarget") + 0.1);
        assert!(
            number(frame, "textY") + number(frame, "textHeight")
                >= number(frame, "viewportHeight") - number(frame, "lineHeight") * 1.5
        );
    }
    widget.ipc(&["scroll", "5000"]);
    let manual = widget.motion(wrapped_text, 1.4, json!({}));
    assert!(
        number(manual.last().unwrap(), "textY").abs() < 1.0
            && number(manual.last().unwrap(), "scrollTarget").abs() < 1.0
    );
    widget.ipc(&["scroll", "-5000"]);
    widget.publish(json!({"phase":"processing","preview":wrapped_text}));
    let processing = widget.observe("processing", Some(wrapped_text));
    assert!(flag(&processing, "visible"));
    same_fields(&processing, &short, GEOMETRY);
    widget.publish(json!({"phase":"recording","preview":wrapped_text}));
    widget.observe("recording", Some(wrapped_text));
    let pulse = widget.motion(wrapped_text, 3.6, json!({}));
    let radius = pulse
        .iter()
        .map(|frame| number(frame, "dotRadius"))
        .collect::<Vec<_>>();
    let unique = radius
        .iter()
        .map(|r| (r * 1000.0).round() as i64)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(unique.len() >= 10);
    let minimum = radius.iter().copied().reduce(f64::min).unwrap();
    let maximum = radius.iter().copied().reduce(f64::max).unwrap();
    assert!(minimum >= 4.8 && maximum <= 8.0 && maximum - minimum > 2.5);
    assert!(
        radius
            .windows(2)
            .all(|pair| (pair[0] - pair[1]).abs() < 0.4)
    );
    for frame in &pulse {
        same_fields(frame, &pulse[0], GEOMETRY);
        same_fields(
            frame,
            &pulse[0],
            &["dotCenterX", "dotCenterY", "dotWidth", "dotHeight"],
        );
    }
    let still = widget.motion(
        &format!("{wrapped_text} Reduced motion."),
        1.4,
        json!({"reduce_motion":true}),
    );
    assert!(!still.is_empty());
    for frame in still {
        assert_eq!(frame["dotRadius"], 6.4);
        assert_eq!(frame["dotPhase"], 0);
        assert!((number(&frame, "textY") - number(&frame, "targetY")).abs() < 0.1);
    }
    saved(
        widget.root.join("recording-pulse.json"),
        &json!({"radius_frames":radius,"disabled_is_static":true,"fixed_geometry":true}),
    );
    let mut full_text = (0..670)
        .map(|index| format!("{}{index:04}", if index < 12 { "🙂" } else { "w" }))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(full_text.chars().count() < 4096);
    widget.publish(json!({"phase":"recording","preview":full_text}));
    widget.observe("recording", Some(&full_text));
    let mut tails = vec![];
    let mut last = Value::Null;
    for index in 0..8 {
        full_text.push_str(&format!(" x{index:04} y{index:04} café🙂 z{index:04}"));
        widget.motion(&full_text, 1.4, json!({}));
        let state = widget.observe("recording", None);
        let expected = widget.queried(&["expectedTail", &full_text]);
        assert!((number(&state, "lastLineFill") - number(&expected, "lastFill")).abs() < 0.002);
        let unanchored = widget.queried(&["expectedTail", text(&state, "preview")]);
        assert_eq!(
            state["preview"],
            full_text
                .chars()
                .skip(state["previewStart"].as_u64().unwrap() as usize)
                .collect::<String>()
        );
        assert!(text(&state, "preview").chars().count() <= 4096);
        tails.push(json!({"offset":state["previewStart"],"actual":state["lastLineFill"],"expected":expected["lastFill"],"without_anchor":unanchored["lastFill"]}));
        last = state;
    }
    assert!(
        number(tails.last().unwrap(), "offset") > 0.0 && number(&last, "discardedHeight") > 0.0
    );
    assert!(
        tails.iter().any(
            |frame| (number(frame, "without_anchor") - number(frame, "expected")).abs() > 0.05
        )
    );
    saved(widget.root.join("bounded-preview.json"), &json!(tails));
    let recording_states = widget.states.len();
    for light in [false, true] {
        widget.ipc(&["theme", if light { "true" } else { "false" }]);
        widget.publish(json!({"phase":"recording","preview":"Dictation goes straight to the selected destination."}));
        let state = widget.observe("recording", None);
        assert!(flag(&state, "visible"));
        widget.picture(if light {
            "recording-light.png"
        } else {
            "recording-dark.png"
        });
    }
    // Late messages from an old installed writer cannot revive a rewrite surface.
    for phase in ["ready", "rewriting", "review-error"] {
        widget
            .publish(json!({"phase":phase,"preview":"legacy text","review_identifier":"obsolete"}));
        assert!(!flag(&widget.observe("idle", None), "visible"));
    }
    widget.publish(json!({"phase":"recording","preview":"Three-line appearance settings","widget_lines":3,"widget_opacity":40,"rewrite_enabled":false}));
    let appearance = widget.observe("recording", Some("Three-line appearance settings"));
    assert!((number(&appearance, "surfaceOpacity") - 0.4).abs() < 0.01);
    assert!(
        (number(&appearance, "viewportHeight") - 3.0 * number(&appearance, "lineHeight")).abs()
            < 1.0
    );
    widget.picture("three-lines-transparent.png");
    widget.publish(json!({"phase":"recording","preview":"Must disappear on owner loss"}));
    widget.observe("recording", None);
    widget.release();
    let stopped = widget.observe("stopped", None);
    assert!(!flag(&stopped, "visible"));
    assert_eq!(stopped["preview"], "");
    saved(widget.root.join("receipt.json"), &json!(widget.states));
    let expected = reference();
    let expected = expected["states"].as_array().unwrap();
    for (index, (actual, expected)) in widget
        .states
        .iter()
        .take(recording_states)
        .zip(expected)
        .enumerate()
    {
        assert_eq!(
            static_state(actual),
            static_state(expected),
            "released widget state {index}"
        );
    }
    eprintln!(
        "Native publisher/bridge/QML matched {} released states, private focus, motion, reduced-motion and owner-loss boundaries",
        widget.states.len()
    );
}

#[test]
#[ignore = "requires the private widget runner and ffmpeg/ffprobe for the retained 60-fps regression replay"]
fn released_preview_contraction_replay() {
    let mut widget = Widget::new("native-widget-replay");
    widget.observe("idle", None);
    widget.focus_editor();
    widget.observe("idle", None);
    widget.ipc(&["theme", "false"]);
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../linux/tests/fixtures/preview-contraction.json"
    ))
    .unwrap();
    let events = fixture["events"].as_array().unwrap();
    let publish = |event: &Value| {
        widget.publish(json!({"phase":"recording","elapsed_seconds":26,"preview":event["text"]}))
    };
    publish(&events[0]);
    let before = widget.observe("recording", Some(text(&events[0], "text")));
    let log = fs::File::create(widget.root.join("capture.log")).unwrap();
    let dimensions = format!(
        "{:.0}x{:.0}",
        number(&before, "screenWidth"),
        number(&before, "screenHeight")
    );
    let video = widget.root.join("replay.mp4");
    let mut capture = Process::start(
        Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-y",
                "-v",
                "error",
                "-f",
                "x11grab",
                "-framerate",
                "60",
                "-video_size",
                &dimensions,
                "-i",
                &std::env::var("DISPLAY").unwrap(),
                "-t",
                "5",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "0",
            ])
            .arg(&video)
            .stdout(Stdio::null())
            .stderr(log),
    );
    let start = Instant::now();
    for event in &events[1..] {
        while start.elapsed().as_secs_f64() < 0.5 + number(event, "seconds") {
            pump(Duration::from_millis(2));
        }
        widget.publish(json!({"phase":"recording","elapsed_seconds":26,"preview":event["text"]}));
    }
    let mut status = capture.0.try_wait().unwrap();
    while status.is_none() && start.elapsed() < Duration::from_secs(10) {
        pump(Duration::from_millis(2));
        status = capture.0.try_wait().unwrap();
    }
    assert!(
        status.is_some_and(|status| status.success()),
        "private 60-fps capture failed; inspect capture.log"
    );
    let after = widget.observe("recording", Some(text(events.last().unwrap(), "text")));
    let metadata: Value = serde_json::from_str(&output(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_entries",
                "stream=codec_name,width,height,r_frame_rate,nb_read_frames",
                "-of",
                "json",
            ])
            .arg(&video),
    ))
    .unwrap();
    saved(widget.root.join("video.json"), &metadata);
    let baseline = reference();
    assert_eq!(metadata["streams"], baseline["video"]["streams"]);
    assert_eq!(
        static_state(&before),
        static_state(&baseline["replay"]["before"])
    );
    assert_eq!(
        static_state(&after),
        static_state(&baseline["replay"]["after"])
    );
    saved(
        widget.root.join("replay.json"),
        &json!({"fixture":fixture,"display":std::env::var("DISPLAY").unwrap(),"fps":60,"boundary":"Offline provider-event replay, production QML, private X11/D-Bus; no microphone/providers","before":before,"after":after}),
    );
    eprintln!(
        "Native preview contraction replay retained released before/after states and video metadata: {metadata}"
    );
}
