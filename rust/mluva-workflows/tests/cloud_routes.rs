//! Microsoft and xAI dictation through a whole capture session: realtime, the explicit batch
//! fallback, and the batch mode the person chose. Phone audio, local protocol peers and a
//! deferred delivery keep every check away from the host microphone and clipboard.
use futures_util::{SinkExt, StreamExt};
use mluva_audio::capture::{CaptureRecorder, CaptureStorage};
use mluva_core::{config::AppConfig, diagnostics::DiagnosticsStore, history::HistoryStore};
use mluva_providers::{
    Secret,
    microsoft::{MicrosoftClient, MicrosoftRealtimeClient},
    mistral::{MistralClient, MistralRealtimeClient},
    realtime::RealtimeOptions,
    speech::SpeechClient,
    xai::{XaiClient, XaiRealtimeClient},
};
use mluva_workflows::{
    capture::{CaptureOptions, CaptureReady, CaptureRecognitionClient, CaptureSession},
    dictation::{DictationWorkflow, WorkflowResult},
};
use serde_json::json;
use std::{rc::Rc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Provider {
    Microsoft,
    Xai,
    Mistral,
}
#[derive(Clone, Copy, Debug)]
enum Realtime {
    /// Finishes with this text.
    Succeeds,
    /// Accepts the stream, then reports an error once audio stops.
    FailsAtStop,
    /// Answers the connection with HTTP 401.
    Refuses,
    /// Finishes the turn successfully with no text at all.
    EmptyFinal,
}

impl Provider {
    fn id(self) -> &'static str {
        match self {
            Self::Microsoft => "microsoft",
            Self::Xai => "xai",
            Self::Mistral => "mistral",
        }
    }
    fn route(self, mode: &str) -> &'static str {
        match (self, mode) {
            (Self::Microsoft, "realtime") => "mai-realtime",
            (Self::Microsoft, _) => "mai-batch",
            (Self::Xai, "realtime") => "grok-realtime",
            (Self::Xai, _) => "grok-batch",
            (Self::Mistral, "realtime") => "mistral-realtime",
            (Self::Mistral, _) => "mistral-batch",
        }
    }
    fn model(self) -> &'static str {
        match self {
            Self::Microsoft => "MAI-Transcribe-2",
            Self::Xai => "Grok",
            Self::Mistral => "Mistral",
        }
    }
    fn diagnostic(self) -> &'static str {
        match self {
            Self::Microsoft => "microsoft-mai",
            Self::Xai => "xai-grok",
            Self::Mistral => "mistral-voxtral",
        }
    }
}

/// A one-shot HTTP peer for the batch route; the body suits both providers.
async fn batch_peer() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut received = vec![];
            let mut buffer = [0u8; 8192];
            // Read the request: headers, then the declared body.
            loop {
                let read = socket.read(&mut buffer).await.unwrap();
                if read == 0 {
                    break;
                }
                received.extend_from_slice(&buffer[..read]);
                if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&received[..end]).to_lowercase();
                    let length = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|value| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if received.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = br#"{"text":"Batch text.","combinedPhrases":[{"text":"Batch text."}]}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(body).await;
            let _ = socket.shutdown().await;
        }
    });
    url
}

/// A streaming peer speaking just enough of either wire protocol.
async fn stream_peer(provider: Provider, behavior: Realtime) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/stream", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        if matches!(behavior, Realtime::Refuses) {
            let mut buffer = [0u8; 4096];
            let _ = socket.read(&mut buffer).await;
            let _ = socket
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await;
            return;
        }
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let opening = match provider {
            Provider::Xai => Some(json!({"type": "transcript.created"})),
            Provider::Mistral => Some(json!({"type": "session.created"})),
            Provider::Microsoft => None,
        };
        if let Some(opening) = opening {
            socket
                .send(Message::Text(opening.to_string().into()))
                .await
                .unwrap();
        }
        while let Some(Ok(message)) = socket.next().await {
            let ended = match (&message, provider) {
                (Message::Binary(bytes), Provider::Microsoft) => {
                    let header = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
                    bytes.len() == 2 + header
                }
                (Message::Text(text), Provider::Xai) => text.contains("audio.done"),
                (Message::Text(text), Provider::Mistral) => text.contains("input_audio.end"),
                _ => false,
            };
            if !ended {
                continue;
            }
            if matches!(behavior, Realtime::FailsAtStop) {
                let error = match provider {
                    Provider::Microsoft => Message::Text(
                        "Path: speech.phrase\r\n\r\n{\"RecognitionStatus\":\"Error\"}".into(),
                    ),
                    Provider::Xai | Provider::Mistral => {
                        Message::Text(json!({"type": "error", "message": "x"}).to_string().into())
                    }
                };
                socket.send(error).await.unwrap();
            } else if matches!(behavior, Realtime::EmptyFinal) {
                let frames = match provider {
                    Provider::Microsoft => vec![
                        "Path: speech.phrase\r\n\r\n{\"RecognitionStatus\":\"EndOfDictation\"}"
                            .to_owned(),
                        "Path: turn.end\r\n\r\n{}".to_owned(),
                    ],
                    Provider::Xai => {
                        vec![json!({"type": "transcript.done", "text": ""}).to_string()]
                    }
                    Provider::Mistral => vec![
                        json!({"type": "transcription.done", "text": "", "usage": {}}).to_string(),
                    ],
                };
                for frame in frames {
                    socket.send(Message::Text(frame.into())).await.unwrap();
                }
            } else if provider == Provider::Microsoft {
                for frame in [
                    "Path: speech.phrase\r\n\r\n{\"RecognitionStatus\":\"Success\",\"DisplayText\":\"Realtime text.\",\"Offset\":0}",
                    "Path: speech.phrase\r\n\r\n{\"RecognitionStatus\":\"EndOfDictation\"}",
                    "Path: turn.end\r\n\r\n{}",
                ] {
                    socket.send(Message::Text(frame.into())).await.unwrap();
                }
            } else if provider == Provider::Mistral {
                for event in [
                    json!({"type": "transcription.text.delta", "text": "Realtime"}),
                    json!({"type": "transcription.done", "text": "Realtime text.", "usage": {}}),
                ] {
                    socket
                        .send(Message::Text(event.to_string().into()))
                        .await
                        .unwrap();
                }
            } else {
                for event in [
                    json!({"type": "transcript.partial", "text": "Realtime text.", "is_final": true, "speech_final": false, "start": 0.1}),
                    json!({"type": "transcript.partial", "text": "Realtime text.", "is_final": true, "speech_final": true, "start": 0.1}),
                    json!({"type": "transcript.done", "text": ""}),
                ] {
                    socket
                        .send(Message::Text(event.to_string().into()))
                        .await
                        .unwrap();
                }
            }
            break;
        }
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
    });
    url
}

struct Outcome {
    ready: CaptureReady,
    result: WorkflowResult,
    diagnostics: Vec<(String, String, String)>,
}

/// Dictate `chunks` of 0.1 s phone audio through `provider` in `mode`; `realtime` shapes the stream peer.
async fn dictate(provider: Provider, mode: &str, realtime: Realtime, chunks: usize) -> Outcome {
    let root = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(root.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let diagnostics = DiagnosticsStore::new(root.path().join("diagnostics.sqlite3"), 100).unwrap();
    diagnostics.initialize().unwrap();
    let config = AppConfig {
        transcription_provider: provider.id().into(),
        transcription_mode: mode.into(),
        microsoft_region: "swedencentral".into(),
        language_code: "eng".into(),
        ..AppConfig::default()
    };
    let batch = batch_peer().await;
    let key = || Secret::new("synthetic-credential");
    let speech = match provider {
        Provider::Microsoft => SpeechClient::Microsoft(
            MicrosoftClient::new(key(), &batch, Duration::from_secs(5)).unwrap(),
        ),
        Provider::Xai => {
            SpeechClient::Xai(XaiClient::new(key(), &batch, Duration::from_secs(5)).unwrap())
        }
        Provider::Mistral => SpeechClient::Mistral(
            MistralClient::new(key(), &batch, Duration::from_secs(5)).unwrap(),
        ),
    };
    let mut workflow = DictationWorkflow::new(config, speech, history, root.path().into());
    workflow.diagnostics = Some(diagnostics.clone());
    let options = RealtimeOptions {
        session_timeout: Duration::from_secs(2),
        finalization_timeout: Duration::from_millis(800),
        ..Default::default()
    };
    // Batch mode builds no stream, exactly as the application does.
    let peer = if mode == "realtime" {
        Some(stream_peer(provider, realtime).await)
    } else {
        None
    };
    let client = peer.map(|peer| match provider {
        Provider::Microsoft => CaptureRecognitionClient::Microsoft(Rc::new(
            MicrosoftRealtimeClient::new(key(), &peer, options.clone()).unwrap(),
        )),
        Provider::Xai => CaptureRecognitionClient::Xai(Rc::new(
            XaiRealtimeClient::new(key(), &peer, options.clone()).unwrap(),
        )),
        Provider::Mistral => CaptureRecognitionClient::Mistral(Rc::new(
            MistralRealtimeClient::new(key(), &peer, options.clone()).unwrap(),
        )),
    });
    let session = CaptureSession::with_recorder(
        Rc::new(workflow),
        CaptureRecorder::phone().unwrap(),
        CaptureStorage::Persistent(root.path().join("recordings")),
        client,
        CaptureOptions {
            defer_delivery: true,
            ..Default::default()
        },
    )
    .unwrap();
    let ready = session.prepare_and_start().await.unwrap();
    for _ in 0..chunks {
        session.append_phone_audio(vec![0; 3200]).await.unwrap();
    }
    let result = session.complete(None).await.unwrap();
    let diagnostics = diagnostics
        .recent(100)
        .unwrap()
        .into_iter()
        .map(|event| (event.stage, event.provider, event.outcome))
        .collect();
    Outcome {
        ready,
        result,
        diagnostics,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn realtime_text_is_committed_under_the_providers_realtime_route() {
    for provider in [Provider::Microsoft, Provider::Xai, Provider::Mistral] {
        let outcome = dictate(provider, "realtime", Realtime::Succeeds, 3).await;
        let result = &outcome.result;
        assert_eq!(result.transcription.text, "Realtime text.", "{provider:?}");
        assert_eq!(result.recognition_route, provider.route("realtime"));
        assert!(!result.recognition_fallback);
        assert_eq!(result.recognition_fallback_reason, None);
        assert_eq!(outcome.ready.fallback_reason, None);
        assert_eq!(
            outcome.ready.status(),
            "Ready · microphone capture started."
        );
        let entry = result.history_entry.as_ref().unwrap();
        assert_eq!(entry.raw_text, "Realtime text.");
        assert_eq!(
            entry.recognition_route.as_deref(),
            Some(provider.route("realtime"))
        );
        assert_eq!(entry.recognition_fallback_reason, None);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|(stage, who, outcome)| stage == "recognition"
                    && who == provider.diagnostic()
                    && outcome == "completed"),
            "{provider:?} {:?}",
            outcome.diagnostics
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_stream_is_recognized_again_in_batch_and_says_so() {
    for provider in [Provider::Microsoft, Provider::Xai, Provider::Mistral] {
        let outcome = dictate(provider, "realtime", Realtime::FailsAtStop, 3).await;
        let result = &outcome.result;
        assert_eq!(result.transcription.text, "Batch text.", "{provider:?}");
        assert_eq!(result.recognition_route, provider.route("batch"));
        assert!(result.recognition_fallback);
        assert_eq!(
            result.recognition_fallback_reason.as_deref(),
            Some("realtime-stream-failed")
        );
        assert!(
            result.delivery.guidance.contains(&format!(
                "this capture was completed with batch {}.",
                provider.model()
            )),
            "{}",
            result.delivery.guidance
        );
        let entry = result.history_entry.as_ref().unwrap();
        assert_eq!(
            entry.recognition_route.as_deref(),
            Some(provider.route("batch"))
        );
        assert_eq!(
            entry.recognition_fallback_reason.as_deref(),
            Some("realtime-stream-failed")
        );
        assert!(outcome.diagnostics.iter().any(|(stage, who, outcome)| {
            stage == "recognition" && who == provider.diagnostic() && outcome == "safe-fallback"
        }));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_stream_starts_in_batch_and_names_the_provider_in_the_ready_status() {
    for provider in [Provider::Microsoft, Provider::Xai, Provider::Mistral] {
        let outcome = dictate(provider, "realtime", Realtime::Refuses, 3).await;
        assert_eq!(
            outcome.ready.fallback_reason.as_deref(),
            Some("realtime-startup-failed")
        );
        assert_eq!(
            outcome.ready.status(),
            format!(
                "Ready · microphone capture started with {} batch fallback.",
                provider.model()
            )
        );
        assert_eq!(outcome.result.transcription.text, "Batch text.");
        assert_eq!(outcome.result.recognition_route, provider.route("batch"));
        assert_eq!(
            outcome.result.recognition_fallback_reason.as_deref(),
            Some("realtime-startup-failed")
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn batch_mode_is_a_choice_not_a_fallback() {
    for provider in [Provider::Microsoft, Provider::Xai, Provider::Mistral] {
        let outcome = dictate(provider, "batch", Realtime::Succeeds, 3).await;
        // Nothing streamed, nothing fell back: no reason, no fallback wording.
        assert_eq!(outcome.ready.fallback_reason, None);
        assert_eq!(
            outcome.ready.status(),
            "Ready · microphone capture started."
        );
        let result = &outcome.result;
        assert_eq!(result.transcription.text, "Batch text.", "{provider:?}");
        assert_eq!(result.recognition_route, provider.route("batch"));
        assert!(!result.recognition_fallback);
        assert_eq!(result.recognition_fallback_reason, None);
        assert!(!result.delivery.guidance.contains("fallback"));
        assert!(!result.delivery.guidance.contains("unavailable"));
        let entry = result.history_entry.as_ref().unwrap();
        assert_eq!(
            entry.recognition_route.as_deref(),
            Some(provider.route("batch"))
        );
        assert_eq!(entry.recognition_fallback_reason, None);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_realtime_result_for_a_real_recording_is_recognized_again_in_batch() {
    for provider in [Provider::Microsoft, Provider::Xai, Provider::Mistral] {
        // 1.2 s of audio and no text from the stream: the retained recording goes to batch.
        let outcome = dictate(provider, "realtime", Realtime::EmptyFinal, 12).await;
        let result = &outcome.result;
        assert_eq!(result.transcription.text, "Batch text.", "{provider:?}");
        assert_eq!(result.recognition_route, provider.route("batch"));
        assert!(result.recognition_fallback);
        assert_eq!(
            result.recognition_fallback_reason.as_deref(),
            Some("realtime-stream-failed")
        );
        assert_eq!(
            result.history_entry.as_ref().unwrap().raw_text,
            "Batch text.",
            "{provider:?}"
        );

        // 0.3 s is a brief tap, not a recording: an empty result is genuinely no speech.
        let outcome = dictate(provider, "realtime", Realtime::EmptyFinal, 3).await;
        let result = &outcome.result;
        assert_eq!(result.transcription.text, "", "{provider:?}");
        assert_eq!(result.recognition_route, provider.route("realtime"));
        assert!(!result.recognition_fallback);
        assert!(result.delivery.guidance.contains("No speech detected"));
    }
}
