//! Controlled JSONL worker/public local factory driver; development only.
use mluva_providers::{local::LocalSpeechClient, local_asr::OnnxOptions};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
#[path = "mod.rs"]
mod support;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn text(value: &str) -> Value {
    if value.chars().count() < 4096 {
        json!(value)
    } else {
        json!({"characters":value.chars().count(),"sha256":hash(value.as_bytes())})
    }
}
fn append(root: &Path, value: Value) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("trace.jsonl"))
        .unwrap();
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    file.write_all(&bytes).unwrap();
}
fn trace(root: &Path) -> Vec<Value> {
    let contents = fs::read_to_string(root.join("trace.jsonl")).unwrap_or_default();
    contents
        .rsplit_once('\n')
        .map(|(complete, _)| {
            complete
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        })
        .unwrap_or_default()
}
fn state(root: &Path) -> Value {
    let starts = trace(root)
        .into_iter()
        .filter(|v| v["kind"] == "start")
        .collect::<Vec<_>>();
    json!({"processes":starts.len(),"alive":starts.iter().map(|v|Path::new(&format!("/proc/{}",v["pid"])).exists()).collect::<Vec<_>>()})
}

async fn runtime(args: &[String]) {
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) },
        0
    );
    let legacy = args.first().is_some_and(|v| v == "-m");
    let config: Value = serde_json::from_slice(
        &fs::read(
            std::env::current_exe()
                .unwrap()
                .with_file_name("fixture.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let root = PathBuf::from(config["root"].as_str().unwrap());
    let first_process = !trace(&root).iter().any(|row| row["kind"] == "start");
    let offset = if legacy { 2 } else { 1 };
    let model = &args[offset];
    let path = &args[offset + 1];
    let device = &args[offset + 2];
    let env = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    let mut common = env.clone();
    if legacy {
        assert!(common.remove("PYTHONPATH").is_some());
    } else {
        assert!(!common.contains_key("PYTHONPATH"));
        if device == "cuda" {
            let library = common.remove("LD_LIBRARY_PATH").unwrap();
            assert_eq!(
                library,
                root.join("xdg-data/mluva/gpu-runtime/lib")
                    .to_str()
                    .unwrap()
            );
            let cache = PathBuf::from(common.remove("CUDA_CACHE_PATH").unwrap());
            assert_eq!(
                fs::metadata(cache.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(cache).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
    let allowed = [
        "PATH",
        "LANG",
        "SYSTEMROOT",
        "HF_HUB_OFFLINE",
        "TRANSFORMERS_OFFLINE",
        "OMP_NUM_THREADS",
        "OPENBLAS_NUM_THREADS",
    ];
    assert!(
        common.keys().all(|key| allowed.contains(&key.as_str())),
        "worker inherits no private account/control environment"
    );
    append(
        &root,
        json!({"kind":"start","pid":std::process::id(),"arguments":[model,path.replace(root.to_str().unwrap(),"$ROOT"),device],"environment":common,"stderr_null":fs::read_link("/proc/self/fd/2").unwrap()==Path::new("/dev/null")}),
    );
    if config["exit_start"] == true {
        return;
    }
    if let Some(delay) = config["startup_delay_ms"].as_u64() {
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut count = 0;
    loop {
        let mut raw = Vec::new();
        if input.read_until(b'\n', &mut raw).unwrap() == 0 {
            return;
        }
        let normalized = String::from_utf8(raw.clone())
            .unwrap()
            .replace(root.to_str().unwrap(), "$ROOT");
        let payload: Value = serde_json::from_slice(normalized.as_bytes()).unwrap();
        append(
            &root,
            json!({"kind":"request","payload":payload,"body_sha256":hash(normalized.as_bytes()),"body_bytes":normalized.len()}),
        );
        let responses = config["responses"].as_array().unwrap();
        let response = &responses[count.min(responses.len() - 1)];
        count += 1;
        if let Some(delay) = response["delay_ms"].as_u64() {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        let memory = if let Some(bytes) = response["memory_bytes"].as_u64() {
            append(&root, json!({"kind":"allocation","bytes":bytes}));
            Some(vec![1_u8; bytes as usize])
        } else {
            None
        };
        std::hint::black_box(&memory);
        if response["hold"] == true || (first_process && response["hold_first_process"] == true) {
            std::future::pending::<()>().await;
        }
        let mut bytes = if let Some(hex) = response["hex"].as_str() {
            support::unhex(hex)
        } else if let Some(raw) = response["raw"].as_str() {
            raw.as_bytes().to_vec()
        } else if let Some(count) = response["repeat_text"].as_u64() {
            serde_json::to_vec(
                &json!({"text":response["unit"].as_str().unwrap_or("X").repeat(count as usize)}),
            )
            .unwrap()
        } else {
            serde_json::to_vec(&response["json"]).unwrap()
        };
        if response["newline"] != false {
            bytes.push(b'\n');
        }
        let mut out = std::io::stdout().lock();
        for part in bytes.chunks(response["fragment"].as_u64().unwrap_or(8191).max(1) as usize) {
            out.write_all(part).unwrap();
            out.flush().unwrap();
        }
        drop(memory);
        if response["hold_after_output"] == true {
            std::future::pending::<()>().await;
        }
        if response["exit_after"] == true {
            return;
        }
    }
}
fn observed(result: mluva_providers::Result<mluva_providers::TranscriptionResult>) -> Value {
    match result {
        Ok(value) => {
            json!({"ok":{"text":text(&value.text),"language_code":value.language_code,"language_probability":value.language_probability,"transcription_id":value.transcription_id,"speaker_segments":value.speaker_segments,"audio_duration_seconds":value.audio_duration_seconds}})
        }
        Err(error) => json!({"error":error.to_string()}),
    }
}
async fn wait_trace(root: &Path, kind: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !trace(root).iter().any(|v| v["kind"] == kind) {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
async fn driver(spec: Value, root: &Path) {
    let mut options = OnnxOptions::new(
        root.join("xdg-data/mluva"),
        spec["model"].as_str().unwrap(),
        root.join("worker"),
    );
    options.device = spec["device"].as_str().unwrap_or("cpu").into();
    options.keep_alive = spec["keep_alive"] == true;
    let client = Arc::new(LocalSpeechClient::new(options).unwrap());
    let mut results = Vec::new();
    let mut states = Vec::new();
    for call in spec["calls"].as_array().unwrap() {
        let started = std::time::Instant::now();
        match call["action"].as_str().unwrap_or("transcribe") {
            "close" => {
                client.close().await;
                results.push(json!({"ok":null}));
            }
            "cancel" => {
                client.cancel().await;
                results.push(json!({"ok":null}));
            }
            "cancel_pair" => {
                let (a, b) = tokio::join!(
                    async {
                        client.cancel().await;
                        state(root)
                    },
                    async {
                        client.cancel().await;
                        state(root)
                    }
                );
                results.push(json!({"ok":[a,b]}));
            }
            action @ ("close_loading" | "cancel_inference" | "drop_request") => {
                let worker = client.clone();
                let path = root.join("audio.wav");
                let task =
                    tokio::spawn(async move { worker.transcribe(&path, "auto", None).await });
                wait_trace(
                    root,
                    if action == "close_loading" {
                        "start"
                    } else {
                        "request"
                    },
                )
                .await;
                let before = std::time::Instant::now();
                if action == "drop_request" {
                    task.abort();
                    assert!(task.await.unwrap_err().is_cancelled());
                    tokio::time::timeout(Duration::from_secs(5), async {
                        while state(root)["alive"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|value| value == true)
                            || fs::read_dir(root.join("tmp")).unwrap().count() != 0
                        {
                            tokio::time::sleep(Duration::from_millis(2)).await;
                        }
                    })
                    .await
                    .expect("dropping the request alone reaps the owned child and CUDA cache");
                    results.push(
                        json!({"aborted":true,"quick":before.elapsed()<Duration::from_secs(1)}),
                    );
                } else {
                    if action == "cancel_inference" {
                        client.cancel().await;
                    } else {
                        client.close().await;
                    }
                    let result = task.await.unwrap();
                    assert!(result.is_err());
                    results.push(
                        json!({"interrupted":true,"quick":before.elapsed()<Duration::from_secs(1)}),
                    );
                }
            }
            _ => {
                let path = root.join(call["path"].as_str().unwrap_or("audio.wav"));
                let mut callbacks = 0;
                let mut callback = |_: String| {
                    callbacks += 1;
                };
                results.push(observed(
                    client
                        .transcribe(
                            &path,
                            call["language"].as_str().unwrap_or("auto"),
                            Some(&mut callback),
                        )
                        .await,
                ));
                assert_eq!(callbacks, 0, "ONNX returns no provisional callbacks");
            }
        }
        if spec["deadline_fault"] == true {
            results.last_mut().unwrap()["deadline_window"] =
                json!((177.0..=190.0).contains(&started.elapsed().as_secs_f64()));
        }
        states.push(state(root));
    }
    client.close().await;
    let mut trace = trace(root);
    for row in &mut trace {
        if row["kind"] == "start" {
            row.as_object_mut().unwrap().shift_remove("pid");
        }
    }
    println!(
        "{}",
        json!({"results":results,"states":states,"closed":state(root),"trace":trace})
    );
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if !args.is_empty() {
        runtime(&args).await;
        return;
    }
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    let spec = serde_json::from_slice(&input).unwrap();
    let root = PathBuf::from(std::env::var_os("LOCAL_ASR_FIXTURE_ROOT").unwrap());
    driver(spec, &root).await;
}
