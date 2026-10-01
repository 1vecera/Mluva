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
        let mut driver = driver_command(root).spawn().unwrap();
        let mut stdin = driver.stdin.take().unwrap();
        stdin
            .write_all(&serde_json::to_vec(&case["spec"]).unwrap())
            .await
            .unwrap();
        drop(stdin);
        let output = tokio::time::timeout(Duration::from_secs(20), driver.wait_with_output())
            .await
            .expect("bounded Qwen fixture client")
            .unwrap();
        let name = case["name"].as_str().unwrap();
        assert!(output.status.success(), "Qwen fixture process {name}");
        assert!(
            output.stderr.is_empty(),
            "silent Qwen fixture process {name}"
        );
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
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
