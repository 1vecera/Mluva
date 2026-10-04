//! Real file and SQLite boundaries that protect saved work and volatile capture ownership.

use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use chrono::{TimeZone, Utc};
use mluva_core::conversation::{ConversationStore, MAX_CONVERSATION_CHARACTERS};
use mluva_core::history::{HistoryInput, HistoryStore, RetryRecognition};
use mluva_core::screenshots::{
    ImageInput, MAX_IMAGE_BYTES, MAX_SCREENSHOTS, ScreenshotStore, validate_images, validate_png,
};
use rusqlite::Connection;

fn stores() -> (
    tempfile::TempDir,
    HistoryStore,
    ConversationStore,
    ScreenshotStore,
) {
    let directory = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(directory.path().join("private/history.sqlite3"));
    history.initialize().unwrap();
    let conversations = ConversationStore::new(history.clone());
    let screenshots = ScreenshotStore::new(&history.database.path);
    (directory, history, conversations, screenshots)
}

#[test]
fn search_filters_complete_history_before_limit_with_literal_wildcards() {
    let (_directory, history, conversations, _) = stores();
    let old = history
        .add(HistoryInput::dictation("old original", "old original"))
        .unwrap();
    conversations
        .append(
            &old.identifier,
            "Structure",
            "Ship the 25% improvement.",
            "fixture-model",
        )
        .unwrap();
    Connection::open(&history.database.path).unwrap().execute(
        "UPDATE transcription_history SET created_at = '2000-01-01T00:00:00+00:00' WHERE identifier = ?",
        [&old.identifier],
    ).unwrap();
    for index in 0..90 {
        let text = format!("New {index}");
        history.add(HistoryInput::dictation(&text, &text)).unwrap();
    }
    assert_eq!(conversations.search("", 80, None).unwrap().len(), 80);
    for query in ["25%", "old original", "OLD ORIGINAL"] {
        assert_eq!(
            conversations.search(query, 80, None).unwrap(),
            vec![history.find(&old.identifier).unwrap()],
            "{query}",
        );
    }
    assert!(conversations.search("_", 80, None).unwrap().is_empty());
}

fn png() -> &'static [u8] {
    include_bytes!("fixtures/synthetic-context.png")
}

#[test]
fn a_frozen_request_keeps_its_bytes_while_the_next_request_reads_an_editor_save() {
    let (_directory, history, _, screenshots) = stores();
    let entry = history
        .add(HistoryInput::dictation("Describe this", "Describe this"))
        .unwrap();
    let screenshot = screenshots
        .add(&entry.identifier, png(), false, Some(12.5))
        .unwrap();
    let frozen = screenshots.snapshot(&entry.identifier, false).unwrap();
    let mut annotated = png().to_vec();
    // The released chunk validator accepts an ancillary chunk, as a real editor may write.
    let kind = b"tEXt";
    let content = b"Description\0Synthetic annotation";
    let mut chunk = (content.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(content);
    chunk.extend_from_slice(&crc32fast::hash(&chunk[4..]).to_be_bytes());
    let offset = annotated.len() - 12;
    annotated.splice(offset..offset, chunk);
    validate_png(&annotated).unwrap();
    fs::write(&screenshot.path, &annotated).unwrap();
    assert_eq!(frozen[0].data, png());
    assert_eq!(
        screenshots.snapshot(&entry.identifier, false).unwrap()[0].data,
        annotated
    );
    fs::write(&screenshot.path, &annotated[..annotated.len() - 9]).unwrap();
    assert!(screenshots.snapshot(&entry.identifier, false).is_err());
    assert_eq!(frozen[0].data, png());
}

#[test]
fn a_late_capture_for_a_deleted_owner_erases_its_new_file_without_losing_other_images() {
    let (_directory, history, _, screenshots) = stores();
    let active = screenshots
        .add("active-capture", png(), true, Some(2.0))
        .unwrap();
    assert!(
        screenshots
            .add("deleted-conversation", png(), false, None)
            .is_err()
    );
    assert!(
        screenshots
            .bind_capture("active-capture", "deleted-conversation")
            .is_err()
    );
    assert_eq!(fs::read_dir(screenshots.directory()).unwrap().count(), 1);
    assert_eq!(screenshots.pending_captures().unwrap(), ["active-capture"]);
    let entry = history
        .add(HistoryInput::dictation("Saved", "Saved"))
        .unwrap();
    screenshots
        .bind_capture("active-capture", &entry.identifier)
        .unwrap();
    assert!(screenshots.pending_captures().unwrap().is_empty());
    assert_eq!(
        screenshots.snapshot(&entry.identifier, false).unwrap()[0].captured_after_seconds,
        Some(2.0)
    );
    assert_eq!(
        fs::metadata(&active.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(screenshots.directory())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn screenshot_links_are_never_read_and_cleanup_unlinks_only_the_managed_link() {
    let (directory, _, _, screenshots) = stores();
    let screenshot = screenshots.add("capture", png(), true, None).unwrap();
    let outside = directory.path().join("unrelated-file");
    fs::write(&outside, b"private unrelated content").unwrap();
    fs::remove_file(&screenshot.path).unwrap();
    symlink(&outside, &screenshot.path).unwrap();
    assert!(screenshots.snapshot("capture", true).is_err());
    screenshots.delete_owner("capture", true).unwrap();
    assert_eq!(fs::read(&outside).unwrap(), b"private unrelated content");
    assert!(!screenshot.path.is_symlink());
    assert!(screenshots.pending_captures().unwrap().is_empty());
    fs::remove_dir(screenshots.directory()).unwrap();
    symlink(directory.path(), screenshots.directory()).unwrap();
    assert!(screenshots.path_for(&screenshot.identifier).is_err());
    assert!(screenshots.delete(&screenshot.identifier).is_err());
    assert!(screenshots.path_for("../unrelated-file").is_err());
}

#[test]
fn a_replaced_attachment_fifo_fails_without_waiting_for_a_writer() {
    let (_directory, _, _, screenshots) = stores();
    let screenshot = screenshots.add("capture", png(), true, None).unwrap();
    fs::remove_file(&screenshot.path).unwrap();
    let path = CString::new(screenshot.path.as_os_str().as_encoded_bytes()).unwrap();
    // This external test fixture is a private FIFO; the production reader must reject it by file type.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(
        screenshots
            .snapshot("capture", true)
            .unwrap_err()
            .to_string()
            .contains("regular image")
    );
}

#[test]
fn image_count_and_byte_budgets_fail_before_the_provider_can_receive_a_request() {
    let image = ImageInput {
        data: png().to_vec(),
        captured_after_seconds: None,
    };
    validate_images(&vec![image.clone(); MAX_SCREENSHOTS]).unwrap();
    assert!(validate_images(&vec![image; MAX_SCREENSHOTS + 1]).is_err());
    assert!(
        validate_png(&vec![0; MAX_IMAGE_BYTES + 1])
            .unwrap_err()
            .to_string()
            .contains("8 MB")
    );
    let oversized_request: Vec<_> = (0..4)
        .map(|_| ImageInput {
            data: vec![0; MAX_IMAGE_BYTES],
            captured_after_seconds: None,
        })
        .collect();
    assert!(
        validate_images(&oversized_request)
            .unwrap_err()
            .to_string()
            .contains("Too many")
    );
}

#[test]
fn a_failed_merge_rolls_back_source_replies_screenshot_owners_and_title_guards() {
    let (_directory, history, conversations, screenshots) = stores();
    let first = history
        .add(HistoryInput::dictation("First raw", "First prepared"))
        .unwrap();
    let second = history
        .add(HistoryInput::dictation("Second raw", "Second prepared"))
        .unwrap();
    let reply = conversations
        .append(
            &second.identifier,
            "Polish",
            "Saved reply",
            "synthetic-model",
        )
        .unwrap();
    let screenshot = screenshots
        .add(&second.identifier, png(), false, None)
        .unwrap();
    Connection::open(&history.database.path).unwrap().execute_batch("CREATE TRIGGER fail_merge BEFORE INSERT ON recording_continuations BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END;").unwrap();
    assert!(
        conversations
            .merge(&first.identifier, &second.identifier)
            .is_err()
    );
    assert_eq!(
        conversations.source_text(&first, false).unwrap(),
        first.raw_text
    );
    assert_eq!(
        conversations.source_text(&second, false).unwrap(),
        second.raw_text
    );
    assert!(conversations.replies(&first.identifier).unwrap().is_empty());
    assert_eq!(conversations.replies(&second.identifier).unwrap(), [reply]);
    assert_eq!(
        screenshots.recent(&second.identifier, false).unwrap(),
        [screenshot]
    );
    assert!(
        screenshots
            .recent(&first.identifier, false)
            .unwrap()
            .is_empty()
    );
    assert!(
        history
            .save_generated_title(&first.identifier, "First title", None)
            .unwrap()
    );
    assert!(
        history
            .save_generated_title(&second.identifier, "Second title", None)
            .unwrap()
    );
}

#[test]
fn manual_titles_and_database_deletion_block_late_worker_results() {
    let (_directory, history, conversations, _) = stores();
    let entry = history
        .add(HistoryInput::dictation("Original", "Prepared"))
        .unwrap();
    history.update_title(&entry.identifier, None).unwrap();
    assert!(
        !history
            .save_generated_title(&entry.identifier, "Late automatic title", None)
            .unwrap()
    );
    conversations
        .save_text(&entry.identifier, "Manually edited", None)
        .unwrap();
    conversations
        .append(
            &entry.identifier,
            "Polish",
            "Saved reply",
            "synthetic-model",
        )
        .unwrap();
    Connection::open(&history.database.path)
        .unwrap()
        .execute(
            "DELETE FROM transcription_history WHERE identifier = ?",
            [&entry.identifier],
        )
        .unwrap();
    assert!(conversations.replies(&entry.identifier).unwrap().is_empty());
    assert!(
        conversations
            .append(
                &entry.identifier,
                "Late",
                "Cannot resurrect",
                "synthetic-model"
            )
            .is_err()
    );
    assert!(
        conversations
            .save_text(&entry.identifier, "Cannot resurrect", None)
            .is_err()
    );
    assert!(
        !history
            .save_generated_title(&entry.identifier, "Cannot resurrect", None)
            .unwrap()
    );
    let count: i64 = Connection::open(&history.database.path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM conversation_sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn oversized_documents_and_absorbed_conversations_remain_recoverable() {
    let (_directory, history, conversations, _) = stores();
    let first = history
        .add(HistoryInput::dictation("First", "First prepared"))
        .unwrap();
    let second = history
        .add(HistoryInput::dictation("Second", "Second prepared"))
        .unwrap();
    conversations
        .save_text(
            &first.identifier,
            &"🙂".repeat(MAX_CONVERSATION_CHARACTERS),
            None,
        )
        .unwrap();
    assert!(
        conversations
            .merge(&first.identifier, &second.identifier)
            .is_err()
    );
    assert!(
        conversations
            .append_recording(&first.identifier, &second)
            .is_err()
    );
    assert!(history.continuations(&first.identifier).unwrap().is_empty());
    assert_eq!(
        conversations
            .search("", 80, Some(&first.identifier))
            .unwrap(),
        std::slice::from_ref(&second)
    );
    conversations
        .save_text(&first.identifier, "First edited", None)
        .unwrap();
    conversations
        .merge(&first.identifier, &second.identifier)
        .unwrap();
    assert!(
        conversations
            .merge(&first.identifier, &second.identifier)
            .is_err()
    );
    assert!(
        conversations
            .merge(&second.identifier, &first.identifier)
            .is_err()
    );
    assert_eq!(conversations.search("", 80, None).unwrap().len(), 1);
    assert_eq!(history.find(&second.identifier).unwrap(), second);
}

#[test]
fn audio_retry_and_explicit_delivery_follow_the_frozen_retention_choice() {
    let (_directory, history, _, _) = stores();
    let recordings = history.database.path.parent().unwrap().join("recordings");
    fs::create_dir(&recordings).unwrap();
    let audio = recordings.join("capture.wav");
    fs::write(&audio, b"synthetic recovery bytes").unwrap();
    let entry = history
        .add(HistoryInput {
            retained_audio_path: Some(audio.to_string_lossy().into()),
            audio_retention_policy: Some("always".into()),
            ..HistoryInput::dictation("", "")
        })
        .unwrap();
    assert_eq!(
        history.managed_retained_audio(&entry.identifier).unwrap(),
        audio
    );
    let retry = history
        .mark_retry_ready(
            &entry.identifier,
            RetryRecognition {
                raw_text: "Recovered recognition",
                delivered_text: "Recovered text",
                language_code: "eng",
                transcription_id: Some("synthetic-retry"),
                retain_audio: true,
                recognition_ms: Some(43),
                recognition_route: "managed-local-retry",
            },
        )
        .unwrap();
    assert_eq!(retry.delivery_outcome, "retry-ready");
    assert_eq!(retry.recognition_ms, Some(43));
    history
        .mark_delivered(&entry.identifier, "Accepted", "copied", true, Some(8))
        .unwrap();
    assert!(audio.exists());
    let delivered = history
        .mark_delivered(&entry.identifier, "Accepted", "copied", false, None)
        .unwrap();
    assert!(!audio.exists());
    assert_eq!(delivered.retained_audio_path, None);
    assert_eq!(delivered.delivery_ms, Some(8));
}

#[test]
fn unsafe_persisted_audio_paths_cannot_erase_unrelated_files() {
    let (directory, history, _, _) = stores();
    let outside = directory.path().join("unrelated.wav");
    fs::write(&outside, b"unrelated private bytes").unwrap();
    let recordings = history.database.path.parent().unwrap().join("recordings");
    fs::create_dir(&recordings).unwrap();
    let linked = recordings.join("linked.wav");
    symlink(&outside, &linked).unwrap();
    for path in [&outside, &linked] {
        let entry = history
            .add(HistoryInput {
                retained_audio_path: Some(path.to_string_lossy().into()),
                ..HistoryInput::dictation("Raw", "Prepared")
            })
            .unwrap();
        assert!(history.managed_retained_audio(&entry.identifier).is_err());
        assert!(history.delete(&entry.identifier).is_err());
        assert_eq!(history.find(&entry.identifier).unwrap(), entry);
        assert_eq!(fs::read(&outside).unwrap(), b"unrelated private bytes");
    }
}

#[test]
fn retention_uses_the_latest_segment_and_preserves_active_drafts_and_captures() {
    let (_directory, history, conversations, screenshots) = stores();
    let parent = history
        .add(HistoryInput::dictation(
            "Old conversation",
            "Old conversation",
        ))
        .unwrap();
    let child = history
        .add(HistoryInput::dictation("New segment", "New segment"))
        .unwrap();
    conversations
        .append_recording(&parent.identifier, &child)
        .unwrap();
    conversations
        .append(
            &parent.identifier,
            "Polish",
            "Saved reply",
            "synthetic-model",
        )
        .unwrap();
    let image = screenshots
        .add(&parent.identifier, png(), false, None)
        .unwrap();
    let active = screenshots
        .add("current-capture", png(), true, None)
        .unwrap();
    let connection = Connection::open(&history.database.path).unwrap();
    connection.execute("UPDATE transcription_history SET created_at = '2026-09-01T10:00:00+00:00' WHERE identifier = ?", [&parent.identifier]).unwrap();
    connection.execute("UPDATE transcription_history SET created_at = '2026-09-29T10:00:00+00:00' WHERE identifier = ?", [&child.identifier]).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 30, 12, 0, 0).unwrap();
    assert_eq!(
        history.prune_older_than(7, &BTreeSet::new(), now).unwrap(),
        0
    );
    connection.execute("UPDATE transcription_history SET created_at = '2026-09-02T10:00:00+00:00' WHERE identifier = ?", [&child.identifier]).unwrap();
    assert_eq!(
        history
            .prune_older_than(7, &BTreeSet::from([child.identifier.clone()]), now)
            .unwrap(),
        0
    );
    assert_eq!(
        history.prune_older_than(7, &BTreeSet::new(), now).unwrap(),
        2
    );
    assert!(!image.path.exists());
    assert!(active.path.exists());
    assert!(
        conversations
            .replies(&parent.identifier)
            .unwrap()
            .is_empty()
    );
    assert_eq!(screenshots.pending_captures().unwrap(), ["current-capture"]);
    assert!(history.recent(100).unwrap().is_empty());
    let orphaned: i64 = connection
        .query_row("SELECT COUNT(*) FROM conversation_sources", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(orphaned, 0);
}

#[test]
fn deleting_a_combined_conversation_erases_owned_recordings_replies_and_images() {
    let (_directory, history, conversations, screenshots) = stores();
    let first = history
        .add(HistoryInput::dictation("First", "First"))
        .unwrap();
    let second = history
        .add(HistoryInput::dictation("Second", "Second"))
        .unwrap();
    let first_image = screenshots
        .add(&first.identifier, png(), false, None)
        .unwrap();
    let second_image = screenshots
        .add(&second.identifier, png(), false, Some(3.5))
        .unwrap();
    conversations
        .append(
            &second.identifier,
            "Polish",
            "Completed rewrite",
            "synthetic-model",
        )
        .unwrap();
    conversations
        .merge(&first.identifier, &second.identifier)
        .unwrap();
    assert_eq!(
        screenshots
            .snapshot(&first.identifier, false)
            .unwrap()
            .len(),
        2
    );
    history.delete(&first.identifier).unwrap();
    assert!(!first_image.path.exists());
    assert!(!second_image.path.exists());
    assert!(history.recent(100).unwrap().is_empty());
    let connection = Connection::open(&history.database.path).unwrap();
    for table in [
        "conversation_sources",
        "conversation_rewrites",
        "recording_continuations",
        "conversation_screenshots",
    ] {
        assert_eq!(
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        fs::metadata(&history.database.path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn json_export_keeps_provenance_and_private_file_permissions() {
    let (directory, history, conversations, _) = stores();
    let entry = history
        .add(HistoryInput {
            recognition_route: Some("scribe-v2-batch".into()),
            recognition_fallback_reason: Some("realtime-stream-failed".into()),
            enhancement_provider_id: Some("codex-app-server".into()),
            enhancement_model_identifier: Some("synthetic-model".into()),
            enhancement_context_sources: vec!["selected-text".into(), "screenshots".into()],
            enhancement_outcome: Some("completed".into()),
            recognition_ms: Some(123),
            enhancement_ms: Some(456),
            delivery_ms: Some(7),
            ..HistoryInput::dictation("Raw český text", "Delivered český text")
        })
        .unwrap();
    conversations
        .save_text(&entry.identifier, "Working text", None)
        .unwrap();
    let reply = conversations
        .append(&entry.identifier, "Polish", "Reply", "synthetic-model")
        .unwrap();
    let output = history
        .export(
            &entry,
            &directory.path().join("exports"),
            "json",
            std::slice::from_ref(&reply),
            Some("Working text"),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(value["raw_text"], entry.raw_text);
    assert_eq!(value["working_source"], "Working text");
    assert_eq!(value["rewrites"][0], serde_json::to_value(reply).unwrap());
    assert_eq!(
        value["enhancement_context_sources"],
        serde_json::json!(["selected-text", "screenshots"])
    );
    assert_eq!(value["recognition_ms"], 123);
    assert_eq!(
        fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(output.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(history.find(&entry.identifier).unwrap(), entry);
}
