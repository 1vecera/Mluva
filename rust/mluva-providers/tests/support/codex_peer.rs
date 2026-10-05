//! Native external protocol peer and isolated public-client driver; never installed.
use mluva_core::{config::AppConfig, screenshots::ImageInput};
use mluva_providers::{
    codex::{CodexAppServerClient, CodexOptions},
    compatible::RewriteOptions,
    rewriting::RewriteClient,
};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn record(path: &Path, value: &Value) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    let mut bytes = serde_json::to_vec(value).unwrap();
    bytes.push(b'\n');
    file.write_all(&bytes).unwrap();
}
fn send(value: Value) {
    println!("{value}");
    io::stdout().flush().unwrap();
}
fn result<T: serde::Serialize, E: std::fmt::Display>(value: Result<T, E>) -> Value {
    match value {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn text(kind: &str, value: Value) -> Value {
    json!({"method":kind,"params":value})
}
fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|value| u8::from_str_radix(std::str::from_utf8(value).unwrap(), 16).unwrap())
        .collect()
}

fn server(spec: &Value) {
    let evidence = PathBuf::from(spec["evidence"].as_str().unwrap());
    let cwd = std::env::current_dir().unwrap();
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".codex"));
    let mut keys = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    keys.sort();
    record(
        &evidence.join("process.jsonl"),
        &json!({"pid":std::process::id(),"cwd":cwd,"mode":fs::metadata(&cwd).unwrap().permissions().mode()&0o777,"argv":std::env::args().skip(1).collect::<Vec<_>>(),"environment":keys,"instructions":[fs::read_to_string(home.join("AGENTS.md")).ok(),fs::read_to_string(home.join("AGENTS.override.md")).ok()]}),
    );
    if spec["scenario"] == "ignore-term" {
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
    }
    if spec["scenario"] == "stderr" {
        let noise = vec![b'x'; 2_000_000];
        io::stderr().write_all(&noise).unwrap();
        io::stderr().flush().unwrap();
    }
    let scenario = spec["scenario"].as_str().unwrap_or("clean");
    let escaped_cwd = serde_json::to_string(&cwd).unwrap();
    let escaped_cwd = &escaped_cwd[1..escaped_cwd.len() - 1];
    let mut pages = 0;
    let input = io::stdin();
    let mut lines = input.lock().lines();
    while let Some(line) = lines.next() {
        let line = line.unwrap();
        let message: Value = serde_json::from_str(&line).unwrap();
        record(
            &evidence.join("requests.jsonl"),
            &json!({"raw":line.replace(escaped_cwd,"<workspace>"),"message":message}),
        );
        let method = message["method"].as_str().unwrap_or("");
        let id = &message["id"];
        if method == "initialized" {
            continue;
        }
        if scenario == "reject" {
            send(json!({"id":id,"error":{"message":"synthetic-private-service-response"}}));
            continue;
        }
        if scenario == "stall" || scenario == "ignore-term" {
            continue;
        }
        if scenario == "malformed" {
            println!("not JSON");
            io::stdout().flush().unwrap();
            continue;
        }
        let answer = match method {
            "initialize" => json!({"userAgent":"synthetic-codex/1.0"}),
            "model/list" => {
                if let Some(gate) = spec["model_gate"].as_str() {
                    while !Path::new(gate).exists() {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                let catalog=spec.get("catalog").cloned().unwrap_or_else(||json!([{"id":"fixture-default","model":"fixture-model","displayName":"Fixture","isDefault":true,"supportedReasoningEfforts":[{"reasoningEffort":"low"},{"reasoningEffort":"high"}],"serviceTiers":[{"id":"priority","name":"Fast"}]},{"id":"fixture-explicit","model":"second-model","displayName":"Second","isDefault":false,"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"low"}],"hidden":true}]));
                pages += 1;
                let total = spec["pages"].as_u64().unwrap_or(1);
                json!({"data":catalog,"nextCursor":if pages<total {json!(format!("page-{pages}"))}else{Value::Null}})
            }
            "config/read" => {
                json!({"config":{"mcp_servers":if scenario=="bad-config" {json!([])}else{json!({"fixture":{"command":"never-run"},"other":{"url":"never-contact"}})}}})
            }
            "thread/start" => {
                let model = if scenario == "changed-model" {
                    json!("unexpected-model")
                } else {
                    message["params"]["model"].clone()
                };
                json!({"model":model,"thread":{"id":"thread-test","environments":if scenario=="old-server" {Value::Null}else{json!([])},"ephemeral":scenario!="not-ephemeral"},"instructionSources":if scenario=="instructions" {json!(["private-file"])}else{json!([])}})
            }
            "mcpServerStatus/list" => {
                json!({"data":if scenario=="exposed-mcp" {json!([{"tools":{"unexpected":{}}}])}else{json!([])},"nextCursor":if scenario=="inventory-cursor" {json!("next")}else{Value::Null}})
            }
            "turn/start" => {
                if scenario == "unexpected-request" {
                    send(json!({"id":id,"method":"item/permissions/requestApproval","params":{}}));
                    // Confirm refusal before completing the pending RPC. Process shutdown
                    // cannot race the peer's observation of the required error frame.
                    let reply = lines.next().unwrap().unwrap();
                    let value: Value = serde_json::from_str(&reply).unwrap();
                    assert_eq!(value["error"]["code"], -32601);
                    record(
                        &evidence.join("requests.jsonl"),
                        &json!({"raw":reply.replace(escaped_cwd,"<workspace>"),"message":value}),
                    );
                }
                let answer = json!({"turn":{"id":"turn-test"}});
                send(json!({"id":id,"result":answer}));
                if scenario == "exit-during-turn" {
                    return;
                }
                if scenario == "turn-stall" {
                    continue;
                }
                let prepared = message["params"]["input"][0]["text"]
                    .as_str()
                    .unwrap()
                    .rsplit_once("DICTATION:\n")
                    .map(|(_, text)| text);
                let live_key = message["params"]["input"][0]["text"]
                    .as_str()
                    .and_then(|prompt| {
                        let (header, context) = prompt.split_once('\n')?;
                        if !header
                            .starts_with("You are an editor updating a draft as someone dictates.")
                        {
                            return None;
                        }
                        let context: Value = serde_json::Deserializer::from_str(context)
                            .into_iter()
                            .next()?
                            .ok()?;
                        Some(format!(
                            "{}|{}",
                            if context["transcript_status"] == "final committed recognition" {
                                "final"
                            } else {
                                "preview"
                            },
                            context["transcript"].as_str()?
                        ))
                    });
                let control = prepared
                    .and_then(|text| spec["segment_controls"].get(text))
                    .or_else(|| {
                        live_key
                            .as_ref()
                            .and_then(|key| spec["live_controls"].get(key))
                    })
                    .or_else(|| {
                        let prompt = message["params"]["input"][0]["text"].as_str()?;
                        let (header, context) = prompt.split_once('\n')?;
                        if !header.starts_with("You are a writing editor.") {
                            return None;
                        }
                        let context: Value = serde_json::Deserializer::from_str(context)
                            .into_iter()
                            .next()?
                            .ok()?;
                        spec["document_controls"].get(context["next_instruction"].as_str()?)
                    })
                    .or_else(|| {
                        let prompt = message["params"]["input"][0]["text"].as_str()?;
                        let (_, context) = prompt.split_once('\n')?;
                        let context: Value = serde_json::Deserializer::from_str(context)
                            .into_iter()
                            .next()?
                            .ok()?;
                        spec["title_controls"].get(context["transcript_excerpt"].as_str()?)
                    });
                if let Some(gate) = control.and_then(|value| value["gate"].as_str()) {
                    while !Path::new(gate).exists() {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                if scenario.starts_with("tool-") {
                    send(text(
                        "item/started",
                        json!({"threadId":"thread-test","turnId":"turn-test","item":{"type":scenario.strip_prefix("tool-").unwrap()}}),
                    ));
                }
                if scenario == "tool-method" {
                    send(text(
                        "item/commandExecution/outputDelta",
                        json!({"threadId":"thread-test","turnId":"turn-test","delta":"secret"}),
                    ));
                }
                send(text(
                    "item/agentMessage/delta",
                    json!({"threadId":"wrong-thread","turnId":"turn-test","delta":"IGNORE"}),
                ));
                send(text(
                    "item/agentMessage/delta",
                    json!({"threadId":"thread-test","turnId":"wrong-turn","delta":"IGNORE"}),
                ));
                let deltas = control
                    .and_then(|value| value.get("deltas"))
                    .or_else(|| spec.get("deltas"))
                    .cloned()
                    .unwrap_or_else(|| match scenario {
                        "oversized" => json!(["x".repeat(8001)]),
                        "document" => json!(["x".repeat(9000)]),
                        "malformed-delta" => json!([42]),
                        "empty" => json!([" \u{1f}\t "]),
                        _ => json!(["Clean ", "text. 🙂"]),
                    });
                for delta in deltas.as_array().unwrap() {
                    send(text(
                        "item/agentMessage/delta",
                        json!({"threadId":"thread-test","turnId":"turn-test","itemId":"item-test","delta":delta}),
                    ));
                    if let Some(delay) = control.and_then(|value| value["delta_delay_ms"].as_u64())
                    {
                        std::thread::sleep(Duration::from_millis(delay));
                    }
                }
                if let Some(gate) = control.and_then(|value| value["completion_gate"].as_str()) {
                    while !Path::new(gate).exists() {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                let mut turn = json!({"id":"turn-test","status":spec.get("status").cloned().unwrap_or_else(||json!("completed"))});
                if scenario == "completed-tool" {
                    turn["items"] = json!([{"type":"fileChange"}]);
                }
                send(text(
                    "turn/completed",
                    json!({"threadId":"thread-test","turn":turn}),
                ));
                if scenario == "single-turn" {
                    return;
                }
                continue;
            }
            _ => return,
        };
        send(json!({"id":id,"result":answer}));
    }
}

fn observe_parent_crash(path: &Path) {
    use std::process::{Command, Stdio};
    use std::time::Instant;
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
        0
    );
    let spec: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let evidence = PathBuf::from(spec["evidence"].as_str().unwrap());
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["drive", path.to_str().unwrap()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut owned = None;
    let mut cleanup = None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let before = Instant::now();
        while !fs::read_to_string(evidence.join("requests.jsonl"))
            .unwrap_or_default()
            .contains("turn/start")
        {
            assert!(parent.try_wait().unwrap().is_none());
            assert!(before.elapsed() < Duration::from_secs(8));
            std::thread::sleep(Duration::from_millis(2));
        }
        let process: Value = serde_json::from_str(
            fs::read_to_string(evidence.join("process.jsonl"))
                .unwrap()
                .lines()
                .last()
                .unwrap(),
        )
        .unwrap();
        let pid = process["pid"].as_u64().unwrap() as libc::pid_t;
        owned = Some(pid);
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        let workspace = Path::new(process["cwd"].as_str().unwrap());
        assert!(workspace.is_dir());
        let memory = mluva_audio::volatile::memory_backed(workspace);
        let private = workspace.metadata().unwrap().permissions().mode() & 0o777 == 0o700;
        fs::write(
            workspace.join("private-image-canary.png"),
            b"private test image",
        )
        .unwrap();
        let foreign = evidence.join("unrelated-image.png");
        fs::write(&foreign, b"preserve unrelated bytes").unwrap();
        std::os::unix::fs::symlink(&foreign, workspace.join("foreign-image.png")).unwrap();
        for task in fs::read_dir(format!("/proc/{}/task", parent.id())).unwrap() {
            let task = task.unwrap().path();
            for child in fs::read_to_string(task.join("children"))
                .unwrap()
                .split_whitespace()
            {
                let child = child.parse::<libc::pid_t>().unwrap();
                let arguments = fs::read(format!("/proc/{child}/cmdline")).unwrap_or_default();
                let arguments: Vec<_> = arguments
                    .split(|byte| *byte == 0)
                    .filter(|part| !part.is_empty())
                    .collect();
                if arguments.len() == 2 && arguments[1] == workspace.as_os_str().as_encoded_bytes()
                {
                    assert!(cleanup.replace(child).is_none());
                }
            }
        }
        parent.kill().unwrap();
        assert!(!parent.wait().unwrap().success());
        let before = Instant::now();
        let mut status = 0;
        loop {
            let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
            assert!(reaped >= 0);
            if reaped == pid {
                break;
            }
            assert!(
                before.elapsed() < Duration::from_secs(2),
                "held rewrite outlived its crashed client"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
        owned = None;
        let before = Instant::now();
        while workspace.exists() {
            assert!(
                before.elapsed() < Duration::from_secs(2),
                "private workspace outlived its crashed client"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(fs::read(&foreign).unwrap(), b"preserve unrelated bytes");
        let janitor = cleanup.expect("workspace cleanup needs an independent owner");
        assert!(memory && private);
        let before = Instant::now();
        loop {
            let reaped = unsafe { libc::waitpid(janitor, &mut status, libc::WNOHANG) };
            assert!(reaped >= 0);
            if reaped == janitor {
                break;
            }
            assert!(before.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 0);
        assert!(!Path::new(&format!("/proc/{janitor}")).exists());
        cleanup = None;
        println!(
            "{}",
            json!({"held_rewrite_reaped_after_client_crash":true,"private_workspace_removed":true})
        );
    }));
    let _ = parent.kill();
    let _ = parent.wait();
    for pid in [owned, cleanup].into_iter().flatten() {
        // Kill only this known adopted child, never a reused unrelated PID.
        if unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) } == 0 {
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

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 2 {
        let gate = std::env::current_exe()
            .unwrap()
            .with_file_name("cleanup-gate.json");
        if gate.is_file() {
            let config: Value = serde_json::from_slice(&fs::read(gate).unwrap()).unwrap();
            let evidence = Path::new(config["evidence"].as_str().unwrap());
            mluva_core::private_files::atomic_write_private(
                &evidence.join("cleanup-started.json"),
                &serde_json::to_vec(&json!({"pid":std::process::id(),"directory":args[1]}))
                    .unwrap(),
            )
            .unwrap();
            while !evidence.join("cleanup.release").exists() {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if mluva_audio::volatile::run_cleanup(Path::new(&args[1])).is_err() {
            std::process::exit(1);
        }
        return;
    }
    if args.last().is_some_and(|arg| arg == "observe-parent-crash") {
        observe_parent_crash(Path::new(&args[2]));
        return;
    }
    let path = if args[1] == "app-server" {
        PathBuf::from(std::env::var_os("CODEX_HOME").unwrap()).join("fixture.json")
    } else {
        PathBuf::from(&args[2])
    };
    let spec: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    if args[1] == "serve" || args[1] == "app-server" {
        server(&spec);
        return;
    }
    let command = spec
        .get("command")
        .map(|value| {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().into())
                .collect()
        })
        .unwrap_or_else(|| {
            vec![
                std::env::current_exe().unwrap().into_os_string(),
                "serve".into(),
                path.into_os_string(),
            ]
        });
    let client = Arc::new(CodexAppServerClient::new(CodexOptions {
        command,
        request_timeout: Duration::from_millis(spec["request_ms"].as_u64().unwrap_or(300)),
        turn_timeout: Duration::from_millis(spec["turn_ms"].as_u64().unwrap_or(300)),
    }));
    let operation = spec["operation"].as_str().unwrap_or("transform");
    let images = spec
        .get("png_hex")
        .and_then(Value::as_str)
        .map(|png| {
            vec![ImageInput {
                data: unhex(png),
                captured_after_seconds: Some(12.5),
            }]
        })
        .unwrap_or_default();
    let mut deltas = vec![];
    let mut callback = |text: &str| deltas.push(text.to_owned());
    let value = match operation {
        "cancel-workspace-startup" => {
            let evidence = PathBuf::from(spec["evidence"].as_str().unwrap());
            let starting_client = client.clone();
            let starting = tokio::spawn(async move { starting_client.start().await });
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while !evidence.join("cleanup-started.json").exists() {
                assert!(tokio::time::Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let cleanup: Value =
                serde_json::from_slice(&fs::read(evidence.join("cleanup-started.json")).unwrap())
                    .unwrap();
            client.cancel();
            let closing_client = client.clone();
            let mut closing = tokio::spawn(async move { closing_client.close().await });
            let waited = tokio::time::timeout(Duration::from_millis(50), &mut closing)
                .await
                .is_err();
            // Release even on the negative route so a failed assertion cannot
            // strand this private external readiness endpoint.
            fs::write(evidence.join("cleanup.release"), []).unwrap();
            let cancelled = result(starting.await.unwrap());
            if waited {
                closing.await.unwrap();
            }
            json!({"waited_for_setup":waited,"directory_removed":!Path::new(cleanup["directory"].as_str().unwrap()).exists(),
                "helper_reaped":!Path::new(&format!("/proc/{}", cleanup["pid"].as_u64().unwrap())).exists(),
                "server_not_started":!evidence.join("process.jsonl").exists(),"cancelled_result":cancelled})
        }
        "park-for-parent-crash" => result(
            client
                .transform(
                    spec["crash_prompt"].as_str().unwrap(),
                    Path::new("/never-use-client-cwd"),
                    RewriteOptions {
                        model: Some("fixture-model"),
                        ..Default::default()
                    },
                    None,
                )
                .await,
        ),
        "catalog" => result(client.list_models().await),
        "resolve" => result(client.resolve_model(spec["model"].as_str()).await),
        "cancel-before" => {
            client.cancel();
            result(client.start().await)
        }
        "spawn" => {
            client.cancel();
            let other = client.spawn();
            let value = result(other.list_models().await);
            other.close().await;
            value
        }
        "factory" => {
            let config: AppConfig = serde_json::from_value(spec["settings"].clone()).unwrap();
            let factory = RewriteClient::new(
                &config,
                Some(Duration::from_millis(300)),
                Some(Duration::from_millis(300)),
            )
            .unwrap();
            let value = result(
                factory
                    .transform(
                        "Clean this 🙂",
                        Path::new("/never-use-client-cwd"),
                        &images,
                        Some(&mut callback),
                    )
                    .await,
            );
            factory.close().await;
            value
        }
        "cancel-during" => {
            let pending = {
                let client = client.clone();
                tokio::spawn(async move {
                    client
                        .transform(
                            "source",
                            Path::new("/"),
                            RewriteOptions {
                                model: Some("fixture-model"),
                                ..Default::default()
                            },
                            None,
                        )
                        .await
                })
            };
            let ready = PathBuf::from(spec["evidence"].as_str().unwrap()).join("requests.jsonl");
            while !fs::read_to_string(&ready)
                .unwrap_or_default()
                .contains("turn/start")
            {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            client.cancel();
            result(pending.await.unwrap())
        }
        _ => {
            let options = RewriteOptions {
                model: spec["model"].as_str(),
                max_output_characters: spec["maximum"].as_u64().unwrap_or(8000) as usize,
                effort: spec["effort"].as_str(),
                service_tier: spec["tier"].as_str(),
                images: &images,
            };
            let value = result(
                client
                    .transform(
                        "Clean this 🙂",
                        Path::new("/never-use-client-cwd"),
                        options,
                        Some(&mut callback),
                    )
                    .await,
            );
            if operation == "restart" {
                while client.process_id().is_some() {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                let second = result(
                    client
                        .transform(
                            "Clean this 🙂",
                            Path::new("/"),
                            RewriteOptions {
                                model: Some("fixture-model"),
                                ..Default::default()
                            },
                            None,
                        )
                        .await,
                );
                json!({"first":value,"second":second})
            } else {
                value
            }
        }
    };
    let last = client.last_model_identifier();
    client.close().await;
    println!(
        "{}",
        json!({"result":value,"deltas":deltas,"last_model":last})
    );
}
