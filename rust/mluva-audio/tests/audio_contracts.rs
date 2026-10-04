#[path = "support/process_contracts.rs"]
mod process_contracts;
mod support;
#[path = "support/volatile_contracts.rs"]
mod volatile_contracts;

use mluva_audio::catalog::{PipeWireDeviceKind, parse_pipewire_devices};
use mluva_audio::{pcm16_audio_level, wav};
use serde_json::json;
use std::fs;
use support::{assert_level, hex, mode, reference, unhex};

#[tokio::test]
async fn meeting_owner_matches_released_mixing_and_preserves_transferred_audio() {
    use mluva_audio::{
        capture::CaptureStorage, meeting::PipeWireMeetingRecorder, meeting_capture::MeetingRecorder,
    };
    for case in reference()["meetings"].as_array().unwrap() {
        let peer = support::Peer::new(&case["config"]);
        let recorder = MeetingRecorder::new(PipeWireMeetingRecorder::new(
            &peer.executable,
            Some("configured.microphone".into()),
            Some("configured.output".into()),
        ))
        .unwrap();
        let output = recorder
            .prepare_destination(
                CaptureStorage::Persistent(peer.path().to_owned()),
                "meeting.wav".into(),
            )
            .await
            .unwrap();
        recorder.start(output.clone()).await.unwrap();
        let mut pids = vec![];
        for name in ["microphone", "system"] {
            let ready = peer.ready(name);
            pids.push(ready["pid"].as_u64().unwrap());
            let mut arguments = ready["argv"].as_array().unwrap().clone();
            *arguments.last_mut().unwrap() = json!("<source.wav>");
            assert_eq!(json!(arguments), case["argv"][name]);
        }
        let actual = match recorder.stop().await {
            Ok(result) => {
                json!({"ok":{"path":result.path.file_name().unwrap().to_str().unwrap(),"audio_sources":result.audio_sources,"warnings":result.warnings,"duration_seconds":result.duration_seconds}})
            }
            Err(error) => json!({"error":error.to_string()}),
        };
        assert_eq!(actual, case["result"], "{}", case["name"]);
        assert!(!recorder.active());
        if let Some(bytes) = case["wav_hex"].as_str() {
            assert_eq!(fs::read(&output).unwrap(), unhex(bytes));
            assert_eq!(mode(&output), 0o600);
            recorder.retain_audio().await.unwrap();
        } else {
            assert!(recorder.retain_audio().await.is_err());
        }
        recorder.close().await.unwrap();
        recorder.close().await.unwrap();
        for pid in pids {
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        }
        assert!(
            !peer
                .path()
                .join(".meeting.wav.microphone.part.wav")
                .exists()
        );
        assert!(!peer.path().join(".meeting.wav.system.part.wav").exists());
        if let Some(bytes) = case["wav_hex"].as_str() {
            assert_eq!(fs::read(output).unwrap(), unhex(bytes));
        } else {
            assert!(!output.exists());
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn dropped_meeting_stop_keeps_timers_running_and_erases_untransferred_audio() {
    use mluva_audio::{
        capture::CaptureStorage, meeting::PipeWireMeetingRecorder, meeting_capture::MeetingRecorder,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    let mut config = reference()["meetings"][0]["config"].clone();
    config["microphone"]["finalize_delay_ms"] = json!(250);
    config["system"]["finalize_delay_ms"] = json!(250);
    let peer = support::Peer::new(&config);
    let recorder = Arc::new(
        MeetingRecorder::new(PipeWireMeetingRecorder::new(&peer.executable, None, None)).unwrap(),
    );
    let output = recorder
        .prepare_destination(
            CaptureStorage::Persistent(peer.path().to_owned()),
            "meeting.wav".into(),
        )
        .await
        .unwrap();
    recorder.start(output.clone()).await.unwrap();
    let pids = [
        peer.ready("microphone")["pid"].as_u64().unwrap(),
        peer.ready("system")["pid"].as_u64().unwrap(),
    ];
    let worker = recorder.clone();
    let stopped = tokio::spawn(async move { worker.stop().await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    stopped.abort();
    assert!(stopped.await.unwrap_err().is_cancelled());
    let finished = AtomicBool::new(false);
    let closing = async {
        recorder.close().await.unwrap();
        finished.store(true, Ordering::Release);
    };
    let heartbeat = async {
        let mut ticks = 0;
        while !finished.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            ticks += 1;
        }
        ticks
    };
    let (_, ticks) = tokio::join!(closing, heartbeat);
    assert!(ticks >= 20, "Meeting finalization blocked its async caller");
    assert!(!output.exists());
    assert!(!recorder.active());
    for pid in pids {
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    assert!(
        !peer
            .path()
            .join(".meeting.wav.microphone.part.wav")
            .exists()
    );
    assert!(!peer.path().join(".meeting.wav.system.part.wav").exists());
}

#[tokio::test]
async fn meeting_owner_keeps_unowned_collisions_and_erases_incognito_staging() {
    use mluva_audio::{
        capture::CaptureStorage, meeting::PipeWireMeetingRecorder,
        meeting_capture::MeetingRecorder, volatile::memory_backed,
    };
    let config = reference()["meetings"][0]["config"].clone();
    let peer = support::Peer::new(&config);
    let recorder =
        MeetingRecorder::new(PipeWireMeetingRecorder::new(&peer.executable, None, None)).unwrap();
    assert!(
        recorder
            .prepare_destination(
                CaptureStorage::Persistent(peer.path().to_owned()),
                "../outside.wav".into()
            )
            .await
            .is_err()
    );
    let output = recorder
        .prepare_destination(
            CaptureStorage::Persistent(peer.path().to_owned()),
            "preserved.wav".into(),
        )
        .await
        .unwrap();
    fs::write(&output, b"preserved synthetic recording").unwrap();
    assert!(recorder.start(output.clone()).await.is_err());
    recorder.close().await.unwrap();
    assert_eq!(fs::read(output).unwrap(), b"preserved synthetic recording");
    let peer = support::Peer::new(&config);
    let recorder =
        MeetingRecorder::new(PipeWireMeetingRecorder::new(&peer.executable, None, None)).unwrap();
    let output = recorder
        .prepare_destination(
            CaptureStorage::Incognito {
                cleanup_executable: env!("CARGO_BIN_EXE_mluva-audio-cleanup").into(),
                memory_root: None,
            },
            "meeting.wav".into(),
        )
        .await
        .unwrap();
    let root = output
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    assert!(memory_backed(&root));
    recorder.start(output.clone()).await.unwrap();
    let pids = [
        peer.ready("microphone")["pid"].as_u64().unwrap(),
        peer.ready("system")["pid"].as_u64().unwrap(),
    ];
    let result = recorder.stop().await.unwrap();
    assert_eq!(result.path, output);
    assert!(output.exists());
    assert_eq!(mode(&output), 0o600);
    assert!(recorder.retain_audio().await.is_err());
    recorder.close().await.unwrap();
    assert!(!root.exists());
    for pid in pids {
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn asynchronous_owner_keeps_timers_running_and_reaps_a_slow_audio_consumer() {
    use mluva_audio::{
        capture::{CaptureRecorder, CaptureStorage},
        recorder::PipeWireRecorder,
    };
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };
    let peer = support::Peer::new(&json!({"pcm_hex":"e80318fc","wait":true}));
    let output = peer.path().join("recordings");
    let recorder = CaptureRecorder::new(PipeWireRecorder::new(&peer.executable, None)).unwrap();
    let audio = recorder
        .prepare_destination(CaptureStorage::Persistent(output), "capture.wav".into())
        .await
        .unwrap();
    recorder
        .start(
            audio.clone(),
            Some(Box::new(|_, _| {
                // A real drain callback stalls on EOF while cancellation joins it.
                std::thread::sleep(Duration::from_millis(250));
                Ok(())
            })),
        )
        .await
        .unwrap();
    let pid = peer.ready("raw")["pid"].as_u64().unwrap();
    assert!(recorder.active());
    assert_eq!(mode(&audio), 0o600);
    let finished = AtomicBool::new(false);
    let cancelling = async {
        recorder.cancel().await.unwrap();
        finished.store(true, Ordering::Release);
    };
    let heartbeat = async {
        let mut ticks = 0;
        while !finished.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            ticks += 1;
        }
        ticks
    };
    let (_, ticks) = tokio::join!(cancelling, heartbeat);
    assert!(ticks >= 10, "Audio-drain cleanup blocked the async owner");
    assert!(!audio.exists());
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(!recorder.active());
    assert_eq!(recorder.audio_level(), 0.0);
    recorder.close().await.unwrap();
    recorder.close().await.unwrap();
}

#[test]
fn normalized_rms_matches_released_pcm_outputs() {
    for case in reference()["levels"].as_array().unwrap() {
        assert_level(
            pcm16_audio_level(&unhex(case["pcm_hex"].as_str().unwrap())),
            case["level"].as_f64().unwrap(),
        );
    }
}

#[test]
fn wav_metadata_validation_retains_the_released_disposition() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("audio.wav");
    for (index, case) in reference()["waves"].as_array().unwrap().iter().enumerate() {
        fs::write(&path, unhex(case["wav_hex"].as_str().unwrap())).unwrap();
        let actual = match wav::is_compatible_pcm_wav(&path) {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error.to_string()}),
        };
        assert_eq!(actual, case["result"], "WAV metadata case {index}");
    }
    assert!(!wav::is_compatible_pcm_wav(&directory.path().join("missing.wav")).unwrap());
}

#[test]
fn meeting_mixer_matches_exact_released_wav_bytes_and_erases_failed_outputs() {
    let directory = tempfile::tempdir().unwrap();
    let microphone = directory.path().join("microphone.wav");
    let system = directory.path().join("system.wav");
    let output = directory.path().join("mixed.wav");
    for (index, case) in reference()["mixes"].as_array().unwrap().iter().enumerate() {
        fs::write(&microphone, unhex(case["microphone_hex"].as_str().unwrap())).unwrap();
        fs::write(&system, unhex(case["system_hex"].as_str().unwrap())).unwrap();
        if output.exists() {
            fs::remove_file(&output).unwrap();
        }
        let actual = wav::mix_pcm16_wav(&microphone, &system, &output);
        if let Some(expected) = case["result"].get("ok") {
            actual.unwrap_or_else(|error| panic!("Mix case {index}: {error}"));
            assert_eq!(
                hex(&fs::read(&output).unwrap()),
                expected.as_str().unwrap(),
                "Mix case {index}"
            );
            assert_eq!(mode(&output), 0o600);
        } else {
            assert_eq!(
                actual.unwrap_err().to_string(),
                case["result"]["error"].as_str().unwrap(),
                "Mix case {index} must fail identically"
            );
        }
        assert_eq!(output.exists(), case["output_exists"].as_bool().unwrap());
    }
    fs::write(&output, b"retain existing recording").unwrap();
    assert!(wav::mix_pcm16_wav(&microphone, &system, &output).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"retain existing recording");
}

#[test]
fn device_catalog_matches_released_selection_labels_filtering_and_order() {
    for (index, case) in reference()["catalogs"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let actual = match parse_pipewire_devices(&unhex(case["payload_hex"].as_str().unwrap())) {
            Ok(catalog) => json!({"ok":catalog}),
            Err(error) => json!({"error":error.to_string()}),
        };
        assert_eq!(actual, case["result"], "Device graph case {index}");
    }
    let catalog = parse_pipewire_devices(b"[]").unwrap();
    assert_eq!(
        catalog.display_name(PipeWireDeviceKind::Microphone, None),
        "Default (automatic)"
    );
    assert_eq!(
        catalog.display_name(PipeWireDeviceKind::SystemOutput, Some("missing.output")),
        "Unavailable target: missing.output"
    );
}
