//! Actual SDK transport for the existing managed Live process owner.
use super::*;
#[path = "current_codex_profile.rs"]
mod profile;

pub(super) fn extend(fixture: &mut Value) -> Option<PathBuf> {
    let cli = PathBuf::from(std::env::var_os("MLUVA_TEST_INSTALLED_CODEX")?);
    let current: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-current-codex.json"
    ))
    .unwrap();
    assert_eq!(hash(&fs::read(&cli).unwrap()), current["sdk"]["sha256"]);
    let version = Command::new(&cli).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        current["sdk"]["version"].as_str().unwrap()
    );
    assert_eq!(
        hash(include_bytes!(
            "../fixtures/released-bootstrap-managed-live.json"
        )),
        current["base"]["sha256"]
    );
    assert_eq!(
        hash(include_bytes!(
            "../fixtures/released-bootstrap-managed-live-restart.json"
        )),
        current["restart"]["sha256"]
    );
    for changes in current["cases"].as_array().unwrap() {
        let name = changes["name"].as_str().unwrap();
        let mut case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()
            .clone();
        for change in changes["patches"].as_array().unwrap() {
            assert_eq!(change["operation"], "replace");
            let field = case["result"]
                .pointer_mut(change["pointer"].as_str().unwrap())
                .unwrap();
            assert_eq!(*field, change["old"]);
            *field = change["value"].clone();
        }
        case["behavior"] = json!(name);
        case["name"] = json!(format!("current-codex-{name}"));
        case["sdk"] = current["sdk"].clone();
        case["request_settings"] = changes["request_settings"].clone();
        fixture["cases"].as_array_mut().unwrap().push(case);
    }
    Some(fs::canonicalize(cli).unwrap())
}

pub(super) fn input(request: &Value) -> &str {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "message" && item["role"] == "user")
        .flat_map(|item| item["content"].as_array().unwrap())
        .find_map(|part| {
            part["text"].as_str().filter(|text| {
                text.starts_with("You are an editor updating a draft as someone dictates.")
            })
        })
        .unwrap()
}

pub(super) fn setup(
    root: &Path,
    cli: &Path,
    binaries: &Path,
    fixture: &Value,
    case: &Value,
) -> http::Peer {
    let draft = fixture["draft"].as_str().unwrap();
    let final_text = format!(
        "{}{}",
        fixture["final"].as_str().unwrap(),
        if case["behavior"] == "manual-edit" {
            fixture["edit"].as_str().unwrap()
        } else {
            ""
        }
    );
    let mut responses = vec![];
    let response = |text: &str, gate: Option<&str>| {
        let mut result = json!({"route":"codex","status":200,"events":http::codex_response::events(text,None),
            "request_log":root.join("codex-evidence/requests.jsonl"),"allow_disconnect":true});
        if let Some(gate) = gate {
            result["wait_for_file"] = json!(root.join("codex-evidence").join(gate));
            result["incoming_file"] = json!(root.join("codex-evidence/http-incoming"));
        }
        result
    };
    responses.extend([
        response(draft, None),
        response(draft, None),
        response(&final_text, Some("final.release")),
    ]);
    if case["behavior"] == "manual-edit" {
        responses.push(response(&final_text, None));
    } else {
        responses.extend([
            response(fixture["fresh"]["draft"].as_str().unwrap(), None),
            response(fixture["fresh"]["draft"].as_str().unwrap(), None),
            response(
                fixture["fresh"]["final"].as_str().unwrap(),
                Some("fresh.release"),
            ),
        ]);
    }
    let peer = http::Peer::new(&responses);
    profile::prepare(root, &root.join("tools"), binaries, cli, &peer.address);
    peer
}

pub(super) fn held_server(root: &Path, cli: &Path) {
    let server = records(root, "process.jsonl").last().unwrap().clone();
    let pid = server["pid"].as_u64().unwrap();
    assert_eq!(fs::read_link(format!("/proc/{pid}/exe")).unwrap(), cli);
    let workspace = Path::new(server["cwd"].as_str().unwrap());
    assert!(mluva_audio::volatile::memory_backed(workspace));
    assert_eq!(
        workspace.metadata().unwrap().permissions().mode() & 0o777,
        0o700
    );
    let catalog = workspace.join("text-only-models.json");
    assert_eq!(
        catalog.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    let catalog: Value = serde_json::from_slice(&fs::read(catalog).unwrap()).unwrap();
    for model in catalog["models"].as_array().unwrap() {
        assert_eq!(model["tool_mode"], "direct");
        assert_eq!(model["multi_agent_version"], "disabled");
        assert_eq!(model["experimental_supported_tools"], json!([]));
    }
}

pub(super) fn finish(root: &Path, peer: &mut http::Peer, case: &Value) {
    let requests = peer.finish();
    assert_eq!(
        requests.len(),
        case["request_settings"].as_array().unwrap().len()
    );
    for (request, expected) in requests
        .iter()
        .zip(case["request_settings"].as_array().unwrap())
    {
        assert_eq!(request["path"], "/responses");
        let request = &request["json"];
        assert_eq!(
            json!({"model":request["model"],"reasoning":request["reasoning"],"service_tier":request["service_tier"]}),
            *expected
        );
        let instructions = request["instructions"]
            .as_str()
            .or_else(|| {
                request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "message" && item["role"] == "developer")
                    .flat_map(|item| item["content"].as_array().unwrap())
                    .find_map(|part| {
                        part["text"]
                            .as_str()
                            .filter(|text| text.starts_with("You transform dictated text."))
                    })
            })
            .unwrap();
        assert_eq!(
            instructions,
            case["sdk"]["base_instructions"].as_str().unwrap()
        );
        assert!(request.get("tools").is_none_or(|tools| *tools == json!([])));
        assert!(
            request["input"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| !matches!(
                    item["type"].as_str(),
                    Some("additional_tools" | "tool_search_output")
                ) || item["tools"] == json!([]))
        );
        let request = request.to_string();
        assert!(
            !request.contains("PRIVATE_INSTRUCTION_CANARY")
                && !request.contains("PRIVATE_OVERRIDE_CANARY")
        );
    }
    assert!(!root.join("mcp-started").exists());
    assert_eq!(
        fs::read_to_string(root.join("home/.codex/AGENTS.md")).unwrap(),
        "PRIVATE_INSTRUCTION_CANARY"
    );
    assert_eq!(
        fs::read_to_string(root.join("home/.codex/AGENTS.override.md")).unwrap(),
        "PRIVATE_OVERRIDE_CANARY"
    );
    for row in records(root, "catalog.jsonl") {
        assert!(!Path::new(&format!("/proc/{}", row["pid"].as_u64().unwrap())).exists());
    }
    write(
        &root.join("actual-sdk.json"),
        &json!({"requests":requests,"tools_empty":true,"instruction_canaries_absent":true,
        "mcp_not_started":true,"catalog_children_reaped":true}),
    );
}
