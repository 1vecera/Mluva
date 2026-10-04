//! Opt-in actual pinned CPU/CUDA recognition. Never touches the installed app.
use flate2::read::GzDecoder;
use mluva_providers::{
    local::LocalSpeechClient, local_asr::OnnxOptions, onnx_runtime::ONNX_RUNTIME,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn verified_copy(source: &Path, target: &Path, spec: &Value) {
    let mut file = fs::File::open(source).unwrap();
    assert_eq!(
        file.metadata().unwrap().len(),
        spec["size"].as_u64().unwrap()
    );
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1_048_576];
    loop {
        let length = file.read(&mut buffer).unwrap();
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    let actual: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(actual, spec["sha256"].as_str().unwrap());
    private_directory(target.parent().unwrap());
    fs::copy(source, target).unwrap();
    fs::set_permissions(target, fs::Permissions::from_mode(0o600)).unwrap();
}
fn private_directory(path: &Path) {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .unwrap();
}
fn public_pcm(language: &str) -> Vec<u8> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let frontend: Value =
        serde_json::from_str(include_str!("fixtures/released-frontends.json")).unwrap();
    let name = format!("whisper-public-{language}");
    let spec = frontend["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    let mut pcm = Vec::new();
    GzDecoder::new(fs::File::open(fixtures.join(format!("{name}.pcm16le.gz"))).unwrap())
        .take(spec["frames"].as_u64().unwrap() * 2 + 1)
        .read_to_end(&mut pcm)
        .unwrap();
    assert_eq!(pcm.len() as u64, spec["frames"].as_u64().unwrap() * 2);
    assert_eq!(hash(&pcm), spec["pcm_sha256"].as_str().unwrap());
    pcm
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit checksum-verified public native runtime/model assets (MLUVA_ONNX_ASSETS)"]
async fn actual_cpu_whisper_and_parakeet_match_released_recognition() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-onnx-inference.json")).unwrap();
    actual_recognition(&reference, "cpu").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a physical NVIDIA GPU and explicitly prepared native-only public runtime/model assets"]
async fn actual_cuda_whisper_and_parakeet_match_released_recognition() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-onnx-cuda-inference.json")).unwrap();
    actual_recognition(&reference, "cuda").await;
}

async fn actual_recognition(reference: &Value, device: &str) {
    let assets = std::env::var_os("MLUVA_ONNX_ASSETS")
        .expect("prepare pinned public ONNX assets separately");
    let assets = Path::new(&assets);
    let directory = tempfile::Builder::new()
        .prefix("mluva-native-onnx-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    let data = root.join("data");
    for folder in ["home", "data", "config", "cache", "tmp", "runtime"] {
        private_directory(&root.join(folder));
    }
    let library = if device == "cuda" {
        let runtime = assets.join("native-runtime/xdg-data/mluva/gpu-runtime");
        let target = ONNX_RUNTIME.root(&data, "cuda");
        let mut expected = std::collections::BTreeSet::from([".native-ready".to_owned()]);
        for spec in reference["runtime"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|archive| archive["members"].as_array().unwrap())
        {
            let name = spec["path"].as_str().unwrap();
            expected.insert(name.to_owned());
            verified_copy(&runtime.join(name), &target.join(name), spec);
        }
        fn files(root: &Path, path: &Path, names: &mut std::collections::BTreeSet<String>) {
            for entry in fs::read_dir(path).unwrap().map(Result::unwrap) {
                let path = entry.path();
                let metadata = path.symlink_metadata().unwrap();
                assert!(
                    !metadata.is_symlink(),
                    "native-only runtime contains no links"
                );
                if metadata.is_dir() {
                    files(root, &path, names);
                } else {
                    assert!(metadata.is_file());
                    names.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_owned(),
                    );
                }
            }
        }
        let mut actual = std::collections::BTreeSet::new();
        files(&runtime, &runtime, &mut actual);
        assert_eq!(
            actual, expected,
            "only pinned native libraries/notices and completion marker are installed"
        );
        let stamp = fs::read_to_string(runtime.join(".native-ready")).unwrap();
        assert_eq!(stamp, ONNX_RUNTIME.stamp("cuda").unwrap());
        fs::write(target.join(".native-ready"), stamp).unwrap();
        assert!(ONNX_RUNTIME.ready(&data, "cuda"));
        ONNX_RUNTIME.library(&data, "cuda").unwrap()
    } else {
        for spec in reference["runtime"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|file| file["archive"] == "cpu")
        {
            let name = Path::new(spec["member"].as_str().unwrap())
                .file_name()
                .unwrap();
            verified_copy(
                &assets
                    .join("native-runtime/xdg-data/mluva/onnx-runtime/cpu/lib")
                    .join(name),
                &root.join("runtime").join(name),
                spec,
            );
        }
        root.join("runtime/libonnxruntime.so.1.30.0")
    };
    for (language, name) in [("en", "asr-en.wav"), ("zh", "asr-zh.wav")] {
        let pcm = public_pcm(language);
        fs::write(root.join(name), mluva_audio::wav::pcm16_wav(&pcm).unwrap()).unwrap();
        if language == "en" {
            let doubled = [pcm.as_slice(), pcm.as_slice()].concat();
            fs::write(
                root.join("asr-en-double.wav"),
                mluva_audio::wav::pcm16_wav(&doubled).unwrap(),
            )
            .unwrap();
        }
    }
    for case in reference["cases"].as_array().unwrap() {
        let id = case["model"].as_str().unwrap();
        let model = reference["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["id"] == id)
            .unwrap();
        let name = format!("{}-{}", id, &model["revision"].as_str().unwrap()[..12]);
        let model_root = data.join("models").join(&name);
        for file in model["files"].as_array().unwrap() {
            let file_name = file["name"].as_str().unwrap();
            verified_copy(
                &assets
                    .join(id)
                    .join("xdg-data/mluva/models")
                    .join(&name)
                    .join(file_name),
                &model_root.join(file_name),
                file,
            );
        }
        fs::write(
            model_root.join(".ready"),
            model["revision"].as_str().unwrap(),
        )
        .unwrap();
        let mut options = OnnxOptions::new(&data, id, env!("CARGO_BIN_EXE_mluva-asr-worker"));
        options.cpu_runtime = library.clone();
        options.device = device.into();
        options.keep_alive = true;
        let client = Arc::new(LocalSpeechClient::new(options).unwrap());
        let actions = case["actions"].as_array().unwrap();
        let mut retained = None;
        for action in actions {
            let mut callbacks = 0;
            let mut partial = |_: String| callbacks += 1;
            let result = client
                .transcribe(
                    &root.join(action["filename"].as_str().unwrap()),
                    action["language"].as_str().unwrap(),
                    Some(&mut partial),
                )
                .await
                .unwrap();
            assert_eq!(callbacks, 0, "ONNX recognition has no provisional callback");
            assert_result(&result, action, id);
            let processes = owned_processes(&model_root);
            assert_eq!(
                processes.len(),
                1,
                "only this client's native worker owns the model"
            );
            let pid = processes[0];
            if let Some(previous) = retained {
                assert_eq!(pid, previous, "weights retained between requests");
            } else {
                retained = Some(pid);
            }
            if device == "cuda" && action["name"] == "english-auto" {
                assert_eq!(case["gpu"]["owned_context"], true);
                assert_gpu_context(pid, id).await;
            }
        }
        let pid = retained.unwrap();
        let cache = worker_cache(pid, device);
        client.close().await;
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "close returns only after its worker has been reaped"
        );
        assert_cache_removed(&cache);
        let first = &actions[0];
        let result = client
            .transcribe(
                &root.join(first["filename"].as_str().unwrap()),
                first["language"].as_str().unwrap(),
                None,
            )
            .await
            .unwrap();
        assert_result(&result, first, id);
        let processes = owned_processes(&model_root);
        assert_eq!(processes.len(), 1);
        let restarted = processes[0];
        assert_ne!(restarted, pid, "a closed capture loads a new owned worker");
        let cache = worker_cache(restarted, device);
        client.close().await;
        assert!(!Path::new(&format!("/proc/{restarted}")).exists());
        assert_cache_removed(&cache);
        // Interrupt actual weight loading through the same application-facing factory.
        let capture = client.clone();
        let path = root.join(first["filename"].as_str().unwrap());
        let task = tokio::spawn(async move { capture.transcribe(&path, "auto", None).await });
        let loading = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let owned = owned_processes(&model_root);
                if let Some(pid) = owned.first() {
                    return *pid;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("a real model-loading worker is externally observable");
        assert!(
            !task.is_finished(),
            "cancellation reaches the real worker before recognition completes"
        );
        let cache = worker_cache(loading, device);
        let before = std::time::Instant::now();
        client.cancel().await;
        assert!(
            before.elapsed() < Duration::from_secs(2),
            "cancellation interrupts actual loading promptly"
        );
        assert!(
            task.await.unwrap().is_err(),
            "cancelled loading cannot return a transcription"
        );
        assert!(!Path::new(&format!("/proc/{loading}")).exists());
        assert_cache_removed(&cache);
        assert!(owned_processes(&model_root).is_empty());
        eprintln!(
            "{id}/{device}: parent metadata, weight reuse, explicit close/restart and loading cancellation verified"
        );
    }
}

fn assert_result(result: &mluva_providers::TranscriptionResult, action: &Value, id: &str) {
    let observed = json!({"text_sha256":hash(result.text.as_bytes()),"text_characters":result.text.chars().count(),"metadata":{
        "language_code":result.language_code,"language_probability":result.language_probability,"transcription_id":result.transcription_id,
        "speaker_segments":result.speaker_segments,"audio_duration_seconds":result.audio_duration_seconds}});
    let expected = json!({"text_sha256":action["text_sha256"],"text_characters":action["text_characters"],"metadata":action["metadata"]});
    assert_eq!(
        observed, expected,
        "{id}/{} matches unchanged released recognition and metadata",
        action["name"]
    );
    eprintln!(
        "{id}/{} matches released recognition and metadata",
        action["name"]
    );
}
fn owned_processes(model: &Path) -> Vec<u32> {
    fs::read_dir("/proc")
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            if fs::read_link(entry.path().join("exe")).ok()?.as_path()
                != Path::new(env!("CARGO_BIN_EXE_mluva-asr-worker"))
            {
                return None;
            }
            let args = fs::read(entry.path().join("cmdline")).ok()?;
            let model_arg = args.split(|byte| *byte == 0).nth(3)?;
            (model_arg == model.as_os_str().as_encoded_bytes()).then_some(pid)
        })
        .collect()
}
fn worker_cache(pid: u32, device: &str) -> Option<std::path::PathBuf> {
    let env = fs::read(format!("/proc/{pid}/environ")).unwrap();
    let cache = env
        .split(|byte| *byte == 0)
        .find_map(|value| value.strip_prefix(b"CUDA_CACHE_PATH="));
    if device == "cpu" {
        assert!(cache.is_none());
        return None;
    }
    let cache = std::path::PathBuf::from(
        std::str::from_utf8(cache.expect("CUDA worker uses only its owned shader cache")).unwrap(),
    );
    assert_eq!(
        fs::metadata(&cache).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(cache.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    Some(cache)
}
fn assert_cache_removed(cache: &Option<std::path::PathBuf>) {
    if let Some(cache) = cache {
        assert!(
            !cache.parent().unwrap().exists(),
            "closing the worker removes its shader-cache directory"
        );
    }
}
async fn assert_gpu_context(pid: u32, id: &str) {
    let mut probe = tokio::process::Command::new("/usr/bin/nvidia-smi");
    probe
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let telemetry = tokio::time::timeout(Duration::from_secs(2), probe.output())
        .await
        .unwrap()
        .unwrap();
    assert!(telemetry.status.success());
    let output = String::from_utf8(telemetry.stdout).unwrap();
    let memory = output
        .lines()
        .filter_map(|line| line.split_once(','))
        .find(|(owner, _)| owner.trim().parse::<u32>().ok() == Some(pid))
        .and_then(|(_, memory)| memory.trim().parse::<u64>().ok())
        .expect("owned native worker has an independently observed NVIDIA compute context");
    assert!(memory > 0);
    eprintln!("{id}: externally observed owned NVIDIA context ({memory} MiB)");
}
