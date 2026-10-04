//! Prompt edits and unresolved-draft privacy through real owner-local documents.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::history::{HistoryInput, HistoryStore};
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;
use mluva_core::scratchpad::{ScratchpadDraft, ScratchpadDraftStore};
use mluva_core::titles::{clean_title, fallback_title, title_prompt};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct PromptCase {
    identifier: String,
    text: String,
    error: Option<String>,
}

#[test]
fn prompt_limits_and_empty_custom_instructions_match_the_release() {
    let directory = tempfile::tempdir().unwrap();
    let store = PromptStore::new(directory.path(), "", &[]).unwrap();
    let cases: Vec<PromptCase> =
        serde_json::from_str(include_str!("fixtures/prompt-cases.json")).unwrap();
    for case in cases {
        assert_eq!(
            store
                .validate(&case.identifier, &case.text)
                .err()
                .map(|error| error.to_string()),
            case.error,
            "{}",
            case.identifier
        );
    }
}

#[derive(Deserialize)]
struct PromptReadCase {
    data: String,
    text: String,
    error: String,
    overridden: bool,
}

#[test]
fn malformed_overrides_keep_the_exact_bytes_and_the_released_repair_diagnostic() {
    let directory = tempfile::tempdir().unwrap();
    let store = PromptStore::new(directory.path(), "", &[]).unwrap();
    let cases: Vec<PromptReadCase> =
        serde_json::from_str(include_str!("fixtures/prompt-read-cases.json")).unwrap();
    let path = store.path("cleanup").unwrap();
    for case in cases {
        let bytes = STANDARD.decode(case.data).unwrap();
        fs::write(&path, &bytes).unwrap();
        let state = store.read("cleanup").unwrap();
        assert_eq!(state.text, case.text);
        assert_eq!(state.error, case.error);
        assert_eq!(state.overridden, case.overridden);
        assert_eq!(state.token.as_deref(), Some(bytes.as_slice()));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        store
            .save("cleanup", "Repaired text", state.token.as_deref())
            .unwrap();
        assert_eq!(store.read("cleanup").unwrap().text, "Repaired text");
    }
}

#[test]
fn every_prompt_keeps_exact_multiline_edits_across_restart_and_reset() {
    let directory = tempfile::tempdir().unwrap();
    let store = PromptStore::new(directory.path().join("prompts"), "", &[]).unwrap();
    let text = "Žluťoučký prompt\n\n```mermaid\nflowchart LR\n A --> B\n```\n{literal}  \r\n";
    for prompt in store.catalog() {
        store.save(&prompt.identifier, text, None).unwrap();
        let reopened = PromptStore::new(&store.directory, "", &[]).unwrap();
        let state = reopened.read(&prompt.identifier).unwrap();
        assert_eq!(state.text, text);
        assert_eq!(
            fs::metadata(reopened.path(&prompt.identifier).unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        reopened
            .reset(&prompt.identifier, state.token.as_deref())
            .unwrap();
        assert_eq!(
            reopened.read(&prompt.identifier).unwrap().text,
            prompt.default
        );
    }
    assert!(store.path("../config.json").is_err());
}

#[test]
fn conflicts_keep_external_edits_and_an_existing_snapshot_stays_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let store = PromptStore::new(directory.path(), "", &[]).unwrap();
    let opened = store.read("rewrite-polish").unwrap();
    let frozen = store.snapshot().unwrap();
    fs::write(
        store.path("rewrite-polish").unwrap(),
        "Changed in local editor",
    )
    .unwrap();
    assert!(
        store
            .save(
                "rewrite-polish",
                "Unsaved UI draft",
                opened.token.as_deref()
            )
            .unwrap_err()
            .to_string()
            .contains("changed locally")
    );
    assert!(
        store
            .reset("rewrite-polish", opened.token.as_deref())
            .is_err()
    );
    let current = store.read("rewrite-polish").unwrap();
    assert_eq!(current.text, "Changed in local editor");
    assert_ne!(
        frozen["rewrite-polish"],
        store.snapshot().unwrap()["rewrite-polish"]
    );
    assert!(
        store
            .save("rewrite-polish", "  ", current.token.as_deref())
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(store.path("rewrite-polish").unwrap()).unwrap(),
        current.text
    );
}

#[test]
fn legacy_custom_text_and_saved_style_identities_remain_recoverable() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = "My existing\nLive instructions";
    let store = PromptStore::new(directory.path(), legacy, &DEFAULTS.styles).unwrap();
    assert_eq!(store.read("live-custom").unwrap().text, legacy);
    for style in &DEFAULTS.styles {
        let key = "style-".to_owned() + &style.identifier.to_lowercase();
        assert_eq!(store.read(&key).unwrap().text, style.instructions);
        store.save(&key, "New local instructions", None).unwrap();
        let reopened = PromptStore::new(directory.path(), legacy, &DEFAULTS.styles).unwrap();
        let state = reopened.read(&key).unwrap();
        assert_eq!(state.text, "New local instructions");
        reopened.reset(&key, state.token.as_deref()).unwrap();
        assert_eq!(reopened.read(&key).unwrap().text, style.instructions);
    }
    let saved_key = "style-fdd26fef-80e7-4ad3-9d08-7874266a12e8";
    fs::write(
        directory.path().join(format!("{saved_key}.md")),
        "Existing Message style override",
    )
    .unwrap();
    let reopened = PromptStore::new(directory.path(), legacy, &DEFAULTS.styles).unwrap();
    assert_eq!(
        reopened.read(saved_key).unwrap().text,
        "Existing Message style override"
    );
}

#[test]
fn title_text_and_bounded_untrusted_source_match_the_frozen_release() {
    let fixtures: Value = serde_json::from_str(include_str!("fixtures/title-cases.json")).unwrap();
    for case in fixtures["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        assert_eq!(fallback_title(source), case["fallback"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(clean_title(source)).unwrap(),
            case["cleaned"]
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let source = fixtures["request"]["source"].as_str().unwrap();
    let entry = history
        .add(HistoryInput::dictation(source, source))
        .unwrap();
    assert_eq!(
        title_prompt(&entry, None),
        fixtures["request"]["expected"].as_str().unwrap()
    );
    assert_eq!(history.find(&entry.identifier).unwrap().raw_text, source);
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
            assert!(store.save(draft(), true).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn incognito_keeps_the_live_draft_in_memory_and_erases_durable_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("drafts/scratchpad.json");
    let mut store = ScratchpadDraftStore::new(&path);
    store.save(draft(), true).unwrap();
    let reopened = ScratchpadDraftStore::new(&path);
    assert_eq!(reopened.draft, store.draft);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::write(path.with_extension("tmp"), "old private recovery").unwrap();
    let private = ScratchpadDraft {
        incognito: true,
        text: "Private edited source".into(),
        ..draft()
    };
    store.save(private.clone(), true).unwrap();
    assert_eq!(store.draft, Some(private));
    assert!(!path.exists());
    assert!(!path.with_extension("tmp").exists());
    assert!(ScratchpadDraftStore::new(&path).draft.is_none());
    store.clear(false).unwrap();
    assert!(store.draft.is_none());
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
    let mut store = ScratchpadDraftStore::new(&path);
    store
        .save(
            ScratchpadDraft {
                audio_path: Some(linked.to_string_lossy().into()),
                ..draft()
            },
            true,
        )
        .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(store.clear(true).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_to_string(&outside).unwrap(), "unrelated audio");
    assert!(store.draft.is_some());
    let managed = directory.path().join("recordings/managed.wav");
    fs::write(&managed, "managed audio").unwrap();
    store
        .save(
            ScratchpadDraft {
                audio_path: Some(managed.to_string_lossy().into()),
                ..draft()
            },
            true,
        )
        .unwrap();
    store.clear(true).unwrap();
    assert!(!managed.exists());
    assert!(!path.exists());
    assert!(outside.exists());
}
