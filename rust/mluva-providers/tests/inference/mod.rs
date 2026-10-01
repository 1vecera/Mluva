//! Opt-in, actual pinned inference using public clips and disposable model stores.
use super::driver_command;
use mluva_providers::local_assets::{AssetStore, QWEN_RUNTIME, model};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn text(value: &str) -> Value {
    json!({"characters":value.chars().count(), "sha256":hash(value.as_bytes())})
}
fn file_hash(path: &Path) -> String {
    let mut file = fs::File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let size = file.read(&mut buffer).unwrap();
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn copy_tree(source: &Path, target: &Path) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(target)
        .unwrap();
    fs::set_permissions(target, fs::metadata(source).unwrap().permissions()).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let destination = target.join(entry.file_name());
        let kind = path.symlink_metadata().unwrap().file_type();
        if kind.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&path).unwrap(), destination).unwrap();
        } else if kind.is_dir() {
            copy_tree(&path, &destination);
        } else {
            assert!(
                kind.is_file(),
                "public pinned artifacts contain only ordinary files"
            );
            fs::hard_link(&path, destination).unwrap();
        }
    }
}

struct Runtime {
    pid: u32,
    key: PathBuf,
    observation: Value,
}
fn runtime(pid: u32, root: &Path) -> Option<Runtime> {
    let process = PathBuf::from(format!("/proc/{pid}"));
    let arguments = fs::read(process.join("cmdline")).ok()?;
    let arguments: Vec<_> = std::str::from_utf8(&arguments)
        .ok()?
        .trim_end_matches('\0')
        .split('\0')
        .collect();
    let argument = |name| {
        arguments
            .iter()
            .position(|arg| *arg == name)
            .and_then(|index| arguments.get(index + 1))
            .copied()
    };
    if !Path::new(argument("-m")?).starts_with(root.join("xdg-data/mluva/models")) {
        return None;
    }
    let key = PathBuf::from(argument("--api-key-file")?);
    if !key.starts_with(root.join("tmp")) {
        return None;
    }
    let port = argument("--port")?;
    let normalize = |value: &str| {
        if value == port {
            "$PORT".into()
        } else {
            value
                .replace(key.parent().unwrap().to_str().unwrap(), "$TEMP")
                .replace(root.to_str().unwrap(), "$ROOT")
        }
    };
    let environment = fs::read(process.join("environ")).ok()?;
    let environment = std::str::from_utf8(&environment)
        .ok()?
        .split('\0')
        .filter(|item| !item.is_empty())
        .map(|item| {
            item.split_once('=')
                .map(|(key, value)| (key.to_string(), normalize(value)))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    let stdio = (0..3)
        .map(|fd| {
            fs::read_link(process.join(format!("fd/{fd}")))
                .ok()
                .map(|path| path == Path::new("/dev/null"))
        })
        .collect::<Option<Vec<_>>>()?;
    let key_mode = fs::metadata(&key).ok()?.permissions().mode() & 0o777;
    let temporary_mode = fs::metadata(key.parent()?).ok()?.permissions().mode() & 0o777;
    Some(Runtime {
        pid,
        key: key.clone(),
        observation: json!({"arguments":arguments[1..].iter().map(|argument|normalize(argument)).collect::<Vec<_>>(),"environment":environment,"key_mode":key_mode,"temporary_mode":temporary_mode,"stdio":stdio}),
    })
}
struct OwnedRuntimes<'a> {
    root: &'a Path,
    values: BTreeMap<u32, Runtime>,
}
impl Drop for OwnedRuntimes<'_> {
    fn drop(&mut self) {
        for owned in self.values.values() {
            // A failed harness may kill its driver before normal library cleanup.
            // Signal only a still-identical process in this disposable data root.
            if let Some(current) = runtime(owned.pid, self.root)
                && current.key == owned.key
            {
                unsafe {
                    libc::kill(owned.pid as libc::pid_t, libc::SIGKILL);
                }
            }
        }
    }
}
fn observe_children(driver: u32, owned: &mut OwnedRuntimes<'_>) {
    let Ok(tasks) = fs::read_dir(format!("/proc/{driver}/task")) else {
        return;
    };
    for task in tasks.flatten() {
        if let Ok(children) = fs::read_to_string(task.path().join("children")) {
            for pid in children
                .split_whitespace()
                .filter_map(|pid| pid.parse::<u32>().ok())
            {
                if !owned.values.contains_key(&pid)
                    && let Some(value) = runtime(pid, owned.root)
                {
                    owned.values.insert(pid, value);
                }
            }
        }
    }
}

async fn prepare_samples(fixture: &Value, directory: &Path) -> BTreeMap<String, Vec<u8>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap();
    let mut samples = BTreeMap::new();
    for sample in fixture["samples"].as_array().unwrap() {
        let response = client
            .get(sample["url"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let bytes = response.bytes().await.unwrap();
        assert_eq!(
            bytes.len() as u64,
            sample["original_size"].as_u64().unwrap()
        );
        assert_eq!(hash(&bytes), sample["original_sha256"].as_str().unwrap());
        let original = directory.join("public-original.wav");
        fs::write(&original, bytes).unwrap();
        let conversion = Command::new("/usr/bin/ffmpeg")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(&original)
            .args([
                "-map_metadata",
                "-1",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-c:a",
                "pcm_s16le",
                "-f",
                "s16le",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();
        let output = tokio::time::timeout(Duration::from_secs(10), conversion)
            .await
            .expect("bounded public sample normalization")
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout.len() as u64,
            sample["pcm_bytes"].as_u64().unwrap()
        );
        assert_eq!(hash(&output.stdout), sample["pcm_sha256"].as_str().unwrap());
        samples.insert(sample["name"].as_str().unwrap().to_owned(), output.stdout);
    }
    samples
}

#[tokio::test]
#[ignore = "downloads pinned Qwen/public samples and runs CPU plus physical NVIDIA Vulkan inference"]
async fn actual_pinned_qwen_cpu_and_gpu_match_released_recognition() {
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/released-qwen-inference.json")).unwrap();
    let directory = tempfile::Builder::new()
        .prefix("qwen-inference-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let assets = directory.path().join("assets");
    let store = AssetStore::new(&assets).unwrap();
    let mut download = store.begin_download().unwrap();
    let spec = model("qwen3-1.7b").unwrap();
    let cancelled = CancellationToken::new();
    for device in ["cpu", "cuda"] {
        download
            .install_qwen_runtime(&QWEN_RUNTIME, device, &cancelled)
            .await
            .unwrap();
    }
    if let Some(source) = std::env::var_os("MLUVA_QWEN_WEIGHTS") {
        let source = PathBuf::from(source);
        for file in &spec.files {
            let path = source.join(&file.name);
            assert_eq!(fs::metadata(&path).unwrap().len(), file.size);
            assert_eq!(
                file_hash(&path),
                file.sha256,
                "explicitly supplied public weights"
            );
        }
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(spec.path(&assets))
            .unwrap();
        for file in &spec.files {
            fs::copy(source.join(&file.name), spec.path(&assets).join(&file.name)).unwrap();
        }
        fs::write(spec.path(&assets).join(".ready"), &spec.revision).unwrap();
    }
    download
        .install_model_files(spec, &cancelled, &mut |_| {})
        .await
        .unwrap();
    drop(download);
    let samples = prepare_samples(&fixture, directory.path()).await;
    for case in fixture["cases"].as_array().unwrap() {
        let root = directory.path().join(case["name"].as_str().unwrap());
        let data = root.join("xdg-data/mluva");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&data)
            .unwrap();
        copy_tree(&assets.join("models"), &data.join("models"));
        copy_tree(&assets.join("qwen-runtime"), &data.join("qwen-runtime"));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join("tmp"))
            .unwrap();
        for (name, pcm) in &samples {
            fs::write(root.join(name), mluva_audio::wav::pcm16_wav(pcm).unwrap()).unwrap();
        }
        fs::write(
            root.join("asr-en-double.wav"),
            mluva_audio::wav::pcm16_wav(&samples["asr-en.wav"].repeat(2)).unwrap(),
        )
        .unwrap();
        let mut driver = driver_command(&root).spawn().unwrap();
        let pid = driver.id().unwrap();
        let mut stdin = driver.stdin.take().unwrap();
        stdin
            .write_all(&serde_json::to_vec(&case["spec"]).unwrap())
            .await
            .unwrap();
        drop(stdin);
        let mut owned = OwnedRuntimes {
            root: &root,
            values: BTreeMap::new(),
        };
        let waiting = driver.wait_with_output();
        tokio::pin!(waiting);
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        let deadline = tokio::time::sleep(Duration::from_secs(600));
        tokio::pin!(deadline);
        let before = std::time::Instant::now();
        let output = loop {
            tokio::select! {
                output = &mut waiting => break output.unwrap(),
                _ = interval.tick() => observe_children(pid,&mut owned),
                _ = &mut deadline => panic!("bounded real pinned Qwen inference"),
            }
        };
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let mut actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        for result in actual["results"].as_array_mut().unwrap() {
            if let Some(value) = result["ok"]["text"].as_str() {
                result["ok"]["text"] = text(value);
            }
        }
        for call in actual["partials"].as_array_mut().unwrap() {
            for partial in call.as_array_mut().unwrap() {
                if let Some(value) = partial.as_str() {
                    *partial = text(value);
                }
            }
        }
        let processes = owned
            .values
            .values()
            .map(|owned| {
                assert!(
                    !Path::new("/proc").join(owned.pid.to_string()).exists(),
                    "real runtime was reaped"
                );
                assert!(!owned.key.exists(), "real temporary key was deleted");
                owned.observation.clone()
            })
            .collect::<Vec<_>>();
        let observed = json!({"results":actual["results"],"partials":actual["partials"],"processes":processes});
        if observed != case["result"] {
            fs::write(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp/qwen-real-mismatch.json"),
                serde_json::to_vec_pretty(
                    &json!({"case":case["name"],"actual":observed,"expected":case["result"]}),
                )
                .unwrap(),
            )
            .unwrap();
            panic!("physical Qwen observation differs; hashes in tmp/qwen-real-mismatch.json");
        }
        eprintln!(
            "{}: real recognition/partials/processes match in {:.3}s",
            case["name"],
            before.elapsed().as_secs_f64()
        );
    }
}
