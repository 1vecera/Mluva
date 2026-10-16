//! Opt-in bounded provider verification. Saves synthetic audio; never opens an output device.
use mluva_providers::{credentials::CredentialStore, spoken_reply};
use std::{path::PathBuf, time::Instant};
use tokio_util::sync::CancellationToken;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: spoken_status_probe OUTPUT_DIRECTORY ELEVENLABS_VOICE_ID (six synthetic sentences; may incur provider charges)".into());
    }
    let output = PathBuf::from(&args[1]);
    std::fs::create_dir_all(&output)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o700))?;
    let key = CredentialStore::new().elevenlabs_api_key().await?;
    let sentences = [
        "The test agent is ready for your next message.",
        "Testovací agent je připravený na další zprávu.",
        "The test agent needs your attention before it can continue.",
        "Testovací agent čeká na vaše potvrzení.",
        "The test agent is working; its latest result is not confirmed yet.",
        "Testovací agent pracuje, výsledek ještě není potvrzený.",
    ];
    let mut samples = Vec::new();
    for (index, sentence) in sentences.iter().enumerate() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(4);
        let collecting = tokio::spawn(async move {
            let mut pcm = Vec::new();
            while let Some(chunk) = receiver.recv().await {
                pcm.extend(chunk);
            }
            pcm
        });
        let start = Instant::now();
        let timing = spoken_reply::stream(
            sentence,
            &args[2],
            key.clone(),
            &CancellationToken::new(),
            sender,
        )
        .await?;
        let pcm = collecting.await?;
        let length = pcm.len() as u32;
        let mut wav = b"RIFF".to_vec();
        wav.extend((36 + length).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(24_000u32.to_le_bytes());
        wav.extend(48_000u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(length.to_le_bytes());
        wav.extend(pcm);
        mluva_core::private_files::atomic_write_private(
            &output.join(format!("synthetic-{index}.wav")),
            &wav,
        )?;
        samples.push(serde_json::json!({"language": if index%2==0 {"English"} else {"Czech"}, "first_received_audio_ms": timing.first_audio_ms,
            "stream_complete_ms": timing.total_ms, "wall_ms": start.elapsed().as_millis(), "audio_duration_ms": length/48, "audio_bytes": length}));
    }
    let result = serde_json::json!({"model": spoken_reply::MODEL, "samples": samples, "playback": false,
        "note": "Connection plus synthesis measured to first received PCM; not physical playback or a steady-state latency gate."});
    mluva_core::private_files::atomic_write_private(
        &output.join("timings.json"),
        &serde_json::to_vec_pretty(&result)?,
    )?;
    println!("Six synthetic samples saved; no audio played.");
    Ok(())
}
