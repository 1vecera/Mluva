use mluva_core::screenshots::ImageInput;
use mluva_providers::{
    ProviderError, Secret,
    compatible::{CompatibleClient, MAX_HTTP_BYTES, RewriteOptions},
    elevenlabs::ElevenLabsClient,
};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

mod support;
use support::{Response, http_once, unhex};

fn compatible(url: &str, model: Option<&str>, timeout: Duration) -> CompatibleClient {
    CompatibleClient::new(url, "", model.map(str::to_owned), timeout).unwrap()
}
fn normalized<T: serde::Serialize>(result: Result<T, ProviderError>) -> Value {
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

#[tokio::test]
async fn fragmented_sse_and_exact_requests_match_released_rewrites() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-providers.json")).unwrap();
    for row in fixture["rewrites"].as_array().unwrap() {
        let mut response = Response::ok(unhex(row["body_hex"].as_str().unwrap()));
        response.fragment = 3;
        let (url, captured, server) = http_once(response).await;
        let client = compatible(&url, Some("writer"), Duration::from_secs(2));
        let mut deltas = vec![];
        let mut callback = |text: &str| deltas.push(text.to_owned());
        let result = client
            .transform(
                "Describe 🙂",
                Path::new("/"),
                RewriteOptions {
                    effort: Some("high"),
                    service_tier: Some("priority"),
                    ..Default::default()
                },
                Some(&mut callback),
            )
            .await;
        assert_eq!(
            normalized(result),
            expected(&row["result"]),
            "{}",
            row["name"]
        );
        assert_eq!(json!(deltas), row["deltas"], "{}", row["name"]);
        let request = captured.await.unwrap();
        server.await.unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, row["request"]["path"]);
        assert_eq!(
            request.body,
            unhex(row["request"]["body_hex"].as_str().unwrap())
        );
        assert_eq!(
            request.headers["content-type"],
            row["request"]["content_type"]
        );
        assert!(!request.headers.contains_key("authorization"));
    }
}

#[tokio::test]
async fn multipart_and_batch_results_match_released_wire_calls() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/released-wire.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("provider-wire.wav");
    std::fs::write(&path, b"RIFF\x00\x01\xffsynthetic").unwrap();
    for row in fixture["multipart"].as_array().unwrap() {
        let (url, captured, server) =
            http_once(Response::ok(serde_json::to_vec(&row["response"]).unwrap())).await;
        let language = row["language"].as_str().unwrap();
        let result = if row["family"] == "compatible" {
            compatible(&url, Some("alias"), Duration::from_secs(2))
                .transcribe(&path, language, "scribe")
                .await
        } else {
            let client = ElevenLabsClient::new(
                Secret::new("synthetic-wire-credential"),
                &url,
                Duration::from_secs(2),
            )
            .unwrap();
            if row["operation"] == "meeting" {
                client.transcribe_meeting(&path, language, "scribe").await
            } else {
                client.transcribe(&path, language, "scribe").await
            }
        };
        assert_eq!(normalized(result), expected(&row["result"]));
        let request = captured.await.unwrap();
        server.await.unwrap();
        let content_type = &request.headers["content-type"];
        let boundary = content_type.split_once("boundary=").unwrap().1;
        assert_eq!(
            content_type.replace(boundary, "BOUNDARY"),
            row["request"]["content_type"]
        );
        let normalized_body = replace(&request.body, boundary.as_bytes(), b"BOUNDARY");
        assert_eq!(
            normalized_body,
            unhex(row["request"]["body_hex"].as_str().unwrap())
        );
        if row["family"] == "elevenlabs" {
            assert_eq!(request.headers["xi-api-key"], row["request"]["credential"]);
            assert_eq!(row["request"]["user_agent"], "MluvaLinux/1.6.0");
            assert_eq!(
                request.headers["user-agent"],
                format!("MluvaLinux/{}", env!("CARGO_PKG_VERSION")),
            );
        }
    }
}

fn replace(body: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut output = vec![];
    let mut offset = 0;
    while offset < body.len() {
        if body[offset..].starts_with(needle) {
            output.extend_from_slice(replacement);
            offset += needle.len();
        } else {
            output.push(body[offset]);
            offset += 1;
        }
    }
    output
}

#[tokio::test]
async fn screenshot_order_offsets_and_upload_bytes_match_released_wire_requests() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/released-wire.json")).unwrap();
    for row in fixture["visual"].as_array().unwrap() {
        let png = unhex(row["png_hex"].as_str().unwrap());
        let images = row["offsets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|offset| ImageInput {
                data: png.clone(),
                captured_after_seconds: offset.as_f64(),
            })
            .collect::<Vec<_>>();
        let response=b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" visual result \"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".to_vec();
        let (url, captured, server) = http_once(Response::ok(response)).await;
        let result = compatible(&url, Some("visual-model"), Duration::from_secs(2))
            .transform(
                "Explain 🙂\x7f",
                Path::new("/"),
                RewriteOptions {
                    images: &images,
                    ..Default::default()
                },
                None,
            )
            .await;
        assert_eq!(normalized(result), expected(&row["result"]));
        let request = captured.await.unwrap();
        server.await.unwrap();
        assert_eq!(
            request.body,
            unhex(row["request"]["body_hex"].as_str().unwrap())
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let client = compatible(&url, Some("visual-model"), Duration::from_secs(1));
    let images = [ImageInput {
        data: b"damaged image".to_vec(),
        captured_after_seconds: None,
    }];
    assert_eq!(
        client
            .transform(
                "text",
                Path::new("/"),
                RewriteOptions {
                    images: &images,
                    ..Default::default()
                },
                None
            )
            .await
            .unwrap_err()
            .to_string(),
        "Screenshot is not a PNG image."
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn redirects_and_provider_failures_never_forward_content_or_echo_bodies() {
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = format!("http://{}/do-not-reach", trap.local_addr().unwrap());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private.wav");
    std::fs::write(&path, b"synthetic-private-audio").unwrap();
    for status in [301, 302, 307, 308, 401, 429, 500] {
        for elevenlabs in [false, true] {
            let mut response = Response::ok(b"synthetic-secret-provider-body".to_vec());
            response.status = status;
            response.headers.push(("Location".into(), target.clone()));
            let (url, captured, server) = http_once(response).await;
            let error = if elevenlabs {
                ElevenLabsClient::new(
                    Secret::new("synthetic-private-key"),
                    &url,
                    Duration::from_secs(2),
                )
                .unwrap()
                .transcribe(&path, "auto", "scribe")
                .await
                .unwrap_err()
            } else {
                compatible(&url, Some("model"), Duration::from_secs(2))
                    .transcribe(&path, "auto", "scribe")
                    .await
                    .unwrap_err()
            };
            let expected = if elevenlabs {
                format!("ElevenLabs transcription failed with HTTP {status}.")
            } else {
                format!("Provider request failed (HTTP {status}). Check model and credentials.")
            };
            assert_eq!(error.to_string(), expected);
            captured.await.unwrap();
            server.await.unwrap();
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(80), trap.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_interrupts_stalled_requests_and_close_allows_a_new_request() {
    for permanent in [true, false] {
        let mut response = Response::ok(vec![]);
        response.stall = true;
        let (url, captured, server) = http_once(response).await;
        let client = Arc::new(compatible(&url, None, Duration::from_secs(30)));
        let pending = {
            let client = client.clone();
            tokio::spawn(async move { client.list_models(None).await })
        };
        captured.await.unwrap();
        if permanent {
            client.cancel();
        } else {
            client.close();
        }
        let error = tokio::time::timeout(Duration::from_millis(500), pending)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.to_string(), "Request cancelled.");
        server.abort();
        let _ = server.await;
        if permanent {
            assert_eq!(
                client.list_models(None).await.unwrap_err().to_string(),
                "Request cancelled."
            );
        } else {
            // The same immutable endpoint remains usable after closing its current response.
            let endpoint = url.strip_prefix("http://").unwrap();
            let listener = TcpListener::bind(endpoint).await.unwrap();
            let next = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                support::request(&mut socket).await;
                support::respond(&mut socket, &Response::ok(br#"{"data":[]}"#.to_vec())).await;
            });
            assert!(client.list_models(None).await.unwrap().is_empty());
            next.await.unwrap();
        }
    }
}

#[tokio::test]
async fn response_size_document_size_and_socket_timeouts_are_enforced() {
    let (url, _, server) = http_once(Response::ok(vec![b' '; MAX_HTTP_BYTES + 1])).await;
    assert_eq!(
        compatible(&url, None, Duration::from_secs(2))
            .list_models(None)
            .await
            .unwrap_err()
            .to_string(),
        "The provider returned an invalid model catalog."
    );
    server.await.unwrap();
    let response = Response::ok(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"abc\"},\"finish_reason\":\"stop\"}]}\n\n"
            .to_vec(),
    );
    let (url, _, server) = http_once(response).await;
    let error = compatible(&url, Some("writer"), Duration::from_secs(2))
        .transform(
            "text",
            Path::new("/"),
            RewriteOptions {
                max_output_characters: 2,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "The rewrite exceeded the document size limit."
    );
    server.await.unwrap();
    let (url, _, server) = http_once(Response::ok(vec![b'x'; MAX_HTTP_BYTES + 1])).await;
    assert_eq!(
        compatible(&url, Some("writer"), Duration::from_secs(2))
            .transform("text", Path::new("/"), RewriteOptions::default(), None)
            .await
            .unwrap_err()
            .to_string(),
        "The provider stream exceeded the response size limit."
    );
    server.await.unwrap();
    let mut response = Response::ok(vec![]);
    response.stall = true;
    let (url, _, server) = http_once(response).await;
    let client = compatible(&url, None, Duration::from_millis(60));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), client.list_models(None))
            .await
            .unwrap()
            .unwrap_err()
            .to_string(),
        "Could not connect to the configured provider."
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn elevenlabs_large_meetings_preserve_the_released_unbounded_batch_contract() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("meeting.wav");
    std::fs::write(&path, b"synthetic").unwrap();
    let text = "meeting ".repeat(260_000);
    let body = serde_json::to_vec(&json!({"text":text,"language_code":"eng"})).unwrap();
    assert!(body.len() > MAX_HTTP_BYTES);
    let (url, _, server) = http_once(Response::ok(body)).await;
    let result = ElevenLabsClient::new(Secret::new("synthetic-key"), &url, Duration::from_secs(3))
        .unwrap()
        .transcribe_meeting(&path, "auto", "scribe")
        .await
        .unwrap();
    assert_eq!(result.text, text.trim());
    server.await.unwrap();
}

#[tokio::test]
async fn cancel_after_response_headers_discards_partial_rewrites_and_batch_text() {
    for elevenlabs in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (body_sent, body_seen) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            support::request(&mut socket).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99999\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let partial = if elevenlabs {
                br#"{"text":"private partial","language_code":"eng""#.as_slice()
            } else {
                b"data: {\"choices\":[{\"delta\":{\"content\":\"private provisional\"},\"finish_reason\":null}]}\n\n".as_slice()
            };
            socket.write_all(partial).await.unwrap();
            let _ = body_sent.send(());
            std::future::pending::<()>().await;
        });
        if elevenlabs {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("audio.wav");
            std::fs::write(&path, b"synthetic").unwrap();
            let client = Arc::new(
                ElevenLabsClient::new(
                    Secret::new("synthetic-private-key"),
                    &url,
                    Duration::from_secs(30),
                )
                .unwrap(),
            );
            let pending = {
                let client = client.clone();
                tokio::spawn(async move { client.transcribe(&path, "auto", "scribe").await })
            };
            body_seen.await.unwrap();
            client.cancel();
            let error = tokio::time::timeout(Duration::from_millis(500), pending)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                "ElevenLabs transcription could not reach the service."
            );
        } else {
            let client = Arc::new(compatible(&url, Some("writer"), Duration::from_secs(30)));
            let (delta_tx, delta_rx) = oneshot::channel();
            let pending = {
                let client = client.clone();
                tokio::spawn(async move {
                    let mut tx = Some(delta_tx);
                    let mut delta = move |text: &str| {
                        assert_eq!(text, "private provisional");
                        if let Some(tx) = tx.take() {
                            let _ = tx.send(());
                        }
                    };
                    client
                        .transform(
                            "text",
                            Path::new("/"),
                            RewriteOptions::default(),
                            Some(&mut delta),
                        )
                        .await
                })
            };
            body_seen.await.unwrap();
            delta_rx.await.unwrap();
            client.cancel();
            let error = tokio::time::timeout(Duration::from_millis(500), pending)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                "The provider returned an invalid or interrupted rewrite."
            );
        }
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn malformed_environment_credentials_are_rejected_without_network_or_secret_diagnostics() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_provider-fixture-peer"))
        .env_clear()
        .env(
            "MLUVA_FIXTURE_KEY",
            "synthetic-private-key\ninjected-header",
        )
        .args(["catalog", &url])
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({"error":"Could not connect to the configured provider."})
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(
        format!("{:?}", Secret::new("synthetic-private-key")),
        "<credential>"
    );
}
