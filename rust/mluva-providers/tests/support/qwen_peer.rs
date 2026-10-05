//! External synthetic Qwen runtime and public native client driver; never installed.
use mluva_providers::{
    local_assets::QWEN_RUNTIME,
    qwen::{QwenOptions, QwenSpeechClient},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
#[path = "mod.rs"]
mod support;

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn text(value: &str) -> Value {
    if value.chars().count() < 4096 {
        json!(value)
    } else {
        json!({"characters":value.chars().count(),"sha256":hash(value.as_bytes())})
    }
}
fn append(path: &Path, value: Value) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    file.write_all(&bytes).unwrap();
}
fn normalized(value: &str, root: &Path, key: &Path, port: &str) -> String {
    if value == port {
        return "$PORT".into();
    }
    value
        .replace(key.parent().unwrap().to_str().unwrap(), "$TEMP")
        .replace(root.to_str().unwrap(), "$ROOT")
}
fn body(value: &Value) -> Vec<u8> {
    if let Some(hex) = value["body_hex"].as_str() {
        return support::unhex(hex);
    }
    let events = value["events"].as_array().unwrap();
    let mut bytes = vec![];
    for event in events {
        if let Some(raw) = event["raw"].as_str() {
            bytes.extend_from_slice(raw.as_bytes());
        } else if let Some(size) = event["comment_size"].as_u64() {
            bytes.push(b':');
            bytes.extend(std::iter::repeat_n(b'X', size as usize - 2));
            bytes.push(b'\n');
        } else {
            bytes.extend_from_slice(b"data: ");
            bytes.extend_from_slice(serde_json::to_string(event).unwrap().as_bytes());
            bytes.extend_from_slice(b"\n\n");
        }
    }
    if value["done"] != false {
        bytes.extend_from_slice(
            value["done_text"]
                .as_str()
                .unwrap_or("data: [DONE]\n")
                .as_bytes(),
        );
    }
    bytes
}
async fn runtime(arguments: &[String]) {
    // A resident model outlives the thread that created it. PDEATHSIG follows
    // that thread on Linux, so it would kill the fixture at preview Stop even
    // while the recording's actual owner still needs final recognition.
    let executable = std::env::current_exe().unwrap();
    let config: Value =
        serde_json::from_slice(&fs::read(executable.with_file_name("fixture.json")).unwrap())
            .unwrap();
    let root = PathBuf::from(config["root"].as_str().unwrap());
    let trace = root.join("trace.jsonl");
    if arguments == ["--list-devices"] {
        append(
            &trace,
            json!({"kind":"probe","pid":std::process::id(),"environment":std::env::vars().map(|(key,value)|(key,value.replace(root.to_str().unwrap(),"$ROOT"))).collect::<std::collections::BTreeMap<_,_>>() }),
        );
        if let Some(delay) = config["probe_delay_ms"].as_u64() {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        if let Some(bytes) = config["probe_stderr_hex"].as_str() {
            std::io::stderr().write_all(&support::unhex(bytes)).unwrap();
        }
        if let Some(bytes) = config["probe_stdout_hex"].as_str() {
            std::io::stdout().write_all(&support::unhex(bytes)).unwrap();
        } else {
            print!(
                "{}",
                config["devices"]
                    .as_str()
                    .unwrap_or("Vulkan0: NVIDIA Fixture\n")
            );
        }
        std::io::stdout().flush().unwrap();
        std::process::exit(config["probe_exit"].as_i64().unwrap_or(0) as i32);
    }
    let argument = |name: &str| {
        arguments[arguments.iter().position(|value| value == name).unwrap() + 1].clone()
    };
    let key = PathBuf::from(argument("--api-key-file"));
    let token = fs::read_to_string(&key).unwrap();
    let port = argument("--port");
    let first_process = !fs::read_to_string(&trace)
        .unwrap_or_default()
        .lines()
        .any(|line| serde_json::from_str::<Value>(line).unwrap()["kind"] == "start");
    append(
        &trace,
        json!({"kind":"start","pid":std::process::id(),"key_path":key,"arguments":arguments.iter().map(|value|normalized(value,&root,&key,&port)).collect::<Vec<_>>(),"environment":std::env::vars().map(|(key,value)|(key,value.replace(root.to_str().unwrap(),"$ROOT"))).collect::<std::collections::BTreeMap<_,_>>(),"token_length":token.len(),"token_urlsafe":token.bytes().all(|byte|byte.is_ascii_alphanumeric()||b"-_".contains(&byte)),"key_mode":fs::metadata(&key).unwrap().permissions().mode()&0o777,"temporary_mode":fs::metadata(key.parent().unwrap()).unwrap().permissions().mode()&0o777,"stdin_eof":std::io::stdin().read(&mut[0]).unwrap()==0,"stdout_null":fs::read_link("/proc/self/fd/1").unwrap()==Path::new("/dev/null"),"stderr_null":fs::read_link("/proc/self/fd/2").unwrap()==Path::new("/dev/null")}),
    );
    if config["ignore_term"] == true {
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
    }
    if config["exit_start"] == true {
        return;
    }
    let listener = TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let mut requests = 0;
    let mut health = 0;
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = support::request(&mut socket).await;
        let mut response = support::Response::ok(Vec::<u8>::new());
        let mut fragment_delay = Duration::ZERO;
        if request.path == "/health" {
            let status = config["health"]
                .as_array()
                .and_then(|values| values.get(health).or_else(|| values.last()))
                .and_then(Value::as_u64)
                .unwrap_or(200) as u16;
            health += 1;
            response.status = status;
            append(
                &trace,
                json!({"kind":"health","status":status,"authenticated":request.headers.contains_key("authorization")}),
            );
        } else {
            assert_eq!(request.path, "/v1/chat/completions");
            let mut payload: Value = serde_json::from_slice(&request.body).unwrap();
            let audio = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                payload["messages"][0]["content"][0]["input_audio"]["data"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            payload["messages"][0]["content"][0]["input_audio"]["data"] = json!({"bytes":audio.len(),"sha256":hash(&audio),"header_hex":audio[..44].iter().map(|byte|format!("{byte:02x}")).collect::<String>()});
            append(
                &trace,
                json!({"kind":"request","method":request.method,"path":request.path,"content_type":request.headers["content-type"],"authenticated":request.headers.get("authorization")==Some(&format!("Bearer {token}")),"body_sha256":hash(&request.body),"payload":payload}),
            );
            let specification = &config["responses"]
                [requests.min(config["responses"].as_array().unwrap().len() - 1)];
            requests += 1;
            let memory = if let Some(bytes) = specification["memory_bytes"].as_u64() {
                append(&trace, json!({"kind":"allocation","bytes":bytes}));
                Some(vec![1_u8; bytes as usize])
            } else {
                None
            };
            std::hint::black_box(&memory);
            if specification["hold"] == true
                || (first_process && specification["hold_first_process"] == true)
            {
                std::future::pending::<()>().await;
            }
            response.status = specification["status"].as_u64().unwrap_or(200) as u16;
            response.body = body(specification);
            response.fragment = specification["fragment"].as_u64().unwrap_or(8191) as usize;
            fragment_delay =
                Duration::from_millis(specification["fragment_delay_ms"].as_u64().unwrap_or(0));
            if let Some(redirect) = specification["redirect"].as_str() {
                response
                    .headers
                    .push(("Location".into(), redirect.replace("$PORT", &port)));
            }
            if let Some(delay) = specification["delay_ms"].as_u64() {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        }
        let head = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
            response.status,
            reqwest::StatusCode::from_u16(response.status)
                .unwrap()
                .canonical_reason()
                .unwrap(),
            response.body.len(),
            response
                .headers
                .iter()
                .map(|(key, value)| format!("{key}: {value}\r\n"))
                .collect::<String>()
        );
        if socket.write_all(head.as_bytes()).await.is_err() {
            continue;
        }
        for (index, chunk) in response.body.chunks(response.fragment.max(1)).enumerate() {
            if socket.write_all(chunk).await.is_err() {
                break;
            }
            if config["observe_fragments"] == true {
                append(
                    &trace,
                    json!({"kind":"fragment","request":requests,"index":index,"bytes":chunk.len()}),
                );
            }
            tokio::task::yield_now().await;
            if !fragment_delay.is_zero() {
                tokio::time::sleep(fragment_delay).await;
            }
        }
        let _ = socket.shutdown().await;
    }
}
fn starts(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &Value| value["kind"] == "start")
        .collect()
}
fn state(root: &Path) -> Value {
    let values = starts(root);
    let probes: Vec<Value> = fs::read_to_string(root.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &Value| value["kind"] == "probe")
        .collect();
    json!({"processes":values.len(),"alive":values.iter().map(|value|Path::new("/proc").join(value["pid"].as_u64().unwrap().to_string()).exists()).collect::<Vec<_>>(),"keys_exist":values.iter().map(|value|Path::new(value["key_path"].as_str().unwrap()).exists()).collect::<Vec<_>>(),"probes":probes.len(),"probe_alive":probes.iter().map(|value|Path::new("/proc").join(value["pid"].as_u64().unwrap().to_string()).exists()).collect::<Vec<_>>()})
}
async fn transcribe(client: &QwenSpeechClient, root: &Path, call: &Value) -> (Value, Vec<Value>) {
    let mut observed = vec![];
    let mut callback = |value: String| {
        observed.push(text(&value));
        if call["cancel_at_partial"]
            .as_u64()
            .is_some_and(|count| observed.len() as u64 >= count)
        {
            drop(client.cancel());
        }
    };
    let path = root.join(call["path"].as_str().unwrap_or("audio.wav"));
    let result = client
        .transcribe(
            &path,
            call["language"].as_str().unwrap_or("auto"),
            if call["partials"] == false {
                None
            } else {
                Some(&mut callback)
            },
        )
        .await;
    let result = match result {
        Ok(result) => {
            json!({"ok":{"text":text(&result.text),"language_code":result.language_code,"language_probability":result.language_probability,"transcription_id":result.transcription_id,"speaker_segments":result.speaker_segments,"audio_duration_seconds":result.audio_duration_seconds}})
        }
        Err(error) => json!({"error":error.to_string()}),
    };
    (result, observed)
}
async fn driver(spec: Value, root: &Path) {
    let mut options = QwenOptions::new(root.join("xdg-data/mluva"));
    options.device = spec["device"].as_str().unwrap_or("cpu").into();
    options.keep_alive = spec["keep_alive"] == true;
    let mut client = Arc::new(QwenSpeechClient::new(options.clone()).unwrap());
    let mut results: Vec<Value> = vec![];
    let mut states = vec![];
    let mut partials = vec![];
    let mut stream_observations = vec![];
    for call in spec["calls"].as_array().unwrap() {
        match call["action"].as_str().unwrap_or("transcribe") {
            "cancel_incomplete_stream" => {
                let worker = client.clone();
                let working_root = root.to_owned();
                let fragments = || {
                    fs::read_to_string(root.join("trace.jsonl"))
                        .unwrap_or_default()
                        .lines()
                        .filter(|line| {
                            serde_json::from_str::<Value>(line).unwrap()["kind"] == "fragment"
                        })
                        .count()
                };
                let fragments_before = fragments();
                let mut working =
                    tokio::spawn(
                        async move { transcribe(&worker, &working_root, &json!({})).await },
                    );
                tokio::time::timeout(Duration::from_secs(5), async {
                    while fragments() == fragments_before {
                        assert!(
                            !working.is_finished(),
                            "actual incomplete stream was reached"
                        );
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("runtime writes an actual response fragment");
                let before = std::time::Instant::now();
                let wait = Duration::from_secs(call["observe_seconds"].as_u64().unwrap());
                let completed = if call["wait_for_timeout"] == true {
                    Some(
                        tokio::time::timeout(wait, &mut working)
                            .await
                            .expect(
                                "incomplete stream must return within its real 180-second deadline",
                            )
                            .unwrap(),
                    )
                } else {
                    tokio::time::sleep(wait).await;
                    None
                };
                let seconds = before.elapsed().as_secs_f64();
                let received = fragments() - fragments_before;
                let pending = !working.is_finished();
                let state_before = state(root);
                let before = std::time::Instant::now();
                tokio::time::timeout(Duration::from_secs(5), client.cancel())
                    .await
                    .expect("public cancellation reaps the owned runtime");
                let (mut result, observed) = match completed {
                    Some(result) => result,
                    None => tokio::time::timeout(Duration::from_secs(5), working)
                        .await
                        .expect("cancelled transcription returns")
                        .unwrap(),
                };
                stream_observations.push(json!({"seconds_from_first_fragment":seconds,"fragments":received,
                    "cancellation_seconds":before.elapsed().as_secs_f64(),
                    "temporary_entries_after_cancel":fs::read_dir(root.join("tmp")).unwrap().count()}));
                result["pending_before_cancel"] = json!(pending);
                result["state_before_cancel"] = state_before;
                results.push(result);
                partials.push(observed);
                states.push(state(root));
                // Only the external runtime changes after cancellation. A new
                // public client must recognize successfully with the same settings.
                let config_path = QWEN_RUNTIME
                    .binary(&options.data_dir, &options.device)
                    .with_file_name("fixture.json");
                let mut config: Value =
                    serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
                config["responses"] = json!([call["recovery_response"]]);
                fs::write(config_path, serde_json::to_vec(&config).unwrap()).unwrap();
                client = Arc::new(QwenSpeechClient::new(options.clone()).unwrap());
                let (result, observed) = tokio::time::timeout(
                    Duration::from_secs(5),
                    transcribe(&client, root, &json!({})),
                )
                .await
                .expect("fresh capture completes");
                results.push(result);
                partials.push(observed);
            }
            "park_for_parent_crash" => {
                assert_eq!(results.last().unwrap()["ok"]["text"], "hello");
                fs::write(root.join("parent-crash-ready"), b"ready").unwrap();
                std::future::pending::<()>().await;
            }
            "close_during_startup" => {
                let worker = client.clone();
                let path = root.join("audio.wav");
                let working =
                    tokio::spawn(async move { worker.transcribe(&path, "auto", None).await });
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        if fs::read_to_string(root.join("trace.jsonl"))
                            .unwrap_or_default()
                            .lines()
                            .any(|line| {
                                serde_json::from_str::<Value>(line).unwrap()["kind"] == "health"
                            })
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("fixture reached startup health");
                let before = std::time::Instant::now();
                client.close().await;
                let quick = before.elapsed() < Duration::from_secs(1);
                let error = working
                    .await
                    .unwrap()
                    .expect_err("closed startup cannot succeed");
                results.push(json!({"error":error.to_string(),"close_quick":quick}));
                partials.push(vec![]);
            }
            "cancel_pair" => {
                let (first, second) = tokio::join!(
                    async {
                        client.cancel().await;
                        state(root)
                    },
                    async {
                        client.cancel().await;
                        state(root)
                    }
                );
                results.push(json!({"ok":[first,second]}));
                partials.push(vec![]);
            }
            "cancel" => {
                client.cancel().await;
                results.push(json!({"ok":null}));
                partials.push(vec![]);
            }
            "close" => {
                client.close().await;
                results.push(json!({"ok":null}));
                partials.push(vec![]);
            }
            _ => {
                let (result, observed) = transcribe(&client, root, call).await;
                results.push(result);
                partials.push(observed);
            }
        }
        states.push(state(root));
    }
    client.close().await;
    let mut trace: Vec<Value> = fs::read_to_string(root.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for value in &mut trace {
        if value["kind"] == "start" {
            value.as_object_mut().unwrap().shift_remove("pid");
            value.as_object_mut().unwrap().shift_remove("key_path");
        } else if value["kind"] == "probe" {
            value.as_object_mut().unwrap().shift_remove("pid");
        }
    }
    let cache = root.join("xdg-data/mluva/qwen-cache");
    let mut output = json!({"results":results,"partials":partials,"states":states,"closed":state(root),"trace":trace,"cache_mode":fs::metadata(cache).ok().map(|metadata|metadata.permissions().mode()&0o777)});
    if !stream_observations.is_empty() {
        output["stream_observations"] = json!(stream_observations);
    }
    println!("{output}");
}

fn observe_parent_crash(root: &Path) {
    use std::os::fd::AsRawFd;
    use std::process::{Command, Stdio};
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
        0
    );
    let device = std::env::var("QWEN_CRASH_DEVICE").unwrap();
    let phase = std::env::var("QWEN_CRASH_PHASE").unwrap();
    let spec = json!({"device":device,"keep_alive":true,"calls":[
        {"action":"transcribe"},{"action":"park_for_parent_crash"}
    ]});
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    parent
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&spec).unwrap())
        .unwrap();
    let mut owned = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let before = std::time::Instant::now();
        let stage_ready = || {
            if phase == "ready" {
                root.join("parent-crash-ready").exists()
            } else {
                fs::read_to_string(root.join("trace.jsonl"))
                    .unwrap_or_default()
                    .lines()
                    .any(|line| serde_json::from_str::<Value>(line).unwrap()["kind"] == "probe")
            }
        };
        while !stage_ready() {
            assert!(
                parent.try_wait().unwrap().is_none(),
                "client exited before its verified request"
            );
            assert!(before.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        let started: Value = fs::read_to_string(root.join("trace.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|value| value["kind"] == if phase == "ready" { "start" } else { "probe" })
            .unwrap();
        let pid = started["pid"].as_u64().unwrap() as libc::pid_t;
        owned = Some(pid);
        let key = started["key_path"].as_str().map(PathBuf::from);
        let anonymous = key.as_ref().is_some_and(|key| {
            let file = fs::File::open(key).unwrap();
            let seals = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GET_SEALS) };
            let required =
                libc::F_SEAL_SEAL | libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_WRITE;
            // Drop this descriptor before crashing the credential's real owner.
            fs::read_link(key).is_ok() && seals >= 0 && seals & required == required
        });
        parent.kill().unwrap();
        assert!(!parent.wait().unwrap().success());
        let before = std::time::Instant::now();
        let mut status = 0;
        loop {
            let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            assert!(reaped >= 0);
            if reaped == pid {
                break;
            }
            assert!(
                before.elapsed() < Duration::from_secs(2),
                "owned model outlived its crashed parent"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        owned = None;
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
        assert!(
            key.as_ref().is_none_or(|key| !key.exists()),
            "no readable key survives its owner"
        );
        assert!(
            phase != "ready" || anonymous,
            "temporary authentication uses immutable anonymous memory"
        );
        println!(
            "{}",
            json!({"phase":phase,"worker_killed_after_parent_crash":true,"key_revoked":true,
            "anonymous_key_sealed":anonymous,"normal_request_completed":phase == "ready"})
        );
    }));
    let _ = parent.kill();
    let _ = parent.wait();
    if let Some(pid) = owned {
        let args = fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        if args
            .split(|byte| *byte == 0)
            .any(|arg| arg.starts_with(root.as_os_str().as_encoded_bytes()))
        {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, std::ptr::null_mut(), 0);
            }
        }
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--observe-parent-crash"] {
        let root = PathBuf::from(std::env::var_os("QWEN_FIXTURE_ROOT").unwrap());
        observe_parent_crash(&root);
        return;
    }
    if !arguments.is_empty() {
        runtime(&arguments).await;
        return;
    }
    let mut bytes = vec![];
    std::io::stdin().read_to_end(&mut bytes).unwrap();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();
    let root = PathBuf::from(std::env::var_os("QWEN_FIXTURE_ROOT").unwrap());
    driver(spec, &root).await;
}
