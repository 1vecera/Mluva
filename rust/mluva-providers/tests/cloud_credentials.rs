//! Per-provider keyring entries, environment aliases and caches for the Microsoft and xAI keys,
//! through the synthetic `secret-tool` peer. The ElevenLabs behavior has its own released corpus.
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

mod support;

struct Run {
    observations: Value,
    trace: Vec<Value>,
    /// The credential each request carried: the Azure header, or the xAI bearer token.
    credentials: Vec<Option<String>>,
}

async fn run(keyring: Value, environment: &[(&str, &str)], operations: Value) -> Run {
    let root = tempfile::Builder::new()
        .prefix("cloud-credentials-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let path = root.path();
    std::fs::create_dir(path.join("bin")).unwrap();
    std::fs::create_dir(path.join("home")).unwrap();
    std::os::unix::fs::symlink(
        env!("CARGO_BIN_EXE_credential-fixture-peer"),
        path.join("bin/secret-tool"),
    )
    .unwrap();
    std::fs::write(
        path.join("keyring.json"),
        serde_json::to_vec(&keyring).unwrap(),
    )
    .unwrap();
    let audio = path.join("synthetic.wav");
    std::fs::write(&audio, b"RIFFsynthetic").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let credentials = Arc::new(Mutex::new(vec![]));
    let recorded = credentials.clone();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move {
        loop {
            let mut socket = tokio::select! {biased; _ = stopped.cancelled() => break, value = listener.accept() => value.unwrap().0};
            let request = support::request(&mut socket).await;
            recorded.lock().unwrap().push(
                request
                    .headers
                    .get("ocp-apim-subscription-key")
                    .cloned()
                    .or_else(|| {
                        request
                            .headers
                            .get("authorization")
                            .map(|value| value.trim_start_matches("Bearer ").to_owned())
                    }),
            );
            // Both providers accept this body: Azure reads combinedPhrases, xAI reads text.
            support::respond(
                &mut socket,
                &support::Response::ok(br#"{"text":"Synthetic.","combinedPhrases":[]}"#.to_vec()),
            )
            .await;
        }
    });
    let spec = json!({
        "keyring": keyring,
        "operations": operations,
        "endpoint": endpoint,
        "audio_path": audio,
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_credential-fixture-peer"));
    command
        .env_clear()
        .env("HOME", path.join("home"))
        .env("PATH", path.join("bin"))
        .env("LC_ALL", "C.UTF-8")
        .env("CREDENTIAL_FIXTURE_ROOT", path)
        .env("NO_PROXY", "127.0.0.1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut driver = command.spawn().unwrap();
    let mut stdin = driver.stdin.take().unwrap();
    stdin
        .write_all(&serde_json::to_vec(&spec).unwrap())
        .await
        .unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(30), driver.wait_with_output())
        .await
        .expect("bounded fixture driver")
        .unwrap();
    stop.cancel();
    server.await.unwrap();
    assert!(
        output.status.success(),
        "driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let trace_path = path.join("trace.jsonl");
    let trace = if trace_path.exists() {
        std::fs::read_to_string(trace_path)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    } else {
        vec![]
    };
    let credentials = credentials.lock().unwrap().clone();
    Run {
        observations: serde_json::from_slice(&output.stdout).unwrap(),
        trace,
        credentials,
    }
}

fn lookup_args(provider: &str) -> Value {
    json!({"args": ["lookup", "application", "mluva", "provider", provider], "input_hex": ""})
}
fn hex(value: &str) -> String {
    value.bytes().map(|byte| format!("{byte:02x}")).collect()
}

#[tokio::test]
async fn each_cloud_provider_reads_its_own_keyring_entry_before_its_environment_aliases() {
    for (provider, environment, expected) in [
        (
            "microsoft",
            vec![("AZURE_SPEECH_KEY", "synthetic-env")],
            "synthetic-saved",
        ),
        (
            "xai",
            vec![("XAI_API_KEY", "synthetic-env")],
            "synthetic-saved",
        ),
        (
            "mistral",
            vec![("MISTRAL_API_KEY", "synthetic-env")],
            "synthetic-saved",
        ),
    ] {
        let run = run(
            json!({"lookup": {"stdout": "synthetic-saved\n"}}),
            &environment,
            json!([{"op": "key_for", "provider": provider}]),
        )
        .await;
        assert_eq!(run.observations, json!([{"ok": "sent"}]), "{provider}");
        assert_eq!(run.trace, [lookup_args(provider)], "{provider}");
        assert_eq!(run.credentials, [Some(expected.into())], "{provider}");
    }
}

#[tokio::test]
async fn environment_aliases_apply_in_order_when_nothing_is_saved() {
    // The keyring answers with nothing.
    let empty = json!({"lookup": {"stdout": ""}});
    for (provider, environment, expected) in [
        (
            "microsoft",
            vec![
                ("AZURE_SPEECH_KEY", "synthetic-azure"),
                ("SPEECH_STUDIO_KEY", "synthetic-studio"),
            ],
            "synthetic-azure",
        ),
        (
            "microsoft",
            vec![("SPEECH_STUDIO_KEY", "synthetic-studio")],
            "synthetic-studio",
        ),
        (
            "xai",
            vec![
                ("XAI_API_KEY", "synthetic-xai"),
                ("DAS_ITEM_GROK_STT_API_KEY__KEY", "synthetic-managed"),
            ],
            "synthetic-xai",
        ),
        (
            "xai",
            vec![("DAS_ITEM_GROK_STT_API_KEY__KEY", "synthetic-managed")],
            "synthetic-managed",
        ),
        (
            "mistral",
            vec![
                ("MISTRAL_API_KEY", "synthetic-mistral"),
                ("DAS_ITEM_MISTRAL_API_KEY__KEY", "synthetic-managed"),
            ],
            "synthetic-mistral",
        ),
        (
            "mistral",
            vec![("DAS_ITEM_MISTRAL_API_KEY__KEY", "synthetic-managed")],
            "synthetic-managed",
        ),
    ] {
        let run = run(
            empty.clone(),
            &environment,
            json!([{"op": "key_for", "provider": provider}]),
        )
        .await;
        assert_eq!(
            run.observations,
            json!([{"ok": "sent"}]),
            "{provider} {environment:?}"
        );
        assert_eq!(run.credentials, [Some(expected.into())]);
    }
}

#[tokio::test]
async fn a_missing_key_names_the_provider_and_its_variable_and_sends_nothing() {
    for (provider, message) in [
        (
            "microsoft",
            "Microsoft Azure Speech credential unavailable. Set AZURE_SPEECH_KEY in Mluva's process environment through a secret manager or session service.",
        ),
        (
            "xai",
            "xAI credential unavailable. Set XAI_API_KEY in Mluva's process environment through a secret manager or session service.",
        ),
        (
            "mistral",
            "Mistral credential unavailable. Set MISTRAL_API_KEY in Mluva's process environment through a secret manager or session service.",
        ),
    ] {
        let run = run(
            json!({"lookup": {"stdout": ""}}),
            &[],
            json!([{"op": "key_for", "provider": provider}]),
        )
        .await;
        assert_eq!(run.observations, json!([{"error": message}]));
        assert!(run.credentials.is_empty());
    }
}

#[tokio::test]
async fn storing_a_key_uses_the_provider_entry_label_and_refreshes_only_that_cache() {
    for (provider, label) in [
        ("microsoft", "--label=Mluva Microsoft Azure Speech key"),
        ("xai", "--label=Mluva xAI API key"),
        ("mistral", "--label=Mluva Mistral API key"),
    ] {
        let run = run(
            json!({"lookup": {"stdout": ""}}),
            &[],
            json!([
                {"op": "lookup_for", "provider": provider},
                {"op": "store_for", "provider": provider, "value": "synthetic-new"},
                {"op": "lookup_for", "provider": provider},
            ]),
        )
        .await;
        assert_eq!(
            run.observations,
            json!([{"ok": {"present": false}}, {"ok": null}, {"ok": {"present": true}}]),
            "{provider}"
        );
        assert_eq!(
            run.trace,
            [
                lookup_args(provider),
                json!({"args": ["store", label, "application", "mluva", "provider", provider], "input_hex": hex("synthetic-new")}),
                lookup_args(provider),
            ],
            "{provider}"
        );
    }
}

#[tokio::test]
async fn lookups_are_cached_per_provider() {
    let run = run(
        json!({"lookup": {"stdout": "synthetic-saved\n"}}),
        &[],
        json!([
            {"op": "lookup_for", "provider": "elevenlabs"},
            {"op": "lookup_for", "provider": "microsoft"},
            {"op": "lookup_for", "provider": "xai"},
            {"op": "lookup_for", "provider": "mistral"},
            {"op": "lookup_for", "provider": "elevenlabs"},
            {"op": "lookup_for", "provider": "microsoft"},
            {"op": "lookup_for", "provider": "xai"},
            {"op": "lookup_for", "provider": "mistral"},
        ]),
    )
    .await;
    assert_eq!(
        run.observations,
        json!(vec![json!({"ok": {"present": true}}); 8])
    );
    assert_eq!(
        run.trace,
        [
            lookup_args("elevenlabs"),
            lookup_args("microsoft"),
            lookup_args("xai"),
            lookup_args("mistral")
        ]
    );
}

#[tokio::test]
async fn an_unusable_key_is_rejected_before_the_keyring_is_touched() {
    let run = run(
        json!({"lookup": {"stdout": ""}}),
        &[],
        json!([
            {"op": "store_for", "provider": "microsoft", "value": "has a space"},
            {"op": "store_for", "provider": "xai", "value": ""},
        ]),
    )
    .await;
    assert_eq!(
        run.observations,
        json!([{"error": "Enter a valid API key."}, {"error": "Enter a valid API key."}])
    );
    assert!(run.trace.is_empty());
}
