//! Isolated synthetic client/capture process; never part of installed binaries.
use mluva_audio::recorder::PipeWireRecorder;
use mluva_providers::{
    Secret,
    compatible::CompatibleClient,
    realtime::{ElevenLabsRealtimeClient, RealtimeOptions},
};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

fn audio() {
    unsafe {
        libc::signal(libc::SIGINT, stop as *const () as libc::sighandler_t);
    }
    let pcm = (0..9600)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    io::stdout().write_all(&pcm).unwrap();
    io::stdout().flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !STOP.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn safe_endpoint(value: &str) {
    let url = url::Url::parse(value).unwrap();
    assert!(
        matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
        "fixture peers only use loopback"
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|value| value == "--rate") {
        audio();
        return;
    }
    safe_endpoint(&args[2]);
    let result: Value = match args[1].as_str() {
        "catalog" => {
            let client =
                CompatibleClient::new(&args[2], "MLUVA_FIXTURE_KEY", None, Duration::from_secs(2))
                    .unwrap();
            match client.list_models(None).await {
                Ok(models) => json!({"ok":models}),
                Err(error) => json!({"error":error.to_string()}),
            }
        }
        "realtime" => {
            let client = ElevenLabsRealtimeClient::new(
                Secret::new("synthetic-wire-credential"),
                &args[2],
                RealtimeOptions {
                    session_timeout: Duration::from_secs(2),
                    finalization_timeout: Duration::from_secs(2),
                    ..Default::default()
                },
            )
            .unwrap();
            match client.start("auto", None, None).await {
                Ok(session) => match session.finish().await {
                    Ok(result) => json!({"ok":result.transcription}),
                    Err(error) => json!({"error":error.to_string()}),
                },
                Err(error) => json!({"error":error.to_string()}),
            }
        }
        "audio-realtime" => {
            record(
                &args[2],
                Path::new(&args[3]),
                args.get(4).is_some_and(|value| value == "failed"),
            )
            .await
        }
        _ => panic!("unknown fixture peer command"),
    };
    println!("{result}");
}

async fn record(endpoint: &str, path: &Path, failed: bool) -> Value {
    let client = ElevenLabsRealtimeClient::new(
        Secret::new("synthetic-wire-credential"),
        endpoint,
        RealtimeOptions {
            session_timeout: Duration::from_secs(2),
            finalization_timeout: Duration::from_secs(2),
            ..Default::default()
        },
    )
    .unwrap();
    let session = Arc::new(client.start("auto", None, None).await.unwrap());
    let forwarded = Arc::new(AtomicUsize::new(0));
    let feed = session.clone();
    let count = forwarded.clone();
    let path: PathBuf = path.into();
    let recorder = tokio::task::spawn_blocking(move || {
        let mut recorder = PipeWireRecorder::new(std::env::current_exe().unwrap(), None);
        recorder
            .start(
                &path,
                Some(Box::new(move |pcm, _| {
                    count.fetch_add(pcm.len(), Ordering::Relaxed);
                    if feed.submit_audio(pcm).map_err(|error| error.to_string())? {
                        Ok(())
                    } else {
                        Err("synthetic streaming failure".into())
                    }
                })),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while forwarded.load(Ordering::Relaxed) == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let saved = recorder.stop().unwrap();
        assert!(mluva_audio::wav::is_compatible_pcm_wav(&saved).unwrap());
    });
    recorder.await.unwrap();
    if failed {
        let deadline = Instant::now() + Duration::from_secs(2);
        while session.is_healthy() {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    match session.finish().await {
        Ok(result) => json!({"ok":result.transcription}),
        Err(error) => json!({"error":error.to_string()}),
    }
}
