//! Shared external SDK profile for the existing application and Live owners.
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}

pub fn prepare(root: &Path, tools: &Path, binaries: &Path, cli: &Path, address: &str) {
    let evidence = root.join("codex-evidence");
    fs::create_dir_all(&evidence).unwrap();
    fs::write(
        root.join("sdk-exec.json"),
        serde_json::to_vec(&json!({"evidence":evidence,"config":[
            "features.remote_models=false","features.responses_websockets=false","features.responses_websockets_v2=false",
            "features.enable_request_compression=false","model_provider=\"mock\"",
            format!("model_providers.mock={{name=\"Mock\",base_url=\"{address}\",wire_api=\"responses\"}}")]})).unwrap(),
    ).unwrap();
    fs::write(
        tools.join("codex"),
        format!(
            "#!/bin/sh\nexec {} exec-installed {} {} \"$@\"\n",
            quote(&binaries.join("codex-fixture-peer")),
            quote(&root.join("sdk-exec.json")),
            quote(cli)
        ),
    )
    .unwrap();
    fs::set_permissions(tools.join("codex"), fs::Permissions::from_mode(0o700)).unwrap();
    let home = root.join("home/.codex");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("AGENTS.md"), "PRIVATE_INSTRUCTION_CANARY").unwrap();
    fs::write(home.join("AGENTS.override.md"), "PRIVATE_OVERRIDE_CANARY").unwrap();
    fs::write(home.join("config.toml"), format!(
        "[mcp_servers.canary]\ncommand=\"/usr/bin/touch\"\nargs=[\"{}\"]\n[features]\ncode_mode_only=true\ncurrent_time_reminder=true\nsend_message_to_user_async=true\ndeferred_executor=true\nmulti_agent_v2=true\nshell_tool=true\nunified_exec=true\n", root.join("mcp-started").display()),
    ).unwrap();
}
