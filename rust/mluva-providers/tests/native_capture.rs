use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::process::Command;
use tokio_tungstenite::tungstenite::Message;

mod support;

#[tokio::test]
async fn native_pcm_flows_to_websocket_in_order_and_stream_failure_retains_complete_wav() {
    for failed in [false, true] {
        let (url,_,server)=support::websocket(move |mut socket|async move {
            socket.send(Message::Text(json!({"message_type":"session_started","session_id":"native-audio"}).to_string().into())).await.unwrap();
            let mut pcm=vec![];let mut commits=0;let mut rejection=false;
            while let Some(Ok(message))=socket.next().await {
                let Message::Text(text)=message else {break;};let event:Value=serde_json::from_str(&text).unwrap();
                pcm.extend_from_slice(&STANDARD.decode(event["audio_base_64"].as_str().unwrap()).unwrap());
                if failed&&!rejection {rejection=true;socket.send(Message::Text(json!({"message_type":"auth_error","error":"private provider detail"}).to_string().into())).await.unwrap();}
                if event["commit"]==true {commits+=1;socket.send(Message::Text(json!({"message_type":"committed_transcript","text":" complete native capture "}).to_string().into())).await.unwrap();}
            }
            (pcm,commits)
        }).await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture/audio.wav");
        let mut command = Command::new(env!("CARGO_BIN_EXE_provider-fixture-peer"));
        command
            .env_clear()
            .arg("audio-realtime")
            .arg(url)
            .arg(&path);
        if failed {
            command.arg("failed");
        }
        let output =
            tokio::time::timeout(Duration::from_secs(5), command.kill_on_drop(true).output())
                .await
                .unwrap()
                .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let (sent, commits) = tokio::time::timeout(Duration::from_secs(3), server)
            .await
            .unwrap()
            .unwrap();
        let expected = (0..9600)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        assert!(mluva_audio::wav::is_compatible_pcm_wav(&path).unwrap());
        let wav = std::fs::read(&path).unwrap();
        assert_eq!(&wav[44..], expected);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        if failed {
            assert_eq!(
                result["error"],
                "ElevenLabs rejected realtime transcription authentication."
            );
            assert_eq!(commits, 0);
            assert!(expected.starts_with(&sent));
            assert!(!sent.is_empty());
        } else {
            assert_eq!(result["ok"]["text"], "complete native capture");
            assert_eq!(result["ok"]["audio_duration_seconds"], 0.3);
            assert_eq!(sent, expected);
            assert_eq!(commits, 1);
        }
    }
}
