//! Actual Firefox clipboard/key delivery, frozen release receipts and independent DOM observations.

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

#[test]
#[ignore = "requires run-isolated-browser.sh, real Firefox/xclip/xdotool/Openbox and built firefox_text_peer"]
fn actual_browser_clipboard_edits_and_focus_guards_match_the_released_client() {
    let root = private_root();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/firefox-target-cases.json")).unwrap();
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
    let mut peer = Peer::new(root.join("native-firefox-target"), "firefox_text_peer");
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
        let mut tracker = FocusedTextTargetTracker::new().unwrap();
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
        tracker.close();
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
    write(
        &root.join("native-firefox-target-observations.json"),
        &json!({"cases":results,"browser":fixture["browser"]}),
    );
    assert!(
        failures.is_empty(),
        "released browser mismatches: {failures:?}"
    );
}
