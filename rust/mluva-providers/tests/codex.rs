use mluva_core::config::AppConfig;
use mluva_providers::{
    codex_policy,
    models::{Model, select_codex_model},
    rewriting,
};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::process::Command;
mod support;

fn observed<T: serde::Serialize, E: std::fmt::Display>(result: Result<T, E>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn expected(value: &Value) -> Value {
    let mut value = value.clone();
    value.as_object_mut().unwrap().remove("exception");
    value
}

#[test]
fn catalogs_alias_defaults_and_rewrite_guards_match_the_released_policy() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-codex.json")).unwrap();
    assert_eq!(*codex_policy::TEXT_ONLY_CONFIG, fixture["text_only"]);
    let environment = fixture["environment"]["input"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value.as_str().unwrap())));
    let filtered = codex_policy::child_environment(environment);
    let filtered = filtered
        .into_iter()
        .map(|(key, value)| {
            (
                key.into_string().unwrap(),
                json!(value.into_string().unwrap()),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    assert_eq!(json!(filtered), fixture["environment"]["output"]);
    for (index, row) in fixture["catalogs"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            observed(Model::from_codex_catalog(&row["row"])),
            expected(&row["result"]),
            "catalog {index}"
        );
    }
    for (index, row) in fixture["selections"].as_array().unwrap().iter().enumerate() {
        let models: Vec<Model> = serde_json::from_value(row["models"].clone()).unwrap();
        assert_eq!(
            observed(select_codex_model(&models, row["requested"].as_str())),
            expected(&row["result"]),
            "selection {index}"
        );
    }
    for (index, row) in fixture["rewrites"].as_array().unwrap().iter().enumerate() {
        let models: Vec<Model> = serde_json::from_value(row["models"].clone()).unwrap();
        let settings: AppConfig = serde_json::from_value(row["settings"].clone()).unwrap();
        let selection = rewriting::selection(&settings, &models);
        if row["result"].get("error").is_some() {
            let kind = match selection.as_ref().unwrap_err() {
                rewriting::RewriteError::UnsupportedRewriteSpeed => "UnsupportedRewriteSpeed",
                rewriting::RewriteError::UnsupportedThinkingLevel => "UnsupportedThinkingLevel",
                rewriting::RewriteError::Provider(_) => "provider",
            };
            let released_kind = row["result"]["exception"].as_str().unwrap();
            assert_eq!(
                kind,
                match released_kind {
                    "UnsupportedRewriteSpeed" | "UnsupportedThinkingLevel" => released_kind,
                    _ => "provider",
                },
                "rewrite error category {index}"
            );
            assert_eq!(
                observed(selection),
                expected(&row["result"]),
                "rewrite {index}"
            );
        } else {
            let selected = selection.unwrap();
            let call = &row["calls"][0];
            assert_eq!(selected.model, call["model"]);
            assert_eq!(json!(selected.effort), call["effort"]);
            assert_eq!(json!(selected.service_tier), call["service_tier"]);
            assert_eq!(
                call["max_output_characters"],
                mluva_core::conversation::MAX_REWRITE_CHARACTERS
            );
        }
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    path: PathBuf,
    evidence: PathBuf,
    executable: PathBuf,
}

#[test]
fn abrupt_client_failure_reaps_the_held_rewrite_and_private_workspace() {
    for masked in [false, true] {
        let case = Fixture::new(&json!({"scenario":"clean","masked":masked}));
        let mut spec: Value = serde_json::from_slice(&std::fs::read(&case.path).unwrap()).unwrap();
        spec["operation"] = json!("park-for-parent-crash");
        spec["turn_ms"] = json!(30_000);
        spec["request_ms"] = json!(5000);
        spec["crash_prompt"] = json!(
            "You are an editor updating a draft as someone dictates.\n{\"transcript_status\":\"final committed recognition\",\"transcript\":\"held crash rewrite\"}"
        );
        spec["live_controls"] = json!({"final|held crash rewrite":{"gate":case.directory.path().join("never.release"),"deltas":["Never deliver this."]}});
        std::fs::write(&case.path, serde_json::to_vec(&spec).unwrap()).unwrap();
        let output = case
            .command()
            .as_std_mut()
            .arg("observe-parent-crash")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "masked={masked}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"held_rewrite_reaped_after_client_crash":true,"private_workspace_removed":true})
        );
    }
}
impl Fixture {
    fn new(spec: &Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for name in ["evidence", "bin", "home", "codex"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_codex-fixture-peer"),
            root.join("bin/codex"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            env!("CARGO_BIN_EXE_codex-fixture-peer"),
            root.join("bin/mluva-audio-cleanup"),
        )
        .unwrap();
        if spec["masked"] == true {
            std::fs::write(root.join("codex/AGENTS.md"), "PRIVATE_INSTRUCTION_CANARY").unwrap();
            std::fs::write(
                root.join("codex/AGENTS.override.md"),
                "PRIVATE_OVERRIDE_CANARY",
            )
            .unwrap();
        }
        let evidence = root.join("evidence");
        let mut spec = spec.clone();
        spec["evidence"] = json!(evidence);
        let path = root.join("codex/fixture.json");
        std::fs::write(&path, serde_json::to_vec(&spec).unwrap()).unwrap();
        Self {
            directory,
            path,
            evidence,
            executable: env!("CARGO_BIN_EXE_codex-fixture-peer").into(),
        }
    }
    fn command(&self) -> Command {
        let root = self.directory.path();
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", root.join("bin").display()),
            )
            .env("HOME", root.join("home"))
            .env("CODEX_HOME", root.join("codex"))
            .env("LANG", "C.UTF-8")
            .env("OPENAI_API_KEY", "synthetic-allowed")
            .env("ELEVENLABS_API_KEY", "synthetic-private")
            .env("MLUVA_SECRET_CANARY", "synthetic-private")
            .env(
                "TMPDIR",
                std::env::var_os("TMPDIR").unwrap_or_else(|| self.directory.path().into()),
            )
            .arg("client")
            .arg(&self.path)
            .kill_on_drop(true);
        command
    }
    fn rows(&self, name: &str) -> Vec<Value> {
        std::fs::read_to_string(self.evidence.join(name))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    async fn run(&self) -> Value {
        let output = tokio::time::timeout(Duration::from_secs(8), self.command().output())
            .await
            .unwrap()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
fn normalize(value: &mut Value, workspaces: &[String], spec: &Path) {
    match value {
        Value::String(string) => {
            for workspace in workspaces {
                *string = string.replace(workspace, "<workspace>");
            }
            *string = string.replace(&spec.to_string_lossy().to_string(), "<spec>");
        }
        Value::Array(array) => {
            for value in array {
                normalize(value, workspaces, spec);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                normalize(value, workspaces, spec);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn actual_jsonl_process_requests_results_and_isolation_match_released_sessions() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-codex-wire.json")).unwrap();
    for (index, row) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let scenario = &row["spec"];
        let case = Fixture::new(scenario);
        let observed = case.run().await;
        assert_eq!(observed, row["observed"], "case {index}: {scenario}");
        let processes = case.rows("process.jsonl");
        let workspaces = processes
            .iter()
            .map(|process| process["cwd"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut frames = json!(case.rows("requests.jsonl"));
        normalize(&mut frames, &workspaces, &case.path);
        assert_eq!(
            frames.as_array().unwrap().len(),
            row["frames"].as_array().unwrap().len(),
            "frame count {index}: {scenario}"
        );
        for (frame_index, (frame, expected)) in frames
            .as_array()
            .unwrap()
            .iter()
            .zip(row["frames"].as_array().unwrap())
            .enumerate()
        {
            let mut expected = expected.clone();
            if expected["message"]["method"] == "initialize" {
                // Preserve the released wire bytes except the intentional
                // application release version; never derive expectations from
                // the request being tested.
                assert_eq!(
                    expected["message"]["params"]["clientInfo"]["version"],
                    "1.6.0"
                );
                expected["message"]["params"]["clientInfo"]["version"] =
                    json!(env!("CARGO_PKG_VERSION"));
                expected["raw"] = json!(expected["raw"].as_str().unwrap().replacen(
                    "\"version\":\"1.6.0\"",
                    &format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION")),
                    1,
                ));
            }
            assert_eq!(frame, &expected, "frame {index}/{frame_index}: {scenario}");
        }
        let policy=processes.iter().map(|process| {
            assert!(!Path::new(process["cwd"].as_str().unwrap()).exists(),"private workspace leaked");
            assert!(!Path::new(&format!("/proc/{}",process["pid"].as_u64().unwrap())).exists(),"child was not reaped");
            let mut argv=process["argv"].clone();normalize(&mut argv,&workspaces,&case.path);
            if argv.as_array().unwrap().last().is_some_and(|arg| arg.as_str().is_some_and(|arg| arg.starts_with("model_catalog_json="))) {
                let args=argv.as_array_mut().unwrap();
                assert_eq!(args.pop().unwrap(), "model_catalog_json=\"<workspace>/text-only-models.json\"");
                assert_eq!(args.pop().unwrap(), "-c");
                for feature in ["deferred_executor", "send_message_to_user_async", "current_time_reminder", "code_mode_only"] {
                    assert_eq!(args.pop().unwrap(), format!("features.{feature}=false"));
                    assert_eq!(args.pop().unwrap(), "-c");
                }
            }
            json!({"mode":process["mode"],"argv":argv,"environment":process["environment"],"instructions":process["instructions"]})
        }).collect::<Vec<_>>();
        assert_eq!(
            json!(policy),
            row["process_policy"],
            "process policy {index}: {scenario}"
        );
        if scenario["masked"] == true {
            assert_eq!(
                std::fs::read_to_string(case.directory.path().join("codex/AGENTS.md")).unwrap(),
                "PRIVATE_INSTRUCTION_CANARY"
            );
            assert_eq!(
                std::fs::read_to_string(case.directory.path().join("codex/AGENTS.override.md"))
                    .unwrap(),
                "PRIVATE_OVERRIDE_CANARY"
            );
        }
    }
}

#[tokio::test]
async fn cancellation_cleans_up_pending_work_and_missing_commands_do_not_echo_paths() {
    for scenario in [
        "catalog-invalid",
        "catalog-oversized",
        "catalog-held-cancel",
        "catalog-held-timeout",
    ] {
        let case = Fixture::new(
            &json!({"scenario":scenario,"masked":true,"operation":"catalog-failure","request_ms":1000}),
        );
        let mut spec: Value = serde_json::from_slice(&std::fs::read(&case.path).unwrap()).unwrap();
        spec["command"] = json!([
            env!("CARGO_BIN_EXE_codex-fixture-peer"),
            "app-server",
            "--listen",
            "stdio://"
        ]);
        std::fs::write(&case.path, serde_json::to_vec(&spec).unwrap()).unwrap();
        let error = if scenario == "catalog-held-cancel" {
            "Codex app-server work was cancelled."
        } else {
            "Codex could not establish text-only permissions."
        };
        assert_eq!(
            case.run().await["result"],
            json!({"start":{"error":error},"workspace_removed":true,"snapshot_reaped":true,"server_not_started":true}),
            "{scenario}"
        );
    }
    let mut preparing =
        Fixture::new(&json!({"scenario":"clean","operation":"cancel-workspace-startup"}));
    let copied = preparing.directory.path().join("bin/private-client");
    std::fs::copy(&preparing.executable, &copied).unwrap();
    preparing.executable = copied.clone();
    let helper = preparing.directory.path().join("bin/mluva-audio-cleanup");
    std::fs::remove_file(&helper).unwrap();
    std::os::unix::fs::symlink(copied, helper).unwrap();
    std::fs::write(
        preparing.directory.path().join("bin/cleanup-gate.json"),
        serde_json::to_vec(&json!({"evidence":preparing.evidence})).unwrap(),
    )
    .unwrap();
    let startup = preparing.run().await["result"].clone();
    assert_eq!(
        startup["waited_for_setup"], true,
        "close acknowledged while startup resource remained"
    );
    assert_eq!(
        startup,
        json!({"waited_for_setup":true,"directory_removed":true,"helper_reaped":true,
        "server_not_started":true,"cancelled_result":{"error":"Codex app-server work was cancelled."}})
    );
    let case =
        Fixture::new(&json!({"scenario":"turn-stall","operation":"cancel-during","turn_ms":10000}));
    let result = case.run().await;
    assert!(result["result"].get("error").is_some());
    assert!(result["result"].get("ok").is_none());
    for process in case.rows("process.jsonl") {
        assert!(!Path::new(process["cwd"].as_str().unwrap()).exists());
        assert!(!Path::new(&format!("/proc/{}", process["pid"].as_u64().unwrap())).exists());
    }
    let missing =
        Fixture::new(&json!({"scenario":"clean","command":["/synthetic/private/missing-codex"]}));
    assert_eq!(
        missing.run().await["result"],
        json!({"error":"Codex app-server could not start."})
    );
    assert!(missing.rows("process.jsonl").is_empty());
}

fn model_response(tool: bool, marker: &Path) -> Vec<u8> {
    let item = json!({"id":"msg_fixture","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Synthetic transformed text.","annotations":[]}]});
    let mut events = vec![
        json!({"type":"response.created","response":{"id":"resp_fixture","object":"response","status":"in_progress"}}),
    ];
    if tool {
        let call = json!({"id":"tool_fixture","type":"function_call","name":"exec_command","call_id":"call_fixture","arguments":json!({"cmd":format!("touch {}",marker.display())}).to_string()});
        events.extend([json!({"type":"response.output_item.done","output_index":0,"item":call}),json!({"type":"response.completed","response":{"id":"resp_fixture","object":"response","status":"completed","output":[call]}})]);
    } else {
        let mut pending = item.clone();
        pending["status"] = json!("in_progress");
        pending["content"] = json!([]);
        events.extend([
            json!({"type":"response.output_item.added","output_index":0,"item":pending}),
            json!({"type":"response.content_part.added","item_id":"msg_fixture","output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
            json!({"type":"response.output_text.delta","item_id":"msg_fixture","output_index":0,"content_index":0,"delta":"Synthetic transformed text."}),
            json!({"type":"response.output_item.done","output_index":0,"item":item}),
            json!({"type":"response.completed","response":{"id":"resp_fixture","object":"response","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14}}}),
        ]);
    }
    events
        .into_iter()
        .map(|event| {
            format!(
                "event: {}\ndata: {event}\n\n",
                event["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}

#[tokio::test]
#[ignore = "requires installed Codex and working bubblewrap; all model traffic stays loopback"]
async fn installed_codex_preserves_reference_behavior_without_tools_or_instruction_leaks() {
    let executable = mluva_core::executables::find_executable("codex").expect("installed Codex");
    let version = std::process::Command::new(&executable)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let version = String::from_utf8(version.stdout).unwrap();
    let baseline = [
        include_str!("fixtures/released-installed-codex.json"),
        include_str!("fixtures/released-installed-codex-0160.json"),
    ]
    .into_iter()
    .map(|fixture| serde_json::from_str::<Value>(fixture).unwrap())
    .find(|fixture| fixture["reference"]["installed_cli"] == version.trim())
    .expect("renew independent installed CLI observations before accepting another version");
    for row in baseline["cases"].as_array().unwrap() {
        let tool = row["backend_tool_call"].as_bool().unwrap();
        let case = Fixture::new(&json!({"scenario":"clean","masked":true}));
        let root = case.directory.path();
        let mcp = root.join("mcp-started");
        let marker = root.join("command-started");
        std::fs::write(
            root.join("codex/config.toml"),
            format!(
                "[mcp_servers.canary]\ncommand=\"/usr/bin/touch\"\nargs=[\"{}\"]\n[features]\ncode_mode_only=true\ncurrent_time_reminder=true\nsend_message_to_user_async=true\ndeferred_executor=true\nmulti_agent_v2=true\nshell_tool=true\nunified_exec=true\n",
                mcp.display()
            ),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut command = vec![
            executable.to_string_lossy().to_string(),
            "app-server".into(),
            "--listen".into(),
            "stdio://".into(),
        ];
        for value in [
            "features.remote_models=false".to_string(),
            "features.responses_websockets=false".into(),
            "features.responses_websockets_v2=false".into(),
            "features.enable_request_compression=false".into(),
            "model_provider=\"mock\"".into(),
            format!(
                "model_providers.mock={{name=\"Mock\",base_url=\"{endpoint}\",wire_api=\"responses\"}}"
            ),
        ] {
            command.extend(["-c".into(), value]);
        }
        let mut spec = json!({"scenario":"clean","evidence":case.evidence,"command":command,"model":"gpt-5.4","request_ms":10000,"turn_ms":10000});
        if let Some(input) = row["input"].as_object() {
            spec.as_object_mut().unwrap().extend(input.clone());
        }
        std::fs::write(&case.path, serde_json::to_vec(&spec).unwrap()).unwrap();
        let requests = Arc::new(Mutex::new(vec![]));
        let captured = requests.clone();
        let command_marker = marker.clone();
        let server = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let request = support::request(&mut socket).await;
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let index = {
                    let mut captured = captured.lock().unwrap();
                    let index = captured.len();
                    captured.push(body);
                    index
                };
                let mut response =
                    support::Response::ok(model_response(tool && index == 0, &command_marker));
                response
                    .headers
                    .push(("Content-Type".into(), "text/event-stream".into()));
                support::respond(&mut socket, &response).await;
            }
        });
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            case.command().env_remove("OPENAI_API_KEY").output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        server.abort();
        let _ = server.await;
        assert_eq!(result, row["observed"], "{}", row["name"]);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), row["requests"].as_u64().unwrap() as usize);
        assert!(
            requests.iter().all(|request| request["input"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| (item["type"] != "additional_tools"
                    && item["type"] != "tool_search_output")
                    || item["tools"] == json!([]))),
            "model-input capabilities must also be empty: {}",
            row["name"]
        );
        assert!(
            requests
                .iter()
                .all(|request| request.get("tools").is_none_or(|tools| *tools == json!([])))
        );
        if let Some(expected) = row.get("rewrite_inputs") {
            let inputs = requests
                .iter()
                .map(|request| {
                    request["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|item| {
                            item["type"] == "message"
                                && item["role"] == "user"
                                && item["content"].as_array().unwrap().iter().any(|part| {
                                    part["type"] == "input_text"
                                        && part["text"]
                                            .as_str()
                                            .is_some_and(|text| text.starts_with("Clean this 🙂"))
                                })
                        })
                        .map(|item| item["content"].clone())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            assert_eq!(json!(inputs), *expected);
            assert_eq!(
                json!(
                    requests
                        .iter()
                        .map(|request| &request["model"])
                        .collect::<Vec<_>>()
                ),
                row["model_identifiers"]
            );
            assert_eq!(json!(requests.iter().map(|request| json!({"reasoning":request["reasoning"],"service_tier":request["service_tier"]})).collect::<Vec<_>>()), row["request_settings"]);
        }
        let serialized = serde_json::to_string(&*requests).unwrap();
        assert!(!serialized.contains("PRIVATE_INSTRUCTION_CANARY"));
        assert!(!serialized.contains("PRIVATE_OVERRIDE_CANARY"));
        assert!(!mcp.exists());
        assert!(!marker.exists());
        assert_eq!(
            std::fs::read_to_string(root.join("codex/AGENTS.md")).unwrap(),
            "PRIVATE_INSTRUCTION_CANARY"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("codex/AGENTS.override.md")).unwrap(),
            "PRIVATE_OVERRIDE_CANARY"
        );
        println!(
            "{}",
            json!({"installed_cli":version.trim(),"case":row.get("name").cloned().unwrap_or_else(||json!(format!("legacy-tool-{tool}"))),"observed":result,"requests":*requests,"mcp_started":mcp.exists(),"command_started":marker.exists(),"instructions_unchanged":true})
        );
    }
}

#[tokio::test]
async fn compatible_factory_freezes_alias_ignores_native_speed_and_disabled_routes_do_no_work() {
    let (url,captured,server)=support::http_once(support::Response::ok(b"data: {\"choices\":[{\"delta\":{\"content\":\"kept text\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec())).await;
    let mut config = AppConfig {
        rewrite_provider: "litellm".into(),
        litellm_base_url: url,
        litellm_api_key_env: String::new(),
        litellm_model: Some("frozen-deployment".into()),
        litellm_reasoning_effort: Some("high".into()),
        rewrite_model: Some("unavailable-native-model".into()),
        rewrite_fast_mode: true,
        ..Default::default()
    };
    let client = rewriting::RewriteClient::new(&config, None, None).unwrap();
    config.litellm_model = Some("changed".into());
    let result = client
        .transform("source", Path::new("/"), &[], None)
        .await
        .unwrap();
    assert_eq!(result.model, "frozen-deployment");
    assert_eq!(result.text, "kept text");
    let request = captured.await.unwrap();
    server.await.unwrap();
    assert_eq!(request.path, "/chat/completions");
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], "frozen-deployment");
    assert_eq!(body["reasoning_effort"], "high");
    assert!(body.get("service_tier").is_none());
    client.close().await;
    let disabled = rewriting::RewriteClient::new(
        &AppConfig {
            rewrite_provider: "none".into(),
            ..Default::default()
        },
        None,
        None,
    )
    .unwrap();
    assert!(disabled.list_models().await.unwrap().is_empty());
    assert_eq!(
        disabled
            .transform("source", Path::new("/"), &[], None)
            .await
            .unwrap_err()
            .to_string(),
        "Choose a rewriting provider in Settings first."
    );
    disabled.cancel();
    disabled.close().await;
}

#[tokio::test]
async fn a_child_ignoring_termination_is_killed_reaped_and_its_workspace_removed() {
    let case = Fixture::new(&json!({"scenario":"ignore-term","request_ms":50}));
    let result = case.run().await;
    assert_eq!(
        result["result"],
        json!({"error":"Codex app-server did not answer initialize."})
    );
    for process in case.rows("process.jsonl") {
        assert!(!Path::new(process["cwd"].as_str().unwrap()).exists());
        assert!(!Path::new(&format!("/proc/{}", process["pid"].as_u64().unwrap())).exists());
    }
}
