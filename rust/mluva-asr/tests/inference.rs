//! Opt-in actual pinned CPU recognition. Asset preparation never touches the installed app.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

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
    let assets = std::env::var_os("MLUVA_ONNX_ASSETS")
        .expect("prepare pinned public ONNX assets separately");
    let assets = Path::new(&assets);
    let directory = tempfile::Builder::new()
        .prefix("mluva-native-onnx-")
        .tempdir()
        .unwrap();
    let root = directory.path();
    for folder in ["home", "data", "config", "cache", "tmp", "runtime"] {
        private_directory(&root.join(folder));
    }
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
            &assets.join("runtime-cpu").join(name),
            &root.join("runtime").join(name),
            spec,
        );
    }
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
        let model_root = root.join("models").join(&name);
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
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mluva-asr-worker"));
        command
            .arg(root.join("runtime/libonnxruntime.so.1.30.0"))
            .arg(id)
            .arg(&model_root)
            .arg("cpu")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .env("HOME", root.join("home"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("TMPDIR", root.join("tmp"))
            .env("OMP_NUM_THREADS", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let pid = child.id().unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let outcome: std::result::Result<(), String> = async {
            for action in case["actions"].as_array().unwrap() {
                let mut request = serde_json::to_vec(&json!({"path":root.join(action["filename"].as_str().unwrap()),"language":match action["language"].as_str().unwrap() {"eng"=>"en","zho"=>"zh",language=>language}})).unwrap();
                request.push(b'\n');
                input.write_all(&request).await.map_err(|_| "worker input closed".to_owned())?;
                input.flush().await.map_err(|_| "worker input closed".to_owned())?;
                let mut line = Vec::new();
                tokio::time::timeout(Duration::from_secs(180), (&mut output).take(2_000_001).read_until(b'\n', &mut line))
                    .await.map_err(|_| "worker deadline exceeded".to_owned())?
                    .map_err(|_| "worker output closed".to_owned())?;
                if line.len() > 2_000_000 || !line.ends_with(b"\n") { return Err("worker returned no bounded complete result".to_owned()); }
                let result: Value = serde_json::from_slice(&line).map_err(|_| "worker returned invalid JSON".to_owned())?;
                let text = result["text"].as_str().ok_or("worker returned no text")?;
                let observed = json!({"text_sha256":hash(text.as_bytes()),"text_characters":text.chars().count()});
                let expected = json!({"text_sha256":action["text_sha256"],"text_characters":action["text_characters"]});
                if observed != expected { return Err(format!("{id}/{}: {observed} != {expected}", action["name"])); }
                eprintln!("{id}/{} matches released recognition", action["name"]);
            }
            Ok(())
        }.await;
        drop(input);
        if outcome.is_err() {
            let _ = child.kill().await;
        }
        let status = tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
        assert!(status.success());
    }
}
