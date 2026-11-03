//! Live check of one cloud speech route against a public 16 kHz mono WAV clip. Never use your own
//! recordings. The key comes from the desktop keyring or the provider's environment aliases and
//! is never printed; Microsoft also needs `AZURE_SPEECH_REGION`.
//!
//! ```sh
//! cargo run -p mluva-providers --example cloud_probe -- <microsoft|xai|mistral> <realtime|batch> clip.wav [ces|eng|auto]
//! ```
use mluva_providers::{
    credentials::{CloudProvider, CredentialStore},
    microsoft::{self, MicrosoftClient, MicrosoftRealtimeClient},
    mistral::{self, MistralClient, MistralRealtimeClient},
    realtime::RealtimeOptions,
    stream::StreamSession,
    xai::{self, XaiClient, XaiRealtimeClient},
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CHUNK_BYTES: usize = 3200; // 100 ms of 16 kHz 16-bit mono PCM

/// The PCM payload of a canonical 16 kHz, 16-bit, mono WAV file.
fn pcm(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).expect("readable clip");
    assert_eq!(&bytes[0..4], b"RIFF", "a WAV file");
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let bits = u16::from_le_bytes(bytes[34..36].try_into().unwrap());
    assert_eq!(
        (rate, channels, bits),
        (16_000, 1, 16),
        "16 kHz mono 16-bit"
    );
    let data = bytes
        .windows(4)
        .position(|window| window == b"data")
        .expect("a data chunk");
    bytes[data + 8..].to_vec()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [route, mode, clip] = [&args[0], &args[1], &args[2]];
    let language = args.get(3).map_or("auto", String::as_str);
    let provider = CloudProvider::from_id(route).expect("microsoft, xai or mistral");
    let key = CredentialStore::new()
        .api_key(provider)
        .await
        .unwrap_or_else(|error| panic!("{error}"));
    let region = std::env::var("AZURE_SPEECH_REGION").unwrap_or_default();
    let path = Path::new(clip);
    let started = Instant::now();
    if mode == "batch" {
        let result = match provider {
            CloudProvider::Microsoft => {
                let endpoint = microsoft::batch_endpoint(&region).unwrap();
                MicrosoftClient::new(key, &endpoint, Duration::from_secs(120))
                    .unwrap()
                    .transcribe(path, language)
                    .await
            }
            CloudProvider::Mistral => {
                MistralClient::new(key, mistral::BATCH_ENDPOINT, Duration::from_secs(120))
                    .unwrap()
                    .transcribe(path, language)
                    .await
            }
            _ => {
                XaiClient::new(key, xai::BATCH_ENDPOINT, Duration::from_secs(120))
                    .unwrap()
                    .transcribe(path, language)
                    .await
            }
        };
        match result {
            Ok(result) => println!(
                "batch  request {:.2}s  language {}  text: {}",
                started.elapsed().as_secs_f64(),
                result.language_code,
                result.text
            ),
            Err(error) => println!("batch  FAILED: {error}"),
        }
        return;
    }
    let first_text = Arc::new(Mutex::new(None::<f64>));
    let seen = first_text.clone();
    let preview = Box::new(move |preview: mluva_providers::realtime::RealtimePreview| {
        let mut first = seen.lock().unwrap();
        if first.is_none() && !preview.display_text().is_empty() {
            *first = Some(started.elapsed().as_secs_f64());
        }
        Ok(())
    });
    let options = RealtimeOptions::default();
    let session: StreamSession = match provider {
        CloudProvider::Microsoft => MicrosoftRealtimeClient::new(
            key,
            &microsoft::realtime_endpoint(&region).unwrap(),
            options,
        )
        .unwrap()
        .start(language, Some(preview), None)
        .await
        .unwrap_or_else(|error| panic!("start failed: {error}")),
        CloudProvider::Mistral => {
            MistralRealtimeClient::new(key, mistral::REALTIME_ENDPOINT, options)
                .unwrap()
                .start(language, Some(preview), None)
                .await
                .unwrap_or_else(|error| panic!("start failed: {error}"))
        }
        _ => XaiRealtimeClient::new(key, xai::REALTIME_ENDPOINT, options)
            .unwrap()
            .start(language, Some(preview), None)
            .await
            .unwrap_or_else(|error| panic!("start failed: {error}")),
    };
    let audio = pcm(path);
    let begun = Instant::now();
    for chunk in audio.chunks(CHUNK_BYTES) {
        session.submit_audio(chunk).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let stopped = Instant::now();
    match session.finish().await {
        Ok(result) => println!(
            "realtime  first text {}  after stop {:.2}s  language {}  text: {}",
            first_text
                .lock()
                .unwrap()
                .map_or("none".into(), |at| format!(
                    "{:.2}s",
                    at - (begun - started).as_secs_f64()
                )),
            stopped.elapsed().as_secs_f64(),
            result.transcription.language_code,
            result.transcription.text
        ),
        Err(error) => println!("realtime  FAILED: {error}"),
    }
}
