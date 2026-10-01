//! Independent released delivery observations through actual private helpers and Unix sockets.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn expanded(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(value) => json!(value.replace("$ROOT", root.to_str().unwrap())),
        Value::Array(values) => json!(
            values
                .iter()
                .map(|value| expanded(value, root))
                .collect::<Vec<_>>()
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), expanded(value, root)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn run(spec: &Value) -> Value {
    let directory = tempfile::Builder::new()
        .prefix("dl-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    let spec = expanded(spec, root);
    fs::create_dir(root.join("bin")).unwrap();
    fs::create_dir(root.join("r")).unwrap();
    fs::set_permissions(root.join("r"), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("fixture.json"),
        serde_json::to_vec(&spec).unwrap(),
    )
    .unwrap();
    for helper in spec["helpers"].as_array().unwrap() {
        let helper = helper.as_str().unwrap();
        let path = root.join("bin").join(helper);
        fs::copy(env!("CARGO_BIN_EXE_delivery-fixture-peer"), &path).unwrap();
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if spec["not_executable"] == helper {
                0o600
            } else {
                0o700
            }),
        )
        .unwrap();
        if spec["invalid_executable"] == helper {
            fs::write(path, "not an executable").unwrap();
        }
    }
    let peer = env!("CARGO_BIN_EXE_delivery-fixture-peer");
    let mut command = if spec["sandbox"] == true {
        // The legacy /tmp fallback is tested only below a new tmpfs and PID namespace.
        let mut command = Command::new("/usr/bin/bwrap");
        command
            .args(["--ro-bind", "/", "/", "--bind"])
            .arg(root)
            .arg(root)
            .args([
                "--tmpfs",
                "/tmp",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--unshare-pid",
                "--die-with-parent",
                "--",
                peer,
            ]);
        command
    } else {
        Command::new(peer)
    };
    command
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("XDG_RUNTIME_DIR", root.join("r"))
        .env("LANG", "C.UTF-8")
        .env("DELIVERY_FIXTURE_ROOT", root)
        .env("OPENAI_API_KEY", "synthetic-private-key-canary")
        .env("BASH_ENV", "synthetic-hook-canary")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in spec["environment"].as_object().unwrap() {
        command.env(key, value.as_str().unwrap());
    }
    let mut driver = command
        .spawn()
        .expect("native isolated delivery driver (fallback requires bubblewrap)");
    driver
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&spec).unwrap())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while driver.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            driver.kill().unwrap();
            driver.wait().unwrap();
            panic!("bounded delivery fixture did not return");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = driver.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "native fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "silent helper boundary: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut observed: Value = serde_json::from_slice(&output.stdout).unwrap();
    observed.as_object_mut().unwrap().remove("elapsed_ms");
    observed
}

#[test]
fn clipboard_insertion_receipts_and_layout_safe_helpers_match_the_unchanged_release() {
    compare(include_str!("fixtures/released-delivery.json"));
}

#[test]
fn terminal_capture_and_revalidation_match_actual_released_process_observations() {
    compare(include_str!("fixtures/released-terminal-target.json"));
}

fn compare(fixture: &str) {
    let fixture: Value = serde_json::from_str(fixture).unwrap();
    let mut failures = vec![];
    for case in fixture["cases"].as_array().unwrap() {
        let observed = run(&case["spec"]);
        if observed != case["expected"] {
            failures
                .push(json!({"name":case["name"],"actual":observed,"expected":case["expected"]}));
        }
    }
    assert!(
        failures.is_empty(),
        "released delivery differences: {}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}
