//! External synthetic keyring and public credential/provider driver; never installed.
use mluva_providers::{
    ProviderError, Secret,
    credentials::{CredentialStore, SPEECH_KEY_VARIABLES, speech_key_from_environment},
    elevenlabs::ElevenLabsClient,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::Duration;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn unhex(bytes: &str) -> Vec<u8> {
    bytes
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn stream(value: &Value, name: &str) -> Vec<u8> {
    if let Some(hex) = value.get(format!("{name}_hex")).and_then(Value::as_str) {
        unhex(hex)
    } else {
        value[name].as_str().unwrap_or("").as_bytes().to_vec()
    }
}
fn serve(args: &[String]) {
    let root = std::env::var_os("CREDENTIAL_FIXTURE_ROOT").unwrap();
    let root = Path::new(&root);
    let path = root.join("keyring.json");
    let mut spec: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let operation = &args[0];
    let options = &spec[operation];
    std::fs::write(root.join("active.pid"), std::process::id().to_string()).unwrap();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    if options["diagnostic_first"] == true {
        stderr.write_all(&vec![b'x'; 131_072]).unwrap();
        stderr.flush().unwrap();
    }
    let mut input = vec![];
    if operation == "store" && options["early_exit"] != true {
        std::io::stdin().read_to_end(&mut input).unwrap();
    }
    let trace = json!({"args":args,"input_hex":hex(&input)});
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("trace.jsonl"))
        .unwrap();
    let mut line = serde_json::to_vec(&trace).unwrap();
    line.push(b'\n');
    file.write_all(&line).unwrap();
    let sleep = options["sleep_ms"].as_u64().unwrap_or(0);
    let exit = options["exit"].as_i64().unwrap_or(0) as i32;
    let out = stream(options, "stdout");
    let err = stream(options, "stderr");
    std::thread::sleep(Duration::from_millis(sleep));
    stdout.write_all(&out).unwrap();
    stderr.write_all(&err).unwrap();
    stdout.flush().unwrap();
    stderr.flush().unwrap();
    if operation == "store" && exit == 0 && options["early_exit"] != true {
        spec["lookup"] = json!({"stdout":format!("{}\n",String::from_utf8(input).unwrap())});
        std::fs::write(path, serde_json::to_vec(&spec).unwrap()).unwrap();
    }
    std::process::exit(exit);
}

fn result(value: Result<Value, ProviderError>) -> Value {
    match value {
        Ok(value) => json!({"ok":value}),
        Err(error) if error.to_string() == "Desktop keyring returned invalid text." => {
            json!({"error_kind":"invalid_text"})
        }
        Err(error) if error.to_string() == "ElevenLabs credential contains invalid text." => {
            json!({"error_kind":"invalid_credential"})
        }
        Err(error) => json!({"error":error.to_string()}),
    }
}
async fn send_key(
    key: Result<Secret, ProviderError>,
    spec: &Value,
) -> Result<Value, ProviderError> {
    let key = key?;
    assert_eq!(format!("{key:?}"), "<credential>");
    let client = ElevenLabsClient::new(
        key,
        spec["endpoint"].as_str().unwrap(),
        Duration::from_secs(2),
    )?;
    client
        .transcribe(
            Path::new(spec["audio_path"].as_str().unwrap()),
            "auto",
            "scribe_v2",
        )
        .await?;
    Ok(json!("sent"))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("lookup" | "store")) {
        serve(&args);
    }
    let mut input = vec![];
    std::io::stdin().read_to_end(&mut input).unwrap();
    let spec: Value = serde_json::from_slice(&input).unwrap();
    let root = std::env::var_os("CREDENTIAL_FIXTURE_ROOT").unwrap();
    let root = Path::new(&root);
    let store = CredentialStore::new();
    let mut observations = vec![];
    for operation in spec["operations"].as_array().unwrap() {
        let value = match operation["op"].as_str().unwrap() {
            "fixture" => {
                std::fs::write(
                    root.join("keyring.json"),
                    serde_json::to_vec(&operation["value"]).unwrap(),
                )
                .unwrap();
                Ok(Value::Null)
            }
            "key" => send_key(store.elevenlabs_api_key().await, &spec).await,
            "lookup" => store
                .stored_speech_key()
                .await
                .map(|key| json!({"present":key.is_some()})),
            "environment" => {
                let environ: HashMap<String, String> =
                    serde_json::from_value(operation["value"].clone()).unwrap();
                let names = operation["names"].as_array().map(|names| {
                    names
                        .iter()
                        .map(|name| name.as_str().unwrap())
                        .collect::<Vec<_>>()
                });
                send_key(
                    speech_key_from_environment(
                        &environ,
                        names.as_deref().unwrap_or(SPEECH_KEY_VARIABLES),
                    ),
                    &spec,
                )
                .await
            }
            "store" => store
                .store_speech_key(operation["value"].as_str().unwrap())
                .await
                .map(|()| Value::Null),
            _ => panic!("Unknown fixture operation"),
        };
        observations.push(result(value));
    }
    println!("{}", json!(observations));
}
