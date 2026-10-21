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
                persist_bindings: Rc::new(|| true),
                agent: None,
                annotation: None,
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
    agent_toggles(runtime, &bus).await;
    annotation_actions(runtime, &bus).await;
    bus.close_future().await.unwrap();
    runtime.shutdown(async {}).await.unwrap();
    eprintln!(
        "Compared {} released portal workflows / {states} observed states, including acknowledged cleanup",
        fixture["workflows"].as_array().unwrap().len()
    );
}

async fn agent_toggles(runtime: &Rc<DesktopRuntime>, bus: &gio::DBusConnection) {
    control(bus, "Reset", Some("normal")).await;
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let event = |name: &'static str| {
        let events = events.clone();
        Rc::new(move || events.borrow_mut().push(name.into())) as Rc<dyn Fn()>
    };
    let binding = events.clone();
    let owner = GlobalShortcutService::new(
        runtime.clone(),
        "F9",
        ShortcutCallbacks {
            persist_bindings: Rc::new(|| true),
            annotation: None,
            agent: Some(mluva_gtk::global_shortcuts::AgentShortcutCallbacks {
                hold_key: "F8".into(),
                draft_key: "F7".into(),
                toggle_recording: event("toggle"),
                keep_draft: event("draft"),
                binding_changed: Rc::new(move |hold, draft| {
                    assert_eq!(hold, Some("F8"));
                    assert_eq!(draft, Some("F7"));
                    binding.borrow_mut().push("bound".into());
                }),
            }),
            toggle_recording: event("record"),
            cancel: event("cancel"),
            open_rewrite: event("rewrite"),
            binding_changed: Rc::new(|_, _| {}),
            rewrite_binding_changed: Rc::new(|_| {}),
            error: Rc::new(|_| {}),
        },
    )
    .unwrap();
    owner.start();
    let deadline = Instant::now() + Duration::from_secs(5);
    while events.borrow().is_empty() {
        assert!(Instant::now() < deadline);
        settle().await;
    }
    for action in [
        json!({"op":"Deactivated","id":"agent-hold"}),
        json!({"op":"Activated","id":"agent-hold","foreign":true}),
        json!({"op":"Activated","id":"agent-hold"}),
        json!({"op":"Activated","id":"agent-hold"}),
        json!({"op":"Deactivated","id":"agent-hold","foreign":true}),
        json!({"op":"Deactivated","id":"agent-hold"}),
        json!({"op":"Deactivated","id":"agent-hold"}),
        json!({"op":"Activated","id":"agent-hold"}),
        json!({"op":"Deactivated","id":"agent-hold"}),
        json!({"op":"Activated","id":"agent-keep-draft"}),
        json!({"op":"Activated","id":"toggle-recording-f9"}),
    ] {
        control(bus, "Drive", Some(&action.to_string())).await;
        settle().await;
    }
    assert_eq!(
        &*events.borrow(),
        &["bound", "toggle", "toggle", "draft", "record"]
    );
    owner.shutdown().await;
    control(
        bus,
        "Drive",
        Some(&json!({"op":"Activated","id":"agent-hold"}).to_string()),
    )
    .await;
    settle().await;
    assert_eq!(events.borrow().len(), 5);
}

fn annotation_service(
    runtime: &Rc<DesktopRuntime>,
    events: &Rc<RefCell<Vec<String>>>,
) -> Rc<GlobalShortcutService> {
    let ink = events.clone();
    let pin = events.clone();
    let registered = events.clone();
    let bound = events.clone();
    GlobalShortcutService::new(
        runtime.clone(),
        "F9",
        ShortcutCallbacks {
            persist_bindings: Rc::new(|| true),
            agent: None,
            annotation: Some(mluva_gtk::global_shortcuts::AnnotationShortcutCallbacks {
                ink: Rc::new(move |down| {
                    ink.borrow_mut()
                        .push(if down { "down" } else { "up" }.into())
                }),
                pin: Rc::new(move || pin.borrow_mut().push("pin".into())),
                registered: Rc::new(move |ready| {
                    registered.borrow_mut().push(format!("registered:{ready}"))
                }),
            }),
            toggle_recording: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            open_rewrite: Rc::new(|| {}),
            binding_changed: Rc::new(move |_, trigger| {
                bound
                    .borrow_mut()
                    .push(format!("bound:{}", trigger.is_some()))
            }),
            rewrite_binding_changed: Rc::new(|_| {}),
            error: Rc::new(|_| {}),
        },
    )
    .unwrap()
}

/// Ink delivers press and release, including a release whose press predates the binding;
/// pins fire on every click and nothing arrives after shutdown. The application learns whether
/// the portal registered both actions, and binds Hyprland keys only when it did.
async fn annotation_actions(runtime: &Rc<DesktopRuntime>, bus: &gio::DBusConnection) {
    control(bus, "Reset", Some("missing-shortcuts")).await;
    let refused = Rc::new(RefCell::new(Vec::<String>::new()));
    let owner = annotation_service(runtime, &refused);
    owner.start();
    let deadline = Instant::now() + Duration::from_secs(5);
    while refused.borrow().len() < 2 {
        assert!(Instant::now() < deadline);
        settle().await;
    }
    // A failed portal session reports the actions unregistered and the recording key unbound.
    assert_eq!(&*refused.borrow(), &["registered:false", "bound:false"]);
    owner.shutdown().await;

    control(bus, "Reset", Some("normal")).await;
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let owner = annotation_service(runtime, &events);
    owner.start();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !events.borrow().iter().any(|event| event == "bound:true") {
        assert!(Instant::now() < deadline);
        settle().await;
    }
    for action in [
        json!({"op":"Deactivated","id":"annotate-ink"}),
        json!({"op":"Activated","id":"annotate-ink","foreign":true}),
        json!({"op":"Activated","id":"annotate-ink"}),
        json!({"op":"Activated","id":"annotate-ink"}),
        json!({"op":"Activated","id":"annotate-pin"}),
        json!({"op":"Deactivated","id":"annotate-pin"}),
        json!({"op":"Activated","id":"annotate-pin"}),
        json!({"op":"Deactivated","id":"annotate-ink"}),
    ] {
        control(bus, "Drive", Some(&action.to_string())).await;
        settle().await;
    }
    assert_eq!(
        &*events.borrow(),
        &[
            "registered:true",
            "bound:true",
            "up",
            "down",
            "pin",
            "pin",
            "up"
        ]
    );
    owner.shutdown().await;
    control(
        bus,
        "Drive",
        Some(&json!({"op":"Activated","id":"annotate-pin"}).to_string()),
    )
    .await;
    settle().await;
    assert_eq!(events.borrow().len(), 7);
}
