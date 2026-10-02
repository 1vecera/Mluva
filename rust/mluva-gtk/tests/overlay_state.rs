//! Frozen projection limits and independent actual session-bus receipts.
use mluva_gtk::overlay_state::{INTERFACE, OBJECT_PATH, OverlayPublisher, OverlayState};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

type SignalValues = (
    bool,
    String,
    String,
    u32,
    String,
    String,
    f64,
    String,
    String,
);
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/released-overlay-state.json")).unwrap()
}
fn shell(value: &glib::Variant) -> Value {
    let values = value
        .get::<BTreeMap<String, glib::Variant>>()
        .unwrap()
        .into_iter()
        .map(|(key, value)| {
            let result = match value.type_().as_str() {
                "s" => json!(value.get::<String>().unwrap()),
                "b" => json!(value.get::<bool>().unwrap()),
                "u" => json!(value.get::<u32>().unwrap()),
                "d" => json!(value.get::<f64>().unwrap()),
                "a(ss)" => json!(value.get::<Vec<(String, String)>>().unwrap()),
                other => panic!("Unknown projection type {other}"),
            };
            (
                key,
                json!({"signature":value.type_().as_str(),"value":result}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Value::Object(values)
}
fn state(spec: &Value) -> OverlayState {
    let mut state: OverlayState = serde_json::from_value(spec.clone()).unwrap();
    if let Some(repeat) = spec.get("preview_spec") {
        state.preview = repeat["unit"]
            .as_str()
            .unwrap()
            .repeat(repeat["count"].as_u64().unwrap() as usize);
    }
    if let Some(level) = spec["level_spec"].as_str() {
        state.level = match level {
            "nan" => f64::NAN,
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            other => panic!("Unknown level {other}"),
        };
    }
    state
}
#[test]
fn bounded_native_projections_match_released_signatures_and_values() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let state = state(&case["input"]);
        let signal = state.signal();
        let shell_value = state.shell();
        assert_eq!(
            json!({"signal_signature":signal.type_().as_str(),"signal":signal.get::<SignalValues>().unwrap(),"shell_signature":shell_value.type_().as_str(),"shell":shell(&shell_value.child_value(0))}),
            case["result"],
            "{}",
            case["name"]
        );
    }
}
fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
#[test]
#[ignore = "requires private Linux network/PID/display/session-bus environment"]
fn actual_overlay_signals_replay_and_erase_cached_text() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/uinput", "/dev/input", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    let flags = gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
        | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION;
    let observer =
        gio::DBusConnection::for_address_sync(&address, flags, None, gio::Cancellable::NONE)
            .unwrap();
    let events = Rc::new(RefCell::new(vec![]));
    let received = events.clone();
    let subscription=observer.subscribe_to_signal(None,Some(INTERFACE),None,Some(OBJECT_PATH),None,gio::DBusSignalFlags::NONE,move|signal|{
        assert_eq!(signal.object_path,OBJECT_PATH);assert_eq!(signal.interface_name,INTERFACE);
        let values=if signal.parameters.type_().as_str()=="(a{sv})"{shell(&signal.parameters.child_value(0))}else{json!(signal.parameters.get::<SignalValues>().unwrap())};
        received.borrow_mut().push(json!({"name":signal.signal_name,"signature":signal.parameters.type_().as_str(),"values":values}));
    });
    observer.flush_sync(gio::Cancellable::NONE).unwrap();
    let connection =
        gio::DBusConnection::for_address_sync(&address, flags, None, gio::Cancellable::NONE)
            .unwrap();
    let mut publisher = OverlayPublisher::new(connection.clone());
    let fixture = fixture();
    for operation in fixture["operations"].as_array().unwrap() {
        let before = events.borrow().len();
        let result = match operation["method"].as_str().unwrap() {
            "publish" => publisher.publish(&state(&operation["input"])),
            "replay" => publisher.replay(),
            "clear" => publisher.clear(),
            other => panic!("Unknown publication {other}"),
        };
        assert_eq!(json!(result), operation["result"]);
        let limit = Instant::now() + Duration::from_secs(2);
        while events.borrow().len() < before + 2 {
            assert!(
                Instant::now() < limit,
                "Native private-bus receipt timed out"
            );
            drain();
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            json!(&events.borrow()[before..]),
            operation["events"],
            "{}",
            operation["method"]
        );
    }
    drop(subscription);
    observer.close_sync(gio::Cancellable::NONE).unwrap();
    connection.close_sync(gio::Cancellable::NONE).unwrap();
    assert_eq!(
        json!(publisher.publish(&OverlayState {
            phase: "recording".into(),
            preview: "Never delivered after close".into(),
            ..OverlayState::default()
        })),
        fixture["closed_connection_result"]
    );
    println!(
        "NATIVE_COMPLETE {} independent projections and {} actual private-bus signals",
        fixture["cases"].as_array().unwrap().len(),
        events.borrow().len()
    );
}
