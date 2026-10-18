//! One bounded sentence through ElevenLabs v4 Turbo. No captured audio is an input.
use crate::{ProviderError, Result, Secret};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{io::AsyncWriteExt, process::Command, sync::mpsc};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::WebSocketConfig,
};
use tokio_util::sync::CancellationToken;

pub const MODEL: &str = "eleven_v4_turbo";
pub const ENDPOINT: &str = "wss://api.elevenlabs.io/v1/text-to-dialogue/stream-input?model_id=eleven_v4_turbo&output_format=pcm_24000";
const FAILURE: &str = "Spoken reply unavailable. The text is still here.";
const MAX_AUDIO: usize = 24_000 * 2 * 25;

fn connection_error(error: tokio_tungstenite::tungstenite::Error) -> ProviderError {
    // Never display an upstream body, URL or credential-bearing request.
    match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => ProviderError::message(&format!(
            "ElevenLabs refused the voice connection (HTTP {}). Check your key, voice and v4 Turbo access; the text is still here.",
            response.status().as_u16()
        )),
        _ => ProviderError::message(FAILURE),
    }
}

#[derive(Clone, Debug)]
pub struct SpeechTiming {
    /// Includes connection and synthesis, ending at the first received PCM chunk.
    pub first_audio_ms: u128,
    pub total_ms: u128,
    pub audio_bytes: usize,
}

pub async fn stream(
    sentence: &str,
    voice: &str,
    key: Secret,
    cancel: &CancellationToken,
    audio: mpsc::Sender<Vec<u8>>,
) -> Result<SpeechTiming> {
    stream_at(ENDPOINT, sentence, voice, key, cancel, audio).await
}

async fn stream_at(
    endpoint: &str,
    sentence: &str,
    voice: &str,
    key: Secret,
    cancel: &CancellationToken,
    audio: mpsc::Sender<Vec<u8>>,
) -> Result<SpeechTiming> {
    if sentence.trim().is_empty()
        || sentence.chars().count() > 320
        || sentence.chars().any(char::is_control)
        || voice.is_empty()
        || voice.len() > 128
        || !voice
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err(ProviderError::message(
            "Choose a voice and a short status sentence.",
        ));
    }
    let start = Instant::now();
    let work = async {
        let mut request = endpoint
            .into_client_request()
            .map_err(|_| ProviderError::message(FAILURE))?;
        let mut header = key
            .value()
            .parse::<tokio_tungstenite::tungstenite::http::HeaderValue>()
            .map_err(|_| ProviderError::message(FAILURE))?;
        header.set_sensitive(true);
        request.headers_mut().insert("xi-api-key", header);
        let config = WebSocketConfig::default()
            .max_message_size(Some(1_048_576))
            .max_frame_size(Some(1_048_576));
        let (mut socket, _) =
            tokio_tungstenite::connect_async_with_config(request, Some(config), false)
                .await
                .map_err(connection_error)?;
        for value in [
            json!({"voices": [voice]}),
            json!({"inputs": [{"text": sentence, "voice_id": voice, "new_turn": true}]}),
            json!({"close_socket": true}),
        ] {
            socket
                .send(Message::Text(value.to_string().into()))
                .await
                .map_err(|_| ProviderError::message(FAILURE))?;
        }
        let mut first_audio_ms = None;
        let mut audio_bytes = 0;
        while let Some(message) = socket.next().await {
            match message.map_err(|_| ProviderError::message(FAILURE))? {
                Message::Text(text) => {
                    let event: Value =
                        serde_json::from_str(&text).map_err(|_| ProviderError::message(FAILURE))?;
                    if event
                        .get("error")
                        .is_some_and(|value| !value.is_null() && value != false)
                    {
                        let message = match event.get("error").and_then(Value::as_str) {
                            Some("missing_permissions") => {
                                "The ElevenLabs key needs text_to_speech permission. Text status is still available."
                            }
                            Some("quota_exceeded") => {
                                "ElevenLabs speech quota is exhausted. Text status is still available."
                            }
                            Some("invalid_api_key") => {
                                "ElevenLabs rejected the API key. Text status is still available."
                            }
                            Some("voice_not_found") => {
                                "That ElevenLabs voice is unavailable. Choose another voice ID."
                            }
                            _ => FAILURE,
                        };
                        return Err(ProviderError::message(message));
                    }
                    if let Some(encoded) = event
                        .get("audio")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                    {
                        let pcm = STANDARD
                            .decode(encoded)
                            .map_err(|_| ProviderError::message(FAILURE))?;
                        audio_bytes += pcm.len();
                        if audio_bytes > MAX_AUDIO || pcm.len() % 2 != 0 {
                            return Err(ProviderError::message(FAILURE));
                        }
                        first_audio_ms.get_or_insert_with(|| start.elapsed().as_millis());
                        audio
                            .send(pcm)
                            .await
                            .map_err(|_| ProviderError::message(FAILURE))?;
                    }
                    if event.get("is_final").and_then(Value::as_bool) == Some(true) {
                        return Ok(SpeechTiming {
                            first_audio_ms: first_audio_ms
                                .ok_or_else(|| ProviderError::message(FAILURE))?,
                            total_ms: start.elapsed().as_millis(),
                            audio_bytes,
                        });
                    }
                }
                Message::Ping(data) => {
                    socket
                        .send(Message::Pong(data))
                        .await
                        .map_err(|_| ProviderError::message(FAILURE))?;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        Err(ProviderError::message(FAILURE))
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(ProviderError::message("Spoken reply stopped.")),
        result = tokio::time::timeout(Duration::from_secs(30), work) => result.map_err(|_| ProviderError::message(FAILURE))?,
    }
}

/// Starts the player only after audio arrives; cancellation kills and reaps it before returning.
async fn play(mut audio: mpsc::Receiver<Vec<u8>>, cancel: &CancellationToken) -> Result<()> {
    let first = tokio::select! { biased; _ = cancel.cancelled() => return Ok(()), value = audio.recv() => value };
    let Some(first) = first else {
        return Ok(());
    };
    let executable = mluva_core::executables::find_executable("pw-cat").ok_or_else(|| {
        ProviderError::message("Install PipeWire's pw-cat to hear spoken replies.")
    })?;
    play_chunks(&executable, first, audio, cancel).await
}

async fn play_chunks(
    executable: &std::path::Path,
    first: Vec<u8>,
    mut audio: mpsc::Receiver<Vec<u8>>,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut child = Command::new(executable)
        .args([
            "--playback",
            "--rate=24000",
            "--channels=1",
            "--format=s16",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| ProviderError::message(FAILURE))?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| ProviderError::message(FAILURE))?;
    let playback = async {
        input
            .write_all(&first)
            .await
            .map_err(|_| ProviderError::message(FAILURE))?;
        while let Some(chunk) = audio.recv().await {
            input
                .write_all(&chunk)
                .await
                .map_err(|_| ProviderError::message(FAILURE))?;
        }
        input
            .shutdown()
            .await
            .map_err(|_| ProviderError::message(FAILURE))?;
        drop(input);
        Ok(())
    };
    let result = tokio::select! { biased;
        _ = cancel.cancelled() => Err(ProviderError::message("Spoken reply stopped.")),
        result = tokio::time::timeout(Duration::from_secs(30), playback) => result.unwrap_or_else(|_| Err(ProviderError::message(FAILURE))),
    };
    if result.is_err() {
        let _ = child.kill().await;
    }
    let status = tokio::select! { biased;
        _ = cancel.cancelled() => { let _ = child.kill().await; child.wait().await },
        status = tokio::time::timeout(Duration::from_secs(30), child.wait()) => match status {
            Ok(status) => status, Err(_) => { let _ = child.kill().await; child.wait().await }
        },
    };
    result?;
    if !status
        .map_err(|_| ProviderError::message(FAILURE))?
        .success()
    {
        return Err(ProviderError::message(FAILURE));
    }
    Ok(())
}

pub async fn speak(
    sentence: &str,
    voice: &str,
    key: Secret,
    cancel: &CancellationToken,
) -> Result<SpeechTiming> {
    let (sender, receiver) = mpsc::channel(4);
    let local = cancel.child_token();
    let synthesis = async {
        let result = stream(sentence, voice, key, &local, sender).await;
        if result.is_err() {
            local.cancel();
        }
        result
    };
    let playback = async {
        let result = play(receiver, &local).await;
        if result.is_err() {
            local.cancel();
        }
        result
    };
    let (result, playback) = tokio::join!(synthesis, playback);
    if result
        .as_ref()
        .is_err_and(|error| error.to_string() != "Spoken reply stopped.")
    {
        result
    } else {
        playback?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn permission_failure_is_actionable_and_never_echoes_provider_details() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            for _ in 0..3 {
                socket.next().await.unwrap().unwrap();
            }
            socket.send(Message::Text(json!({"error":"missing_permissions", "message":"untrusted provider detail synthetic-secret"}).to_string().into())).await.unwrap();
        });
        let (tx, mut rx) = mpsc::channel(1);
        let error = stream_at(
            &format!("ws://{address}"),
            "Agent is ready.",
            "voice",
            Secret::new("synthetic-secret"),
            &CancellationToken::new(),
            tx,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("text_to_speech"));
        assert!(!error.to_string().contains("synthetic-secret"));
        assert!(rx.recv().await.is_none());
        peer.await.unwrap();
    }

    #[tokio::test]
    async fn cancelling_playback_reaps_the_player_before_capture_can_resume() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let player = temp.path().join("private-player");
        let pid_file = temp.path().join("pid");
        std::fs::write(
            &player,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&player, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let stopped = cancel.clone();
        let work =
            tokio::spawn(async move { play_chunks(&player, vec![0; 480], rx, &stopped).await });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !pid_file.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(pid_file).unwrap();
        cancel.cancel();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), work)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
        drop(tx);
    }
    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite fixes the handshake callback error type.
    async fn v4_uses_one_voice_flushes_short_text_and_never_sends_audio_or_key_in_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                tcp,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["xi-api-key"], "synthetic-key");
                    assert!(!request.uri().to_string().contains("synthetic-key"));
                    Ok(response)
                },
            )
            .await
            .unwrap();
            let mut frames = Vec::new();
            for _ in 0..3 {
                frames.push(
                    serde_json::from_str::<Value>(
                        &ws.next().await.unwrap().unwrap().into_text().unwrap(),
                    )
                    .unwrap(),
                );
            }
            assert_eq!(frames[0], json!({"voices": ["test-voice"]}));
            assert_eq!(
                frames[1],
                json!({"inputs": [{"text": "Agent is ready.", "voice_id": "test-voice", "new_turn": true}]})
            );
            assert_eq!(frames[2], json!({"close_socket": true}));
            ws.send(Message::Text(
                json!({"audio": STANDARD.encode([0u8; 480]), "is_final": true})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        });
        let (tx, mut rx) = mpsc::channel(1);
        let timing = stream_at(
            &format!("ws://{address}"),
            "Agent is ready.",
            "test-voice",
            Secret::new("synthetic-key"),
            &CancellationToken::new(),
            tx,
        )
        .await
        .unwrap();
        assert_eq!(timing.audio_bytes, 480);
        assert_eq!(rx.recv().await.unwrap().len(), 480);
        peer.await.unwrap();
    }
}
