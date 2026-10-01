use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use mluva_providers::{
    ProviderError, Secret, USER_AGENT,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

mod support;

fn client(endpoint: &str) -> ElevenLabsRealtimeClient {
    ElevenLabsRealtimeClient::new(
        Secret::new("synthetic-wire-credential"),
        endpoint,
        RealtimeOptions {
            session_timeout: Duration::from_secs(1),
            finalization_timeout: Duration::from_millis(350),
            ..Default::default()
        },
    )
    .unwrap()
}
fn observed<T: serde::Serialize>(result: Result<T, ProviderError>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn send(event: Value) -> Message {
    Message::Text(serde_json::to_string(&event).unwrap().into())
}
fn started() -> Message {
    send(json!({"message_type":"session_started","session_id":"synthetic-session"}))
}

#[tokio::test]
async fn real_websocket_commits_results_and_callbacks_match_released_sessions() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/released-wire.json")).unwrap();
    for row in fixture["realtime"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap().to_owned();
        let sizes = row["sizes"].as_array().unwrap().clone();
        let scenario = name.clone();
        let empty = sizes.is_empty();
        let (fault_tx, fault_rx) = oneshot::channel();
        let (url,handshake,server)=support::websocket(move |mut socket|async move {
            socket.send(started()).await.unwrap();
            socket.send(send(json!({"message_type":"partial_transcript","text":"  provisional  "}))).await.unwrap();
            let mut events=vec![];let mut index=0;let mut fault=Some(fault_tx);
            while let Some(Ok(message))=socket.next().await {
                let Message::Text(text)=message else {break;};
                let event:Value=serde_json::from_str(&text).unwrap();
                let pcm=STANDARD.decode(event["audio_base_64"].as_str().unwrap()).unwrap();
                assert_eq!(event["message_type"],"input_audio_chunk");assert!(pcm.iter().all(|byte|*byte==0));
                events.push(json!({"bytes":pcm.len(),"commit":event.get("commit").and_then(Value::as_bool).unwrap_or(false)}));
                if scenario=="auth-error"||scenario=="bad-event" {
                    let message=if scenario=="auth-error" {send(json!({"message_type":"auth_error","error":"do not expose me"}))}else{Message::Text("bad JSON".into())};
                    if socket.send(message).await.is_err(){break;}
                    if let Some(fault)=fault.take(){let _=fault.send(());}
                    continue;
                }
                if event["commit"]==true {
                    if scenario=="provisional-only" {socket.send(send(json!({"message_type":"final_transcript","text":" provisional final "}))).await.unwrap();continue;}
                    if scenario=="late-language" {socket.send(send(json!({"message_type":"committed_transcript_with_timestamps","text":"ignored companion","language_code":"cs"}))).await.unwrap();}
                    socket.send(send(json!({"message_type":"committed_transcript","text":if empty {String::new()}else{format!("  part {index}  ")}}))).await.unwrap();index+=1;
                }
            }
            events
        }).await;
        let segments = Arc::new(Mutex::new(vec![]));
        let captured = segments.clone();
        let fails = name == "callback-failure";
        let session = client(&url)
            .start(
                "auto",
                None,
                Some(Box::new(move |segment| {
                    captured.lock().unwrap().push(segment);
                    if fails {
                        Err("synthetic callback failure".into())
                    } else {
                        Ok(())
                    }
                })),
            )
            .await
            .unwrap();
        let (path, headers) = handshake.await.unwrap();
        assert_eq!(
            path,
            "/realtime?model_id=scribe_v2_realtime&audio_format=pcm_16000&commit_strategy=manual&include_language_detection=true"
        );
        assert_eq!(headers["xi-api-key"], "synthetic-wire-credential");
        assert_eq!(headers["user-agent"], USER_AGENT);
        let mut submitted = vec![];
        for size in sizes {
            submitted.push(
                session
                    .submit_audio(&vec![0; size.as_u64().unwrap() as usize])
                    .unwrap(),
            );
        }
        assert_eq!(json!(submitted), row["submitted"], "{name}");
        if name == "auth-error" || name == "bad-event" {
            fault_rx.await.unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            while session.is_healthy() {
                assert!(Instant::now() < deadline);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        let result = session.finish().await.map(|result| result.transcription);
        let mut expected = row["result"].clone();
        expected.as_object_mut().unwrap().remove("exception");
        assert_eq!(observed(result), expected, "{name}");
        session.cancel();
        let events = tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(json!(events), row["events"], "{name}");
        assert_eq!(json!(*segments.lock().unwrap()), row["segments"], "{name}");
    }
}

#[tokio::test]
async fn ready_handshake_rejects_invalid_sessions_and_sanitizes_provider_errors() {
    for (event, error) in [
        (
            Message::Text("[]".into()),
            "ElevenLabs returned an invalid realtime transcription event.",
        ),
        (
            send(json!({"message_type":"session_started","session_id":""})),
            "ElevenLabs returned an invalid realtime session response.",
        ),
        (
            send(json!({"message_type":"partial_transcript","text":"private"})),
            "ElevenLabs did not confirm that realtime transcription was ready.",
        ),
        (
            send(json!({"message_type":"quota_exceeded","error":"private"})),
            "ElevenLabs realtime transcription is rate limited.",
        ),
        (
            send(json!({"message_type":"scribe_auth_error","error":"private"})),
            "ElevenLabs rejected realtime transcription authentication.",
        ),
    ] {
        let (url, _, server) = support::websocket(move |mut socket| async move {
            socket.send(event).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                if message.is_close() {
                    break;
                }
            }
        })
        .await;
        let result = client(&url).start("auto", None, None).await;
        assert_eq!(result.err().unwrap().to_string(), error);
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn finalization_waits_for_committed_callback_and_cancel_never_commits() {
    let (callback_tx, callback_rx) = oneshot::channel();
    let callback_tx = Arc::new(Mutex::new(Some(callback_tx)));
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, _, server) = support::websocket(|mut socket| async move {
        socket.send(started()).await.unwrap();
        let mut commits = 0;
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Text(text) = message {
                let event: Value = serde_json::from_str(&text).unwrap();
                if event["commit"] == true {
                    commits += 1;
                    socket
                        .send(send(
                            json!({"message_type":"committed_transcript","text":" final "}),
                        ))
                        .await
                        .unwrap();
                }
            } else {
                break;
            }
        }
        commits
    })
    .await;
    let session = Arc::new(
        client(&url)
            .start(
                "ces",
                None,
                Some(Box::new(move |_| {
                    if let Some(tx) = callback_tx.lock().unwrap().take() {
                        let _ = tx.send(());
                    }
                    release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    Ok(())
                })),
            )
            .await
            .unwrap(),
    );
    session.submit_audio(&[0; 3200]).unwrap();
    let finish = {
        let session = session.clone();
        tokio::spawn(async move { session.finish().await })
    };
    callback_rx.await.unwrap();
    assert!(!finish.is_finished());
    release_tx.send(()).unwrap();
    let result = finish.await.unwrap().unwrap();
    assert_eq!(result.transcription.text, "final");
    assert_eq!(server.await.unwrap(), 1);

    let (preview_tx, preview_rx) = oneshot::channel();
    let preview_tx = Arc::new(Mutex::new(Some(preview_tx)));
    let (url, _, server) = support::websocket(|mut socket| async move {
        socket.send(started()).await.unwrap();
        socket
            .send(send(
                json!({"message_type":"partial_transcript","text":"private volatile"}),
            ))
            .await
            .unwrap();
        let mut commits = 0;
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Text(text) = message {
                let event: Value = serde_json::from_str(&text).unwrap();
                if event["commit"] == true {
                    commits += 1;
                }
            } else {
                break;
            }
        }
        commits
    })
    .await;
    let session = client(&url)
        .start(
            "auto",
            Some(Box::new(move |_| {
                if let Some(tx) = preview_tx.lock().unwrap().take() {
                    let _ = tx.send(());
                }
                Ok(())
            })),
            None,
        )
        .await
        .unwrap();
    preview_rx.await.unwrap();
    session.cancel();
    assert!(session.snapshot().volatile_text.is_empty());
    assert!(!session.is_healthy());
    assert!(!session.submit_audio(&[0; 2]).unwrap());
    assert!(session.submit_audio(&[]).unwrap());
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Realtime transcription was cancelled."
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test(flavor = "current_thread")]
async fn saturated_queue_never_blocks_capture_and_odd_samples_are_rejected() {
    let (url, _, server) = support::websocket(|mut socket| async move {
        socket.send(started()).await.unwrap();
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
    })
    .await;
    let client = ElevenLabsRealtimeClient::new(
        Secret::new("synthetic-wire-credential"),
        &url,
        RealtimeOptions {
            maximum_queued_chunks: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let session = client.start("auto", None, None).await.unwrap();
    assert_eq!(
        session.submit_audio(&[0]).unwrap_err().to_string(),
        "Realtime audio chunks must contain complete PCM16 samples."
    );
    assert!(session.submit_audio(&[0; 2]).unwrap());
    // Both calls occur before this single-thread runtime yields to the real sender task.
    assert!(!session.submit_audio(&[0; 2]).unwrap());
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Realtime transcription could not keep up with microphone audio."
    );
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn ping_binary_events_and_server_close_after_commit_keep_the_final_result() {
    let (url, _, server) = support::websocket(|mut socket| async move {
        socket
            .send(Message::Ping(b"synthetic-ping".to_vec().into()))
            .await
            .unwrap();
        assert!(matches!(
            socket.next().await.unwrap().unwrap(),
            Message::Pong(_)
        ));
        socket
            .send(Message::Binary(
                json!({"message_type":"session_started","session_id":"binary-session"})
                    .to_string()
                    .into_bytes()
                    .into(),
            ))
            .await
            .unwrap();
        let Message::Text(text) = socket.next().await.unwrap().unwrap() else {
            panic!("expected commit");
        };
        let event: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(event["commit"], true);
        socket
            .send(Message::Binary(
                json!({"message_type":"committed_transcript","text":" complete "})
                    .to_string()
                    .into_bytes()
                    .into(),
            ))
            .await
            .unwrap();
        let _ = socket.close(None).await;
    })
    .await;
    let session = client(&url).start("ces", None, None).await.unwrap();
    let result = session.finish().await.unwrap();
    assert_eq!(result.transcription.text, "complete");
    assert_eq!(result.transcription.language_code, "ces");
    server.await.unwrap();
}

#[tokio::test]
async fn oversized_incoming_events_fail_closed_with_a_bounded_receiver() {
    let (url,_,server)=support::websocket(|mut socket|async move {
        socket.send(started()).await.unwrap();
        let oversized=json!({"message_type":"partial_transcript","text":"x".repeat(mluva_providers::realtime::MAX_EVENT_BYTES)});
        let _=socket.send(send(oversized)).await;
        while let Some(Ok(message))=socket.next().await {if message.is_close(){break;}}
    }).await;
    let session = client(&url).start("auto", None, None).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while session.is_healthy() {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(session.snapshot().volatile_text.is_empty());
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "The ElevenLabs realtime connection closed before transcription completed."
    );
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancellation_during_a_callback_drops_it_and_does_not_restore_volatile_text() {
    struct Lifetime(Option<oneshot::Sender<()>>);
    impl Drop for Lifetime {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }
    let (entered_tx, entered_rx) = oneshot::channel();
    let mut entered = Some(entered_tx);
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let lifetime = Lifetime(Some(dropped_tx));
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, _, server) = support::websocket(|mut socket| async move {
        socket.send(started()).await.unwrap();
        socket
            .send(send(
                json!({"message_type":"partial_transcript","text":"private provisional"}),
            ))
            .await
            .unwrap();
        socket
            .send(send(
                json!({"message_type":"partial_transcript","text":"late provisional"}),
            ))
            .await
            .unwrap();
        while let Some(Ok(message)) = socket.next().await {
            if message.is_close() {
                break;
            }
        }
    })
    .await;
    let session = client(&url)
        .start(
            "auto",
            Some(Box::new(move |_| {
                let _ = &lifetime;
                if let Some(tx) = entered.take() {
                    let _ = tx.send(());
                }
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            })),
            None,
        )
        .await
        .unwrap();
    entered_rx.await.unwrap();
    session.cancel();
    assert_eq!(
        session.finish().await.unwrap_err().to_string(),
        "Realtime transcription was cancelled."
    );
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_millis(500), dropped_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(session.snapshot().volatile_text.is_empty());
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
