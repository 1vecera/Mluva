//! Mistral Voxtral 3 over local protocol peers: batch uploads and the realtime stream, whose
//! deltas only preview and whose final message carries the committed text.
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use mluva_providers::{
    Secret, USER_AGENT,
    mistral::{MistralClient, MistralRealtimeClient, realtime_uri},
    realtime::RealtimeOptions,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

mod support;
use support::{Response, http_once};

const KEY: &str = "synthetic-mistral-credential";

fn batch_client(url: &str) -> MistralClient {
    MistralClient::new(Secret::new(KEY), url, Duration::from_secs(3)).unwrap()
}
fn realtime_client(url: &str) -> MistralRealtimeClient {
    MistralRealtimeClient::new(
        Secret::new(KEY),
        url,
        RealtimeOptions {
            session_timeout: Duration::from_secs(2),
            finalization_timeout: Duration::from_millis(600),
            ..Default::default()
        },
    )
    .unwrap()
}
fn audio_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.wav");
    std::fs::write(&path, b"RIFF\x00\x01\xffsynthetic-audio").unwrap();
    (directory, path)
}
/// Multipart field names in order, with each text value (the file's is its length).
fn fields(content_type: &str, body: &[u8]) -> Vec<(String, String)> {
    let boundary = content_type.split_once("boundary=").unwrap().1;
    let delimiter = format!("--{boundary}").into_bytes();
    let positions: Vec<usize> = (0..body.len())
        .filter(|index| body[*index..].starts_with(&delimiter))
        .collect();
    positions
        .windows(2)
        .map(|pair| {
            let part = &body[pair[0] + delimiter.len() + 2..pair[1] - 2];
            let split = part.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            let head = std::str::from_utf8(&part[..split]).unwrap();
            let name = head
                .split("name=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            let value = &part[split + 4..];
            (
                name.to_owned(),
                if name == "file" {
                    format!("{} bytes", value.len())
                } else {
                    String::from_utf8(value.to_vec()).unwrap()
                },
            )
        })
        .collect()
}

#[tokio::test]
async fn batch_sends_the_small_model_and_language_with_the_file_last() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        r#"{"text":"  Dobrý den.  ","language":"cs"}"#.as_bytes().to_vec(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "ces").await.unwrap();
    assert_eq!(result.text, "Dobrý den.");
    assert_eq!(result.language_code, "ces");
    let request = captured.await.unwrap();
    server.await.unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(request.headers["user-agent"], USER_AGENT);
    let size = std::fs::read(&path).unwrap().len();
    assert_eq!(
        fields(&request.headers["content-type"], &request.body),
        [
            ("model".into(), "voxtral-small-transcribe-3".into()),
            ("language".into(), "cs".into()),
            ("file".into(), format!("{size} bytes")),
        ]
    );
}

#[tokio::test]
async fn batch_auto_language_sends_no_language_and_reports_the_detected_one() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        br#"{"text":"Hello.","language":"en"}"#.to_vec(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "auto").await.unwrap();
    assert_eq!(result.language_code, "eng");
    let request = captured.await.unwrap();
    server.await.unwrap();
    let names: Vec<_> = fields(&request.headers["content-type"], &request.body)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["model", "file"]);
}

#[tokio::test]
async fn batch_empty_text_is_an_empty_result_and_failures_use_fixed_messages() {
    let (_directory, path) = audio_file();
    let (url, _, server) = http_once(Response::ok(br#"{"text":""}"#.to_vec())).await;
    assert_eq!(
        batch_client(&url)
            .transcribe(&path, "eng")
            .await
            .unwrap()
            .text,
        ""
    );
    server.await.unwrap();
    let (url, _, server) = http_once(Response::ok(br#"{"segments":[]}"#.to_vec())).await;
    assert_eq!(
        batch_client(&url)
            .transcribe(&path, "eng")
            .await
            .unwrap_err()
            .to_string(),
        "Mistral returned an invalid transcription response."
    );
    server.await.unwrap();
    for (status, message) in [
        (401, "Mistral rejected the API key."),
        (403, "Mistral rejected the API key."),
        (500, "Mistral transcription failed with HTTP 500."),
    ] {
        let mut response = Response::ok(b"synthetic-secret-provider-body".to_vec());
        response.status = status;
        let (url, _, server) = http_once(response).await;
        let error = batch_client(&url)
            .transcribe(&path, "eng")
            .await
            .unwrap_err();
        server.await.unwrap();
        assert_eq!(error.to_string(), message);
    }
}

async fn sequence(
    responses: Vec<Response>,
) -> (
    String,
    Arc<Mutex<Vec<support::Request>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let record = seen.clone();
    let server = tokio::spawn(async move {
        for response in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = support::request(&mut socket).await;
            record.lock().unwrap().push(request);
            support::respond(&mut socket, &response).await;
        }
    });
    (url, seen, server)
}
fn limited() -> Response {
    let mut response = Response::ok(b"{}".to_vec());
    response.status = 429;
    response.headers.push(("Retry-After".into(), "0".into()));
    response
}

#[tokio::test]
async fn batch_waits_out_a_burst_limit_with_the_same_upload_then_succeeds() {
    let (_directory, path) = audio_file();
    let ok = Response::ok(br#"{"text":"After the wait."}"#.to_vec());
    let (url, seen, server) = sequence(vec![limited(), limited(), ok]).await;
    let result = batch_client(&url).transcribe(&path, "eng").await.unwrap();
    server.await.unwrap();
    assert_eq!(result.text, "After the wait.");
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|request| request.body == requests[0].body)
    );
}

#[tokio::test]
async fn batch_gives_up_after_bounded_retries_with_a_clear_message() {
    let (_directory, path) = audio_file();
    let (url, seen, server) = sequence(vec![limited(), limited(), limited(), limited()]).await;
    let error = batch_client(&url)
        .transcribe(&path, "eng")
        .await
        .unwrap_err();
    server.await.unwrap();
    assert!(error.to_string().contains("rate limited"), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 4);
}

#[test]
fn realtime_uri_selects_the_model_and_replaces_a_configured_query() {
    assert_eq!(
        realtime_uri("wss://api.mistral.ai/v1/audio/transcriptions/realtime").unwrap(),
        "wss://api.mistral.ai/v1/audio/transcriptions/realtime?model=voxtral-mini-transcribe-realtime-3"
    );
    assert_eq!(
        realtime_uri("wss://api.mistral.ai/v1/audio/transcriptions/realtime?model=other&x=1")
            .unwrap(),
        "wss://api.mistral.ai/v1/audio/transcriptions/realtime?model=voxtral-mini-transcribe-realtime-3"
    );
}

fn event(value: Value) -> Message {
    Message::Text(value.to_string().into())
}
fn created() -> Message {
    event(json!({"type": "session.created", "session": {"id": "synthetic"}}))
}
fn delta(text: &str) -> Message {
    event(json!({"type": "transcription.text.delta", "text": text}))
}
fn done(text: &str) -> Message {
    event(json!({
        "type": "transcription.done",
        "model": "voxtral-mini-transcribe-realtime-3",
        "text": text,
        "language": "en",
        "segments": [],
        "usage": {"prompt_audio_seconds": 1},
    }))
}

/// What the client sent: the `session.update`, the decoded PCM chunks and the control messages.
struct Received {
    update: Option<Value>,
    pcm: Vec<Vec<u8>>,
    control: Vec<Value>,
}
async fn receive(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> Received {
    let mut received = Received {
        update: None,
        pcm: vec![],
        control: vec![],
    };
    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).unwrap();
        match value["type"].as_str().unwrap() {
            "session.update" => {
                received.update = Some(value);
                socket
                    .send(event(json!({"type": "session.updated"})))
                    .await
                    .unwrap();
            }
            "input_audio.append" => received
                .pcm
                .push(STANDARD.decode(value["audio"].as_str().unwrap()).unwrap()),
            "input_audio.end" => {
                received.control.push(value);
                break;
            }
            other => panic!("unexpected client message {other}"),
        }
    }
    received
}

#[tokio::test]
async fn realtime_updates_the_session_streams_pcm_and_commits_only_the_final_text() {
    let previews = Arc::new(Mutex::new(vec![]));
    let (url, handshake, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        let received = receive(&mut socket).await;
        // Deltas keep their own leading spaces; other message types carry no turn text.
        socket.send(delta("Hello")).await.unwrap();
        socket.send(delta(" there")).await.unwrap();
        socket
            .send(event(
                json!({"type": "transcription.language", "language": "en"}),
            ))
            .await
            .unwrap();
        socket
            .send(event(
                json!({"type": "transcription.segment", "text": "Hello there", "start": 0.0}),
            ))
            .await
            .unwrap();
        socket.send(done("  Hello there.  ")).await.unwrap();
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
        received
    })
    .await;
    let seen = previews.clone();
    let session = realtime_client(&url)
        .start(
            "ces",
            Some(Box::new(move |preview| {
                seen.lock().unwrap().push(preview);
                Ok(())
            })),
            None,
        )
        .await
        .unwrap();
    let (path, headers) = handshake.await.unwrap();
    assert_eq!(path, "/realtime?model=voxtral-mini-transcribe-realtime-3");
    assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(headers["user-agent"], USER_AGENT);
    session.submit_audio(&[0; 3200]).unwrap();
    session.submit_audio(&[5; 1600]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "Hello there.");
    assert_eq!(result.transcription.language_code, "ces");
    assert_eq!(result.transcription.audio_duration_seconds, Some(0.15));
    let received = tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        received.update.unwrap(),
        json!({"type": "session.update", "session": {
            "audio_format": {"encoding": "pcm_s16le", "sample_rate": 16000},
            "language": "cs",
        }})
    );
    assert_eq!(received.pcm, [vec![0; 3200], vec![5; 1600]]);
    assert_eq!(received.control, [json!({"type": "input_audio.end"})]);

    let previews = previews.lock().unwrap();
    // The deltas built a volatile preview with nothing committed ...
    assert!(
        previews
            .iter()
            .any(|p| p.volatile_text == "Hello there" && p.committed_text.is_empty())
    );
    assert!(
        previews
            .iter()
            .take_while(|p| p.committed_text.is_empty())
            .count()
            >= 2
    );
    // ... and only the final message committed text.
    assert_eq!(previews.last().unwrap().committed_text, "Hello there.");
    assert_eq!(previews.last().unwrap().volatile_text, "");
}

#[tokio::test]
async fn realtime_auto_language_sets_no_language_and_a_typeless_final_still_ends_the_turn() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        let received = receive(&mut socket).await;
        // The final object is recognized by its text and usage, whatever its type.
        socket
            .send(event(
                json!({"text": "Auto text.", "usage": {"seconds": 1}, "language": "en"}),
            ))
            .await
            .unwrap();
        received
    })
    .await;
    let session = realtime_client(&url)
        .start("auto", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "Auto text.");
    assert_eq!(result.transcription.language_code, "und");
    let received = server_task.await.unwrap();
    assert!(
        received.update.unwrap()["session"]
            .get("language")
            .is_none()
    );
}

#[tokio::test]
async fn realtime_without_speech_is_empty_but_words_that_never_finish_are_not() {
    // Nothing recognized: a valid, empty result.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive(&mut socket).await;
        socket.send(done("")).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    assert_eq!(session.finish().await.unwrap().transcription.text, "");
    server_task.await.unwrap();

    // A real recording (one second or more) with an empty final is a failed stream, not silence.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive(&mut socket).await;
        socket.send(done("")).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    for _ in 0..10 {
        session.submit_audio(&[0; 3200]).unwrap();
    }
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Mistral returned no realtime text for the recording."
    );
    server_task.await.unwrap();

    // Deltas arrived, but the final text is empty: never promote the preview.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive(&mut socket).await;
        socket.send(delta("only a preview")).await.unwrap();
        socket.send(done("")).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Mistral did not return final text for the last words."
    );
    server_task.await.unwrap();

    // No final message at all: the finalization timeout fails the stream.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive(&mut socket).await;
        socket.send(delta("never finished")).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Mistral did not return a final realtime transcript in time."
    );
    session.cancel_and_wait().await;
    server_task.abort();
}

#[tokio::test]
async fn realtime_surfaces_errors_early_finals_and_refused_streams() {
    // An error after the stream is ready.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive(&mut socket).await;
        socket
            .send(event(
                json!({"type": "error", "message": "private provider detail"}),
            ))
            .await
            .unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let error = session.finish().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Mistral realtime transcription reported an error."
    );
    session.cancel_and_wait().await;
    server_task.abort();

    // A final message while the person is still talking ends the stream early.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        socket.send(done("Too early.")).await.unwrap();
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.is_healthy() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("an early final marks the stream unhealthy");
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Mistral ended the realtime stream before recording stopped."
    );
    session.cancel_and_wait().await;
    server_task.abort();

    // The first message must be session.created.
    let error_event = || event(json!({"type": "error", "message": "private provider detail"}));
    for (first, expected) in [
        (
            Some(error_event()),
            "Mistral realtime transcription reported an error.",
        ),
        (
            Some(delta("too early")),
            "Mistral did not confirm that realtime transcription was ready.",
        ),
        (
            Some(Message::Text("not json".into())),
            "Mistral returned an invalid realtime transcription event.",
        ),
        (
            None,
            "Mistral did not confirm that realtime transcription was ready.",
        ),
    ] {
        let (url, _, server_task) = support::websocket(move |mut socket| async move {
            if let Some(message) = first {
                socket.send(message).await.unwrap();
            }
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await;
        let client = MistralRealtimeClient::new(
            Secret::new(KEY),
            &url,
            RealtimeOptions {
                session_timeout: Duration::from_millis(300),
                ..Default::default()
            },
        )
        .unwrap();
        let error = client.start("eng", None, None).await.err().unwrap();
        assert_eq!(error.to_string(), expected);
        assert!(!error.to_string().contains("private"));
        tokio::time::timeout(Duration::from_secs(3), server_task)
            .await
            .expect("the client closes a refused stream")
            .unwrap();
    }
}

#[tokio::test]
async fn realtime_handshake_limits_and_bad_credentials_are_clear_and_never_echo_the_body() {
    for (status, expected) in [
        (
            401,
            "Mistral rejected the credentials for realtime transcription.",
        ),
        (
            429,
            "Mistral realtime transcription is rate limited or already in use. Try again shortly.",
        ),
        (
            500,
            "Mistral realtime transcription could not reach the service.",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/realtime", listener.local_addr().unwrap());
        let server_task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = support::request(&mut socket).await;
            let mut response = Response::ok(b"synthetic-secret-provider-body".to_vec());
            response.status = status;
            support::respond(&mut socket, &response).await;
        });
        let error = realtime_client(&url)
            .start("eng", None, None)
            .await
            .err()
            .unwrap();
        server_task.await.unwrap();
        assert_eq!(error.to_string(), expected);
        assert!(!error.to_string().contains("synthetic"));
    }
    assert!(
        MistralRealtimeClient::new(
            Secret::new(KEY),
            "https://api.mistral.ai/x",
            RealtimeOptions::default()
        )
        .is_err()
    );
    assert!(
        MistralRealtimeClient::new(
            Secret::new(KEY),
            "wss://u:p@api.mistral.ai/x",
            RealtimeOptions::default()
        )
        .is_err()
    );
}
