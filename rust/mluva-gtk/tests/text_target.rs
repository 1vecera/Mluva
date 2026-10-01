//! Frozen release receipts, real GTK edits and independently monitored disclosure on private buses.

use gtk::prelude::*;
use mluva_gtk::text_target::{FocusedTextTargetTracker, system_accessibility_enabled};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[path = "support/text_transport.rs"]
pub mod transport;
use transport::{Monitor, Peer, bus_name_has_owner, delivery, settle, snapshot, write};

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
    // Advertise the client's cache before the first native accessibility query can see own focus.
    let own = gtk::ApplicationWindow::builder()
        .application(&application)
        .title("Private Mluva own client")
        .default_width(180)
        .default_height(100)
        .build();
    let own_entry = gtk::Entry::new();
    own.set_child(Some(&own_entry));
    own.present();
    own_entry.grab_focus();
    settle(Duration::from_millis(400));
    own.set_visible(false);
    settle(Duration::from_millis(200));
    let mut peer = Peer::new(root.join("native-text-target"), "text_target_peer");
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
                own.present();
                own_entry.grab_focus();
                settle(Duration::from_millis(300));
                outcome["own_capture"] = snapshot(delivery(&tracker).as_ref(), &peer.executable);
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
        if input["own_focus"] == true {
            own.set_visible(false);
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
    own.close();
    assert!(
        failures.is_empty(),
        "released transport mismatches: {failures:?}"
    );
}
