//! Microsoft MAI-Transcribe-2 over local protocol peers: fast-transcription uploads and the Azure
//! Speech WebSocket stream, with synthetic audio and a synthetic key.
use futures_util::{SinkExt, StreamExt};
use mluva_providers::{
    ProviderError, Secret, USER_AGENT,
    microsoft::{MicrosoftClient, MicrosoftRealtimeClient, batch_endpoint, realtime_endpoint},
    realtime::RealtimeOptions,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

mod support;
use support::{Response, http_once};

const KEY: &str = "synthetic-azure-credential";

fn batch_client(url: &str) -> MicrosoftClient {
    MicrosoftClient::new(Secret::new(KEY), url, Duration::from_secs(3)).unwrap()
}
fn realtime_client(url: &str) -> MicrosoftRealtimeClient {
    MicrosoftRealtimeClient::new(
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

/// The named multipart parts as (name, content type, body).
fn parts(content_type: &str, body: &[u8]) -> Vec<(String, String, Vec<u8>)> {
    let boundary = content_type.split_once("boundary=").unwrap().1;
    let delimiter = format!("--{boundary}").into_bytes();
    let mut found = vec![];
    let positions: Vec<usize> = (0..body.len())
        .filter(|index| body[*index..].starts_with(&delimiter))
        .collect();
    for pair in positions.windows(2) {
        let part = &body[pair[0] + delimiter.len() + 2..pair[1] - 2];
        let split = part.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = std::str::from_utf8(&part[..split]).unwrap();
        let name = head
            .split("name=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned();
        let kind = head
            .lines()
            .find_map(|line| line.strip_prefix("Content-Type: "))
            .unwrap_or("")
            .to_owned();
        found.push((name, kind, part[split + 4..].to_vec()));
    }
    found
}

#[test]
fn endpoints_pin_the_region_and_refuse_hostile_names() {
    assert_eq!(
        batch_endpoint("swedencentral").unwrap(),
        "https://swedencentral.api.cognitive.microsoft.com/speechtotext/transcriptions:transcribe?api-version=2025-10-15"
    );
    assert_eq!(
        realtime_endpoint("eastus2").unwrap(),
        "wss://eastus2.stt.speech.microsoft.com/stt/speech/universal/v2"
    );
    for hostile in [
        "",
        "a",
        "Sweden",
        "evil.example.com/",
        "x@y",
        "sweden central",
        "eastus2/../",
    ] {
        assert!(batch_endpoint(hostile).is_err(), "{hostile}");
        assert!(realtime_endpoint(hostile).is_err(), "{hostile}");
    }
}

#[tokio::test]
async fn batch_posts_the_definition_and_audio_and_joins_the_phrases() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        serde_json::to_vec(&json!({
            "durationMilliseconds": 5400,
            "combinedPhrases": [{"text": " První věta. "}, {"text": ""}, {"text": "Druhá věta."}],
            "phrases": [{"locale": "cs", "text": "x"}],
        }))
        .unwrap(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "ces").await.unwrap();
    assert_eq!(result.text, "První věta. Druhá věta.");
    assert_eq!(result.language_code, "ces");
    assert_eq!(result.audio_duration_seconds, Some(5.4));
    let request = captured.await.unwrap();
    server.await.unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.headers["ocp-apim-subscription-key"], KEY);
    assert_eq!(request.headers["user-agent"], USER_AGENT);
    let parts = parts(&request.headers["content-type"], &request.body);
    assert_eq!(
        parts.iter().map(|part| part.0.as_str()).collect::<Vec<_>>(),
        ["definition", "audio"]
    );
    let definition: Value = serde_json::from_slice(&parts[0].2).unwrap();
    assert_eq!(
        definition,
        json!({
            "locales": ["cs"],
            "enhancedMode": {
                "enabled": true,
                "model": "MAI-Transcribe-2",
                "modelOptions": {"transcribeStyle": "verbatim"},
            },
        })
    );
    assert_eq!(parts[1].1, "audio/wav");
    assert_eq!(parts[1].2, std::fs::read(&path).unwrap());
}

#[tokio::test]
async fn batch_auto_language_sends_no_locale_and_reports_the_detected_one() {
    let (_directory, path) = audio_file();
    let (url, captured, server) = http_once(Response::ok(
        serde_json::to_vec(&json!({
            "combinedPhrases": [{"text": "Hello."}],
            "phrases": [{"locale": "en-US"}],
        }))
        .unwrap(),
    ))
    .await;
    let result = batch_client(&url).transcribe(&path, "auto").await.unwrap();
    assert_eq!(result.text, "Hello.");
    assert_eq!(result.language_code, "eng");
    let request = captured.await.unwrap();
    server.await.unwrap();
    let parts = parts(&request.headers["content-type"], &request.body);
    let definition: Value = serde_json::from_slice(&parts[0].2).unwrap();
    assert!(definition.get("locales").is_none());
}

#[tokio::test]
async fn batch_without_speech_is_an_empty_result_and_a_bad_payload_is_an_error() {
    let (_directory, path) = audio_file();
    for (body, expected) in [
        (json!({"combinedPhrases": []}), Ok("")),
        (
            json!({"phrases": []}),
            Err("Microsoft returned an invalid transcription response."),
        ),
        (
            json!({"combinedPhrases": [{"nothing": 1}]}),
            Err("Microsoft returned an invalid transcription response."),
        ),
    ] {
        let (url, _, server) = http_once(Response::ok(serde_json::to_vec(&body).unwrap())).await;
        let result = batch_client(&url).transcribe(&path, "eng").await;
        server.await.unwrap();
        match expected {
            Ok(text) => assert_eq!(result.unwrap().text, text),
            Err(message) => assert_eq!(result.unwrap_err().to_string(), message),
        }
    }
}

#[tokio::test]
async fn batch_statuses_map_to_fixed_messages_that_never_echo_the_body() {
    let (_directory, path) = audio_file();
    for (status, message) in [
        (401, "Microsoft rejected the Azure Speech key or region."),
        (403, "Microsoft rejected the Azure Speech key or region."),
        (500, "Microsoft transcription failed with HTTP 500."),
        (400, "Microsoft transcription failed with HTTP 400."),
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

/// A listener that answers each connection with the next response and records every request.
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

#[tokio::test]
async fn batch_retries_a_rate_limit_after_retry_after_then_succeeds() {
    let (_directory, path) = audio_file();
    let mut limited = Response::ok(b"{}".to_vec());
    limited.status = 429;
    limited.headers.push(("Retry-After".into(), "0".into()));
    let ok = Response::ok(br#"{"combinedPhrases":[{"text":"After the wait."}]}"#.to_vec());
    let (url, seen, server) =
        sequence(vec![limited.clone_for_test(), limited.clone_for_test(), ok]).await;
    let result = batch_client(&url).transcribe(&path, "eng").await.unwrap();
    server.await.unwrap();
    assert_eq!(result.text, "After the wait.");
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 3);
    // Every attempt carries the same complete upload.
    assert!(
        requests
            .iter()
            .all(|request| request.body == requests[0].body)
    );
}

#[tokio::test]
async fn batch_gives_up_after_bounded_rate_limit_retries() {
    let (_directory, path) = audio_file();
    let mut limited = Response::ok(b"{}".to_vec());
    limited.status = 429;
    limited.headers.push(("Retry-After".into(), "0".into()));
    let (url, seen, server) = sequence(vec![
        limited.clone_for_test(),
        limited.clone_for_test(),
        limited.clone_for_test(),
        limited.clone_for_test(),
    ])
    .await;
    let error = batch_client(&url)
        .transcribe(&path, "eng")
        .await
        .unwrap_err();
    server.await.unwrap();
    assert!(error.to_string().contains("rate limited"), "{error}");
    assert_eq!(seen.lock().unwrap().len(), 4);
}

trait CloneResponse {
    fn clone_for_test(&self) -> Response;
}
impl CloneResponse for Response {
    fn clone_for_test(&self) -> Response {
        Response {
            status: self.status,
            headers: self.headers.clone(),
            body: self.body.clone(),
            fragment: self.fragment,
            stall: self.stall,
        }
    }
}

// ---------------------------------------------------------------- realtime

#[derive(Debug)]
struct Frame {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}
impl Frame {
    fn header(&self, name: &str) -> &str {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .unwrap_or_else(|| panic!("missing header {name}"))
    }
}
fn headers(block: &str) -> Vec<(String, String)> {
    block
        .split("\r\n")
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (key, value) = line.split_once(':').unwrap();
            (key.trim().to_owned(), value.trim().to_owned())
        })
        .collect()
}
fn text_frame(message: Message) -> Frame {
    let Message::Text(text) = message else {
        panic!("expected a text frame, got {message:?}");
    };
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    Frame {
        headers: headers(head),
        body: body.as_bytes().to_vec(),
    }
}
fn audio_frame(message: Message) -> Frame {
    let Message::Binary(bytes) = message else {
        panic!("expected a binary frame, got {message:?}");
    };
    let length = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    Frame {
        headers: headers(std::str::from_utf8(&bytes[2..2 + length]).unwrap()),
        body: bytes[2 + length..].to_vec(),
    }
}
fn server(path: &str, body: Value) -> Message {
    Message::Text(format!("Path: {path}\r\nContent-Type: application/json\r\n\r\n{body}").into())
}
fn phrase(text: &str, offset: i64) -> Message {
    server(
        "speech.phrase",
        json!({"RecognitionStatus": "Success", "DisplayText": text, "Offset": offset, "Duration": 1}),
    )
}
fn end_of_turn() -> Vec<Message> {
    vec![
        server("speech.endDetected", json!({"Offset": 1})),
        server(
            "speech.phrase",
            json!({"RecognitionStatus": "EndOfDictation", "Offset": 1, "Duration": 0}),
        ),
        server("turn.end", json!({})),
    ]
}

/// Read the two setup frames, then audio until the empty end-of-audio frame; returns what arrived.
async fn receive_turn(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
) -> (Vec<Frame>, Vec<Frame>) {
    let mut setup = vec![];
    let mut audio = vec![];
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(_) => setup.push(text_frame(message)),
            Message::Binary(_) => {
                let frame = audio_frame(message);
                let ended = frame.body.is_empty();
                audio.push(frame);
                if ended {
                    break;
                }
            }
            _ => {}
        }
    }
    (setup, audio)
}

#[tokio::test]
async fn realtime_frames_follow_the_azure_speech_protocol_and_segments_join_at_turn_end() {
    let previews = Arc::new(Mutex::new(vec![]));
    let (url, handshake, server_task) = support::websocket(|mut socket| async move {
        let (setup, audio) = receive_turn(&mut socket).await;
        socket
            .send(server(
                "turn.start",
                json!({"context": {"serviceTag": "t"}}),
            ))
            .await
            .unwrap();
        socket
            .send(server("speech.startDetected", json!({"Offset": 0})))
            .await
            .unwrap();
        socket
            .send(server(
                "speech.hypothesis",
                json!({"Text": "  první  ", "Channel": 0}),
            ))
            .await
            .unwrap();
        socket.send(phrase(" První věta. ", 0)).await.unwrap();
        socket
            .send(server("speech.hypothesis", json!({"Text": "druhá"})))
            .await
            .unwrap();
        // The same finished phrase twice is one segment.
        socket
            .send(phrase("Druhá věta.", 40_000_000))
            .await
            .unwrap();
        socket
            .send(phrase("Druhá věta.", 40_000_000))
            .await
            .unwrap();
        for message in end_of_turn() {
            socket.send(message).await.unwrap();
        }
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
        (setup, audio)
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
    let (path, request_headers) = handshake.await.unwrap();
    assert_eq!(path, "/realtime");
    assert_eq!(request_headers["ocp-apim-subscription-key"], KEY);
    assert_eq!(request_headers["x-connectionid"].len(), 32);
    assert!(
        request_headers["x-connectionid"]
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    );
    assert_eq!(request_headers["user-agent"], USER_AGENT);
    // Two 100 ms chunks of 16 kHz 16-bit mono silence.
    session.submit_audio(&[0; 3200]).unwrap();
    session.submit_audio(&[1; 3200]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "První věta. Druhá věta.");
    assert_eq!(result.transcription.language_code, "ces");
    assert_eq!(result.transcription.audio_duration_seconds, Some(0.2));
    let (setup, audio) = tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .unwrap()
        .unwrap();

    // Setup frames: config, then context, one request id for the whole turn.
    assert_eq!(setup.len(), 2);
    assert_eq!(setup[0].header("Path"), "speech.config");
    assert_eq!(setup[1].header("Path"), "speech.context");
    let request_id = setup[0].header("X-RequestId").to_owned();
    assert_eq!(request_id.len(), 32);
    assert!(request_id.chars().all(|c| c.is_ascii_hexdigit()));
    for frame in setup.iter().chain(audio.iter()) {
        assert_eq!(frame.header("X-RequestId"), request_id);
        let timestamp = frame.header("X-Timestamp");
        assert!(
            timestamp.len() == 24 && timestamp.ends_with('Z') && timestamp.as_bytes()[19] == b'.',
            "ISO-8601 UTC milliseconds: {timestamp}"
        );
    }
    assert_eq!(setup[0].header("Content-Type"), "application/json");
    let config: Value = serde_json::from_slice(&setup[0].body).unwrap();
    assert_eq!(
        config,
        json!({"context": {
            "system": {"name": "Mluva", "version": env!("CARGO_PKG_VERSION")},
            "audio": {"source": {"type": "Stream", "samplerate": "16000", "bitspersample": "16", "channelcount": "1"}},
        }})
    );
    let context: Value = serde_json::from_slice(&setup[1].body).unwrap();
    assert_eq!(
        context,
        json!({
            "phraseDetection": {"mode": "CONVERSATION", "language": "cs-CZ"},
            "model": {"name": "mai-transcribe-2-streaming"},
            "audio": {"streams": {"1": null}},
        })
    );

    // Audio frames: a WAV header frame, the chunks, then an empty payload.
    assert_eq!(audio.len(), 4);
    for frame in &audio {
        assert_eq!(frame.header("Path"), "audio");
        assert_eq!(frame.header("Content-Type"), "audio/x-wav");
    }
    let wav = &audio[0].body;
    assert_eq!(wav.len(), 44);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[4..8], &[0; 4]);
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1, "PCM");
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
    assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16, "bits");
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(&wav[40..44], &[0; 4]);
    assert_eq!(audio[1].body, vec![0; 3200]);
    assert_eq!(audio[2].body, vec![1; 3200]);
    assert!(audio[3].body.is_empty());

    // Volatile hypotheses reached the preview, finished phrases became committed text.
    let previews = previews.lock().unwrap();
    assert!(
        previews
            .iter()
            .any(|p| p.volatile_text == "první" && p.committed_text.is_empty())
    );
    assert!(
        previews
            .iter()
            .any(|p| p.committed_text == "První věta." && p.volatile_text.is_empty())
    );
    assert_eq!(
        previews.last().unwrap().committed_text,
        "První věta. Druhá věta."
    );
}

#[tokio::test]
async fn realtime_auto_language_omits_the_language() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        let (setup, _) = receive_turn(&mut socket).await;
        for message in end_of_turn() {
            socket.send(message).await.unwrap();
        }
        setup
    })
    .await;
    let session = realtime_client(&url)
        .start("auto", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.language_code, "und");
    let setup = server_task.await.unwrap();
    let context: Value = serde_json::from_slice(&setup[1].body).unwrap();
    assert_eq!(context["phraseDetection"], json!({"mode": "CONVERSATION"}));
}

#[tokio::test]
async fn realtime_without_speech_is_an_empty_result_not_an_error() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        receive_turn(&mut socket).await;
        socket
            .send(server(
                "speech.phrase",
                json!({"RecognitionStatus": "InitialSilenceTimeout", "Offset": 0, "Duration": 0}),
            ))
            .await
            .unwrap();
        for message in end_of_turn() {
            socket.send(message).await.unwrap();
        }
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "");
    server_task.await.unwrap();
}

#[tokio::test]
async fn realtime_never_promotes_unfinished_hypotheses() {
    // The service previews words, then ends the turn without finishing them.
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        receive_turn(&mut socket).await;
        socket
            .send(server(
                "speech.hypothesis",
                json!({"Text": "unfinished words"}),
            ))
            .await
            .unwrap();
        for message in end_of_turn() {
            socket.send(message).await.unwrap();
        }
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
        "Microsoft did not return final text for the last words."
    );
    server_task.await.unwrap();
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    ErrorStatus,
    DroppedConnection,
    NoTurnEnd,
}

#[tokio::test]
async fn realtime_surfaces_service_errors_early_ends_and_dropped_connections() {
    for (fault, expected) in [
        (
            Fault::ErrorStatus,
            "Microsoft realtime transcription reported an error.",
        ),
        (
            Fault::DroppedConnection,
            "The Microsoft realtime connection closed before transcription completed",
        ),
        (
            Fault::NoTurnEnd,
            "Microsoft did not return a final realtime transcript in time.",
        ),
    ] {
        let (url, _, server_task) = support::websocket(move |mut socket| async move {
            receive_turn(&mut socket).await;
            match fault {
                Fault::ErrorStatus => {
                    socket
                        .send(server(
                            "speech.phrase",
                            json!({"RecognitionStatus": "Error", "Offset": 0, "Duration": 0}),
                        ))
                        .await
                        .unwrap();
                }
                Fault::DroppedConnection => {
                    socket.send(phrase("Partial.", 0)).await.unwrap();
                    let _ = socket.close(None).await;
                }
                Fault::NoTurnEnd => {
                    socket.send(phrase("Partial.", 0)).await.unwrap();
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        })
        .await;
        let session = realtime_client(&url)
            .start("eng", None, None)
            .await
            .unwrap();
        session.submit_audio(&[0; 3200]).unwrap();
        let error = session.finish().await.unwrap_err();
        assert!(
            error.to_string().starts_with(expected),
            "{fault:?}: {error}"
        );
        session.cancel_and_wait().await;
        server_task.abort();
    }
}

#[tokio::test]
async fn realtime_turn_end_before_stop_fails_the_stream_so_the_recording_is_recognized_again() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        socket.send(phrase("Early.", 0)).await.unwrap();
        socket.send(server("turn.end", json!({}))).await.unwrap();
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
    .expect("early turn.end marks the stream unhealthy");
    let error = session.finish().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Microsoft ended the realtime stream before recording stopped."
    );
    session.cancel_and_wait().await;
    server_task.abort();
}

#[tokio::test]
async fn realtime_rejects_bad_credentials_and_unreachable_endpoints_without_echoing_either() {
    for (status, expected) in [
        (
            401,
            "Microsoft rejected the credentials for realtime transcription.",
        ),
        (
            403,
            "Microsoft rejected the credentials for realtime transcription.",
        ),
        (
            429,
            "Microsoft realtime transcription is rate limited or already in use. Try again shortly.",
        ),
        (
            500,
            "Microsoft realtime transcription could not reach the service.",
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
        let error: ProviderError = realtime_client(&url)
            .start("eng", None, None)
            .await
            .err()
            .unwrap();
        server_task.await.unwrap();
        assert_eq!(error.to_string(), expected);
        assert!(!error.to_string().contains("synthetic"));
    }
    // Nothing listens here.
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/stt", closed.local_addr().unwrap());
    drop(closed);
    let error = realtime_client(&url)
        .start("eng", None, None)
        .await
        .err()
        .unwrap();
    assert_eq!(
        error.to_string(),
        "Microsoft realtime transcription could not reach the service."
    );
    assert!(
        MicrosoftRealtimeClient::new(
            Secret::new(KEY),
            "https://example.com",
            RealtimeOptions::default()
        )
        .is_err()
    );
    assert!(
        MicrosoftRealtimeClient::new(
            Secret::new(KEY),
            "wss://user:pass@example.com/x",
            RealtimeOptions::default()
        )
        .is_err()
    );
}

#[tokio::test]
async fn realtime_cancel_stops_everything_and_never_commits() {
    let (url, _, server_task) = support::websocket(|mut socket| async move {
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                return true;
            }
        }
        false
    })
    .await;
    let session = realtime_client(&url)
        .start("eng", None, None)
        .await
        .unwrap();
    session.submit_audio(&[0; 3200]).unwrap();
    session.cancel_and_wait().await;
    assert!(!session.is_healthy());
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Realtime transcription was cancelled."
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), server_task)
            .await
            .unwrap()
            .unwrap()
    );
}
