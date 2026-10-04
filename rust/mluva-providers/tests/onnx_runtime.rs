//! Native-only installation contract and independent release storage/artifact witnesses.
use mluva_providers::onnx_runtime::{ONNX_RUNTIME, check_gpu_storage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    process::Command,
};
use tokio_util::sync::CancellationToken;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn member(name: &str, path: &str, bytes: &[u8]) -> Value {
    json!({"member":name,"path":path,"size":bytes.len(),"sha256":hash(bytes)})
}

#[test]
fn pinned_native_libraries_and_gpu_storage_match_independent_release_witnesses() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-onnx-runtime.json")).unwrap();
    assert_eq!(json!(*ONNX_RUNTIME), reference["manifest"]);
    for row in reference["gpu_storage"].as_array().unwrap() {
        let result =
            check_gpu_storage(row["used"].as_u64().unwrap(), row["free"].as_u64().unwrap());
        assert_eq!(result.is_ok(), row["accepted"].as_bool().unwrap(), "{row}");
        if let Err(error) = result {
            assert_eq!(error.to_string(), row["error"].as_str().unwrap());
        }
    }
    assert_eq!(
        ONNX_RUNTIME
            .cpu
            .iter()
            .flat_map(|a| &a.members)
            .map(|m| m.size)
            .sum::<u64>(),
        29_338_945
    );
    assert_eq!(
        ONNX_RUNTIME
            .cuda
            .iter()
            .flat_map(|a| &a.members)
            .map(|m| m.size)
            .sum::<u64>(),
        3_247_698_857
    );
}

struct Server {
    address: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn new(bytes: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = CancellationToken::new();
        let stopping = stop.clone();
        let task = tokio::spawn(async move {
            loop {
                let mut peer = tokio::select! { biased; _=stopping.cancelled()=>break, peer=listener.accept()=>peer.unwrap().0 };
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if peer.read_exact(&mut byte).await.is_err() {
                        break;
                    }
                    head.push(byte[0]);
                    assert!(head.len() < 65_536);
                }
                recorded.lock().unwrap().push(
                    String::from_utf8(head)
                        .unwrap()
                        .lines()
                        .next()
                        .unwrap()
                        .to_owned(),
                );
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                if peer.write_all(header.as_bytes()).await.is_ok() {
                    let _ = peer.write_all(&bytes).await;
                }
            }
        });
        Self {
            address,
            requests,
            stop,
            task,
        }
    }
    async fn close(self) -> Vec<String> {
        self.stop.cancel();
        self.task.await.unwrap();
        Arc::try_unwrap(self.requests)
            .unwrap()
            .into_inner()
            .unwrap()
    }
}
async fn driver(root: &Path, spec: &Value, probe: &str) -> Value {
    for folder in ["home", "xdg-data/mluva", "bin"] {
        fs::create_dir_all(root.join(folder)).unwrap();
    }
    let script = root.join("bin/nvidia-smi");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '%s\\n' \"$$\" > '{}'\n{probe}\n",
            root.join("probe-arguments").display(),
            root.join("probe-pid").display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_asset-fixture-peer"));
    process
        .env_clear()
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg-data"))
        .env("LANG", "C.UTF-8")
        .env("PATH", root.join("bin"))
        .env("LOCAL_ASSET_FIXTURE_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = process.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    input
        .write_all(&serde_json::to_vec(spec).unwrap())
        .await
        .unwrap();
    drop(input);
    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated native installer exits successfully"
    );
    assert!(
        output.stderr.is_empty(),
        "native artifact diagnostics never include private content"
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn runtime_manifest(address: &str, archive: &[u8], members: Value) -> Value {
    let asset = json!({"package":"public-native-fixture","version":"1.0","url":format!("{address}/native.whl"),"size":archive.len(),"sha256":hash(archive),"members":members});
    json!({"cpu":[asset.clone()],"cuda":[asset]})
}
fn files(value: &Value) -> Vec<&str> {
    value["snapshot"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "file")
        .map(|r| r["path"].as_str().unwrap())
        .filter(|p| !p.ends_with(".download.lock"))
        .collect()
}

#[tokio::test]
async fn installs_only_verified_native_files_reuses_cache_and_preserves_old_runtime_on_failure() {
    let library = b"native-library-fixture";
    let license = b"public-license-fixture";
    let archive = zip(&[
        ("public/lib.so", library),
        ("public/LICENSE", license),
        ("public/private.py", b"must never ship"),
    ]);
    let server = Server::new(archive.clone()).await;
    let valid = runtime_manifest(
        &server.address,
        &archive,
        json!([
            member("public/lib.so", "lib/libonnxruntime.so.1", library),
            member("public/LICENSE", "notices/LICENSE", license)
        ]),
    );
    let mut scenarios = vec![
        (
            "cpu",
            valid.clone(),
            json!([{"device":"cpu"},{"device":"cpu","cancel":true,"post_promotion_crash":true}]),
            true,
        ),
        (
            "cuda",
            valid.clone(),
            json!([{"device":"cuda"},{"device":"cuda","post_promotion_crash":true}]),
            true,
        ),
    ];
    for change in [
        "whole-hash",
        "member-hash",
        "missing-member",
        "escape-destination",
        "escape-member",
        "duplicate-destination",
        "declared-size",
    ] {
        let mut changed = valid.clone();
        let asset = &mut changed["cpu"][0];
        match change {
            "whole-hash" => asset["sha256"] = json!("0".repeat(64)),
            "member-hash" => asset["members"][0]["sha256"] = json!("0".repeat(64)),
            "missing-member" => asset["members"][0]["member"] = json!("public/missing.so"),
            "escape-destination" => asset["members"][0]["path"] = json!("../escaped.so"),
            "escape-member" => asset["members"][0]["member"] = json!("../escaped.so"),
            "duplicate-destination" => {
                asset["members"][1]["path"] = json!("lib/libonnxruntime.so.1")
            }
            "declared-size" => asset["members"][0]["size"] = json!(library.len() + 1),
            _ => unreachable!(),
        }
        scenarios.push((change, changed, json!([{"device":"cpu"}]), false));
    }
    for (name, manifest, calls, success) in scenarios {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join(if name == "cuda" {
            "xdg-data/mluva/gpu-runtime"
        } else {
            "xdg-data/mluva/onnx-runtime/cpu"
        });
        fs::create_dir_all(&installed).unwrap();
        fs::write(installed.join("old-runtime"), b"existing-working-runtime").unwrap();
        let actual = driver(
            root.path(),
            &json!({"kind":"onnx-runtime","manifest":manifest,"calls":calls}),
            "printf 'NVIDIA fixture GPU\\n'",
        )
        .await;
        assert_eq!(
            actual["ready"],
            if success {
                json!([true, true])
            } else {
                json!([false])
            },
            "{name}"
        );
        assert!(
            !installed.with_extension("partial").exists(),
            "staging cleanup {name}"
        );
        assert!(
            !installed.with_extension("previous").exists(),
            "replacement cleanup {name}"
        );
        assert!(!installed.parent().unwrap().join("escaped.so").exists());
        if success {
            assert_eq!(
                actual["results"],
                json!([{"ok":null},{"ok":null}]),
                "{name}"
            );
            assert_eq!(
                fs::read(installed.join("lib/libonnxruntime.so.1")).unwrap(),
                library
            );
            assert_eq!(
                fs::read(installed.join("notices/LICENSE")).unwrap(),
                license
            );
            assert!(!installed.join("old-runtime").exists());
            assert_eq!(
                files(&actual).len(),
                3,
                "libraries/notices/marker only {name}"
            );
            for row in actual["snapshot"].as_array().unwrap().iter().filter(|r| {
                r["kind"] == "file" && !r["path"].as_str().unwrap().ends_with(".download.lock")
            }) {
                assert_eq!(row["mode"], 0o600, "private native files {name}");
            }
        } else {
            assert_eq!(
                actual["results"],
                json!([{"error":"Runtime verification failed. Download again."}]),
                "{name}"
            );
            assert_eq!(
                fs::read(installed.join("old-runtime")).unwrap(),
                b"existing-working-runtime"
            );
            assert_eq!(
                files(&actual).len(),
                1,
                "failed download retains only original runtime {name}"
            );
        }
    }
    let root = tempfile::tempdir().unwrap();
    let cache = root
        .path()
        .join("xdg-data/mluva/onnx-runtime/cpu/owned-cache");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::File::create(&cache)
        .unwrap()
        .set_len(1_500_000_001)
        .unwrap();
    let actual = driver(
        root.path(),
        &json!({"kind":"onnx-runtime","manifest":valid,"calls":[{"device":"cuda"}]}),
        "printf 'NVIDIA fixture GPU\\n'",
    )
    .await;
    assert_eq!(
        actual["results"],
        json!([{"error":"GPU support needs 3.5 GB of the 5 GB local storage budget. Remove unused models first."}])
    );
    assert_eq!(actual["ready"], json!([false]));
    assert_eq!(fs::metadata(cache).unwrap().len(), 1_500_000_001);
    assert_eq!(server.close().await, vec!["GET /native.whl HTTP/1.1"; 9]);
}

#[tokio::test]
async fn cancellation_interrupts_extraction_without_blocking_the_runtime_executor() {
    let bytes = vec![0x5a; 30_000_000];
    let archive = zip(&[("public/lib.so", &bytes)]);
    let server = Server::new(archive.clone()).await;
    let manifest = runtime_manifest(
        &server.address,
        &archive,
        json!([member("public/lib.so", "lib/libonnxruntime.so.1", &bytes)]),
    );
    let root = tempfile::tempdir().unwrap();
    let actual=driver(root.path(),&json!({"kind":"onnx-runtime","manifest":manifest,"calls":[{"device":"cpu","cancel_after_ms":20}]}),"exit 1").await;
    assert_eq!(actual["results"], json!([{"error":"Download cancelled."}]));
    assert_eq!(actual["ready"], json!([false]));
    assert!(files(&actual).is_empty());
    assert!(
        !root
            .path()
            .join("xdg-data/mluva/onnx-runtime/cpu.partial")
            .exists()
    );
    assert_eq!(server.close().await, vec!["GET /native.whl HTTP/1.1"]);
    let server = Server::new(archive.clone()).await;
    let manifest = runtime_manifest(
        &server.address,
        &archive,
        json!([member("public/lib.so", "lib/libonnxruntime.so.1", &bytes)]),
    );
    let root = tempfile::tempdir().unwrap();
    let installed = root.path().join("xdg-data/mluva/onnx-runtime/cpu");
    fs::create_dir_all(&installed).unwrap();
    fs::write(installed.join("old-runtime"), b"existing-working-runtime").unwrap();
    let actual = driver(root.path(), &json!({"kind":"onnx-drop","manifest":manifest,"device":"cpu","member":"lib/libonnxruntime.so.1"}), "exit 1").await;
    assert_eq!(
        actual["results"],
        json!([{"aborted":true,"member_observed":true,"lock_release_after_cleanup":true}])
    );
    assert_eq!(actual["ready"], json!([false]));
    assert_eq!(
        fs::read(installed.join("old-runtime")).unwrap(),
        b"existing-working-runtime"
    );
    assert_eq!(files(&actual).len(), 1);
    assert!(!installed.with_extension("partial").exists());
    assert_eq!(server.close().await, vec!["GET /native.whl HTTP/1.1"]);
}

#[tokio::test]
async fn gpu_probe_is_cached_bounded_and_requires_no_python_package_manager() {
    for (script, expected) in [
        (
            "printf ' NVIDIA fixture GPU\\nother GPU\\n'",
            "NVIDIA fixture GPU",
        ),
        ("printf 'GPU\\n'; exit 1", ""),
        ("printf '\\377'", ""),
        ("while :; do :; done", ""),
    ] {
        let root = tempfile::tempdir().unwrap();
        let before = std::time::Instant::now();
        let actual = driver(root.path(), &json!({"kind":"onnx-probe"}), script).await;
        assert_eq!(
            actual["results"],
            json!([{"gpu":expected},{"gpu":expected}])
        );
        assert_eq!(
            fs::read_to_string(root.path().join("probe-arguments")).unwrap(),
            "--query-gpu=name --format=csv,noheader --id=0\n"
        );
        assert!(before.elapsed() < Duration::from_secs(4));
        let pid = fs::read_to_string(root.path().join("probe-pid")).unwrap();
        assert!(
            !Path::new(&format!("/proc/{}", pid.trim())).exists(),
            "probe child is reaped before return"
        );
    }
}
