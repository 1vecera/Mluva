use crate::support::{Peer, assert_level, hex, mode, reference};
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_audio::meeting::PipeWireMeetingRecorder;
use mluva_audio::recorder::PipeWireRecorder;
use mluva_audio::wav::WaveReader;
use serde_json::json;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[test]
fn active_wav_header_matches_released_crash_recovery_metadata() {
    let reference = reference();
    let case = &reference["live_recording"];
    let peer = Peer::new(&case["config"]);
    let output = peer.path().join("recoverable.wav");
    let (sender, receiver) = mpsc::channel();
    let mut recorder = PipeWireRecorder::new(&peer.executable, None);
    recorder
        .start(
            &output,
            Some(Box::new(move |_, _| {
                let _ = sender.send(());
                Ok(())
            })),
        )
        .unwrap();
    peer.ready("raw");
    receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    let reading = WaveReader::open(&output).unwrap();
    let actual = json!({"compatible":reading.compatible(),"frame_count":reading.metadata.frame_count,"header_hex":hex(&fs::read(&output).unwrap()[..44])});
    assert_eq!(actual, case["live_metadata"]);
    recorder.cancel();
}

#[test]
fn raw_capture_matches_released_chunks_wav_stderr_and_finalization() {
    for case in reference()["recordings"].as_array().unwrap() {
        let peer = Peer::new(&case["config"]);
        let output = peer.path().join("capture.wav");
        let events = Arc::new(Mutex::new(Vec::<(String, f64)>::new()));
        let consumer_events = events.clone();
        let fail = case["config"]["callback_fails"].as_bool() == Some(true);
        let mut recorder = PipeWireRecorder::new(
            &peer.executable,
            Some("configured.microphone; $(touch never-execute)".into()),
        );
        recorder
            .start(
                &output,
                Some(Box::new(move |frames, level| {
                    consumer_events.lock().unwrap().push((hex(frames), level));
                    if fail {
                        Err("synthetic optional consumer failure".into())
                    } else {
                        Ok(())
                    }
                })),
            )
            .unwrap();
        assert!(recorder.active());
        assert_eq!(recorder.output_path(), Some(output.as_path()));
        let arguments = peer.ready("raw");
        assert_eq!(arguments["argv"], case["argv"]);
        let actual = match recorder.stop() {
            Ok(path) => json!({"ok":path.file_name().unwrap().to_str().unwrap()}),
            Err(error) => json!({"error":error.to_string()}),
        };
        assert_eq!(actual, case["result"], "Capture {}", case["name"]);
        assert_eq!(
            hex(&fs::read(&output).unwrap()),
            case["wav_hex"].as_str().unwrap(),
            "WAV {}",
            case["name"]
        );
        assert_eq!(mode(&output), case["file_mode"].as_u64().unwrap() as u32);
        assert_eq!(
            mode(peer.path()),
            case["parent_mode"].as_u64().unwrap() as u32
        );
        let events = events.lock().unwrap();
        assert_eq!(
            events.len(),
            case["callback_calls"].as_u64().unwrap() as usize
        );
        for (index, (chunk, level)) in events.iter().enumerate() {
            assert_eq!(chunk, case["chunks"][index].as_str().unwrap());
            assert_level(*level, case["levels"][index].as_f64().unwrap());
        }
        assert!(!recorder.active());
        assert!(recorder.output_path().is_none());
        assert_eq!(recorder.audio_level(), 0.0);
        assert!(!peer.path().join("never-execute").exists());
    }
}

#[test]
fn cancellation_erases_capture_and_recorder_can_start_again() {
    let config = &reference()["recordings"][0]["config"];
    let peer = Peer::new(config);
    let mut recorder = PipeWireRecorder::new(&peer.executable, None);
    recorder.cancel();
    assert_eq!(
        recorder.stop().unwrap_err().to_string(),
        "No recording is active."
    );
    for index in 0..2 {
        let output = peer.path().join(format!("capture-{index}.wav"));
        recorder.start(&output, None).unwrap();
        let ready = peer.ready("raw");
        let pid = ready["pid"].as_u64().unwrap() as i32;
        let other = peer.path().join("other.wav");
        assert_eq!(
            recorder.start(&other, None).unwrap_err().to_string(),
            "A recording is already active."
        );
        assert!(!other.exists());
        recorder.cancel();
        assert!(!output.exists());
        assert!(!recorder.active());
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        fs::remove_file(peer.path().join("raw.ready.json")).unwrap();
    }
}

#[test]
fn capture_start_failure_and_existing_destinations_leave_no_new_audio() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.wav");
    let mut recorder = PipeWireRecorder::new(directory.path().join("missing-pw-record"), None);
    assert!(recorder.start(&path, None).is_err());
    assert!(!path.exists());
    assert!(!recorder.active());
    fs::write(&path, b"existing audio").unwrap();
    assert_eq!(
        recorder.start(&path, None).unwrap_err().to_string(),
        "The recording destination already exists."
    );
    assert_eq!(fs::read(&path).unwrap(), b"existing audio");
    let link = directory.path().join("link.wav");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(recorder.start(&link, None).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"existing audio");
    let broken = directory.path().join("broken.wav");
    std::os::unix::fs::symlink(directory.path().join("not-created.wav"), &broken).unwrap();
    assert!(recorder.start(&broken, None).is_err());
    assert!(!directory.path().join("not-created.wav").exists());
}

#[test]
fn active_recorder_drop_reaps_child_and_anonymous_stderr_never_creates_a_log() {
    let peer = Peer::new(&reference()["recordings"][0]["config"]);
    let output = peer.path().join("capture.wav");
    let mut recorder = PipeWireRecorder::new(&peer.executable, None);
    recorder.start(&output, None).unwrap();
    let pid = peer.ready("raw")["pid"].as_u64().unwrap() as i32;
    let stderr = fs::read_link(format!("/proc/{pid}/fd/2")).unwrap();
    assert!(stderr.to_string_lossy().ends_with(" (deleted)"));
    assert!(
        fs::read_to_string(format!("/proc/{pid}/status"))
            .unwrap()
            .lines()
            .any(|line| line == "Umask:\t0077")
    );
    drop(recorder);
    assert!(!output.exists());
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(fs::read_dir(peer.path()).unwrap().count(), 3);
}

#[test]
fn meeting_sources_provenance_warnings_and_mix_match_released_process_results() {
    for case in reference()["meetings"].as_array().unwrap() {
        let peer = Peer::new(&case["config"]);
        let output = peer.path().join("meeting.wav");
        let mut recorder = PipeWireMeetingRecorder::new(
            &peer.executable,
            Some("configured.microphone".into()),
            Some("configured.output".into()),
        );
        recorder.start(&output).unwrap();
        let (microphone, system) = recorder.source_paths().unwrap();
        let microphone = microphone.to_path_buf();
        let system = system.to_path_buf();
        for name in ["microphone", "system"] {
            let mut arguments = peer.ready(name)["argv"].as_array().unwrap().clone();
            *arguments.last_mut().unwrap() = json!("<source.wav>");
            assert_eq!(json!(arguments), case["argv"][name]);
        }
        assert_eq!(mode(&microphone), 0o600);
        assert_eq!(mode(&system), 0o600);
        let actual = match recorder.stop() {
            Ok(result) => {
                json!({"ok":{"path":result.path.file_name().unwrap().to_str().unwrap(),"audio_sources":result.audio_sources,"warnings":result.warnings,"duration_seconds":result.duration_seconds}})
            }
            Err(error) => json!({"error":error.to_string()}),
        };
        assert_eq!(actual, case["result"], "Meeting {}", case["name"]);
        assert!(!recorder.active());
        assert!(recorder.source_paths().is_none());
        assert!(!microphone.exists());
        assert!(!system.exists());
        if let Some(expected) = case["wav_hex"].as_str() {
            assert_eq!(hex(&fs::read(&output).unwrap()), expected);
            assert_eq!(mode(&output), 0o600);
        } else {
            assert!(!output.exists());
        }
    }
}

#[test]
fn meeting_cancellation_and_start_guards_preserve_existing_recordings() {
    let peer = Peer::new(&reference()["meetings"][0]["config"]);
    let output = peer.path().join("meeting.wav");
    let mut recorder = PipeWireMeetingRecorder::new(&peer.executable, None, None);
    assert_eq!(
        recorder.stop().unwrap_err().to_string(),
        "No meeting recording is active."
    );
    recorder.start(&output).unwrap();
    let microphone = peer.ready("microphone");
    let system = peer.ready("system");
    assert_eq!(
        recorder.start(&output).unwrap_err().to_string(),
        "A meeting recording is already active."
    );
    let sources = recorder
        .source_paths()
        .map(|(mic, sys)| (mic.to_path_buf(), sys.to_path_buf()))
        .unwrap();
    recorder.cancel();
    assert!(!sources.0.exists() && !sources.1.exists() && !output.exists());
    for ready in [microphone, system] {
        assert_eq!(
            unsafe { libc::kill(ready["pid"].as_u64().unwrap() as i32, 0) },
            -1
        );
    }
    fs::write(&output, b"old meeting").unwrap();
    assert_eq!(
        recorder.start(&output).unwrap_err().to_string(),
        "The meeting recording destination already exists."
    );
    assert_eq!(fs::read(&output).unwrap(), b"old meeting");
    fs::remove_file(&output).unwrap();
    fs::write(&sources.0, b"unfinished").unwrap();
    assert_eq!(
        recorder.start(&output).unwrap_err().to_string(),
        "Unfinished meeting capture files already exist at this destination."
    );
    assert_eq!(fs::read(&sources.0).unwrap(), b"unfinished");
}

#[test]
fn device_listing_uses_a_bounded_native_subprocess_and_controlled_errors() {
    let reference = reference();
    let case = reference["catalogs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["result"]["ok"]["microphones"]
                .as_array()
                .is_some_and(|nodes| nodes.len() == 2)
        })
        .unwrap();
    let peer = Peer::new(&json!({"dump_hex":case["payload_hex"]}));
    let catalog = PipeWireDeviceCatalog::from_system(Some(&peer.executable)).unwrap();
    assert_eq!(json!(catalog), case["result"]["ok"]);
    assert_eq!(peer.ready("dump")["argv"], json!(["--no-colors"]));
    let peer = Peer::new(&json!({"dump_hex":"5b5d","exit":2}));
    assert_eq!(
        PipeWireDeviceCatalog::from_system(Some(&peer.executable))
            .unwrap_err()
            .to_string(),
        "PipeWire audio devices could not be listed."
    );
    let peer = Peer::new(&json!({"dump_hex":"5b5d","extra_dump_bytes":20_000_000}));
    assert_eq!(
        PipeWireDeviceCatalog::from_system(Some(&peer.executable))
            .unwrap_err()
            .to_string(),
        "PipeWire returned an unexpectedly large device graph."
    );
    let peer = Peer::new(&json!({"dump_hex":"5b5d","wait":true}));
    let started = Instant::now();
    assert_eq!(
        PipeWireDeviceCatalog::from_system(Some(&peer.executable))
            .unwrap_err()
            .to_string(),
        "PipeWire audio devices could not be listed."
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let pid = peer.ready("dump")["pid"].as_u64().unwrap() as i32;
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[test]
fn system_factories_skip_executables_the_current_owner_cannot_run() {
    // Explicitly confined PATH: neither factory can select a host audio executable.
    let reference = reference();
    let raw = &reference["recordings"][0];
    let catalog = reference["catalogs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["result"]["ok"]["microphones"]
                .as_array()
                .is_some_and(|nodes| nodes.len() == 2)
        })
        .unwrap();
    let mut config = raw["config"].clone();
    config["dump_hex"] = catalog["payload_hex"].clone();
    config["dump_exit"] = json!(0);
    config["wait"] = json!(false);
    let peer = Peer::new(&config);
    symlink(
        env!("CARGO_BIN_EXE_audio-fixture-peer"),
        peer.path().join("pw-dump"),
    )
    .unwrap();
    let refused = tempfile::tempdir().unwrap();
    for name in ["pw-record", "pw-dump"] {
        let path = refused.path().join(name);
        fs::write(&path, b"synthetic unusable executable").unwrap();
        // For an unprivileged owner, another class's execute bit must not select it.
        let mode = if unsafe { libc::getuid() } == 0 {
            0o600
        } else {
            0o601
        };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    let output = peer.path().join("capture.wav");
    let result = Command::new(&peer.executable)
        .args(["factory-capture", output.to_str().unwrap()])
        .env(
            "PATH",
            std::env::join_paths([refused.path(), peer.path()]).unwrap(),
        )
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        catalog["result"]
    );
    assert_eq!(
        hex(&fs::read(&output).unwrap()),
        raw["wav_hex"].as_str().unwrap()
    );
}
