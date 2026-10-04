//! Compare the public owner with independent released observations on a real bus.
use glib::variant::ToVariant;
use mluva_gtk::{
    async_runtime::DesktopRuntime,
    global_shortcuts::{GlobalShortcutService, ShortcutCallbacks},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
    time::{Duration, Instant},
};

struct Peer(Child);
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn control(bus: &gio::DBusConnection, method: &str, value: Option<&str>) -> Value {
    let parameters = value.map(|value| (value,).to_variant());
    let reply = bus
        .call_future(
            Some("org.freedesktop.portal.Desktop"),
            "/com/mluva/TestPortal",
            "com.mluva.TestPortal",
            method,
            parameters.as_ref(),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .unwrap();
    if method == "Snapshot" {
        serde_json::from_str(&reply.get::<(String,)>().unwrap().0).unwrap()
    } else {
        Value::Null
    }
}
async fn settle() {
    glib::timeout_future(Duration::from_millis(35)).await;
}

#[test]
#[ignore = "requires guarded private bus/network/device runner and native portal peer"]
fn released_portal_approval_events_rebinding_and_shutdown() {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private runner required"));
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
    }
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    let mut peer = Peer(
        Command::new(env!("CARGO_BIN_EXE_global-shortcut-fixture-peer"))
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut ready = String::new();
    BufReader::new(peer.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "portal-ready");
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-shortcuts.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let runtime = DesktopRuntime::new().unwrap();
    glib::MainContext::default().block_on(compare(&runtime, &fixture, &mut peer));
}

async fn compare(runtime: &Rc<DesktopRuntime>, fixture: &Value, peer: &mut Peer) {
    let bus = gio::DBusConnection::for_address_future(
        &std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
    )
    .await
    .unwrap();
    let mut states = 0;
    for row in fixture["workflows"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        control(&bus, "Reset", row["scenario"].as_str()).await;
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let callback = |action: &'static str| {
            let events = events.clone();
            Rc::new(move || events.borrow_mut().push(json!([action]))) as Rc<dyn Fn()>
        };
        let binding = events.clone();
        let rewrite = events.clone();
        let errors = events.clone();
        let owner = GlobalShortcutService::new(
            runtime.clone(),
            "F9",
            ShortcutCallbacks {
                toggle_recording: callback("record"),
                cancel: callback("cancel"),
                open_rewrite: callback("rewrite"),
                binding_changed: Rc::new(move |key, value| {
                    binding.borrow_mut().push(json!(["binding", key, value]))
                }),
                rewrite_binding_changed: Rc::new(move |value| {
                    rewrite.borrow_mut().push(json!(["rewrite-binding", value]))
                }),
                error: Rc::new(move |value| errors.borrow_mut().push(json!(["error", value]))),
            },
        )
        .unwrap();
        for step in row["steps"].as_array().unwrap() {
            let action = &step["action"];
            match action["op"].as_str().unwrap() {
                "start" => owner.start(),
                "key" => owner
                    .set_recording_key(action["key"].as_str().unwrap())
                    .unwrap(),
                "invalid-key" => {
                    let error = owner
                        .set_recording_key(action["key"].as_str().unwrap())
                        .unwrap_err();
                    events
                        .borrow_mut()
                        .push(json!(["invalid-key", error.to_string()]));
                }
                _ => {
                    control(&bus, "Drive", Some(&action.to_string())).await;
                }
            }
            let end = Instant::now() + Duration::from_secs(6);
            let pending = action["op"] == "start"
                && ["queued-key", "pending-create", "pending-bind"].contains(&name);
            loop {
                let ready = if pending {
                    let snapshot = control(&bus, "Snapshot", None).await;
                    snapshot["trace"].as_array().unwrap().iter().any(|value| {
                        value["method"]
                            == if name == "pending-create" {
                                "CreateSession"
                            } else {
                                "BindShortcuts"
                            }
                    })
                } else {
                    events.borrow().len() >= step["events"].as_array().unwrap().len()
                };
                if ready {
                    break;
                }
                if Instant::now() >= end {
                    let snapshot = control(&bus, "Snapshot", None).await;
                    panic!(
                        "{name}: {action}: events={:?}; portal={snapshot}",
                        events.borrow()
                    );
                }
                glib::timeout_future(Duration::from_millis(5)).await;
            }
            settle().await;
            assert_eq!(json!(*events.borrow()), step["events"], "{name}: {action}");
            states += 1;
        }
        let before = events.borrow().clone();
        owner.shutdown().await;
        let end = Instant::now() + Duration::from_secs(3);
        let actual = loop {
            let observed = control(&bus, "Snapshot", None).await;
            if observed["disconnected"].as_array().unwrap().len()
                == row["portal"]["disconnected"].as_array().unwrap().len()
            {
                break observed;
            }
            assert!(
                Instant::now() < end,
                "{name}: portal connection survived shutdown: {observed}"
            );
            glib::timeout_future(Duration::from_millis(5)).await;
        };
        assert_eq!(
            actual, row["portal"],
            "{name}: actual request/cleanup trace"
        );
        // The released thread reports its cancelled approval during close.
        // A terminal native owner must remain quiet after shutdown begins.
        control(
            &bus,
            "Drive",
            Some(&json!({"op":"Activated","id":"toggle-recording-f9"}).to_string()),
        )
        .await;
        owner.start();
        owner.set_recording_key("F12").unwrap();
        owner.shutdown().await;
        settle().await;
        assert_eq!(*events.borrow(), before, "{name}: callback after shutdown");
        assert_eq!(
            control(&bus, "Snapshot", None).await,
            actual,
            "{name}: work after shutdown"
        );
        assert!(peer.0.try_wait().unwrap().is_none(), "portal peer exited");
    }
    bus.close_future().await.unwrap();
    runtime.shutdown(async {}).await.unwrap();
    eprintln!(
        "Compared {} released portal workflows / {states} observed states, including acknowledged cleanup",
        fixture["workflows"].as_array().unwrap().len()
    );
}
