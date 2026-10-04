use mluva_providers::local_assets::{MODEL_CATALOG, QWEN_RUNTIME};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

fn setup(root: &Path, spec: &Value) {
    let data = root.join("xdg-data/mluva");
    fs::create_dir_all(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    let model = MODEL_CATALOG
        .iter()
        .find(|model| model.id == "qwen3-1.7b")
        .unwrap();
    let path = model.path(&data);
    fs::create_dir_all(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    for item in &model.files {
        fs::File::create(path.join(&item.name))
            .unwrap()
            .set_len(item.size)
            .unwrap();
    }
    if spec["missing_model"] != true {
        fs::write(path.join(".ready"), &model.revision).unwrap();
    }
    let device = spec["device"].as_str().unwrap_or("cpu");
    let runtime = data.join("qwen-runtime");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let directory = runtime.join(device);
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = QWEN_RUNTIME.binary(&data, device);
    fs::create_dir(executable.parent().unwrap()).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_qwen-fixture-peer"), &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    if spec["invalid_binary"] == true {
        fs::write(&executable, b"not an executable").unwrap();
    }
    if spec["missing_runtime"] != true {
        fs::write(
            directory.join(".ready"),
            &QWEN_RUNTIME.assets[device].sha256,
        )
        .unwrap();
    }
    let mut config = spec.clone();
    config["root"] = json!(root);
    fs::write(
        executable.with_file_name("fixture.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    fs::create_dir(root.join("tmp")).unwrap();
    fs::set_permissions(root.join("tmp"), fs::Permissions::from_mode(0o700)).unwrap();
    if let Some(hex) = spec["invalid_wave"].as_str() {
        fs::write(root.join("audio.wav"), unhex(hex)).unwrap();
    } else if spec["missing_audio"] != true {
        let channels = spec["channels"].as_u64().unwrap_or(1) as u16;
        let width = spec["width"].as_u64().unwrap_or(2) as u16;
        let rate = spec["rate"].as_u64().unwrap_or(16000) as u32;
        let frames =
            vec![
                spec["byte"].as_u64().unwrap_or(88) as u8;
                spec["frames"].as_u64().unwrap_or(4) as usize * channels as usize * width as usize
            ];
        let mut wave = mluva_audio::wav::pcm16_wav(&frames).unwrap();
        wave[22..24].copy_from_slice(&channels.to_le_bytes());
        wave[24..28].copy_from_slice(&rate.to_le_bytes());
        wave[28..32].copy_from_slice(&(rate * channels as u32 * width as u32).to_le_bytes());
        wave[32..34].copy_from_slice(&(channels * width).to_le_bytes());
        wave[34..36].copy_from_slice(&(width * 8).to_le_bytes());
        fs::write(root.join("audio.wav"), wave).unwrap();
    }
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn abrupt_parent_failure_reaps_the_runtime_and_revokes_its_key() {
    for (device, phase) in [("cpu", "ready"), ("cuda", "ready"), ("cuda", "probe")] {
        let directory = tempfile::Builder::new()
            .prefix("qwen-parent-crash-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        setup(
            directory.path(),
            &json!({"device":device,"probe_delay_ms":if phase == "probe" { 10_000 } else { 0 },"responses":[{"events":[
                {"choices":[{"delta":{"content":"language English<asr_text>hello"}}]}
            ]}]}),
        );
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_qwen-fixture-peer"))
            .arg("--observe-parent-crash")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("QWEN_FIXTURE_ROOT", directory.path())
            .env("QWEN_CRASH_DEVICE", device)
            .env("QWEN_CRASH_PHASE", phase)
            .env("TMPDIR", directory.path().join("tmp"))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{device}/{phase}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            observed,
            json!({"phase":phase,"worker_killed_after_parent_crash":true,"key_revoked":true,
                "anonymous_key_sealed":phase == "ready","normal_request_completed":phase == "ready"})
        );
    }
}

fn driver_command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qwen-fixture-peer"));
    command
        .env_clear()
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg-data"))
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("TMPDIR", root.join("tmp"))
        .env("QWEN_FIXTURE_ROOT", root);
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
    ] {
        command.env(name, "http://127.0.0.1:9");
    }
    for name in ["OPENAI_API_KEY", "BASH_ENV"] {
        command.env(name, "fixture-env-canary");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn invoke_driver(
    root: &Path,
    spec: &Value,
    deadline: Duration,
    observe_memory: bool,
) -> (Value, Duration, u64) {
    let mut driver = driver_command(root).spawn().unwrap();
    let mut stdin = driver.stdin.take().unwrap();
    stdin
        .write_all(&serde_json::to_vec(spec).unwrap())
        .await
        .unwrap();
    drop(stdin);
    let before = std::time::Instant::now();
    let mut peak = 0;
    let mut pid = None;
    let waiting = driver.wait_with_output();
    tokio::pin!(waiting);
    let output = tokio::time::timeout(deadline, async {
        loop {
            tokio::select! {
                result = &mut waiting => break result.unwrap(),
                _ = tokio::time::sleep(Duration::from_millis(2)), if observe_memory => {
                    if pid.is_none() {
                        pid = fs::read_to_string(root.join("trace.jsonl")).unwrap_or_default()
                            .lines().filter_map(|line|serde_json::from_str::<Value>(line).ok())
                            .find(|row|row["kind"]=="start")
                            .and_then(|row|row["pid"].as_u64());
                    }
                    if let Some(pid) = pid {
                        let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
                        if let Some(rss) = status.lines().find_map(|line|line.strip_prefix("VmRSS:")
                            .and_then(|value|value.split_whitespace().next())
                            .and_then(|value|value.parse::<u64>().ok())) {
                            peak = peak.max(rss * 1024);
                        }
                    }
                }
            }
        }
    })
    .await
    .expect("bounded actual Qwen client");
    assert!(output.status.success(), "Qwen fixture process");
    assert!(
        output.stderr.is_empty(),
        "silent Qwen fixture process: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        serde_json::from_slice(&output.stdout).unwrap(),
        before.elapsed(),
        peak,
    )
}

fn normalize_guard_result(mut result: Value) -> Value {
    let trace = result["trace"].as_array_mut().unwrap();
    let mut statuses = vec![];
    trace.retain(|row| {
        if row["kind"] == "health" {
            assert_eq!(row["authenticated"], false);
            statuses.push(row["status"].as_u64().unwrap());
            false
        } else {
            row["kind"] != "fragment"
        }
    });
    assert!(!statuses.is_empty());
    statuses.sort();
    statuses.dedup();
    result["health_statuses"] = json!(statuses);
    result["health_requests_unauthenticated"] = json!(true);
    result
}

async fn actual_guard_case(case: &Value, memory: bool) {
    let directory = tempfile::Builder::new()
        .prefix("actual-qwen-guard-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    setup(root, &case["spec"]);
    let (actual, elapsed, peak) =
        invoke_driver(root, &case["spec"], Duration::from_secs(205), memory).await;
    let name = case["name"].as_str().unwrap();
    assert_eq!(
        normalize_guard_result(actual),
        case["result"],
        "{name}: actual released failure, protocol and resource cleanup"
    );
    let seconds = elapsed.as_secs_f64();
    if memory {
        assert!(peak > 5_000_000_000, "real owned-worker RSS exceeds 5 GB");
        assert!(seconds < 30.0);
    } else if name.ends_with("startup") {
        assert!((87.0..=102.0).contains(&seconds));
    } else {
        assert!((177.0..=195.0).contains(&seconds));
    }
    assert_eq!(
        fs::read_dir(root.join("tmp")).unwrap().count(),
        0,
        "no owned temporary key/alias/directory remains at request return"
    );
    eprintln!(
        "{}",
        json!({"name":name,"elapsed_seconds":seconds,"peak_worker_rss_bytes":peak,
            "released_error_and_protocol_match":true,"owned_resources_gone_at_return":true})
    );
}

#[tokio::test]
#[ignore = "waits for real 90/180-second Qwen deadlines; no clock or timeout override"]
async fn actual_qwen_deadlines_match_release_and_reap_owned_resources() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-qwen-faults.json")).unwrap();
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            !case["name"]
                .as_str()
                .unwrap()
                .ends_with("resident-overflow")
        })
        .collect();
    assert_eq!(cases.len(), 6);
    futures_util::future::join_all(cases.into_iter().map(|case| actual_guard_case(case, false)))
        .await;
}

#[tokio::test]
#[ignore = "allocates and observes over 5 GB real RSS in one isolated Qwen peer at a time"]
async fn actual_qwen_resident_overflow_matches_release_and_reaps_owned_resources() {
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
    assert!(available >= 12_000_000_000);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-qwen-faults.json")).unwrap();
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["name"]
                .as_str()
                .unwrap()
                .ends_with("resident-overflow")
        })
        .collect();
    assert_eq!(cases.len(), 2);
    for case in cases {
        actual_guard_case(case, true).await;
    }
}

#[tokio::test]
#[ignore = "observes incomplete response lines for 195 real seconds before public cancellation"]
async fn actual_qwen_incomplete_stream_cancel_and_fresh_capture_match_release() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-qwen-interruption.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    futures_util::future::join_all(cases.iter().map(|case| async move {
        let directory = tempfile::Builder::new()
            .prefix("actual-qwen-interruption-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = directory.path();
        setup(root, &case["spec"]);
        let (mut actual, elapsed, _) =
            invoke_driver(root, &case["spec"], Duration::from_secs(215), false).await;
        let observations = actual
            .as_object_mut()
            .unwrap()
            .shift_remove("stream_observations")
            .unwrap();
        let observation = &observations.as_array().unwrap()[0];
        assert_eq!(observations.as_array().unwrap().len(), 1);
        assert!(
            (195.0..=205.0).contains(&observation["seconds_from_first_fragment"].as_f64().unwrap())
        );
        assert!((190..=205).contains(&observation["fragments"].as_u64().unwrap()));
        assert!(observation["cancellation_seconds"].as_f64().unwrap() < 5.0);
        assert_eq!(observation["temporary_entries_after_cancel"], 0);
        assert!((195.0..=210.0).contains(&elapsed.as_secs_f64()));
        assert_eq!(
            normalize_guard_result(actual),
            case["result"],
            "{}: actual incomplete read, cancellation/no output, reaping and new-client recovery",
            case["name"]
        );
        assert_eq!(fs::read_dir(root.join("tmp")).unwrap().count(), 0);
        eprintln!(
            "{}",
            json!({"name":case["name"],"elapsed_seconds":elapsed.as_secs_f64(),
            "stream_observation":observation,"released_protocol_and_recovery_match":true})
        );
    }))
    .await;
}

#[tokio::test]
async fn native_qwen_client_processes_and_loopback_requests_match_release() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/released-qwen.json")).unwrap();
    let filter = std::env::var("QWEN_CASE_FILTER").ok();
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            filter
                .as_ref()
                .is_none_or(|name| case["name"] == name.as_str())
        })
        .collect();
    assert!(!cases.is_empty(), "selected independent Qwen cases exist");
    for case in cases {
        let directory = tempfile::Builder::new()
            .prefix("native-qwen-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = directory.path();
        setup(root, &case["spec"]);
        let (actual, _, _) =
            invoke_driver(root, &case["spec"], Duration::from_secs(20), false).await;
        let name = case["name"].as_str().unwrap();
        if actual != case["result"] {
            let report = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tmp/qwen-mismatch.json");
            fs::write(
                report,
                serde_json::to_vec_pretty(
                    &json!({"case":name,"actual":actual,"expected":case["result"]}),
                )
                .unwrap(),
            )
            .unwrap();
            panic!(
                "released Qwen observation differs: {name}; bounded report in tmp/qwen-mismatch.json"
            );
        }
    }
}

mod inference;
