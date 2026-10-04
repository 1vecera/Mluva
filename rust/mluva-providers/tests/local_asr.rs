//! Independent released-client observations through real native child processes.
use mluva_providers::{local_assets::MODEL_CATALOG, onnx_runtime::ONNX_RUNTIME};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

fn setup(root: &Path, spec: &Value) {
    let data = root.join("xdg-data/mluva");
    fs::create_dir_all(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    let model = MODEL_CATALOG
        .iter()
        .find(|model| model.id == spec["model"].as_str().unwrap())
        .unwrap();
    let path = model.path(&data);
    fs::create_dir_all(&path).unwrap();
    for (index, item) in model.files.iter().enumerate() {
        if spec["absent_file"] == true && index == 0 {
            continue;
        }
        let target = path.join(&item.name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::File::create(target)
            .unwrap()
            .set_len(item.size - u64::from(spec["wrong_size"] == true && index == 0))
            .unwrap();
    }
    if spec["missing_model"] != true {
        fs::write(path.join(".ready"), &model.revision).unwrap();
    }
    fs::copy(
        env!("CARGO_BIN_EXE_local-asr-fixture-peer"),
        root.join("worker"),
    )
    .unwrap();
    fs::set_permissions(root.join("worker"), fs::Permissions::from_mode(0o700)).unwrap();
    if spec["invalid_binary"] == true {
        fs::write(root.join("worker"), b"not an executable").unwrap();
    }
    let mut config = spec.clone();
    config["root"] = json!(root);
    fs::write(
        root.join("fixture.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    if spec["device"] == "cuda" && spec["missing_runtime"] != true {
        let runtime = ONNX_RUNTIME.root(&data, "cuda");
        fs::create_dir_all(&runtime).unwrap();
        for item in ONNX_RUNTIME
            .cuda
            .iter()
            .flat_map(|archive| &archive.members)
        {
            let target = runtime.join(&item.path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::File::create(target)
                .unwrap()
                .set_len(item.size)
                .unwrap();
        }
        fs::write(
            runtime.join(".native-ready"),
            ONNX_RUNTIME.stamp("cuda").unwrap(),
        )
        .unwrap();
    }
    fs::write(
        root.join("audio.wav"),
        b"synthetic protocol input, not decoded by this peer",
    )
    .unwrap();
    fs::write(root.join("日本語-😊.wav"), b"synthetic protocol input").unwrap();
    std::os::unix::fs::symlink(root.join("audio.wav"), root.join("link.wav")).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    fs::create_dir(root.join("tmp")).unwrap();
    fs::set_permissions(root.join("tmp"), fs::Permissions::from_mode(0o700)).unwrap();
}

#[tokio::test]
async fn aborted_requests_restart_and_concurrent_cancel_waits_for_all_owned_resources() {
    for model in ["whisper-tiny", "parakeet-v3"] {
        for device in ["cpu", "cuda"] {
            let actual = run(&json!({"model":model,"device":device,"keep_alive":true,
                "responses":[{"hold_first_process":true,"json":{"text":"Recovered"}}],
                "calls":[{"action":"drop_request"},{},{"action":"cancel_pair"},{}]}))
            .await;
            assert_eq!(actual["results"][0], json!({"aborted":true,"quick":true}));
            assert_eq!(actual["states"][0], json!({"processes":1,"alive":[false]}));
            assert_eq!(
                actual["results"][1]["ok"]["text"], "Recovered",
                "a cancelled request cannot poison the next capture ({model}/{device})"
            );
            assert_eq!(
                actual["states"][1],
                json!({"processes":2,"alive":[false,true]})
            );
            let closed = json!({"processes":2,"alive":[false,false]});
            assert_eq!(
                actual["results"][2],
                json!({"ok":[closed.clone(),closed.clone()]}),
                "every cancel caller waits until its worker is reaped"
            );
            assert_eq!(
                actual["results"][3],
                json!({"error":"Local transcription cancelled."})
            );
            assert_eq!(actual["closed"], closed);
        }
    }
}
async fn run(spec: &Value) -> Value {
    run_with_deadline(spec, Duration::from_secs(20)).await
}
async fn run_with_deadline(spec: &Value, deadline: Duration) -> Value {
    let directory = tempfile::Builder::new()
        .prefix("native-local-asr-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    setup(root, spec);
    let mut command = Command::new(env!("CARGO_BIN_EXE_local-asr-fixture-peer"));
    command
        .env_clear()
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg-data"))
        .env("TMPDIR", root.join("tmp"))
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("SYSTEMROOT", "fixture-system-root")
        .env("LOCAL_ASR_FIXTURE_ROOT", root)
        .env("OPENAI_API_KEY", "fixture-account-canary")
        .env("BASH_ENV", "fixture-command-canary")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut driver = command.spawn().unwrap();
    let mut input = driver.stdin.take().unwrap();
    input
        .write_all(&serde_json::to_vec(spec).unwrap())
        .await
        .unwrap();
    drop(input);
    let output = tokio::time::timeout(deadline, driver.wait_with_output())
        .await
        .expect("bounded native local-ASR fixture client")
        .unwrap();
    assert!(
        output.status.success(),
        "fixture driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "silent native local-ASR client: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_dir(root.join("tmp")).unwrap().count(),
        0,
        "owned CUDA caches released before returning"
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
#[ignore = "explicitly allocates over 5 GB of real resident memory in one isolated worker at a time"]
async fn actual_resident_overflow_matches_released_guard_and_reaps_owned_workers() {
    let available = fs::read_to_string("/proc/meminfo")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap()
        * 1024;
    assert!(
        available >= 12_000_000_000,
        "explicit memory stress requires at least 12 GB available"
    );
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-asr-faults.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let cases = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["name"]
                .as_str()
                .unwrap()
                .ends_with("resident-overflow")
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let actual = run_with_deadline(&case["spec"], Duration::from_secs(30)).await;
        assert_eq!(
            actual, case["result"],
            "{}: actual resident overflow, exact released error and complete resource cleanup",
            case["name"]
        );
    }
}

#[tokio::test]
#[ignore = "waits for both actual 180-second worker deadlines; no production clock or timeout override"]
async fn actual_worker_deadlines_match_release_and_close_every_owned_resource() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-asr-faults.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let mut cases = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["name"].as_str().unwrap().ends_with("silent-deadline"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 2);
    // The release's silent-worker timeout is also the native complete-reply deadline.
    // Its selector/readline split can hang after a partial byte; retain the timeout
    // contract while deliberately refusing to reproduce that indefinite-read bug.
    let mut partial = cases
        .iter()
        .find(|case| case["name"] == "cpu-silent-deadline")
        .unwrap()
        .clone();
    partial["name"] = json!("cpu-partial-reply-deadline");
    partial["spec"]["responses"] =
        json!([{"raw":"{\"text\":","newline":false,"hold_after_output":true}]);
    cases.push(partial);
    futures_util::future::join_all(cases.into_iter().map(|case| async move {
        let actual = run_with_deadline(&case["spec"], Duration::from_secs(195)).await;
        assert_eq!(
            actual, case["result"],
            "{}: actual elapsed deadline, exact released error and complete resource cleanup",
            case["name"]
        );
    }))
    .await;
}

#[tokio::test]
async fn native_local_factory_and_onnx_client_match_released_process_contracts() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-asr.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let filter = std::env::var("LOCAL_ASR_CASE_FILTER").ok();
    let cases = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            filter
                .as_ref()
                .is_none_or(|name| case["name"] == name.as_str())
        })
        .collect::<Vec<_>>();
    assert!(!cases.is_empty(), "independent local-ASR cases selected");
    let mut differences = Vec::new();
    for case in cases {
        let actual = run(&case["spec"]).await;
        if actual != case["result"] {
            differences
                .push(json!({"case":case["name"],"actual":actual,"expected":case["result"]}));
        }
    }
    if !differences.is_empty() {
        let report =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/local-asr-mismatch.json");
        fs::write(report, serde_json::to_vec_pretty(&differences).unwrap()).unwrap();
        panic!(
            "released local-ASR observations differ: {}; bounded report in tmp/local-asr-mismatch.json",
            differences
                .iter()
                .map(|value| value["case"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
