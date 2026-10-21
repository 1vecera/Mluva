//! Startup recovery and transactional settings through the actual application services.
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::{HistoryInput, HistoryStore},
    scratchpad::ScratchpadDraft,
};
use mluva_workflows::services::{ApplicationServices, SettingsActivity};
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};
fn paths(root: &Path) -> AppPaths {
    AppPaths {
        config: root.join("config"),
        data: root.join("data"),
        runtime: root.join("runtime"),
    }
}

#[test]
fn startup_keeps_broken_documents_and_private_local_stores() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    fs::create_dir_all(&paths.config).unwrap();
    for name in ["config.json", "personalization.json"] {
        fs::write(paths.config.join(name), "{broken private document").unwrap();
    }
    let services = ApplicationServices::open(paths.clone()).unwrap();
    assert!(!services.config_load_error.is_empty());
    assert!(
        services
            .personalization
            .borrow()
            .persistence_error
            .is_some()
    );
    for name in ["config.json", "personalization.json"] {
        assert_eq!(
            fs::read_to_string(paths.config.join(name)).unwrap(),
            "{broken private document"
        );
    }
    for path in [&services.history.database.path, &services.diagnostics.path] {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert_eq!(
        fs::metadata(&services.cwd).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[test]
fn startup_recovers_notes_before_retention_without_uploading_them() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    AppConfig {
        history_retention_days: 1.into(),
        ..Default::default()
    }
    .save(&paths.config.join("config.json"))
    .unwrap();
    let history = HistoryStore::new(paths.data.join("history.sqlite3"));
    history.initialize().unwrap();
    let kept = history
        .add(HistoryInput::dictation(
            "original recognition",
            "original recognition",
        ))
        .unwrap();
    let old = history
        .add(HistoryInput::dictation("expired", "expired"))
        .unwrap();
    rusqlite::Connection::open(&history.database.path)
        .unwrap()
        .execute(
            "UPDATE transcription_history SET created_at = '2001-01-01T00:00:00+00:00'",
            [],
        )
        .unwrap();
    let draft = ScratchpadDraft {
        identifier: "draft".into(),
        history_identifier: Some(kept.identifier.clone()),
        created_at: kept.created_at,
        raw_text: "original recognition".into(),
        text: "Unfinished edit".into(),
        audio_path: None,
        incognito: false,
        audio_retention_policy: "failures".into(),
        session_identifier: None,
    };
    fs::write(
        paths.data.join("scratchpad-draft.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    assert!(!services.recovery_message.is_empty());
    let recovered = services.history.find(&kept.identifier).unwrap();
    assert_eq!(recovered.raw_text, "original recognition");
    assert_eq!(
        services
            .conversations
            .source_text(&recovered, false)
            .unwrap(),
        "Unfinished edit"
    );
    assert!(services.history.find(&old.identifier).is_err());
    assert!(!paths.data.join("scratchpad-draft.json").exists());
}

#[test]
fn active_capture_freezes_settings_and_failed_saves_preserve_the_previous_config() {
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    AppConfig::default()
        .save(&paths.config.join("config.json"))
        .unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    for activity in [
        SettingsActivity {
            preparing: true,
            ..Default::default()
        },
        SettingsActivity {
            recording: true,
            ..Default::default()
        },
        SettingsActivity {
            processing: true,
            ..Default::default()
        },
    ] {
        for change in [
            json!({"transcription_provider":"local"}),
            json!({"incognito_mode":true}),
            json!({"language_code":"ces"}),
        ] {
            assert!(
                services
                    .apply_settings(change.as_object().unwrap(), &activity)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            services
                .apply_settings(json!({"time_format":"12h"}).as_object().unwrap(), &activity)
                .unwrap()
                .is_some()
        );
    }
    assert!(
        services
            .apply_settings(
                json!({"unknown":true}).as_object().unwrap(),
                &Default::default()
            )
            .is_err()
    );
    let before = services.config();
    fs::remove_file(paths.config.join("config.json")).unwrap();
    fs::create_dir(paths.config.join("config.json")).unwrap();
    assert!(
        services
            .apply_settings(
                json!({"transcription_provider":"local"})
                    .as_object()
                    .unwrap(),
                &Default::default()
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(services.config(), before);
}

#[test]
fn startup_gives_an_interrupted_annotation_bundle_a_conversation_and_keeps_its_path() {
    use mluva_core::annotations::AnnotationStore;
    let root = tempfile::tempdir().unwrap();
    let paths = paths(root.path());
    let history = HistoryStore::new(paths.data.join("history.sqlite3"));
    history.initialize().unwrap();
    let store = AnnotationStore::new(&history.database.path);
    let bundle = store.create("interrupted-capture").unwrap();
    fs::create_dir(bundle.directory.join("frames")).unwrap();
    fs::write(bundle.directory.join("frames/frame-01.png"), b"png").unwrap();
    fs::write(
        bundle.directory.join("annotation.json"),
        r#"{"keyframes":[{"file":"frames/frame-01.png"}]}"#,
    )
    .unwrap();
    fs::write(bundle.directory.join("summary.md"), "[Screen annotation]\n").unwrap();
    // Interrupted while being written: no summary, or a listed frame missing.
    let unwritten = store.create("half-written-capture").unwrap();
    fs::write(
        unwritten.directory.join("annotation.json"),
        r#"{"keyframes":[{"file":"frames/frame-01.png"}]}"#,
    )
    .unwrap();
    let missing_frame = store.create("missing-frame-capture").unwrap();
    fs::write(
        missing_frame.directory.join("annotation.json"),
        r#"{"keyframes":[{"file":"frames/frame-01.png"}]}"#,
    )
    .unwrap();
    fs::write(
        missing_frame.directory.join("summary.md"),
        "[Screen annotation]\n",
    )
    .unwrap();
    let services = ApplicationServices::open(paths).unwrap();
    assert!(!unwritten.directory.exists() && !missing_frame.directory.exists());
    assert_eq!(
        services
            .history
            .recent(10)
            .unwrap()
            .iter()
            .filter(|entry| entry.delivered_text.starts_with("Screen annotation from"))
            .count(),
        1,
        "only the finished bundle becomes a History entry"
    );
    let store = AnnotationStore::new(&services.history.database.path);
    assert!(store.pending_captures().unwrap().is_empty());
    let entry = services
        .history
        .recent(10)
        .unwrap()
        .into_iter()
        .find(|entry| entry.delivered_text == "Screen annotation from an interrupted recording.")
        .unwrap();
    let owned = store.recent(&entry.identifier, false).unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].directory, bundle.directory);
    assert!(bundle.directory.join("summary.md").is_file());
}
