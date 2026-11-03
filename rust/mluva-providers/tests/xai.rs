//! xAI Grok Voice Transcribe 2.0 over local protocol peers: batch uploads and the realtime stream,
//! including the finished segment that arrives twice.
use futures_util::{SinkExt, StreamExt};
use mluva_providers::{
    Secret, USER_AGENT,
    realtime::RealtimeOptions,
    xai::{XaiClient, XaiRealtimeClient, realtime_uri},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

mod support;
use support::{Response, http_once};

const KEY: &str = "synthetic-xai-credential";

fn batch_client(url: &str) -> XaiClient {
    XaiClient::new(Secret::new(KEY), url, Duration::from_secs(3)).unwrap()
}
fn realtime_client(url: &str) -> XaiRealtimeClient {
    XaiRealtimeClient::new(
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
async fn batch_sends_model_language_and_format_with_the_file_last() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        r#"{"text":"  Dobrý den.  ","language":"cs","duration":2.5}"#
            .as_bytes()
            .to_vec(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "ces").await.unwrap();
    assert_eq!(result.text, "Dobrý den.");
    assert_eq!(result.language_code, "ces");
    assert_eq!(result.audio_duration_seconds, Some(2.5));
    let request = captured.await.unwrap();
    server.await.unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(request.headers["user-agent"], USER_AGENT);
    let size = std::fs::read(&path).unwrap().len();
    assert_eq!(
        fields(&request.headers["content-type"], &request.body),
        [
            ("model".into(), "grok-voice-transcribe-2.0".into()),
            ("language".into(), "cs".into()),
            ("format".into(), "true".into()),
            ("file".into(), format!("{size} bytes")),
        ]
    );
}

#[tokio::test]
async fn batch_auto_language_sends_neither_language_nor_format() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        br#"{"text":"Hello.","language":"en"}"#.to_vec(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "auto").await.unwrap();
    assert_eq!(result.text, "Hello.");
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
    let (url, _, server) = http_once(Response::ok(br#"{"words":[]}"#.to_vec())).await;
    assert_eq!(
        batch_client(&url)
            .transcribe(&path, "eng")
            .await
            .unwrap_err()
            .to_string(),
        "xAI returned an invalid transcription response."
    );
    server.await.unwrap();
    for (status, message) in [
        (401, "xAI rejected the API key."),
        (403, "xAI rejected the API key."),
        (429, "xAI transcription is rate limited. Try again shortly."),
        (500, "xAI transcription failed with HTTP 500."),
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

#[test]
fn realtime_uri_selects_the_model_pcm_and_language() {
    assert_eq!(
        realtime_uri("wss://api.x.ai/v1/stt", "ces").unwrap(),
        "wss://api.x.ai/v1/stt?model=grok-voice-transcribe-2.0&encoding=pcm&sample_rate=16000&interim_results=true&language=cs"
    );
    // Automatic detection sends no language, and a configured query is replaced, not extended.
    assert_eq!(
        realtime_uri("wss://api.x.ai/v1/stt?language=de&extra=1", "auto").unwrap(),
        "wss://api.x.ai/v1/stt?model=grok-voice-transcribe-2.0&encoding=pcm&sample_rate=16000&interim_results=true"
    );
}

fn created() -> Message {
    Message::Text(
        json!({"type": "transcript.created", "id": "synthetic"})
            .to_string()
            .into(),
    )
}
fn partial(text: &str, start: f64, is_final: bool, speech_final: bool) -> Message {
    Message::Text(
        json!({
            "type": "transcript.partial",
            "text": text,
            "words": [],
            "is_final": is_final,
            "speech_final": speech_final,
            "start": start,
            "duration": 1.0,
            "language": "en",
        })
        .to_string()
        .into(),
    )
}
fn done() -> Message {
    Message::Text(
        json!({"type": "transcript.done", "text": "", "words": [], "duration": 2.0})
            .to_string()
            .into(),
    )
}

/// Collect binary PCM and text control messages until `audio.done`.
async fn receive_audio(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> (Vec<Vec<u8>>, Vec<Value>) {
    let mut pcm = vec![];
    let mut control = vec![];
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Binary(bytes) => pcm.push(bytes.to_vec()),
            Message::Text(text) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                let finished = value["type"] == "audio.done";
                control.push(value);
                if finished {
                    break;
                }
            }
            _ => {}
        }
    }
    (pcm, control)
}

#[tokio::test]
async fn realtime_sends_pcm_then_audio_done_and_counts_a_repeated_final_once() {
    let previews = Arc::new(Mutex::new(vec![]));
    let (url, handshake, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        socket
            .send(partial("Hel", 0.04, false, false))
            .await
            .unwrap();
        socket
            .send(partial("Hello there.", 0.04, false, false))
            .await
            .unwrap();
        let received = receive_audio(&mut socket).await;
        // The first segment arrives as is_final, then again as speech_final: one segment. The
        // finished start time differs from the interim start, as the live service's does.
        socket
            .send(partial("Hello there.", 0.5, true, false))
            .await
            .unwrap();
        socket
            .send(partial("Hello there.", 0.5, true, true))
            .await
            .unwrap();
        socket
            .send(partial("Second", 3.0, false, false))
            .await
            .unwrap();
        socket
            .send(partial("Second segment.", 3.0, true, false))
            .await
            .unwrap();
        socket
            .send(partial("Second segment.", 3.0, true, true))
            .await
            .unwrap();
        socket.send(done()).await.unwrap();
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
            "eng",
            Some(Box::new(move |preview| {
                seen.lock().unwrap().push(preview);
                Ok(())
            })),
            None,
        )
        .await
        .unwrap();
    let (path, headers) = handshake.await.unwrap();
    assert_eq!(
        path,
        "/realtime?model=grok-voice-transcribe-2.0&encoding=pcm&sample_rate=16000&interim_results=true&language=en"
    );
    assert_eq!(headers["authorization"], format!("Bearer {KEY}"));
    assert_eq!(headers["user-agent"], USER_AGENT);
    session.submit_audio(&[0; 3200]).unwrap();
    session.submit_audio(&[2; 1600]).unwrap();
    let result = session.finish().await.unwrap();
    // Built from the finished segments: the service's own final text is empty.
    assert_eq!(result.transcription.text, "Hello there. Second segment.");
    assert_eq!(result.transcription.language_code, "eng");
    assert_eq!(result.transcription.audio_duration_seconds, Some(0.15));
    let (pcm, control) = tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pcm, [vec![0; 3200], vec![2; 1600]]);
    assert_eq!(control, [json!({"type": "audio.done"})]);
    let previews = previews.lock().unwrap();
    assert!(
        previews
            .iter()
            .any(|p| p.volatile_text == "Hel" && p.committed_text.is_empty())
    );
    assert!(
        previews
            .iter()
            .any(|p| p.committed_text == "Hello there." && p.volatile_text.is_empty())
    );
    assert_eq!(
        previews.last().unwrap().committed_text,
        "Hello there. Second segment."
    );
    // Volatile words never became committed text on their own.
    assert!(
        previews
            .iter()
            .all(|p| !p.committed_text.contains("Hel ") && p.committed_text != "Second")
    );
}

#[tokio::test]
async fn realtime_without_speech_is_an_empty_result() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive_audio(&mut socket).await;
        socket.send(done()).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("auto", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "");
    assert_eq!(result.transcription.language_code, "und");
    server_task.await.unwrap();
}

#[tokio::test]
async fn realtime_empty_result_for_a_real_recording_fails_so_batch_recognizes_it() {
    for (chunks, outcome) in [(9, Ok("")), (10, Err(true))] {
        let (url, _, server_task) = support::websocket(|mut socket| async move {
            socket.send(created()).await.unwrap();
            receive_audio(&mut socket).await;
            socket.send(done()).await.unwrap();
        })
        .await;
        let session = realtime_client(&url)
            .start("eng", None, None)
            .await
            .unwrap();
        for _ in 0..chunks {
            session.submit_audio(&[0; 3200]).unwrap();
        }
        let result = session.finish().await;
        match outcome {
            Ok(text) => assert_eq!(result.unwrap().transcription.text, text, "{chunks} chunks"),
            Err(_) => assert_eq!(
                result.unwrap_err().to_string(),
                "xAI returned no realtime text for the recording.",
                "{chunks} chunks"
            ),
        }
        server_task.await.unwrap();
    }
}

#[tokio::test]
async fn realtime_finals_without_a_usable_start_never_overwrite_another_segment() {
    // A segment that really starts at 0 is followed by finals with no start, a null start and a
    // textual one: each is its own segment.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive_audio(&mut socket).await;
        let finished = |text: &str, start: Value| {
            let mut event = json!({"type": "transcript.partial", "text": text, "is_final": true, "speech_final": true});
            if !start.is_null() || text == "Null start." {
                event["start"] = start;
            }
            Message::Text(event.to_string().into())
        };
        for message in [
            finished("First.", json!(0.0)),
            finished("No start.", Value::Null),
            finished("Null start.", Value::Null),
            finished("Text start.", json!("soon")),
            // A repeat of a segment that has a start is still counted once.
            finished("First.", json!(0.0)),
        ] {
            socket.send(message).await.unwrap();
        }
        socket.send(done()).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    assert_eq!(
        session.finish().await.unwrap().transcription.text,
        "First. No start. Null start. Text start."
    );
    server_task.await.unwrap();
}

#[tokio::test]
async fn realtime_never_promotes_an_unfinished_partial() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive_audio(&mut socket).await;
        socket
            .send(partial("only a preview", 0.1, false, false))
            .await
            .unwrap();
        socket.send(done()).await.unwrap();
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "xAI did not return final text for the last words."
    );
    server_task.await.unwrap();
}

#[tokio::test]
async fn realtime_surfaces_errors_and_refuses_a_stream_that_never_becomes_ready() {
    // An error event after the stream is ready.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive_audio(&mut socket).await;
        socket
            .send(Message::Text(
                json!({"type": "error", "message": "private provider detail"})
                    .to_string()
                    .into(),
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
        "xAI realtime transcription reported an error."
    );
    session.cancel_and_wait().await;
    server_task.abort();

    // The first message is an error, anything but transcript.created, or nothing at all.
    let error_event = || {
        Message::Text(
            json!({"type": "error", "message": "private provider detail"})
                .to_string()
                .into(),
        )
    };
    for (first, expected) in [
        (
            Some(error_event()),
            "xAI realtime transcription reported an error.",
        ),
        (
            Some(partial("too early", 0.0, false, false)),
            "xAI did not confirm that realtime transcription was ready.",
        ),
        (
            Some(Message::Text("not json".into())),
            "xAI returned an invalid realtime transcription event.",
        ),
        (
            None,
            "xAI did not confirm that realtime transcription was ready.",
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
        let client = XaiRealtimeClient::new(
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
async fn realtime_rejects_bad_credentials_and_unreachable_endpoints() {
    for (status, expected) in [
        (
            401,
            "xAI rejected the credentials for realtime transcription.",
        ),
        (
            429,
            "xAI realtime transcription is rate limited or already in use. Try again shortly.",
        ),
        (
            500,
            "xAI realtime transcription could not reach the service.",
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/stt", listener.local_addr().unwrap());
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
    }
    assert!(
        XaiRealtimeClient::new(
            Secret::new(KEY),
            "https://api.x.ai/v1/stt",
            RealtimeOptions::default()
        )
        .is_err()
    );
    assert!(
        XaiRealtimeClient::new(
            Secret::new(KEY),
            "wss://user:pass@api.x.ai/v1/stt",
            RealtimeOptions::default()
        )
        .is_err()
    );
}

#[tokio::test]
async fn realtime_end_before_stop_and_dropped_connections_fail_the_stream() {
    // The service ends the stream while the person is still talking.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        socket.send(done()).await.unwrap();
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
    .expect("an early transcript.done marks the stream unhealthy");
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "xAI ended the realtime stream before recording stopped."
    );
    session.cancel_and_wait().await;
    server_task.abort();

    // The connection drops after audio was sent.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(created()).await.unwrap();
        receive_audio(&mut socket).await;
        let _ = socket.close(None).await;
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let error = session.finish().await.unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("The xAI realtime connection closed before transcription completed"),
        "{error}"
    );
    session.cancel_and_wait().await;
    server_task.abort();
}
