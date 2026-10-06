//! Actual browser clipboard/key delivery, frozen release receipts and independent DOM observations.

use gtk::prelude::*;
use mluva_core::delivery::{DeliveryOptions, deliver_text};
use mluva_gtk::text_target::{FocusedTextTargetTracker, system_accessibility_enabled};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[path = "support/text_transport.rs"]
pub mod transport;
use transport::{Monitor, Peer, audit_rows, bus_name_has_owner, delivery, settle, snapshot, write};

fn private_root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net").unwrap().as_os_str(),
        std::env::var_os("MLUVA_HOST_NET_NS").unwrap()
    );
    assert_eq!(std::env::var("XDG_SESSION_TYPE").unwrap(), "x11");
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    let has_audit = audit_rows().is_some();
    if std::env::var_os("MLUVA_BROWSER_DEFAULT_TRANSPORT").is_some() {
        for name in [
            "ATSPI_DISABLE_P2P",
            "ATSPI_IN_TESTS",
            "ATSPI_NO_CACHE",
            "PYATSPI_NOCACHE",
        ] {
            assert!(
                std::env::var_os(name).is_none(),
                "{name} changes default transport/cache"
            );
        }
        assert!(
            has_audit,
            "default transport requires an outgoing-request observer"
        );
    } else {
        assert_eq!(std::env::var("ATSPI_DISABLE_P2P").unwrap(), "1");
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    for path in ["/dev/uinput", "/dev/input", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    let manager = std::env::var("MLUVA_PRIVATE_WM_PID").unwrap();
    assert_eq!(
        fs::read_link(format!("/proc/{manager}/exe"))
            .unwrap()
            .file_name()
            .unwrap(),
        "openbox"
    );
    root
}

fn clipboard() -> String {
    let result = Command::new("xclip")
        .args(["-selection", "clipboard", "-out"])
        .output()
        .unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap()
}

fn merge(base: &mut Value, patch: &Value) {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            merge(base.entry(key).or_insert(Value::Null), value);
        }
    } else {
        *base = patch.clone();
    }
}

fn fixture(engine: &str) -> Value {
    let base = include_str!("fixtures/firefox-target-cases.json");
    let mut fixture: Value = serde_json::from_str(base).unwrap();
    if engine == "chromium" {
        let delta: Value =
            serde_json::from_str(include_str!("fixtures/chromium-target-cases.json")).unwrap();
        assert_eq!(delta["reference_commit"], fixture["reference_commit"]);
        assert_eq!(
            delta["base_sha256"],
            glib::compute_checksum_for_data(glib::ChecksumType::Sha256, base.as_bytes())
                .unwrap()
                .as_str()
        );
        let cases = fixture["cases"].as_array_mut().unwrap();
        let patches = delta["cases"].as_array().unwrap();
        assert_eq!(cases.len(), patches.len());
        for (case, patch) in cases.iter_mut().zip(patches) {
            merge(case, patch);
        }
        fixture["browser"] = delta["browser"].clone();
    } else {
        assert_eq!(engine, "firefox");
    }
    fixture
}

/// Observe the real registry, independently of the Rust listener wrapper.
fn tracker_registrations(bus: &gio::DBusConnection) -> Vec<String> {
    let registrations = bus
        .call_sync(
            Some("org.a11y.atspi.Registry"),
            "/org/a11y/atspi/registry",
            "org.a11y.atspi.Registry",
            "GetRegisteredEvents",
            None,
            None,
            gio::DBusCallFlags::NONE,
            1_000,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .get::<(Vec<(String, String)>,)>()
        .unwrap()
        .0;
    let mut events = Vec::new();
    for (owner, event) in registrations {
        if event != "Object:StateChanged:Focused" && event != "Object:ChildrenChanged:Add" {
            continue;
        }
        let pid = bus
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                Some(&(owner,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                1_000,
                gio::Cancellable::NONE,
            )
            .unwrap()
            .get::<(u32,)>()
            .unwrap()
            .0;
        if pid == std::process::id() {
            events.push(event);
        }
    }
    events.sort();
    events
}

#[test]
#[ignore = "requires private browser runner, Firefox or Chromium/ChromeDriver, xclip/xdotool/Openbox and built browser peer"]
fn actual_browser_clipboard_edits_and_focus_guards_match_the_released_client() {
    let root = private_root();
    let engine = std::env::var("MLUVA_BROWSER_ENGINE").unwrap_or_else(|_| "firefox".into());
    let fixture = fixture(&engine);
    gtk::init().unwrap();
    let application = gtk::Application::new(
        Some("org.example.Mluva.BrowserClient"),
        gio::ApplicationFlags::NON_UNIQUE,
    );
    application.register(gio::Cancellable::NONE).unwrap();
    // Advertise the real client/cache before libatspi can observe own-focus events.
    let own = gtk::ApplicationWindow::builder()
        .application(&application)
        .title("Private Mluva browser client")
        .build();
    let entry = gtk::Entry::new();
    own.set_child(Some(&entry));
    own.present();
    entry.grab_focus();
    settle(Duration::from_millis(400));
    own.set_visible(false);
    settle(Duration::from_millis(200));
    let registry = gio::DBusConnection::for_address_sync(
        &std::env::var("AT_SPI_BUS_ADDRESS").unwrap(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .unwrap();
    assert!(tracker_registrations(&registry).is_empty());
    let expected_registrations = ["Object:ChildrenChanged:Add", "Object:StateChanged:Focused"];
    let mut lifecycle = Vec::new();
    let native_activation = std::env::var_os("MLUVA_CHROMIUM_NATIVE_ACTIVATION").is_some();
    let late_start = std::env::var_os("MLUVA_CHROMIUM_LATE_START").is_some();
    let directory = root.join(format!("native-{engine}-target"));
    let (mut peer, mut startup_tracker) = if late_start {
        assert_eq!(engine, "chromium");
        assert!(native_activation);
        assert_eq!(fixture["cases"][0]["input"]["cold"], true);
        let (peer, tracker) = Peer::before_browser(directory, || {
            assert!(system_accessibility_enabled());
            let tracker = FocusedTextTargetTracker::new().unwrap();
            assert!(delivery(&tracker).is_none(), "no browser is running yet");
            let events = tracker_registrations(&registry);
            assert_eq!(events, expected_registrations);
            lifecycle.push(json!({"stage":"before-browser","events":events}));
            tracker
        });
        (peer, Some(tracker))
    } else {
        (Peer::new(directory, "firefox_text_peer"), None)
    };
    let metadata = peer.observed()["metadata"].clone();
    assert_eq!(metadata["version"], fixture["browser"]["version"]);
    assert_eq!(metadata["build_id"], fixture["browser"]["build_id"]);
    let identity = PathBuf::from(metadata["identity"].as_str().unwrap());
    assert_eq!(
        fs::read_link(format!("/proc/{}/exe", metadata["pid"])).unwrap(),
        identity
    );
    assert!(system_accessibility_enabled());
    assert!(!bus_name_has_owner("org.freedesktop.portal.Desktop"));
    if engine == "chromium" && !native_activation {
        peer.request(json!({"operation":"enable_accessibility"}));
    }
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut explicit_read_controls = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let name = input["name"].as_str().unwrap();
        peer.request(json!({"operation":"focus","kind":"button"}));
        let mut setup = input["setup"].clone();
        setup["operation"] = "setup".into();
        if input["cold"] == true {
            peer.request(setup.clone());
        }
        let mut tracker = startup_tracker
            .take()
            .unwrap_or_else(|| FocusedTextTargetTracker::new().unwrap());
        if results.is_empty() {
            let events = tracker_registrations(&registry);
            assert_eq!(events, expected_registrations);
            lifecycle.push(json!({"stage":"first-target","events":events}));
        }
        if input["cold"] != true {
            peer.request(setup);
        }
        if input["nontext"] == true {
            peer.request(json!({"operation":"focus","kind":"button"}));
        }
        let mut control = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        control
            .stdin
            .take()
            .unwrap()
            .write_all(b"UNCHANGED CLIPBOARD CONTROL")
            .unwrap();
        assert!(control.wait().unwrap().success());
        let monitor = Monitor::new(peer.directory.join(format!("monitor-{name}.log")));
        let audit_start = audit_rows().map(|rows| rows.len());
        let mut outcome = json!({});
        let target = if input["mode"] == "command" {
            match tracker.capture_text_target(input["maximum"].as_i64().unwrap()) {
                Ok(target) => {
                    outcome["snapshot"] = snapshot(target.as_ref(), &identity);
                    target
                }
                Err(error) => {
                    outcome["error"] = error.to_string().into();
                    None
                }
            }
        } else {
            let target = delivery(&tracker);
            outcome["snapshot"] = snapshot(target.as_ref(), &identity);
            target
        };
        outcome["tracker_identity_matches"] =
            (tracker.capture_application_identifier().as_deref() == identity.to_str()).into();
        if let Some(target) = &target {
            outcome["without_selection"] =
                snapshot(Some(&target.without_selected_text()), &identity);
        }
        if input["close"] == true {
            tracker.close();
        }
        if !input["before"].is_null() {
            peer.request(input["before"].clone());
        }
        if input["own_focus"] == true {
            own.present();
            entry.grab_focus();
            settle(Duration::from_millis(300));
            outcome["own_capture"] = snapshot(delivery(&tracker).as_ref(), &identity);
        }
        if outcome.get("error").is_none() {
            let restored = target
                .as_ref()
                .is_some_and(|target| input["auto"] == true && target.restore());
            outcome["restored"] = restored.into();
            let payload = input["payload"].as_str().unwrap();
            let receipt = if let Some(target) = target.as_ref().filter(|_| restored) {
                let mut insert = |text: &str| Ok(target.insert_text(text));
                let mut confirm = || Ok(target.confirm_insertion(payload));
                let mut authorize = || {
                    if input["late_focus"] == true {
                        peer.request(json!({"operation":"focus","kind":"second"}));
                    }
                    settle(Duration::from_millis(30));
                    Ok(target.restore())
                };
                deliver_text(
                    payload,
                    true,
                    DeliveryOptions {
                        insert_directly: Some(&mut insert),
                        confirm_paste: Some(&mut confirm),
                        authorize_keyboard_paste: Some(&mut authorize),
                        application_identifier: target.application_identifier(),
                        ..Default::default()
                    },
                )
            } else {
                deliver_text(payload, false, DeliveryOptions::default())
            }
            .unwrap();
            outcome["receipt"] = json!({"copied":receipt.copied,"pasted":receipt.pasted,"guidance":receipt.guidance,
                "paste_dispatched":receipt.paste_dispatched,"paste_confirmed":receipt.paste_confirmed,"history_outcome":receipt.history_outcome()});
            if let Some(target) = &target {
                outcome["confirmed_after"] = json!(target.confirm_insertion(payload));
            }
        }
        settle(Duration::from_millis(300));
        let observed = peer.observed()["observed"].clone();
        outcome["observed"] = json!({"fields":observed["fields"],"focus":observed["focus"],"events":observed["events"]});
        outcome["clipboard"] = clipboard().into();
        let bus_reads = monitor.finish();
        let audited = audit_start.map(|start| {
            let rows = audit_rows().unwrap().split_off(start);
            assert!(
                rows.iter()
                    .all(|row| row["pid"] == std::process::id() && row["valid"] == true)
            );
            let ranges = json!(
                rows.iter()
                    .map(|row| vec![row["start"].clone(), row["end"].clone()])
                    .collect::<Vec<_>>()
            );
            if std::env::var_os("MLUVA_BROWSER_DEFAULT_TRANSPORT").is_none() {
                assert_eq!(
                    ranges, bus_reads,
                    "observer agrees with the independent bus monitor"
                );
            }
            (rows, ranges)
        });
        outcome["text_reads"] = if std::env::var_os("MLUVA_BROWSER_DEFAULT_TRANSPORT").is_some() {
            audited.as_ref().unwrap().1.clone()
        } else {
            bus_reads.clone()
        };
        if input["mode"] == "command" {
            assert_eq!(
                outcome["text_reads"].as_array().unwrap().len(),
                1,
                "Command {name} must observe its actual GetText request"
            );
            explicit_read_controls += 1;
            if name == "command-ascii-selection" {
                assert_eq!(outcome["snapshot"]["selected_text"], "CONTROL");
                assert_eq!(outcome["text_reads"], json!([[7, 14]]));
            }
        } else {
            assert!(outcome["text_reads"].as_array().unwrap().is_empty());
        }
        if outcome != case["expected"] {
            failures.push(name.to_owned());
            eprintln!("{name}: expected {}, actual {outcome}", case["expected"]);
        }
        let mut result = json!({"name":name,"actual":outcome});
        if let Some((rows, _)) = audited {
            result["outgoing_text_requests"] = json!(rows);
            result["bus_text_requests"] = bus_reads;
        }
        results.push(result);
        let deregistration = (results.len() == 1)
            .then(|| Monitor::deregistration(peer.directory.join("tracker-close.log")));
        tracker.close();
        if let Some(deregistration) = deregistration {
            settle(Duration::from_millis(100));
            let deregistered = deregistration.finish_deregistration();
            assert_eq!(
                deregistered,
                [
                    "object:children-changed:add",
                    "object:state-changed:focused"
                ]
            );
            // 2.60.6 retains global registry entries (NULL vs empty app-name matching),
            // although libatspi removes the local callbacks before this synchronous call.
            let remaining = tracker_registrations(&registry);
            peer.request(json!({"operation":"focus","kind":"second"}));
            assert!(
                delivery(&tracker).is_none(),
                "a closed tracker observed later focus"
            );
            lifecycle.push(json!({"stage":"first-close","deregistered":deregistered,
                "registry_entries":remaining, "capture_after_focus":null}));
        }
        if input["own_focus"] == true {
            own.set_visible(false);
            settle(Duration::from_millis(180));
        }
    }
    assert!(!bus_name_has_owner("org.freedesktop.portal.Desktop"));
    assert_eq!(explicit_read_controls, 3);
    if let Some(rows) = audit_rows() {
        assert_eq!(
            rows.len(),
            3,
            "include requests outside case observation intervals"
        );
        let direct = std::env::var_os("MLUVA_BROWSER_DEFAULT_TRANSPORT").is_some();
        assert!(rows.iter().all(|row| row["pid"] == std::process::id()
            && row["valid"] == true
            && row["peer"] == direct));
    }
    peer.request(json!({"operation":"quit"}));
    peer.await_successful_exit();
    assert!(!Path::new(&format!("/proc/{}", metadata["pid"])).exists());
    assert!(!bus_name_has_owner("org.a11y.Bus"));
    own.close();
    settle(Duration::from_millis(180));
    if let Some(rows) = audit_rows() {
        assert_eq!(rows.len(), 3, "include normal peer/client shutdown");
    }
    write(
        &root.join(format!("native-{engine}-target-observations.json")),
        &json!({"cases":results,"browser":fixture["browser"], "tracker_lifecycle":lifecycle,
            "native_activation":native_activation, "late_start":late_start}),
    );
    assert!(
        failures.is_empty(),
        "released browser mismatches: {failures:?}"
    );
}
