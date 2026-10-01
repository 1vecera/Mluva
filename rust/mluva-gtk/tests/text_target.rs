//! Frozen release receipts, real GTK edits and independently monitored disclosure on private buses.

use gtk::prelude::*;
use mluva_gtk::text_target::{
    DeliveryTargetSnapshot, FocusedTextTargetTracker, TextTargetSnapshot,
    system_accessibility_enabled,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

fn settle(duration: Duration) {
    let deadline = Instant::now() + duration;
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn write(path: &Path, value: &Value) {
    let temporary = path.with_extension("temporary");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}

struct Peer {
    process: Child,
    directory: PathBuf,
    executable: PathBuf,
    serial: u64,
}
impl Peer {
    fn new(directory: PathBuf) -> Self {
        fs::create_dir(&directory).unwrap();
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples/text_target_peer")
            .canonicalize()
            .expect("build text_target_peer first");
        let log = fs::File::create(directory.join("target.log")).unwrap();
        let process = Command::new(&executable)
            .arg(&directory)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut peer = Self {
            process,
            directory,
            executable,
            serial: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(6);
        while !peer.directory.join("observed.json").exists() {
            assert!(peer.process.try_wait().unwrap().is_none(), "target exited");
            assert!(
                Instant::now() < deadline,
                "target did not expose its observer"
            );
            settle(Duration::from_millis(25));
        }
        peer
    }
    fn observed(&self) -> Value {
        serde_json::from_slice(&fs::read(self.directory.join("observed.json")).unwrap()).unwrap()
    }
    fn request(&mut self, mut value: Value) {
        self.serial += 1;
        value["serial"] = self.serial.into();
        write(&self.directory.join("request.json"), &value);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.observed()["serial"] == self.serial {
                settle(Duration::from_millis(180));
                return;
            }
            assert!(
                Instant::now() < deadline,
                "target request not acknowledged: {value}"
            );
            assert!(
                self.process.try_wait().unwrap().is_none(),
                "target exited during request"
            );
            settle(Duration::from_millis(20));
        }
    }
    fn stop(&mut self) {
        if self.process.try_wait().unwrap().is_none() {
            self.process.kill().unwrap();
            self.process.wait().unwrap();
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Monitor {
    process: Child,
    path: PathBuf,
}
impl Monitor {
    fn new(path: PathBuf) -> Self {
        let log = fs::File::create(&path).unwrap();
        let mut process = Command::new("dbus-monitor")
            .args([
                "--address",
                &std::env::var("AT_SPI_BUS_ADDRESS").unwrap(),
                "type='method_call',interface='org.a11y.atspi.Text',member='GetText'",
            ])
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let log = fs::read_to_string(&path).unwrap();
            if log.contains("member=NameLost") {
                break;
            }
            assert!(
                process.try_wait().unwrap().is_none(),
                "private monitor failed: {log}"
            );
            assert!(
                Instant::now() < deadline,
                "monitor never acquired monitoring privileges: {log}"
            );
            settle(Duration::from_millis(10));
        }
        Self { process, path }
    }
    fn finish(mut self) -> Value {
        // SIGINT flushes dbus-monitor's private file before the exit is reaped.
        assert_eq!(
            unsafe { libc::kill(self.process.id() as i32, libc::SIGINT) },
            0
        );
        self.process.wait().unwrap();
        let log = fs::read_to_string(&self.path).unwrap();
        let mut offsets = Vec::new();
        for message in log.split("method call ").skip(1) {
            if message.lines().next().unwrap().contains("member=GetText") {
                let numbers = message
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("int32 "))
                    .map(|value| value.parse::<i32>().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    numbers.len(),
                    2,
                    "not a bounded explicit GetText range: {message}"
                );
                offsets.push(numbers);
            }
        }
        json!(offsets)
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        if self.process.try_wait().unwrap().is_none() {
            self.process.kill().unwrap();
            self.process.wait().unwrap();
        }
    }
}

fn snapshot(target: Option<&TextTargetSnapshot>, executable: &Path) -> Value {
    target.map_or(Value::Null, |target| json!({
        "selected_text":target.selected_text(), "selection":target.selection(), "caret":target.caret_offset(),
        "editable":target.editable_text_available(), "identity_matches":target.application_identifier() == executable.to_str(),
    }))
}
fn delivery(tracker: &FocusedTextTargetTracker) -> Option<TextTargetSnapshot> {
    tracker
        .capture_delivery_target()
        .map(|target| match target {
            DeliveryTargetSnapshot::Text(target) => target,
            DeliveryTargetSnapshot::Terminal(_) => {
                panic!("private X11 session must not capture a host compositor")
            }
        })
}

fn bus_name_has_owner(name: &str) -> bool {
    let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    session
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1_000,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .get::<(bool,)>()
        .unwrap()
        .0
}

#[test]
#[ignore = "requires private Xvfb, session/accessibility buses, data and built text_target_peer"]
fn captured_focus_selection_mutation_and_disclosure_match_real_released_transport() {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private session required"),
    )
    .canonicalize()
    .unwrap();
    for name in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(name).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/text-target-cases.json")).unwrap();
    assert_eq!(
        format!(
            "{}.{}.{}",
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ),
        fixture["gtk_version"]
    );
    gtk::init().unwrap();
    let application = gtk::Application::new(
        Some("org.example.Mluva.TextClient"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    application.register(gio::Cancellable::NONE).unwrap();
    let mut peer = Peer::new(root.join("native-text-target"));
    assert!(system_accessibility_enabled());
    assert!(
        !bus_name_has_owner("org.freedesktop.portal.Desktop"),
        "this comparison must not start a portal"
    );
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        peer.request(json!({"operation":"enabled", "value":input["disabled"] != true}));
        peer.request(json!({"operation":"button"}));
        if input["disabled"] == true {
            assert!(
                bus_name_has_owner("org.a11y.Bus"),
                "disabled service must remain present for this control"
            );
            let error = match FocusedTextTargetTracker::new() {
                Ok(_) => panic!("disabled status admitted a tracker"),
                Err(error) => error.to_string(),
            };
            let outcome = json!({"desktop_enabled":system_accessibility_enabled(), "error":error});
            assert_eq!(outcome, case["expected"], "{name}");
            results.push(json!({"name":name, "actual":outcome}));
            continue;
        }
        let mut setup = input["setup"].clone();
        setup["operation"] = "setup".into();
        if input["cold"] == true {
            peer.request(setup.clone());
        }
        let mut tracker = FocusedTextTargetTracker::new().unwrap();
        if input["cold"] != true {
            peer.request(setup);
        }
        if input["nontext"] == true {
            peer.request(json!({"operation":"button"}));
        }
        let monitor = Monitor::new(peer.directory.join(format!("monitor-{name}.log")));
        let mut outcome = json!({"desktop_enabled":system_accessibility_enabled()});
        let target = if input["mode"] == "command" {
            match tracker.capture_text_target(input["maximum"].as_i64().unwrap()) {
                Ok(target) => {
                    outcome["snapshot"] = snapshot(target.as_ref(), &peer.executable);
                    target
                }
                Err(error) => {
                    outcome["error"] = error.to_string().into();
                    None
                }
            }
        } else {
            let target = delivery(&tracker);
            outcome["snapshot"] = snapshot(target.as_ref(), &peer.executable);
            target
        };
        outcome["tracker_identity_matches"] = (tracker.capture_application_identifier().as_deref()
            == peer.executable.to_str())
        .into();
        let mut own = None;
        if let Some(target) = &target {
            outcome["without_selection"] =
                snapshot(Some(&target.without_selected_text()), &peer.executable);
            if input["close"] == true {
                tracker.close();
            }
            if !input["before"].is_null() {
                peer.request(input["before"].clone());
            }
            if input["own_focus"] == true {
                let window = gtk::ApplicationWindow::builder()
                    .application(&application)
                    .title("Private Mluva own client")
                    .default_width(180)
                    .default_height(100)
                    .build();
                let entry = gtk::Entry::new();
                window.set_child(Some(&entry));
                window.present();
                entry.grab_focus();
                settle(Duration::from_millis(300));
                outcome["own_capture"] = snapshot(delivery(&tracker).as_ref(), &peer.executable);
                own = Some(window);
            }
            if input["kill"] == true {
                peer.stop();
                settle(Duration::from_millis(300));
            }
            let payload = input["payload"].as_str().unwrap();
            outcome["restored"] = target.restore().into();
            outcome["inserted"] = json!(target.insert_text(payload));
            settle(Duration::from_millis(200));
            outcome["confirmed"] = json!(target.confirm_insertion(payload));
        }
        if input["kill"] != true {
            let observed = peer.observed();
            let field = &observed[input["setup"]["kind"].as_str().unwrap()];
            outcome["observed"] = json!({"text":field["text"], "caret":field["caret"], "selection":field["selection"], "editable":field["editable"]});
            outcome["edits"] = observed["edits"].clone();
        }
        tracker.close();
        if let Some(window) = own {
            window.close();
            settle(Duration::from_millis(180));
        }
        outcome["text_reads"] = monitor.finish();
        if outcome != case["expected"] {
            failures.push(name.to_owned());
            eprintln!("{name}: expected {}, actual {outcome}", case["expected"]);
        }
        results.push(json!({"name":name, "actual":outcome}));
    }
    assert!(
        !bus_name_has_owner("org.a11y.Bus"),
        "target status owner must have exited"
    );
    let status = json!({"enabled":system_accessibility_enabled(), "error":match FocusedTextTargetTracker::new() {
        Ok(_) => panic!("absent status owner admitted a tracker"), Err(error) => error.to_string(),
    }});
    assert_eq!(status, fixture["status_after_owner_exit"]);
    assert!(
        !bus_name_has_owner("org.a11y.Bus"),
        "status queries must not auto-start an absent service"
    );
    write(
        &root.join("native-text-target-observations.json"),
        &json!({"cases":results, "status_after_owner_exit":status}),
    );
    assert!(
        failures.is_empty(),
        "released transport mismatches: {failures:?}"
    );
}
