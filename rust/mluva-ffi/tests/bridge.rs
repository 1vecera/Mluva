//! The bridge against real stores in a temporary directory; no provider, Herdr or device.
use mluva_ffi::Bridge;
use serde_json::{Value, json};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

struct Fixture {
    _root: tempfile::TempDir,
    bridge: Bridge,
    events: mpsc::Receiver<Value>,
}

impl Fixture {
    fn open() -> Self {
        let root = tempfile::tempdir().unwrap();
        let (sender, events) = mpsc::channel();
        let sender = std::sync::Mutex::new(sender);
        let bridge = Bridge::new(Arc::new(move |event| {
            let _ = sender.lock().unwrap().send(event);
        }));
        let base = root.path();
        bridge
            .call(
                "open",
                &json!({"config_dir": base.join("config"), "data_dir": base.join("data"),
                    "herdr_config_home": base.join("xdg"), "claude_config_dir": base.join("claude")}),
            )
            .unwrap();
        Self {
            _root: root,
            bridge,
            events,
        }
    }

    fn call(&self, method: &str, params: Value) -> Value {
        self.bridge
            .call(method, &params)
            .unwrap_or_else(|error| panic!("{method}: {error}"))
    }

    fn task(&self, method: &str, params: Value) -> Result<Value, String> {
        let task = self.call(method, params)["task"].clone();
        loop {
            let event = self.events.recv_timeout(Duration::from_secs(10)).unwrap();
            if event["type"] == "task" && event["task"] == task {
                return match event.get("ok") {
                    Some(ok) => Ok(ok.clone()),
                    None => Err(event["error"].as_str().unwrap().to_owned()),
                };
            }
        }
    }
}

#[test]
fn stores_are_owner_only_and_settings_round_trip_in_the_linux_format() {
    let fixture = Fixture::open();
    let loaded = fixture.call("config.load", json!({}));
    assert_eq!(loaded["error"], Value::Null);
    let mut config = loaded["config"].clone();
    assert_eq!(config["transcription_provider"], "elevenlabs");
    config["language_code"] = json!("ces");
    fixture.call("config.save", json!({"config": config}));
    assert_eq!(
        fixture.call("config.load", json!({}))["config"]["language_code"],
        "ces"
    );
    use std::os::unix::fs::PermissionsExt;
    let root = fixture._root.path();
    for directory in ["config", "data"] {
        let mode = std::fs::metadata(root.join(directory))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "{directory}");
    }
    assert!(root.join("data/history.sqlite3").exists());
    assert!(root.join("config/config.json").exists());
}

#[test]
fn unknown_methods_and_bad_parameters_are_errors() {
    let fixture = Fixture::open();
    assert!(fixture.bridge.call("nope", &json!({})).is_err());
    assert!(fixture.bridge.call("history.find", &json!({})).is_err());
    let closed = Bridge::new(Arc::new(|_| {}));
    let error = closed
        .call("history.recent", &json!({"limit": 1}))
        .unwrap_err();
    assert!(error.contains("not open"), "{error}");
}

#[test]
fn personalization_and_spoken_commands_shape_the_delivered_text_but_not_the_raw() {
    let fixture = Fixture::open();
    fixture.call(
        "personalization.save_replacement",
        json!({"spoken": "mluva", "written": "Mluva"}),
    );
    let snapshot = fixture.call("dictation.freeze", json!({"application": null}));
    let processed = fixture.call(
        "dictation.process",
        json!({"snapshot": snapshot, "raw": "open mluva new line thanks"}),
    );
    assert_eq!(processed, "open Mluva\nthanks");
    let entry = fixture.call(
        "history.add",
        json!({"input": {"raw_text": "open mluva new line thanks", "delivered_text": processed,
            "mode": "dictation", "language_code": "eng", "delivery_outcome": "copied"}}),
    );
    let id = entry["identifier"].as_str().unwrap();
    assert_eq!(entry["raw_text"], "open mluva new line thanks");
    assert_eq!(fixture.call("history.source", json!({"id": id})), processed);
    fixture.call("history.save_text", json!({"id": id, "text": "edited"}));
    assert_eq!(fixture.call("history.source", json!({"id": id})), "edited");
    let found = fixture.call("history.search", json!({"query": "MLUVA", "limit": 5}));
    assert_eq!(found.as_array().unwrap().len(), 1);
    let renamed = fixture.call("history.title", json!({"id": id, "title": "Plan"}));
    assert_eq!(renamed["title"], "Plan");
    assert_eq!(
        fixture.call("history.find", json!({"id": id}))["raw_text"],
        "open mluva new line thanks"
    );
    fixture.call("history.delete", json!({"id": id}));
    assert!(
        fixture
            .call("history.recent", json!({"limit": 10}))
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn appendices_follow_the_dictation_as_one_message() {
    let fixture = Fixture::open();
    assert_eq!(
        fixture.call(
            "dictation.append",
            json!({"text": "Fix this. ", "appendix": "[Screen annotation]\n"})
        ),
        "Fix this.\n\n[Screen annotation]"
    );
    assert_eq!(
        fixture.call("dictation.append", json!({"text": "Fix this."})),
        "Fix this."
    );
}

#[test]
fn keys_are_validated_without_being_echoed() {
    let fixture = Fixture::open();
    fixture.call("key.validate", json!({"key": "sk_synthetic"}));
    let error = fixture
        .bridge
        .call("key.validate", &json!({"key": "has space"}))
        .unwrap_err();
    assert!(!error.contains("has space"));
}

#[test]
fn discovery_without_herdr_or_sessions_is_empty_not_an_error() {
    let fixture = Fixture::open();
    let found = fixture.task("agents.discover", json!({})).unwrap();
    assert_eq!(found["herdr"], false);
    assert!(found["sessions"].is_array());
}

#[test]
fn a_draft_only_session_cannot_start_an_agent_send() {
    let fixture = Fixture::open();
    let draft_only = json!({"socket": "", "server_epoch": "", "pane_id": "", "guard": "",
        "name": "notes-1a", "kind": "claude", "project": "/synthetic", "status": "idle",
        "session_id": "s1", "pid": 4242});
    assert!(
        fixture
            .bridge
            .call(
                "route.begin",
                &json!({"recipient": draft_only, "now_ms": 1})
            )
            .is_err()
    );
    assert_eq!(fixture.call("route.state", json!({})), Value::Null);
}

#[test]
fn a_guarded_send_waits_out_the_cancel_window_and_editing_keeps_a_draft() {
    let fixture = Fixture::open();
    let guarded = json!({"socket": "/nonexistent/herdr.sock", "server_epoch": "e1",
        "pane_id": "w1:p1", "guard": "g1", "name": "cos", "kind": "claude",
        "project": "/synthetic", "status": "idle", "session_id": "s1", "pid": 0});
    let id = fixture.call(
        "route.begin",
        json!({"recipient": guarded, "now_ms": 1_000}),
    );
    assert_eq!(fixture.call("route.state", json!({}))["phase"], "recording");
    assert_eq!(
        fixture.call("route.release", json!({"now_ms": 2_000})),
        true
    );
    fixture.call(
        "route.recognized",
        json!({"id": id, "text": "ship it", "now_ms": 3_000}),
    );
    let state = fixture.call("route.tick", json!({"now_ms": 4_000}));
    assert_eq!(state["phase"], "grace");
    assert_eq!(state["deadline_ms"], 5_000);
    assert_eq!(
        fixture.call("route.disarm", json!({"reason": "edited"})),
        true
    );
    let state = fixture.call("route.tick", json!({"now_ms": 6_000}));
    assert_eq!(state["phase"], "draft");
    assert_eq!(state["reason"], "edited");
    assert_eq!(state["text"], "ship it");
}
