//! Diagnostics may wait for disk without delaying input or audio erasure.
use mluva_audio::capture::{CaptureRecorder, CaptureStorage};
use mluva_core::{config::AppConfig, diagnostics::DiagnosticsStore, history::HistoryStore};
use mluva_providers::{compatible::CompatibleClient, speech::SpeechClient};
use mluva_workflows::{
    capture::{CaptureOptions, CapturePhase, CaptureSession},
    dictation::DictationWorkflow,
};
use std::{rc::Rc, time::Duration};

#[tokio::test]
async fn locked_diagnostics_do_not_gate_readiness_or_audio_cleanup_and_are_drained() {
    let root = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(root.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let diagnostics = DiagnosticsStore::new(root.path().join("diagnostics.sqlite3"), 100).unwrap();
    diagnostics.initialize().unwrap();
    let speech = SpeechClient::Compatible(
        CompatibleClient::new(
            "http://127.0.0.1:1/v1",
            "UNUSED_KEY",
            None,
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let mut workflow =
        DictationWorkflow::new(AppConfig::default(), speech, history, root.path().into());
    workflow.diagnostics = Some(diagnostics.clone());
    let recordings = root.path().join("recordings");
    let session = CaptureSession::with_recorder(
        Rc::new(workflow),
        CaptureRecorder::phone().unwrap(),
        CaptureStorage::Persistent(recordings.clone()),
        None,
        CaptureOptions::default(),
    )
    .unwrap();
    let lock = rusqlite::Connection::open(&diagnostics.path).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    tokio::time::timeout(Duration::from_secs(1), session.prepare_and_start())
        .await
        .expect("database contention must not delay input readiness")
        .unwrap();
    assert_eq!(session.phase(), CapturePhase::Recording);
    session.append_phone_audio(vec![0; 320]).await.unwrap();
    let cancel = session.cancel();
    tokio::pin!(cancel);
    tokio::select! {
        _ = &mut cancel => panic!("normal terminal completion must drain diagnostics"),
        _ = tokio::time::timeout(Duration::from_secs(1), async {
            while std::fs::read_dir(&recordings).unwrap().next().is_some() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }) => {
            assert_eq!(std::fs::read_dir(&recordings).unwrap().count(), 0);
        }
    }
    lock.execute_batch("COMMIT").unwrap();
    tokio::time::timeout(Duration::from_secs(2), cancel)
        .await
        .unwrap()
        .unwrap();
    let events = diagnostics.recent(100).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].outcome, "cancelled");
    assert_eq!(events[1].provider, "pipewire");
    assert_eq!(events[2].stage, "capture-ready");
    assert_eq!(session.phase(), CapturePhase::Cancelled);
}
