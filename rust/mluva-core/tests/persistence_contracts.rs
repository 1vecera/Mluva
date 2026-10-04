//! Exercise the native stores against database dumps and observed outputs from the unchanged release.

use std::collections::BTreeMap;
use std::fs;

use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::conversation::{ConversationStore, rewrite_prompt};
use mluva_core::history::{HistoryInput, HistoryStore};
use mluva_core::screenshots::{ImageInput, ScreenshotStore, image_context, validate_png};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/store-baseline.json")).unwrap()
}

fn setup(foundation: bool) -> (tempfile::TempDir, HistoryStore, ConversationStore) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite3");
    Connection::open(&path)
        .unwrap()
        .execute_batch(if foundation {
            include_str!("fixtures/history-foundation.sql")
        } else {
            include_str!("fixtures/released-data.sql")
        })
        .unwrap();
    let history = HistoryStore::new(path);
    history.initialize().unwrap();
    let conversations = ConversationStore::new(history.clone());
    if !foundation {
        let screenshots = ScreenshotStore::new(&history.database.path);
        fs::create_dir_all(screenshots.directory()).unwrap();
        let reference = reference();
        fs::write(
            screenshots
                .path_for(reference["screenshot_identifier"].as_str().unwrap())
                .unwrap(),
            include_bytes!("fixtures/synthetic-context.png"),
        )
        .unwrap();
    }
    (directory, history, conversations)
}

fn observed_state(history: &HistoryStore, conversations: &ConversationStore) -> Value {
    let baseline = reference();
    let identifiers: Vec<_> = baseline["identifiers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let sources: BTreeMap<_, _> = identifiers
        .iter()
        .map(|identifier| {
            (
                identifier.to_string(),
                conversations
                    .source_text(&history.find(identifier).unwrap(), false)
                    .unwrap(),
            )
        })
        .collect();
    let replies: BTreeMap<_, _> = identifiers
        .iter()
        .map(|identifier| {
            let mut replies = conversations.replies(identifier).unwrap();
            for reply in &mut replies {
                if reply.model == "local-merge" {
                    reply.created_at = "<generated>".into();
                }
            }
            (identifier.to_string(), replies)
        })
        .collect();
    let connection = Connection::open(&history.database.path).unwrap();
    let owners: Vec<_> = connection.prepare("SELECT identifier, history_identifier, capture_identifier FROM conversation_screenshots ORDER BY identifier").unwrap()
        .query_map([], |row| Ok(json!({"identifier": row.get::<_, String>(0)?, "history_identifier": row.get::<_, Option<String>>(1)?, "capture_identifier": row.get::<_, Option<String>>(2)?}))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    let revisions: BTreeMap<String, i64> = connection
        .prepare("SELECT identifier, title_revision FROM transcription_history")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let searches: BTreeMap<_, _> = ["", "PŘÍLIŠ", "STRASSE", "25%", "_tag", "%", "_", "\\"]
        .into_iter()
        .map(|query| {
            (
                query,
                conversations
                    .search(query, 80, None)
                    .unwrap()
                    .into_iter()
                    .map(|entry| entry.identifier)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    json!({"entries": history.recent(100).unwrap(), "sources": sources, "replies": replies, "screenshot_owners": owners, "title_revisions": revisions, "searches": searches})
}

#[test]
fn opening_a_foundation_database_preserves_rows_and_adds_all_released_columns() {
    let (_directory, history, _) = setup(true);
    assert_eq!(
        serde_json::to_value(history.recent(100).unwrap()).unwrap(),
        reference()["migrated_entries"]
    );
    history.initialize().unwrap();
    assert_eq!(
        serde_json::to_value(history.recent(100).unwrap()).unwrap(),
        reference()["migrated_entries"]
    );
}

#[test]
fn correction_reprocessing_and_raw_restore_match_the_saved_reference_states() {
    let (_directory, history, _) = setup(false);
    let baseline = reference();
    let identifier = baseline["identifiers"][0].as_str().unwrap();
    let steps = [
        history
            .correct_delivered_text(identifier, " First corrected. ")
            .unwrap(),
        history
            .correct_delivered_text(identifier, "Second corrected.")
            .unwrap(),
        history
            .reprocess(identifier, "Fresh deterministic text.", 109)
            .unwrap(),
        history.restore_raw(identifier).unwrap(),
    ];
    assert_eq!(
        serde_json::to_value(steps).unwrap(),
        baseline["correction_states"]
    );
}

#[test]
fn continued_recordings_match_the_reference_and_preserve_every_original_segment() {
    let (_directory, history, conversations) = setup(false);
    let baseline = reference();
    let identifier = baseline["identifiers"][0].as_str().unwrap();
    let segment = history
        .find(baseline["identifiers"][2].as_str().unwrap())
        .unwrap();
    assert_eq!(
        conversations
            .append_recording(identifier, &segment)
            .unwrap(),
        baseline["continued_output"].as_str().unwrap()
    );
    assert_eq!(history.find(&segment.identifier).unwrap(), segment);
    assert_eq!(
        observed_state(&history, &conversations),
        baseline["continued"]
    );
    assert!(
        conversations
            .append_recording(identifier, &segment)
            .is_err()
    );
}

#[test]
fn merging_matches_source_reply_screenshot_title_and_search_ownership() {
    let (directory, history, conversations) = setup(false);
    let baseline = reference();
    let target = baseline["identifiers"][0].as_str().unwrap();
    let source = baseline["identifiers"][1].as_str().unwrap();
    let target_original = history.find(target).unwrap().raw_text;
    let source_original = history.find(source).unwrap().raw_text;
    conversations.merge(target, source).unwrap();
    assert_eq!(history.find(target).unwrap().raw_text, target_original);
    assert_eq!(history.find(source).unwrap().raw_text, source_original);
    assert_eq!(observed_state(&history, &conversations), baseline["merged"]);
    let entry = history.find(target).unwrap();
    let replies = conversations.replies(target).unwrap();
    let working = conversations.source_text(&entry, false).unwrap();
    assert_eq!(
        rewrite_prompt(&entry, &replies, "  Přelož do češtiny.  ", Some(&working)).unwrap(),
        baseline["merged_prompt"].as_str().unwrap()
    );
    let export = history
        .export(
            &entry,
            &directory.path().join("exports"),
            "markdown",
            &replies,
            Some(&working),
        )
        .unwrap();
    let mut actual = fs::read_to_string(export).unwrap();
    for reply in replies {
        if reply.model == "local-merge" {
            actual = actual.replace(&reply.created_at, "<generated>");
        }
    }
    assert_eq!(actual, baseline["merged_markdown"].as_str().unwrap());
    assert!(
        !history
            .save_generated_title(target, "Late title", None)
            .unwrap()
    );
    assert!(
        !history
            .save_generated_title(source, "Other late title", None)
            .unwrap()
    );
}

#[test]
fn rewriting_and_visual_context_keep_the_released_payload_text() {
    let (_directory, history, conversations) = setup(false);
    let baseline = reference();
    let identifier = baseline["identifiers"][0].as_str().unwrap();
    assert_eq!(
        rewrite_prompt(
            &history.find(identifier).unwrap(),
            &conversations.replies(identifier).unwrap(),
            "Make it shorter.",
            None
        )
        .unwrap(),
        baseline["seed_prompt"].as_str().unwrap()
    );
    let images = vec![
        ImageInput {
            data: include_bytes!("fixtures/synthetic-context.png").to_vec(),
            captured_after_seconds: Some(1.25),
        },
        ImageInput {
            data: include_bytes!("fixtures/synthetic-context.png").to_vec(),
            captured_after_seconds: None,
        },
    ];
    assert_eq!(
        image_context("Synthetic narration.", &images),
        baseline["image_context"].as_str().unwrap()
    );
    assert_eq!(
        images[0].data_url(),
        baseline["image_data_url"].as_str().unwrap()
    );
}

#[derive(Deserialize)]
struct PngCase {
    label: String,
    data: String,
    error: Option<String>,
}

#[test]
fn png_validation_matches_all_observed_truncation_crc_and_pixel_budget_results() {
    let cases: Vec<PngCase> =
        serde_json::from_str(include_str!("fixtures/png-cases.json")).unwrap();
    for case in cases {
        let data = STANDARD.decode(&case.data).unwrap();
        let actual = validate_png(&data).err().map(|error| error.to_string());
        assert_eq!(actual, case.error, "{}", case.label);
    }
}

#[derive(Deserialize)]
struct ProvenanceCase {
    input: Value,
    accepted: bool,
    expected: Option<Value>,
}

#[test]
fn durable_provenance_and_stage_timings_match_the_released_acceptance_rules() {
    let directory = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let cases: Vec<ProvenanceCase> =
        serde_json::from_str(include_str!("fixtures/provenance-cases.json")).unwrap();
    for case in cases {
        let before = history.recent(i64::MAX).unwrap().len();
        let result = serde_json::from_value::<HistoryInput>(case.input.clone())
            .map_err(|error| error.to_string())
            .and_then(|input| history.add(input).map_err(|error| error.to_string()));
        assert_eq!(result.is_ok(), case.accepted, "input: {}", case.input);
        if let Ok(entry) = result {
            let mut actual = serde_json::to_value(entry).unwrap();
            actual.as_object_mut().unwrap().remove("identifier");
            actual.as_object_mut().unwrap().remove("created_at");
            assert_eq!(actual, case.expected.unwrap());
        } else {
            assert_eq!(history.recent(i64::MAX).unwrap().len(), before);
        }
    }
}
