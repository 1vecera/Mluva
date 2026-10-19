//! Retiring controls must neither lose local data nor reactivate generation.
use mluva_core::{
    config::AppConfig,
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
    personalization::{DictionaryCaseBehavior, PersonalizationStore},
    scratchpad::{ScratchpadDraft, ScratchpadDraftStore},
};
use serde_json::{Value, json};
use std::fs;

#[test]
fn retired_preferences_do_not_reset_speech_privacy_or_retention() {
    let old = json!({"rewrite_provider":"litellm", "default_mode":"command", "live_rewrite_enabled":true, "live_rewrite_custom_instructions":"private instructions", "show_copy_action":false, "show_save_action":false, "smooth_scrolling":false, "auto_paste":false, "transcription_provider":"local", "local_model":"parakeet-v3", "local_device":"cuda", "incognito_mode":true, "audio_retention_policy":"never", "history_retention_days":30, "language_code":"ces"});
    let config = AppConfig::from_persisted_json(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(config.transcription_provider, "local");
    assert_eq!(config.local_model, "parakeet-v3");
    assert_eq!(config.local_device, "cuda");
    assert!(config.incognito_mode);
    assert_eq!(config.language_code, "ces");
    assert_eq!(config.history_retention_days, 30.into());
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("config.json");
    config.save(&file).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["audio_retention_policy"], "never");
    for key in [
        "rewrite_provider",
        "default_mode",
        "live_rewrite_enabled",
        "live_rewrite_custom_instructions",
        "auto_paste",
        "show_copy_action",
        "show_save_action",
        "smooth_scrolling",
    ] {
        assert!(saved.get(key).is_none());
    }
    assert_eq!(AppConfig::load(&file).unwrap(), config);
    assert!(AppConfig::from_persisted_json(br#"{"new_unknown_setting":true}"#).is_err());
}

#[test]
fn vocabulary_edits_preserve_retired_private_preferences_verbatim() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("personalization.json");
    let legacy = json!({"styles":[{"id":"saved", "instructions":"private legacy instruction"}], "defaultStyleID":"saved", "applicationModes":{"editor":"command"}, "applicationProviderPreferences":{"editor":{"model":"old"}}});
    fs::write(&file, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let mut store = PersonalizationStore::new(&file);
    store
        .save_dictionary_replacement("heard", "Written", None, DictionaryCaseBehavior::Fixed)
        .unwrap();
    store
        .save_snippet("signature", "Daniel", Some(";sig"), None)
        .unwrap();
    let reopened = PersonalizationStore::new(&file);
    let saved = reopened.state().document();
    for (key, value) in legacy.as_object().unwrap() {
        assert_eq!(&saved[key], value);
    }
    assert_eq!(
        reopened
            .process_transcript("heard snippet signature", None, &Default::default())
            .unwrap(),
        "Written Daniel"
    );
}

#[test]
fn notes_migration_preserves_originals_edits_and_replies_and_never_resurrects() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let history = HistoryStore::new(root.path().join("history.sqlite3"));
        history.initialize().unwrap();
        let conversations = ConversationStore::new(history.clone());
        let owner = existing.then(|| {
            history
                .add(HistoryInput::dictation("immutable raw", "delivered"))
                .unwrap()
        });
        if let Some(owner) = &owner {
            conversations
                .save_text(&owner.identifier, "newer working edit", None)
                .unwrap();
            conversations
                .append(&owner.identifier, "Old polish", "Saved result", "old-model")
                .unwrap();
        }
        let file = root.path().join("scratchpad-draft.json");
        let draft = ScratchpadDraft {
            identifier: uuid::Uuid::new_v4().to_string(),
            history_identifier: owner.as_ref().map(|e| e.identifier.clone()),
            created_at: "2026-10-18T09:00:00+00:00".into(),
            raw_text: "immutable raw".into(),
            text: "pending edit".into(),
            audio_path: None,
            incognito: false,
            audio_retention_policy: "failures".into(),
            session_identifier: None,
        };
        let bytes = serde_json::to_vec(&draft).unwrap();
        fs::write(&file, &bytes).unwrap();
        let id = ScratchpadDraftStore::new(&file)
            .migrate_to_history(&history)
            .unwrap()
            .unwrap();
        let entry = history.find(&id).unwrap();
        assert_eq!(entry.raw_text, "immutable raw");
        assert_eq!(
            conversations.source_text(&entry, false).unwrap(),
            if existing {
                "newer working edit"
            } else {
                "pending edit"
            }
        );
        assert_eq!(
            conversations.replies(&id).unwrap().len(),
            usize::from(existing)
        );
        assert!(!file.exists());
        // Replay simulates crashing after the database commit but before deleting JSON.
        fs::write(&file, &bytes).unwrap();
        assert!(
            ScratchpadDraftStore::new(&file)
                .migrate_to_history(&history)
                .unwrap()
                .is_none()
        );
        assert_eq!(history.recent(10).unwrap().len(), 1);
        history.delete(&id).unwrap();
        fs::write(&file, &bytes).unwrap();
        ScratchpadDraftStore::new(&file)
            .migrate_to_history(&history)
            .unwrap();
        assert!(history.recent(10).unwrap().is_empty());
    }
}

#[test]
fn retired_drafts_respect_deletion_incognito_and_malformed_file_recovery() {
    let root = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(root.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let file = root.path().join("scratchpad-draft.json");
    let deleted = history
        .add(HistoryInput::dictation("deleted", "deleted"))
        .unwrap();
    history.delete(&deleted.identifier).unwrap();
    let mut draft = ScratchpadDraft {
        identifier: uuid::Uuid::new_v4().to_string(),
        history_identifier: Some(deleted.identifier),
        created_at: "2026-10-18T09:00:00+00:00".into(),
        raw_text: "private".into(),
        text: "private edit".into(),
        audio_path: None,
        incognito: false,
        audio_retention_policy: "failures".into(),
        session_identifier: None,
    };
    fs::write(&file, serde_json::to_vec(&draft).unwrap()).unwrap();
    assert!(
        ScratchpadDraftStore::new(&file)
            .migrate_to_history(&history)
            .unwrap()
            .is_none()
    );
    assert!(history.recent(10).unwrap().is_empty());
    let audio = root.path().join("recordings/private.wav");
    fs::create_dir_all(audio.parent().unwrap()).unwrap();
    fs::write(&audio, "private audio").unwrap();
    draft.identifier = uuid::Uuid::new_v4().to_string();
    draft.history_identifier = None;
    draft.incognito = true;
    draft.audio_path = Some(audio.to_str().unwrap().into());
    fs::write(&file, serde_json::to_vec(&draft).unwrap()).unwrap();
    assert!(
        ScratchpadDraftStore::new(&file)
            .migrate_to_history(&history)
            .unwrap()
            .is_none()
    );
    assert!(!audio.exists());
    assert!(!file.exists());
    assert!(history.recent(10).unwrap().is_empty());
    fs::write(&file, "{private broken document").unwrap();
    assert!(
        ScratchpadDraftStore::new(&file)
            .migrate_to_history(&history)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "{private broken document"
    );
}
