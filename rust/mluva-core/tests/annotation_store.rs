//! Annotation bundles keep stable paths while ownership moves from recording to conversation,
//! and leave with that conversation on deletion, pruning and merge.

use std::collections::BTreeSet;
use std::fs::{self, DirBuilder};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
use std::path::PathBuf;

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

fn bundle(store: &AnnotationStore, capture: &str) -> PathBuf {
    let annotation = store.create(capture).unwrap();
    fs::write(annotation.directory.join("01.png"), b"png").unwrap();
    fs::write(
        annotation.directory.join("summary.md"),
        format!(
            "[Screen annotation · 0:05 · 1 keyframe · contact sheet {}]\n- #1 (0:02) [circled] “Synthetic button.” {}\n",
            annotation.directory.join("sheet.png").display(),
            annotation.directory.join("01.png").display()
        ),
    )
    .unwrap();
    annotation.directory
}

/// A bundle as earlier releases wrote it: a UUID directory with `frames/frame-NN.png`.
fn legacy_bundle(history: &HistoryStore, identifier: &str, capture: &str) -> PathBuf {
    let directory = history
        .database
        .path
        .with_file_name("annotations")
        .join(identifier);
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory.join("frames"))
        .unwrap();
    fs::write(directory.join("frames/frame-01.png"), b"png").unwrap();
    fs::write(
        directory.join("annotation.json"),
        r#"{"keyframes":[{"file":"frames/frame-01.png"}]}"#,
    )
    .unwrap();
    fs::write(
        directory.join("summary.md"),
        format!(
            "[Screen annotation · 0:05 · 1 keyframe · contact sheet {}]\n- #1 (0:02) [circled] “Earlier button.” {}\n",
            directory.join("contact-sheet.png").display(),
            directory.join("frames/frame-01.png").display()
        ),
    )
    .unwrap();
    Connection::open(&history.database.path)
        .unwrap()
        .execute(
            "INSERT INTO conversation_annotations (identifier, capture_identifier, created_at, keyframes) VALUES (?, ?, '2026-10-20T10:00:00+00:00', 1)",
            [identifier, capture],
        )
        .unwrap();
    directory
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
    // A short identifier directly under the managed directory keeps delivered paths short.
    let identifier = &owned[0].identifier;
    assert!(
        identifier.len() == 8
            && identifier
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_eq!(path, annotations.directory().join(identifier));
    let markdown = ConversationStore::new(history.clone())
        .export_markdown(&entry.identifier)
        .unwrap();
    let text = fs::read_to_string(markdown).unwrap();
    assert!(text.contains("## Screen annotations"));
    assert!(text.contains("“Synthetic button.”"));
    assert!(text.contains(&format!(
        "![Keyframe 1](../annotations/{identifier}/01.png)"
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
    for accepted in ["0123abcd", "00000000-0000-4000-8000-000000000001"] {
        assert_eq!(
            annotations.path_for(accepted).unwrap(),
            annotations.directory().join(accepted)
        );
    }
    for refused in [
        "../escape",
        "",
        "frames",
        "0123ABCD",
        "0123abc",
        "0123abcde",
        "0123abcg",
        "0123ab/d",
        "00000000-0000-4000-8000-00000000000A",
    ] {
        assert!(annotations.path_for(refused).is_err(), "{refused}");
    }
    for identifier in ["00000000-0000-4000-8000-000000000001", "0123abcd"] {
        symlink(directory.path(), annotations.directory().join(identifier)).unwrap();
        assert!(annotations.path_for(identifier).is_err());
    }
}

#[test]
fn a_bundle_from_an_earlier_release_keeps_its_layout_until_its_conversation_is_pruned() {
    let (_directory, history, annotations) = stores();
    let identifier = "00000000-0000-4000-8000-0000000000e1";
    let path = legacy_bundle(&history, identifier, "capture-earlier");
    assert!(annotations.is_complete(identifier));
    let entry = history
        .add(HistoryInput::dictation("Earlier", "Earlier"))
        .unwrap();
    annotations
        .bind_capture("capture-earlier", &entry.identifier)
        .unwrap();
    let owned = annotations.recent(&entry.identifier, false).unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].directory, path);
    assert_eq!(
        annotations.keyframe_files(identifier).unwrap(),
        ["frames/frame-01.png"]
    );
    let markdown = ConversationStore::new(history.clone())
        .export_markdown(&entry.identifier)
        .unwrap();
    let text = fs::read_to_string(markdown).unwrap();
    assert!(text.contains("“Earlier button.”"));
    assert!(text.contains(&format!(
        "![Keyframe 1](../annotations/{identifier}/frames/frame-01.png)"
    )));
    Connection::open(&history.database.path)
        .unwrap()
        .execute(
            "UPDATE transcription_history SET created_at = '2000-01-01T00:00:00+00:00' WHERE identifier = ?",
            [&entry.identifier],
        )
        .unwrap();
    history
        .prune_older_than(
            30,
            &BTreeSet::new(),
            Utc.with_ymd_and_hms(2026, 10, 21, 0, 0, 0).unwrap(),
        )
        .unwrap();
    assert!(!path.exists());
    assert!(
        annotations
            .recent(&entry.identifier, false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn completeness_accepts_only_keyframe_files_inside_the_bundle() {
    let (_directory, _history, annotations) = stores();
    let path = bundle(&annotations, "capture-written");
    let identifier = path.file_name().unwrap().to_str().unwrap();
    fs::write(
        path.join("annotation.json"),
        r#"{"keyframes":[{"file":"01.png"}]}"#,
    )
    .unwrap();
    assert!(annotations.is_complete(identifier));
    assert_eq!(annotations.keyframe_files(identifier).unwrap(), ["01.png"]);
    for listed in [
        "summary.md",
        "../01.png",
        "/01.png",
        "frames/01.png",
        "02.png",
    ] {
        fs::write(
            path.join("annotation.json"),
            format!(r#"{{"keyframes":[{{"file":"{listed}"}}]}}"#),
        )
        .unwrap();
        assert!(!annotations.is_complete(identifier), "{listed}");
    }
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
