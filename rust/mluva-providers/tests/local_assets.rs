use mluva_providers::local_assets::{
    AssetStore, MODEL_CATALOG, QWEN_RUNTIME, check_model_storage, check_runtime_storage,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
mod support;

#[test]
fn pinned_catalog_and_runtime_metadata_preserve_all_choices() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-assets.json")).unwrap();
    assert_eq!(json!(*MODEL_CATALOG), fixture["catalog"]["models"]);
    assert_eq!(json!(*QWEN_RUNTIME), fixture["catalog"]["qwen_runtime"]);
    let offered = MODEL_CATALOG
        .iter()
        .filter(|model| model.offered)
        .map(|model| &model.id)
        .collect::<Vec<_>>();
    assert_eq!(json!(offered), fixture["catalog"]["offered"]);
}

#[test]
fn storage_preflight_matches_released_request_witnesses() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-assets.json")).unwrap();
    for (index, row) in fixture["storage_cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let used = row["used"].as_u64().unwrap();
        let free = row["free"].as_u64().unwrap();
        let result = if row["kind"] == "model" {
            check_model_storage(used, row["total"].as_u64().unwrap(), free)
        } else {
            check_runtime_storage(used, free)
        };
        assert_eq!(
            result.is_ok(),
            row["accepted"].as_bool().unwrap(),
            "storage acceptance {index}"
        );
        if let Err(error) = result {
            assert_eq!(
                json!({"error":error.to_string()}),
                row["failure"],
                "storage message {index}"
            );
        }
    }
}
fn setup(root: &Path, items: &Value) {
    for item in items.as_array().into_iter().flatten() {
        let path = root.join(item["path"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if item["directory"] == true {
            fs::create_dir_all(&path).unwrap();
        } else if let Some(link) = item["link"].as_str() {
            std::os::unix::fs::symlink(link, &path).unwrap();
        } else {
            fs::write(&path, item["text"].as_str().unwrap_or("")).unwrap();
            if let Some(size) = item["size"].as_u64() {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_len(size)
                    .unwrap();
            }
        }
        if let Some(mode) = item["mode"].as_u64() {
            fs::set_permissions(path, fs::Permissions::from_mode(mode as u32)).unwrap();
        }
    }
}
fn body(response: &Value) -> Vec<u8> {
    if let Some(value) = response["hex"].as_str() {
        support::unhex(value)
    } else {
        vec![
            response["byte"].as_u64().unwrap_or(88) as u8;
            response["size"].as_u64().unwrap_or(0) as usize
        ]
    }
}
async fn tls(root: &Path) -> native_tls::TlsAcceptor {
    let output = Command::new("/usr/bin/openssl")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=huggingface.co",
            "-addext",
            "subjectAltName=DNS:huggingface.co,DNS:github.com",
            "-keyout",
        ])
        .arg(root.join("key.pem"))
        .arg("-out")
        .arg(root.join("cert.pem"))
        .stdin(Stdio::null())
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "private fixture certificate");
    let identity = native_tls::Identity::from_pkcs8(
        &fs::read(root.join("cert.pem")).unwrap(),
        &fs::read(root.join("key.pem")).unwrap(),
    )
    .unwrap();
    native_tls::TlsAcceptor::new(identity).unwrap()
}
async fn run_case(case: &Value, acceptor: &native_tls::TlsAcceptor, certificate: &Path) {
    let directory = tempfile::Builder::new()
        .prefix("native-assets-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    let data = root.join("xdg-data/mluva");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&data)
        .unwrap();
    setup(root, &case["spec"]["setup"]);
    let _held_download = if case["spec"]["lock_held"] == true {
        Some(AssetStore::new(&data).unwrap().begin_download().unwrap())
    } else {
        None
    };
    if let Some(archive) = case["spec"]["archive_hex"].as_str() {
        fs::write(root.join("archive.tar.gz"), support::unhex(archive)).unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(vec![]));
    let recorded = requests.clone();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let acceptor = tokio_native_tls::TlsAcceptor::from(acceptor.clone());
    let responses = case["spec"]["responses"].clone();
    let server = tokio::spawn(async move {
        let mut peers = vec![];
        loop {
            let mut socket = tokio::select! {biased;_=stopped.cancelled()=>break,value=listener.accept()=>value.unwrap().0};
            let acceptor = acceptor.clone();
            let responses = responses.clone();
            let recorded = recorded.clone();
            peers.push(tokio::spawn(async move {
                let connect=support::request(&mut socket).await;
                assert_eq!(connect.method,"CONNECT");
                assert!(matches!(connect.path.as_str(),"huggingface.co:443"|"github.com:443"));
                socket.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await.unwrap();
                let mut socket=acceptor.accept(socket).await.unwrap();
                let request=support::request(&mut socket).await;
                assert!(!request.headers.contains_key("authorization") && !request.headers.contains_key("cookie"));
                recorded.lock().unwrap().push(json!({"method":request.method,"path":request.path,"host":request.headers["host"]}));
                let response=&responses[&request.path];
                let payload=body(response);
                let headers:Vec<(String,String)>=response["redirect"].as_str().map(|url|vec![("Location".into(),url.into())]).unwrap_or_default();
                let status=response["status"].as_u64().unwrap_or(200) as u16;
                let reason=reqwest::StatusCode::from_u16(status).unwrap().canonical_reason().unwrap();
                let mut head=format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",payload.len());
                for (key,value) in headers {head.push_str(&format!("{key}: {value}\r\n"));}
                head.push_str("\r\n");
                if socket.write_all(head.as_bytes()).await.is_err() {return;}
                for part in payload.chunks(response["fragment"].as_u64().unwrap_or(8191) as usize) {
                    if socket.write_all(part).await.is_err() {return;}
                }
                let _=socket.shutdown().await;
            }));
        }
        for peer in peers {
            peer.await.unwrap();
        }
    });
    let mut driver = Command::new(env!("CARGO_BIN_EXE_asset-fixture-peer"));
    driver
        .env_clear()
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg-data"))
        .env("PATH", "/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .env("SSL_CERT_FILE", certificate)
        .env("HTTPS_PROXY", proxy)
        .env("LOCAL_ASSET_FIXTURE_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut driver = driver.spawn().unwrap();
    let mut stdin = driver.stdin.take().unwrap();
    stdin
        .write_all(&serde_json::to_vec(&case["spec"]).unwrap())
        .await
        .unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(20), driver.wait_with_output())
        .await
        .expect("bounded fixture driver")
        .unwrap();
    stop.cancel();
    server.await.unwrap();
    let name = case["name"].as_str().unwrap();
    assert!(output.status.success(), "asset fixture process {name}");
    assert!(
        output.stderr.is_empty(),
        "silent asset fixture process {name}"
    );
    let mut actual: Value = serde_json::from_slice(&output.stdout).unwrap();
    actual["requests"] = json!(*requests.lock().unwrap());
    for field in [
        "results",
        "ready",
        "progress",
        "snapshot",
        "disk_usage",
        "requests",
    ] {
        let equal = if field == "progress" {
            let bits = |value: &Value| {
                value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|call| {
                        call.as_array()
                            .unwrap()
                            .iter()
                            .map(|number| number.as_f64().unwrap().to_bits())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            bits(&actual[field]) == bits(&case["result"][field])
        } else {
            actual[field] == case["result"][field]
        };
        if !equal {
            fs::write(
                root.join("mismatch.json"),
                serde_json::to_vec_pretty(&json!({"actual":actual,"expected":case["result"]}))
                    .unwrap(),
            )
            .unwrap();
            let report = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tmp/local-asset-mismatch.json");
            fs::write(
                report,
                serde_json::to_vec_pretty(
                    &json!({"case":name,"field":field,"actual":actual,"expected":case["result"]}),
                )
                .unwrap(),
            )
            .unwrap();
            panic!(
                "released asset observation differs: {name} / {field}; bounded report in tmp/local-asset-mismatch.json"
            );
        }
    }
}

#[tokio::test]
async fn actual_https_installs_archive_rules_and_readiness_match_the_release() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-assets.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let acceptor = tls(directory.path()).await;
    for case in fixture["cases"].as_array().unwrap() {
        run_case(case, &acceptor, &directory.path().join("cert.pem")).await;
    }
}

#[test]
fn download_lock_is_owned_until_the_native_job_is_released() {
    let directory = tempfile::tempdir().unwrap();
    let store = AssetStore::new(directory.path()).unwrap();
    let first = store.begin_download().unwrap();
    let second = store.clone();
    assert!(second.begin_download().is_err());
    drop(first);
    assert!(second.begin_download().is_ok());
}

#[tokio::test]
#[ignore = "downloads the two pinned public Qwen runtime archives into disposable storage"]
async fn actual_pinned_qwen_archives_match_released_installations() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-pinned-runtimes.json")).unwrap();
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("data");
    let store = AssetStore::new(&data).unwrap();
    let mut download = store.begin_download().unwrap();
    for device in ["cpu", "cuda"] {
        tokio::time::timeout(
            Duration::from_secs(90),
            download.install_qwen_runtime(&QWEN_RUNTIME, device, &CancellationToken::new()),
        )
        .await
        .expect("bounded pinned runtime installation")
        .unwrap();
        assert!(QWEN_RUNTIME.ready(&data, device));
    }
    drop(download);
    let mut rows = vec![];
    fn walk(root: &Path, directory: &Path, rows: &mut Vec<Value>) {
        let mut entries = fs::read_dir(directory)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = path.symlink_metadata().unwrap();
            let name = path.strip_prefix(root).unwrap().to_str().unwrap();
            let mut row = if metadata.is_symlink() {
                json!({"path":name,"kind":"link","target":fs::read_link(&path).unwrap()})
            } else if metadata.is_dir() {
                json!({"path":name,"kind":"directory","mode":metadata.permissions().mode()&0o777})
            } else {
                let mut file = fs::File::open(&path).unwrap();
                let mut buffer = [0; 65_536];
                let mut digest = Sha256::new();
                loop {
                    let size = file.read(&mut buffer).unwrap();
                    if size == 0 {
                        break;
                    }
                    digest.update(&buffer[..size]);
                }
                let hash = digest
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                json!({"path":name,"kind":"file","size":metadata.len(),"mode":metadata.permissions().mode()&0o777,"sha256":hash})
            };
            if !metadata.is_symlink() && name.contains("/llama-b11011") {
                row["mtime"] = json!(
                    metadata
                        .modified()
                        .unwrap()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                );
            }
            rows.push(row);
            if metadata.is_dir() {
                walk(root, &path, rows);
            }
        }
    }
    walk(&data, &data, &mut rows);
    assert_eq!(json!(rows), fixture["snapshot"]);
    assert_eq!(
        json!(mluva_providers::local_assets::disk_usage(&data)),
        fixture["disk_usage"]
    );
}
