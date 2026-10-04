//! Independent released process outputs and native cancellation/ownership faults.
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

fn isolated() -> PathBuf {
    let root =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private runner required"));
    assert!(PathBuf::from(std::env::var_os("HOME").unwrap()).starts_with(&root));
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    root
}
fn fixture() -> Value {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-screenshot-capture.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    fixture
}
async fn run_case(spec: &Value) -> Value {
    let directory = tempfile::Builder::new()
        .prefix("native-picker-")
        .tempdir_in(isolated())
        .unwrap();
    let root = directory.path();
    fs::create_dir(root.join("bin")).unwrap();
    fs::create_dir(root.join("home")).unwrap();
    fs::write(root.join("picker.json"), serde_json::to_vec(spec).unwrap()).unwrap();
    if spec["missing_tool"] != true {
        symlink(
            env!("CARGO_BIN_EXE_screenshot-picker-fixture-peer"),
            root.join("bin/omarchy"),
        )
        .unwrap();
    }
    let child = Command::new(env!("CARGO_BIN_EXE_screenshot-capture-fixture-driver"))
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("LC_ALL", "C.UTF-8")
        .env("MLUVA_SCREENSHOT_FIXTURE_ROOT", root)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(195), child.wait_with_output())
        .await
        .expect("bounded picker driver")
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires private network/PID/device runner and native picker processes"]
async fn released_picker_results_and_private_process_cleanup() {
    let fixture = fixture();
    let mut count = 0;
    for row in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["name"] != "timeout")
    {
        let actual = run_case(&row["spec"]).await;
        assert_eq!(actual, row["observed"], "{}", row["name"]);
        count += 1;
    }
    println!(
        "Compared {count} released picker workflows, exact results, arguments, permissions and owned cleanup"
    );
}

#[tokio::test]
#[ignore = "requires private runner; exercises the real 180-second selection deadline"]
async fn released_picker_timeout_closes_process_group_and_private_directory() {
    let fixture = fixture();
    let row = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "timeout")
        .unwrap();
    let start = std::time::Instant::now();
    assert_eq!(run_case(&row["spec"]).await, row["observed"]);
    assert!(start.elapsed() >= Duration::from_secs(179));
    println!(
        "Released picker deadline and cleanup match after {:.2} seconds",
        start.elapsed().as_secs_f64()
    );
}

#[tokio::test]
#[ignore = "requires private runner for dropped-owner and stubborn-child faults"]
async fn picker_cancellation_reaps_stubborn_children_and_rejects_nonregular_images() {
    let faults = [
        (
            "child-ignores-cancel",
            json!({"wait":true,"child":true,"child_ignores_term":true,"cancel":true}),
            json!({"cancelled":true}),
        ),
        (
            "dropped-waiter",
            json!({"wait":true,"child":true,"ignore_term":true,"child_ignores_term":true,"drop_waiter":true}),
            json!({"dropped":true}),
        ),
        (
            "dropped-owner",
            json!({"wait":true,"child":true,"drop_owner":true}),
            json!({"cancelled":true}),
        ),
        (
            "nonregular-image",
            json!({"kind":"fifo"}),
            json!({"io":libc::EINVAL}),
        ),
    ];
    for (name, spec, result) in faults {
        let observed = run_case(&spec).await;
        assert_eq!(observed["result"], result, "{name}");
        assert_eq!(observed["processes_gone"], true, "{name}: {observed}");
        assert_eq!(observed["directory_empty"], true, "{name}: {observed}");
        println!("Native picker fault {name}: cleanup acknowledged");
    }
}
