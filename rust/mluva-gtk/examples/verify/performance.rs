//! External production-process observations. No application timing/test API.
use super::*;
use std::{cell::RefCell, rc::Rc};

fn pump() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}

fn wait(mut ready: impl FnMut() -> Result<bool>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        pump();
        if ready()? {
            return Ok(());
        }
        require(Instant::now() < deadline, "performance observation timed out")?;
        thread::sleep(Duration::from_millis(2));
    }
}

fn activate(bus: &gio::DBusConnection, name: &str) -> Result<()> {
    call(bus, APP, APP_PATH, "org.gtk.Actions", "Activate", Some(
        &(name, Vec::<glib::Variant>::new(), BTreeMap::<String, glib::Variant>::new()).to_variant(),
    ))?;
    Ok(())
}

fn widget(root: &Path, checkout: &Path, binaries: &Path) -> Result<std::process::Child> {
    let directory = root.join("widget");
    fs::create_dir(&directory)?;
    for (source, destination) in [
        (checkout.join("linux/quickshell/mluva.dictation"), directory.join("Mluva")),
        (PathBuf::from("/usr/share/omarchy/shell/Commons"), directory.join("Commons")),
        (PathBuf::from("/usr/share/omarchy/shell/Ui"), directory.join("Ui")),
    ] {
        require(Command::new("cp").arg("-r").arg(source).arg(destination).status()?.success(), "widget resources unavailable")?;
    }
    fs::write(directory.join("shell.qml"), r#"
import QtQuick
import Quickshell
import "Mluva" as Mluva
ShellRoot {
    id: root
    Mluva.Widget {
        id: widget
        settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
    }
    readonly property var overlay: widget.data.find(child => child.objectName === "mluva-overlay-loader")?.item
    Connections {
        target: root.overlay
        function onVisibleChanged() { if (root.overlay.visible) console.log("PERF_VISIBLE " + Date.now()); }
        function onDisplayedPreviewChanged() {
            if (root.overlay.displayedPreview.startsWith("Synthetic narration")) console.log("PERF_WORDS " + Date.now());
        }
    }
    Component.onCompleted: console.log("PERF_READY")
}
"#)?;
    let log = fs::File::create(root.join("widget.log"))?;
    Ok(Command::new("quickshell")
        .args(["--no-color", "-p"]).arg(directory.join("shell.qml"))
        .env("MLUVA_SHELL_COMMAND", binaries.join("mluva-shell"))
        .stdout(log.try_clone()?).stderr(log).spawn()?)
}

fn log_timestamp(root: &Path, marker: &str, since: u128) -> Option<f64> {
    fs::read_to_string(root.join("widget.log")).ok()?.lines().filter_map(|line| {
        line.split_once(marker)?.1.trim().parse::<u128>().ok()
    }).find(|stamp| *stamp >= since).map(|stamp| (stamp - since) as f64)
}

fn launch_measured(root: &Path) -> Result<(f64, f64, f64)> {
    let log = fs::OpenOptions::new().create(true).append(true).open(root.join("app.log"))?;
    let started = Instant::now();
    let mut child = Command::new(root.join("bundle/bin/mluva"))
        .env("MLUVA_DISABLE_GLOBAL_SHORTCUT", "1")
        .env("GSETTINGS_BACKEND", "memory")
        .env("PATH", format!("{}:{}", root.join("tools").display(), std::env::var("PATH")?))
        .stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log).spawn()?;
    fs::write(root.join("app-process.json"), serde_json::to_vec(
        &json!({"pid":child.id(),"start":process_identity(child.id())?.1}),
    )?)?;
    wait(|| {
        require(child.try_wait()?.is_none(), "application exited during startup")?;
        Ok(app_owner().is_ok())
    })?;
    let bus_ms = started.elapsed().as_secs_f64() * 1000.0;
    wait(|| Ok(Command::new("xdotool").args(["search", "--onlyvisible", "--pid"])
        .arg(child.id().to_string()).output()?.status.success()))?;
    let visible_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut record = Value::Null;
    wait(|| {
        if let Ok(nodes) = accessibility::nodes()
            && let Some(node) = nodes.as_array().unwrap().iter().find(|node| node["name"] == "Dictate")
        {
            record = node.clone();
            return Ok(true);
        }
        Ok(false)
    })?;
    let a11y = connection(&std::env::var("AT_SPI_BUS_ADDRESS")?)?;
    wait(|| {
        let state = call(&a11y, record["bus"].as_str().unwrap(), record["path"].as_str().unwrap(),
            "org.a11y.atspi.Accessible", "GetState", None)?.child_get::<Vec<u32>>(0);
        Ok(state.first().is_some_and(|bits| [24, 25].iter().all(|bit| bits & (1 << bit) != 0)))
    })?;
    Ok((bus_ms, visible_ms, started.elapsed().as_secs_f64() * 1000.0))
}

pub fn run(root: &Path, checkout: &Path, binaries: &Path) -> Result<Value> {
    // Common isolation and executable/resource staging are identical to smoke.
    let unused = fixture::prepare(root, checkout, binaries)?;
    drop(unused);
    let responses = vec![json!({"route":"speech","status":200,"payload":{"text":fixture::RAW,"language_code":"eng"}}); 4];
    let peer = fixture::http::Peer::new(&responses);
    let path = path_env("XDG_CONFIG_HOME")?.join("mluva/config.json");
    let mut config = mluva_core::config::AppConfig::load(&path)?;
    config.transcription_base_url = format!("{}/v1", peer.address);
    config.transcription_chunk_seconds = 3;
    config.save(&path)?;
    fs::write(root.join("tools/test-config.json"), serde_json::to_vec(
        &json!({"pcm_hex":"01000200".repeat(24_000),"fragments":[320],"wait":true}),
    )?)?;
    let mut widget = widget(root, checkout, binaries)?;
    let outcome = (|| {
        wait(|| Ok(fs::read_to_string(root.join("widget.log"))?.contains("PERF_READY")))?;
        let bus = session_bus()?;
        let events = Rc::new(RefCell::new(Vec::<(Instant, String, bool)>::new()));
        let received = events.clone();
        let subscription = bus.signal_subscribe(Some(APP), Some("com.mluva.Linux.RecordingStatus"),
            Some("StateChanged"), Some("/com/mluva/Linux/RecordingStatus"), None,
            gio::DBusSignalFlags::NONE, move |_, _, _, _, _, parameters| {
                received.borrow_mut().push((Instant::now(), parameters.child_get::<String>(1),
                    parameters.child_get::<String>(7).starts_with("Synthetic narration")));
            });
        let mut samples = vec![];
        for startup in ["fresh-profile", "warm-profile"] {
            let (bus_ms, visible_ms, usable_ms) = launch_measured(root)?;
            for capture in ["first-capture", "repeat-capture"] {
                let _ = fs::remove_file(root.join("tools/raw.ready.json"));
                events.borrow_mut().clear();
                let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
                let action_start = Instant::now();
                activate(&bus, "record")?;
                let action_reply_ms = action_start.elapsed().as_secs_f64() * 1000.0;
                // PCM saved by the actual drain, independently of UI state.
                let recordings = path_env("XDG_DATA_HOME")?.join("mluva/recordings");
                wait(|| Ok(fs::read_dir(&recordings).ok().is_some_and(|entries| entries.filter_map(std::result::Result::ok)
                    .any(|entry| entry.metadata().is_ok_and(|metadata| metadata.len() > 44)))))?;
                let pcm_ms = action_start.elapsed().as_secs_f64() * 1000.0;
                wait(|| Ok(events.borrow().iter().any(|(_, phase, words)| phase == "recording" && *words)
                    && log_timestamp(root, "PERF_WORDS ", wall).is_some()))?;
                let observations = events.borrow();
                let phase_time = |phase: &str| observations.iter().find(|(_, value, _)| value == phase)
                    .map(|(stamp, _, _)| stamp.duration_since(action_start).as_secs_f64() * 1000.0);
                let preview_ms = observations.iter().find(|(_, _, words)| *words)
                    .map(|(stamp, _, _)| stamp.duration_since(action_start).as_secs_f64() * 1000.0);
                samples.push(json!({
                    "startup":startup,"capture":capture,"launch_bus_ms":bus_ms,"launch_visible_ms":visible_ms,"launch_usable_ms":usable_ms,
                    "action_reply_ms":action_reply_ms,"action_first_saved_pcm_ms":pcm_ms,
                    "action_preparing_signal_ms":phase_time("preparing"),"action_recording_signal_ms":phase_time("recording"),
                    "action_preview_signal_ms":preview_ms,"action_widget_visible_ms":log_timestamp(root,"PERF_VISIBLE ",wall),
                    "action_widget_words_ms":log_timestamp(root,"PERF_WORDS ",wall)
                }));
                drop(observations);
                activate(&bus, "cancel")?;
                wait(|| Ok(events.borrow().iter().any(|(_, phase, _)| phase == "hidden")))?;
                require(stored()?["count"] == 0, "cancel persisted synthetic speech")?;
                wait(|| Ok(!recordings.exists() || fs::read_dir(&recordings)?.next().is_none()))?;
                thread::sleep(Duration::from_millis(50));
            }
            action("quit")?;
        }
        bus.signal_unsubscribe(subscription);
        require(peer.observed.lock().unwrap().len() == 4, "expected one preview per recording")?;
        let report = json!({
            "samples":samples,
            "scope":"Production release executable, synthetic unpaced 3-second PCM and loopback batch speech. Fresh/warm private profiles; OS caches not dropped. No microphone/provider/Wayland claim.",
            "precision":"2 ms poll plus IPC/subprocess cost; usable endpoint includes external accessibility traversal after mapped window. Qt timestamps use wall clock milliseconds; other durations use monotonic time.",
            "cancel_history_empty":true
        });
        fs::write(root.join("performance.json"), serde_json::to_vec_pretty(&report)?)?;
        screenshot("performance")?;
        Ok(report)
    })();
    if app_owner().is_ok() { let _ = action("quit"); }
    let _ = widget.kill();
    let _ = widget.wait();
    outcome
}
