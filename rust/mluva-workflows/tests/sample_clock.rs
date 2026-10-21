//! Annotation marks are stamped on the samples written, the clock providers time words on.
use mluva_audio::capture::{CaptureRecorder, CaptureStorage};
use mluva_core::{config::AppConfig, history::HistoryStore};
use mluva_providers::{compatible::CompatibleClient, speech::SpeechClient};
use mluva_workflows::{
    capture::{CaptureOptions, CaptureSession},
    dictation::DictationWorkflow,
};
use std::{rc::Rc, time::Duration};

#[tokio::test]
async fn recorded_seconds_counts_written_samples_not_wall_time() {
    let root = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(root.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let speech = SpeechClient::Compatible(
        CompatibleClient::new(
            "http://127.0.0.1:1/v1",
            "UNUSED_KEY",
            None,
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let workflow =
        DictationWorkflow::new(AppConfig::default(), speech, history, root.path().into());
    let session = CaptureSession::with_recorder(
        Rc::new(workflow),
        CaptureRecorder::phone().unwrap(),
        CaptureStorage::Persistent(root.path().join("recordings")),
        None,
        CaptureOptions::default(),
    )
    .unwrap();
    session.prepare_and_start().await.unwrap();
    assert_eq!(session.recorded_seconds(), 0.0);
    tokio::time::sleep(Duration::from_millis(50)).await;
    // 16 kHz, 16-bit mono: 32,000 bytes are one second however long they took to arrive.
    session.append_phone_audio(vec![0; 16_000]).await.unwrap();
    session.append_phone_audio(vec![0; 16_000]).await.unwrap();
    assert_eq!(session.recorded_seconds(), 1.0);
    session.cancel().await;
}
