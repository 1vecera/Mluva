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
                let deltas = spec
                    .get("deltas")
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

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
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
