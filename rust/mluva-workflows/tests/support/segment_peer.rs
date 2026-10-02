//! Public native scheduler driver through real, independently gated Codex children.
use mluva_core::text;
use mluva_providers::codex::{CodexAppServerClient, CodexOptions};
use mluva_workflows::segment_cleanup::{
    CodexSegmentCleanupAttempt, SegmentAttemptFactory, SegmentCleanupConfiguration,
    SegmentCleanupSession, SegmentCleanupTerminalSnapshot,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

fn lines(path: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(path).unwrap_or_default();
    let Some(end) = raw.rfind('\n') else {
        return vec![];
    };
    raw[..end]
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn wire(root: &Path) -> Vec<String> {
    lines(&root.join("evidence/requests.jsonl"))
        .iter()
        .filter(|event| event["message"]["method"] == "turn/start")
        .map(|event| {
            event["message"]["params"]["input"][0]["text"]
                .as_str()
                .unwrap()
                .into()
        })
        .collect()
}
fn observe(session: &SegmentCleanupSession) -> Value {
    json!({"projection":session.projection(), "has_stable_segments":session.has_stable_segments()})
}
fn snapshot(value: SegmentCleanupTerminalSnapshot) -> Value {
    let mut result = serde_json::to_value(&value).unwrap();
    result.as_object_mut().unwrap().remove("stop_drain_seconds");
    result["raw_text"] = json!(value.raw_text());
    result["selected_text"] = json!(value.selected_text());
    result["successful_segments"] = json!(value.successful_segments());
    result["failed_segments"] = json!(value.failed_segments());
    result["enhancement_outcome"] = json!(value.enhancement_outcome());
    result
}
async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
fn blocked(root: &Path, spec: &Value, boundary: &str) {
    if spec["blocked"] == boundary {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !root.join(format!("{boundary}.release")).exists() {
            assert!(
                Instant::now() < deadline,
                "Unreleased controlled construction boundary"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
fn release(root: &Path, spec: &Value, key: &str) {
    let path = if matches!(key, "preparation" | "factory") {
        root.join(format!("{key}.release"))
    } else {
        PathBuf::from(spec["segment_controls"][key]["gate"].as_str().unwrap())
    };
    fs::write(path, []).unwrap();
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    let root = PathBuf::from(&arguments[1]);
    let row: Value = serde_json::from_slice(&fs::read(root.join("case.json")).unwrap()).unwrap();
    let spec: Value =
        serde_json::from_slice(&fs::read(root.join("codex/fixture.json")).unwrap()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let parent = Arc::new(CodexAppServerClient::new(CodexOptions {
        command: vec![
            arguments[2].clone().into(),
            "serve".into(),
            root.join("codex/fixture.json").into_os_string(),
        ],
        request_timeout: Duration::from_secs(2),
        turn_timeout: Duration::from_secs(2),
    }));
    let preparing_root = root.clone();
    let preparing = row.clone();
    let factory_root = root.clone();
    let factory_row = row.clone();
    let factory_calls = calls.clone();
    let factory: SegmentAttemptFactory = Arc::new(move || {
        factory_calls.fetch_add(1, Ordering::AcqRel);
        blocked(&factory_root, &factory_row, "factory");
        if factory_row["factory_error"] == true {
            return Err("Controlled factory failure".into());
        }
        Ok(Arc::new(CodexSegmentCleanupAttempt {
            client: parent.spawn(),
            cwd: "/unused".into(),
            model_identifier: "fixture-model".into(),
            instructions: mluva_core::prompt_catalog::DEFAULTS
                .prompts
                .iter()
                .find(|prompt| prompt.identifier == "cleanup")
                .unwrap()
                .default
                .clone(),
        }))
    });
    let config = &row["configuration"];
    let session = SegmentCleanupSession::new(
        "capture-session".into(),
        "codex-app-server".into(),
        "fixture-model".into(),
        Arc::new(move |raw| {
            blocked(&preparing_root, &preparing, "preparation");
            match preparing["preparation"].as_str().unwrap() {
                "error" => Err("Controlled preparation failure".into()),
                "empty" => Ok(String::new()),
                "oversize" => Ok("x".repeat(9)),
                "uppercase" => Ok(text::upper(raw)),
                _ => Ok(raw.into()),
            }
        }),
        row["vocabulary"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_str().unwrap().into())
            .collect(),
        factory,
        SegmentCleanupConfiguration {
            concurrency_limit: config["concurrency_limit"].as_u64().unwrap() as usize,
            pending_capacity: config["pending_capacity"].as_u64().unwrap() as usize,
            attempt_timeout: Duration::from_secs_f64(
                config["attempt_timeout_seconds"].as_f64().unwrap(),
            ),
            stop_drain_timeout: Duration::from_secs_f64(
                config["stop_drain_timeout_seconds"].as_f64().unwrap(),
            ),
            segment_character_limit: config["segment_character_limit"].as_u64().unwrap() as usize,
            response_character_limit: config["response_character_limit"].as_u64().unwrap() as usize,
        },
    )
    .unwrap();
    let mut accepted = vec![];
    let mut observations = vec![];
    let mut terminal = None;
    for action in row["actions"].as_array().unwrap() {
        if let Some(value) = action.get("accept") {
            accepted.push(session.accept_stable_segment(
                value["identifier"].as_str().unwrap(),
                value["raw"].as_str().unwrap(),
            ));
        }
        if let Some(count) = action["wait_requests"].as_u64() {
            until(|| wire(&root).len() >= count as usize).await;
        }
        if let Some(key) = action["release"].as_str() {
            release(&root, &spec, key);
        }
        if let Some(states) = action.get("wait_states") {
            until(|| {
                json!(
                    session
                        .projection()
                        .iter()
                        .map(|segment| serde_json::to_value(segment.state).unwrap())
                        .collect::<Vec<_>>()
                ) == *states
            })
            .await;
        }
        if action["cancel"] == true {
            session.cancel();
        }
        if let Some(key) = action["drain_release"].as_str() {
            let (releasing_root, releasing_spec, key) =
                (root.clone(), spec.clone(), key.to_owned());
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                release(&releasing_root, &releasing_spec, &key);
            });
            terminal = Some(snapshot(session.stop_and_drain().await));
        }
        if action["observe"] == true {
            observations.push(observe(&session));
        }
    }
    let started = Instant::now();
    let terminal = match terminal {
        Some(value) => value,
        None => snapshot(session.stop_and_drain().await),
    };
    if row["name"] == "stop-fixed-bound" {
        assert!(
            started.elapsed() >= Duration::from_millis(90)
                && started.elapsed() < Duration::from_millis(400)
        );
    }
    let finished = observe(&session);
    let after_stop = session.accept_stable_segment("late", "must never start");
    let second = snapshot(session.stop_and_drain().await);
    tokio::time::timeout(Duration::from_secs(8), session.cancel_and_wait())
        .await
        .unwrap();
    let processes = lines(&root.join("evidence/process.jsonl"));
    for process in &processes {
        assert!(
            !Path::new(&format!("/proc/{}", process["pid"])).exists(),
            "Model child survived cleanup acknowledgement"
        );
        assert!(
            !Path::new(process["cwd"].as_str().unwrap()).exists(),
            "Private model workspace survived cleanup acknowledgement"
        );
        assert_eq!(process["mode"], 0o700);
        assert_eq!(process["instructions"], json!([null, null]));
    }
    let mut prompts = wire(&root);
    prompts.sort();
    println!(
        "{}",
        json!({"accepted":accepted,"observations":observations,"terminal":terminal,"finished":finished,"after_stop":after_stop,"second":second,
        "closed":observe(&session),"prompts":prompts,"factory_calls":calls.load(Ordering::Acquire),"processes":processes.len()})
    );
}
