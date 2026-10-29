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
                    "herdr_config_home": base.join("xdg"), "claude_config_dir": base.join("claude"),
                    "codex": false, "apps": false}),
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

/// A Herdr server with the guarded prompt API and one Claude Code pane it vouches for. It
/// records every prompt it is asked to write and accepts each one.
struct Herdr {
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Herdr {
    fn serve(fixture: &Fixture) -> Self {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::{fs::PermissionsExt, net::UnixListener},
        };
        let directory = fixture._root.path().join("xdg/herdr");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("herdr.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let prompts = Arc::new(std::sync::Mutex::new(vec![]));
        let written = prompts.clone();
        let agent = json!({"pane_id": "w1:p1", "agent": "claude", "name": "lead",
            "agent_status": "idle",
            "agent_session": {"source": "herdr:claude", "agent": "claude", "kind": "id", "value": "s1"}});
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let request: Value = serde_json::from_str(&line).unwrap();
                let params = &request["params"];
                let result = match request["method"].as_str().unwrap() {
                    "agent.guarded_list" => json!({"server_epoch": "epoch",
                        "agents": [{"guard": "a".repeat(64), "agent": agent}]}),
                    "agent.list" => json!({"agents": [agent]}),
                    "agent.prompt_guarded" => {
                        written
                            .lock()
                            .unwrap()
                            .push(params["text"].as_str().unwrap().to_owned());
                        json!({"server_epoch": "epoch", "operation_id": params["operation_id"],
                            "state": "accepted"})
                    }
                    method => panic!("unexpected Herdr method {method}"),
                };
                let reply = json!({"id": request["id"], "result": result});
                let _ = (&stream).write_all(format!("{reply}\n").as_bytes());
            }
        });
        Self { prompts }
    }
}

impl Fixture {
    /// A History row for an agent dictation, as the shell writes it before routing.
    fn row(&self, text: &str) -> String {
        let entry = self.call(
            "history.add",
            json!({"input": {"raw_text": text, "delivered_text": text, "language_code": "en",
                "delivery_outcome": "ready"}}),
        );
        entry["identifier"].as_str().unwrap().to_owned()
    }

    fn outcome(&self, row: &str) -> Value {
        self.call("history.find", json!({"id": row}))["delivery_outcome"].clone()
    }

    /// Record `text` for the first discovered session and leave it in its cancel window.
    fn dictate(&self, text: &str, row: &str) -> String {
        let found = self.task("agents.discover", json!({})).unwrap();
        let id = self.call(
            "route.begin",
            json!({"target": found["sessions"][0]["target"], "now_ms": 1_000}),
        );
        assert_eq!(self.call("route.release", json!({"now_ms": 2_000})), true);
        self.call(
            "route.recognized",
            json!({"id": id, "text": text, "history_id": row, "now_ms": 3_000}),
        );
        id.as_str().unwrap().to_owned()
    }

    fn route_event(&self, id: &str) -> Value {
        loop {
            let event = self.events.recv_timeout(Duration::from_secs(10)).unwrap();
            if event["type"] == "route" && event["id"] == id {
                return event;
            }
        }
    }
}

#[test]
fn discovery_without_herdr_or_sessions_is_empty_not_an_error() {
    let fixture = Fixture::open();
    let found = fixture.task("agents.discover", json!({})).unwrap();
    assert_eq!(found["herdr"], false);
    assert_eq!(found["sessions"], json!([]));
    assert!(
        found["summary"]
            .as_str()
            .unwrap()
            .starts_with("No sessions found")
    );
}

#[test]
fn only_a_session_in_the_latest_discovery_can_start_an_agent_send() {
    let fixture = Fixture::open();
    let remembered = json!({"socket": "/nonexistent/herdr.sock", "server_epoch": "epoch",
        "pane_id": "w1:p1", "guard": "a".repeat(64), "name": "lead", "kind": "claude",
        "project": "/synthetic", "status": "idle", "session_id": "s1", "pid": 0});
    let refused = fixture
        .bridge
        .call("route.begin", &json!({"target": remembered, "now_ms": 1}));
    assert!(refused.unwrap_err().contains("no longer running"));
    assert_eq!(fixture.call("route.state", json!({})), Value::Null);
}

#[test]
fn a_guarded_herdr_pane_is_described_by_the_shared_target_model() {
    let fixture = Fixture::open();
    let _herdr = Herdr::serve(&fixture);
    let found = fixture.task("agents.discover", json!({})).unwrap();
    assert_eq!(found["herdr"], true);
    assert_eq!(found["guarded"], true);
    let session = &found["sessions"][0];
    // Without Claude Code's own title, Herdr's label names it.
    assert_eq!(session["display_name"], "lead");
    assert_eq!(session["kind"], "Claude Code");
    assert_eq!(session["place"], "Herdr w1:p1");
    assert_eq!(session["delivery"], "send");
    assert_eq!(session["verb"], "Send");
    assert_eq!(session["delivery_hint"], Value::Null);
    assert_eq!(session["can_receive"], true);
    assert_eq!(session["target"]["guard"], "a".repeat(64));
}

#[test]
fn editing_in_the_cancel_window_keeps_a_draft_and_history_says_so() {
    let fixture = Fixture::open();
    let herdr = Herdr::serve(&fixture);
    let row = fixture.row("ship it");
    fixture.dictate("ship it", &row);
    let state = fixture.call("route.tick", json!({"now_ms": 4_000}));
    assert_eq!(state["phase"], "grace");
    assert_eq!(state["deadline_ms"], 5_000);
    assert_eq!(state["delivery"], "send");
    assert_eq!(state["display_name"], "lead");
    assert_eq!(
        fixture.call("route.disarm", json!({"reason": "edited"})),
        true
    );
    let state = fixture.call("route.tick", json!({"now_ms": 6_000}));
    assert_eq!(state["phase"], "draft");
    assert_eq!(state["reason"], "edited");
    assert_eq!(state["text"], "ship it");
    assert_eq!(fixture.outcome(&row), "draft-kept");
    assert!(herdr.prompts.lock().unwrap().is_empty());
}

#[test]
fn after_the_window_the_text_is_delivered_once_and_history_says_sent() {
    let fixture = Fixture::open();
    let herdr = Herdr::serve(&fixture);
    let row = fixture.row("ship it");
    let id = fixture.dictate("ship it", &row);
    assert_eq!(
        fixture.call("route.tick", json!({"now_ms": 5_000}))["phase"],
        "submitting"
    );
    assert_eq!(fixture.route_event(&id)["outcome"], "accepted");
    assert_eq!(fixture.call("route.state", json!({}))["phase"], "delivered");
    assert_eq!(fixture.outcome(&row), "sent");
    // Later ticks never write it again.
    fixture.call("route.tick", json!({"now_ms": 9_000}));
    assert_eq!(*herdr.prompts.lock().unwrap(), ["ship it"]);
}

#[test]
fn a_second_press_in_the_window_sends_at_once() {
    let fixture = Fixture::open();
    let herdr = Herdr::serve(&fixture);
    let row = fixture.row("now please");
    let id = fixture.dictate("now please", &row);
    assert_eq!(
        fixture.call("route.send_now", json!({"now_ms": 3_100}))["phase"],
        "submitting"
    );
    assert_eq!(fixture.route_event(&id)["outcome"], "accepted");
    assert_eq!(*herdr.prompts.lock().unwrap(), ["now please"]);
    // Outside a window nothing is consumed.
    assert_eq!(
        fixture.call("route.send_now", json!({"now_ms": 3_200}))["phase"],
        "delivered"
    );
    assert_eq!(herdr.prompts.lock().unwrap().len(), 1);
}

#[test]
fn a_marked_recording_becomes_a_bundle_the_shell_draws_and_its_conversation_owns() {
    let fixture = Fixture::open();
    assert_eq!(
        fixture.call("annotate.settings", json!({})),
        json!({"enabled": true, "modifier": "shift"})
    );
    let frames = fixture._root.path().join("frames");
    std::fs::create_dir(&frames).unwrap();
    let frame = |t: u64, reason: &str| {
        json!({"t": t, "screen": "Built-in", "reason": reason,
            "path": frames.join(format!("{t}.png")), "width": 1440, "height": 900})
    };
    let request = json!({
        "owner": "capture-1",
        "capture": {
            "duration_ms": 6_000,
            "screens": [{"name": "Built-in", "x": 0, "y": 0, "width": 1440, "height": 900, "scale": 2.0}],
            "strokes": [{"t0": 2_000, "t1": 2_600, "screen": "Built-in", "points": [
                {"x": 400.0, "y": 300.0, "t": 2_000}, {"x": 520.0, "y": 300.0, "t": 2_300},
                {"x": 640.0, "y": 304.0, "t": 2_600}]}],
            "frames": [frame(0, "start"), frame(2_600, "mark")],
            "events": [],
        },
        "lumas": [vec![0.2_f32; 64 * 36], vec![0.8_f32; 64 * 36]],
        "text": "This button should be red.",
        "words": [],
    });
    // Nothing is written before the shell's images exist.
    let planned = fixture.call("annotate.plan", request.clone());
    let keyframes = planned["keyframes"].as_array().unwrap();
    assert!(!keyframes.is_empty());
    let identifier = planned["identifier"].as_str().unwrap().to_owned();
    assert!(
        fixture
            .bridge
            .call("annotate.finish", &json!({"identifier": identifier}))
            .is_err()
    );
    assert!(
        fixture
            .bridge
            .call("annotate.finish", &json!({"identifier": identifier}))
            .is_err()
    );

    let planned = fixture.call("annotate.plan", request);
    let identifier = planned["identifier"].as_str().unwrap().to_owned();
    for keyframe in planned["keyframes"].as_array().unwrap() {
        assert_eq!(keyframe["crop"].as_array().unwrap().len(), 4);
        std::fs::write(keyframe["file"].as_str().unwrap(), b"\x89PNG\r\n\x1a\n").unwrap();
    }
    std::fs::write(
        planned["contact_sheet"].as_str().unwrap(),
        b"\x89PNG\r\n\x1a\n",
    )
    .unwrap();
    let finished = fixture.call("annotate.finish", json!({"identifier": identifier}));
    let summary = finished["summary"].as_str().unwrap();
    assert!(summary.contains("01.png"), "{summary}");

    let entry = fixture.call(
        "history.add",
        json!({"input": {"raw_text": "This button should be red.", "delivered_text": "This button should be red.",
            "language_code": "en", "delivery_outcome": "copied"}}),
    );
    let conversation = entry["identifier"].as_str().unwrap();
    fixture.call(
        "annotate.bind",
        json!({"owner": "capture-1", "conversation": conversation}),
    );
    let bundles = fixture.call("annotate.recent", json!({"id": conversation}));
    assert_eq!(bundles.as_array().unwrap().len(), 1);
    assert_eq!(bundles[0]["identifier"], identifier);
}
