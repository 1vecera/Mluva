use mluva_providers::credentials::validate_speech_key;
use serde_json::{Value, json};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
mod support;

#[test]
fn key_validation_matches_released_unicode_and_character_bounds() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-credentials.json")).unwrap();
    for (index, row) in fixture["validation"].as_array().unwrap().iter().enumerate() {
        let observed = match validate_speech_key(row["key"].as_str().unwrap()) {
            Ok(()) => json!({"ok":null}),
            Err(error) => json!({"error":error.to_string()}),
        };
        assert!(observed == row["result"], "validation row {index}");
    }
}

async fn run_case(case: &Value) {
    let root = tempfile::Builder::new()
        .prefix("native-credentials-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path();
    std::fs::create_dir(path.join("bin")).unwrap();
    std::fs::create_dir(path.join("home")).unwrap();
    if case["spec"]["missing_tool"] != true {
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_credential-fixture-peer"),
            path.join("bin/secret-tool"),
        )
        .unwrap();
    }
    std::fs::write(
        path.join("keyring.json"),
        serde_json::to_vec(&case["spec"]["keyring"]).unwrap(),
    )
    .unwrap();
    let audio = path.join("synthetic.wav");
    std::fs::write(&audio, b"RIFFsynthetic").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let headers = Arc::new(Mutex::new(vec![]));
    let recorded = headers.clone();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move {
        loop {
            let socket = tokio::select! {biased;_=stopped.cancelled()=>break,value=listener.accept()=>value.unwrap().0};
            let mut socket = socket;
            let request = support::request(&mut socket).await;
            recorded
                .lock()
                .unwrap()
                .push(request.headers.get("xi-api-key").cloned());
            support::respond(
                &mut socket,
                &support::Response::ok(
                    br#"{"text":"Synthetic speech.","language_code":"eng"}"#.to_vec(),
                ),
            )
            .await;
        }
    });
    let mut spec = case["spec"].clone();
    spec["endpoint"] = json!(endpoint);
    spec["audio_path"] = json!(audio);
    let mut command = Command::new(env!("CARGO_BIN_EXE_credential-fixture-peer"));
    command
        .env_clear()
        .env("HOME", path.join("home"))
        .env("PATH", path.join("bin"))
        .env("LC_ALL", "C.UTF-8")
        .env("CREDENTIAL_FIXTURE_ROOT", path)
        .env("NO_PROXY", "127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(environment) = case["spec"]["environment"].as_object() {
        for (name, value) in environment {
            command.env(name, value.as_str().unwrap());
        }
    }
    if let Some(environment) = case["spec"]["environment_hex"].as_object() {
        for (name, value) in environment {
            command.env(
                name,
                std::ffi::OsString::from_vec(support::unhex(value.as_str().unwrap())),
            );
        }
    }
    let mut driver = command.spawn().unwrap();
    let mut stdin = driver.stdin.take().unwrap();
    stdin
        .write_all(&serde_json::to_vec(&spec).unwrap())
        .await
        .unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(75), driver.wait_with_output())
        .await
        .expect("bounded fixture driver")
        .unwrap();
    stop.cancel();
    server.await.unwrap();
    let name = case["name"].as_str().unwrap();
    assert!(output.status.success(), "credential driver {name}");
    assert!(output.stderr.is_empty(), "silent credential driver {name}");
    let observations: Value = serde_json::from_slice(&output.stdout).unwrap();
    let trace_path = path.join("trace.jsonl");
    let trace = if trace_path.exists() {
        std::fs::read_to_string(trace_path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    assert!(
        observations == case["result"]["observations"],
        "released credential decisions {name}"
    );
    assert!(
        json!(*headers.lock().unwrap()) == case["result"]["headers"],
        "selected key reaches the speech provider {name}"
    );
    assert!(
        json!(trace) == case["result"]["trace"],
        "released keyring arguments/input/cache calls {name}"
    );
    let pid_path = path.join("active.pid");
    if pid_path.exists() {
        let pid = std::fs::read_to_string(pid_path).unwrap();
        let process = std::path::PathBuf::from(format!("/proc/{pid}"));
        for _ in 0..200 {
            if !process.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!process.exists(), "keyring child reaped {name}");
    }
}

#[tokio::test]
async fn saved_precedence_cache_failures_and_stdin_match_real_reference_processes() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-credentials-wire.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        if case["name"] != "store-timeout" {
            run_case(case).await;
        }
    }
}

#[tokio::test]
async fn store_timeout_preserves_cached_key_and_reaps_the_child() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-credentials-wire.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "store-timeout")
        .unwrap();
    run_case(case).await;
}
