//! Annotation bundles keep stable paths while ownership moves from recording to conversation,
//! and leave with that conversation on deletion, pruning and merge.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use chrono::{TimeZone, Utc};
use mluva_core::annotations::AnnotationStore;
use mluva_core::conversation::ConversationStore;
use mluva_core::history::{HistoryInput, HistoryStore};
use rusqlite::Connection;

fn stores() -> (tempfile::TempDir, HistoryStore, AnnotationStore) {
    let directory = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(directory.path().join("private/history.sqlite3"));
    history.initialize().unwrap();
    let annotations = AnnotationStore::new(&history.database.path);
    (directory, history, annotations)
}

fn bundle(store: &AnnotationStore, capture: &str) -> std::path::PathBuf {
    let annotation = store.create(capture).unwrap();
    fs::create_dir(annotation.directory.join("frames")).unwrap();
    fs::write(annotation.directory.join("frames/frame-01.png"), b"png").unwrap();
    fs::write(
        annotation.directory.join("summary.md"),
        format!(
            "[Screen annotation · 0:05 · 1 keyframe · contact sheet {}]\n- #1 (0:02) [circled] “Synthetic button.” {}\n",
            annotation.directory.join("contact-sheet.png").display(),
            annotation.directory.join("frames/frame-01.png").display()
        ),
    )
    .unwrap();
    annotation.directory
}

#[test]
fn a_bound_bundle_keeps_its_path_and_leaves_with_its_conversation() {
    let (_directory, history, annotations) = stores();
    let path = bundle(&annotations, "capture-1");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(annotations.pending_captures().unwrap(), ["capture-1"]);
    let entry = history
        .add(HistoryInput::dictation("Look here", "Look here"))
        .unwrap();
    annotations
        .bind_capture("capture-1", &entry.identifier)
        .unwrap();
    assert!(annotations.pending_captures().unwrap().is_empty());
    let owned = annotations.recent(&entry.identifier, false).unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].directory, path);
    let markdown = ConversationStore::new(history.clone())
        .export_markdown(&entry.identifier)
        .unwrap();
    let text = fs::read_to_string(markdown).unwrap();
    assert!(text.contains("## Screen annotations"));
    assert!(text.contains("“Synthetic button.”"));
    assert!(text.contains(&format!(
        "![Keyframe 1](../annotations/{}/frames/frame-01.png)",
        owned[0].identifier
    )));
    history.delete(&entry.identifier).unwrap();
    assert!(!path.exists());
    assert!(
        annotations
            .recent(&entry.identifier, false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn pruning_and_merging_follow_the_owning_conversation() {
    let (_directory, history, annotations) = stores();
    let conversations = ConversationStore::new(history.clone());
    let source = history.add(HistoryInput::dictation("one", "one")).unwrap();
    let target = history.add(HistoryInput::dictation("two", "two")).unwrap();
    let moved = bundle(&annotations, "capture-source");
    annotations
        .bind_capture("capture-source", &source.identifier)
        .unwrap();
    conversations
        .merge(&target.identifier, &source.identifier)
        .unwrap();
    assert_eq!(
        annotations.recent(&target.identifier, false).unwrap().len(),
        1
    );
    assert!(moved.exists());

    let old = history.add(HistoryInput::dictation("old", "old")).unwrap();
    let pruned = bundle(&annotations, "capture-old");
    annotations
        .bind_capture("capture-old", &old.identifier)
        .unwrap();
    Connection::open(&history.database.path)
        .unwrap()
        .execute(
            "UPDATE transcription_history SET created_at = '2000-01-01T00:00:00+00:00' WHERE identifier = ?",
            [&old.identifier],
        )
        .unwrap();
    history
        .prune_older_than(
            30,
            &BTreeSet::new(),
            Utc.with_ymd_and_hms(2026, 10, 21, 0, 0, 0).unwrap(),
        )
        .unwrap();
    assert!(!pruned.exists());
    assert!(moved.exists());
}

#[test]
fn unbound_bundles_are_removed_with_their_recording_and_links_are_refused() {
    let (directory, _history, annotations) = stores();
    let path = bundle(&annotations, "capture-cancelled");
    annotations.delete_owner("capture-cancelled", true).unwrap();
    assert!(!path.exists());
    assert!(annotations.pending_captures().unwrap().is_empty());
    assert!(annotations.path_for("../escape").is_err());
    let identifier = uuid_like();
    symlink(directory.path(), annotations.directory().join(&identifier)).unwrap();
    assert!(annotations.path_for(&identifier).is_err());
}

fn uuid_like() -> String {
    "00000000-0000-4000-8000-000000000001".into()
}

#[test]
fn the_opt_in_defaults_off_and_round_trips_privately() {
    use mluva_core::annotations::AnnotationSettings;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(AnnotationSettings::FILE);
    assert!(!AnnotationSettings::load(&path).unwrap().enabled);
    AnnotationSettings { enabled: true }.save(&path).unwrap();
    assert!(AnnotationSettings::load(&path).unwrap().enabled);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::write(&path, br#"{"enabled": true, "unknown": 1}"#).unwrap();
    assert!(AnnotationSettings::load(&path).is_err());
    assert!(fs::read(&path).unwrap().starts_with(b"{\"enabled\": true"));
}
