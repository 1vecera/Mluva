//! Legacy Notes recovery and local titles through real private documents.

use mluva_core::history::HistoryStore;
use std::fs;
use std::os::unix::fs::symlink;

use mluva_core::scratchpad::{ScratchpadDraft, ScratchpadDraftStore};
use mluva_core::titles::fallback_title;
use serde_json::Value;

#[test]
fn title_text_and_bounded_untrusted_source_match_the_frozen_release() {
    let fixtures: Value = serde_json::from_str(include_str!("fixtures/title-cases.json")).unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        assert_eq!(fallback_title(source), case["fallback"].as_str().unwrap());
    }
}

fn draft() -> ScratchpadDraft {
    ScratchpadDraft {
        identifier: "draft-one".into(),
        history_identifier: None,
        created_at: "2026-09-01T12:00:00+00:00".into(),
        raw_text: "Raw source".into(),
        text: "Edited source".into(),
        audio_path: None,
        incognito: false,
        audio_retention_policy: "failures".into(),
        session_identifier: Some("capture-one".into()),
    }
}

#[test]
fn scratchpad_legacy_privacy_defaults_and_damaged_documents_match_the_reference() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/scratchpad-cases.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scratchpad.json");
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    for case in cases {
        let bytes = serde_json::to_vec(&case["input"]).unwrap();
        fs::write(&path, &bytes).unwrap();
        let mut store = ScratchpadDraftStore::new(&path);
        assert_eq!(
            serde_json::to_value(&store.draft).unwrap(),
            case["expected"]
        );
        assert_eq!(
            store.persistence_error.is_some(),
            case["damaged"].as_bool().unwrap()
        );
        if store.persistence_error.is_some() {
            assert!(store.migrate_to_history(&history).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn scratchpad_audio_cleanup_preserves_external_files_and_the_active_draft() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scratchpad.json");
    let outside = directory.path().join("unrelated.wav");
    fs::write(&outside, "unrelated audio").unwrap();
    fs::create_dir(directory.path().join("recordings")).unwrap();
    let linked = directory.path().join("recordings/linked.wav");
    symlink(&outside, &linked).unwrap();
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let write_draft = |audio: &std::path::Path| {
        fs::write(
            &path,
            serde_json::to_vec(&ScratchpadDraft {
                audio_path: Some(audio.to_string_lossy().into()),
                incognito: true,
                ..draft()
            })
            .unwrap(),
        )
        .unwrap();
    };
    write_draft(&linked);
    let before = fs::read(&path).unwrap();
    assert!(
        ScratchpadDraftStore::new(&path)
            .migrate_to_history(&history)
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_to_string(&outside).unwrap(), "unrelated audio");
    let managed = directory.path().join("recordings/managed.wav");
    fs::write(&managed, "managed audio").unwrap();
    write_draft(&managed);
    fs::write(path.with_extension("tmp"), "private temporary draft").unwrap();
    ScratchpadDraftStore::new(&path)
        .migrate_to_history(&history)
        .unwrap();
    assert!(!managed.exists());
    assert!(!path.exists());
    assert!(!path.with_extension("tmp").exists());
    assert!(outside.exists());
    assert!(history.recent(10).unwrap().is_empty());
}
