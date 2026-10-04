//! Actual native local clients and owned processes against unchanged released previews.

use mluva_providers::{batch_preview::BatchPreviewSession, local_preview::LocalPreviewClient};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};

#[path = "support/local_models.rs"]
mod local_models;
use local_models::{setup, trace};

fn state(root: &Path) -> Value {
    let starts = trace(root)
        .into_iter()
        .filter(|row| row["kind"] == "start")
        .collect::<Vec<_>>();
    json!({"processes":starts.len(),"alive":starts.iter().map(|row|Path::new(&format!("/proc/{}",row["pid"])).exists()).collect::<Vec<_>>(),
        "keys_exist":starts.iter().filter_map(|row|row["key_path"].as_str().map(|path|Path::new(path).exists())).collect::<Vec<_>>()})
}
fn observe(session: &BatchPreviewSession, root: &Path) -> Value {
    json!({"preview":session.snapshot(),"healthy":session.is_healthy(),"bytes_sent":session.bytes_sent(),"state":state(root)})
}
fn empty(directory: &Path) -> bool {
    !directory.exists() || fs::read_dir(directory).unwrap().next().is_none()
}
async fn until(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("Native local preview did not reach the observed boundary");
}

#[tokio::test]
async fn local_worker_reuse_partial_privacy_and_finalization_match_release() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-local-preview.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for row in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let options = setup(
            root,
            &row["spec"],
            Path::new(env!("CARGO_BIN_EXE_qwen-fixture-peer")),
            Path::new(env!("CARGO_BIN_EXE_local-asr-fixture-peer")),
        );
        let staging = root.join("previews");
        let session = LocalPreviewClient::new(options, staging.clone()).start("auto");
        assert_eq!(state(root)["processes"], 0);
        assert!(!staging.exists());
        let mut observations = vec![];
        for action in row["actions"].as_array().unwrap() {
            if let Some(enabled) = action["enabled"].as_bool() {
                session.set_preview_enabled(enabled);
            }
            if let Some(size) = action["submit"].as_u64() {
                session.submit_audio(&vec![0; size as usize]);
            }
            if action["wait_request"] == true {
                until(|| trace(root).iter().any(|row| row["kind"] == "request")).await;
            }
            if let Some(text) = action["wait"].as_str() {
                until(|| session.snapshot().committed_text == text && empty(&staging)).await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            if let Some(text) = action["wait_partial"].as_str() {
                until(|| session.snapshot().committed_text == text).await;
                assert_eq!(
                    trace(root)
                        .iter()
                        .filter(|row| row["kind"] == "fragment")
                        .map(|row| row["index"].clone())
                        .collect::<Vec<_>>(),
                    vec![json!(0)],
                    "Partial control must precede final HTTP bytes"
                );
                assert!(!empty(&staging));
            }
            if action["wait_unhealthy"] == true {
                until(|| !session.is_healthy()).await;
                until(|| {
                    state(root)["alive"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|value| value == false)
                })
                .await;
            }
            if action["cancel"] == true {
                tokio::time::timeout(Duration::from_secs(4), session.cancel_and_wait())
                    .await
                    .unwrap();
                until(|| {
                    state(root)["alive"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|value| value == false)
                })
                .await;
            }
            observations.push(observe(&session, root));
        }
        let terminal = if row["finish"] == true {
            match session.finish().await {
                Ok(result) => json!({"ok":result.transcription}),
                Err(error) => json!({"error":error.to_string()}),
            }
        } else {
            Value::Null
        };
        let finished = observe(&session, root);
        tokio::time::timeout(Duration::from_secs(4), session.cancel_and_wait())
            .await
            .unwrap();
        assert!(empty(&staging));
        let closed = observe(&session, root);
        let mut audio = vec![];
        let mut requests = vec![];
        for event in trace(root) {
            if event["kind"] == "wav" {
                let mut event = event.clone();
                event.as_object_mut().unwrap().remove("kind");
                audio.push(event);
            }
            if event["kind"] == "request" {
                requests.push(if row["spec"]["model"] == "qwen3-1.7b" {
                    event["payload"].clone()
                } else {
                    json!({"path":"$AUDIO","language":event["payload"]["language"]})
                });
            }
        }
        let actual = json!({"observations":observations,"terminal":terminal,"finished":finished,"closed":closed,"audio":audio,"requests":requests,"processes":state(root)["processes"]});
        let expected = json!({"observations":row["observations"],"terminal":row["terminal"],"finished":row["finished"],"closed":row["closed"],"audio":row["audio"],"requests":row["requests"],"processes":row["processes"]});
        if actual != expected {
            fs::write(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/local-preview-mismatch.json"),
                serde_json::to_vec_pretty(
                    &json!({"case":row["name"],"actual":actual,"expected":expected}),
                )
                .unwrap(),
            )
            .unwrap();
            panic!(
                "Released local preview differs: {}; report in tmp/local-preview-mismatch.json",
                row["name"]
            );
        }
    }
}
