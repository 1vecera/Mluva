use mluva_core::{
    account::{AccountLedger, AccountNote, AccountRewrite},
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
};
fn fixture() -> (tempfile::TempDir, AccountLedger) {
    let dir = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(dir.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let ledger = AccountLedger::new(ConversationStore::new(history), "alice".into()).unwrap();
    (dir, ledger)
}
#[test]
fn sign_in_does_not_upload_old_history_and_snapshots_exclude_audio_and_config() {
    let (_dir, l) = fixture();
    let entry = l
        .store
        .history
        .add(HistoryInput::dictation("Original", "Delivered"))
        .unwrap();
    assert!(l.ids().unwrap().is_empty());
    l.track(&entry.identifier).unwrap();
    let pending = l.pending(&entry.identifier).unwrap().unwrap();
    let payload = serde_json::to_string(&pending).unwrap();
    for field in ["audio", "path", "credential", "provider", "model_directory"] {
        assert!(!payload.contains(field));
    }
    let again = l.pending(&entry.identifier).unwrap().unwrap();
    assert_eq!(pending.operation_id, again.operation_id);
    l.store
        .save_text(&entry.identifier, "Edited during upload", None)
        .unwrap();
    let mut reply = pending.note;
    reply.revision = 1;
    l.acknowledge(&reply).unwrap();
    let next = l.pending(&entry.identifier).unwrap().unwrap();
    assert_ne!(next.operation_id, pending.operation_id);
    assert_eq!(next.base_revision, 1);
    assert_eq!(next.note.text, "Edited during upload");
    assert_eq!(next.note.raw_text, "Original");
}
#[test]
fn incoming_edits_preserve_raw_and_local_conflicts_survive_reconnect() {
    let (_dir, l) = fixture();
    let mut note = AccountNote {
        id: uuid::Uuid::new_v4().to_string(),
        raw_text: "Original".into(),
        text: "Source".into(),
        delivered_text: "Delivered".into(),
        mode: "dictation".into(),
        revision: 1,
        created_at: 1791800000000,
        ..AccountNote::default()
    };
    assert!(l.receive(&note).unwrap());
    assert!(!l.receive(&note).unwrap());
    l.store
        .save_text(&note.id, "Local offline edit", None)
        .unwrap();
    note.revision = 2;
    note.text = "Remote edit".into();
    assert!(l.receive(&note).is_err());
    assert_eq!(
        l.store
            .source_text(&l.store.history.find(&note.id).unwrap(), false)
            .unwrap(),
        "Local offline edit"
    );
    let retry = AccountLedger::new(l.store.clone(), "alice".into()).unwrap();
    assert!(retry.receive(&note).is_err());
    retry.apply(&note).unwrap();
    assert_eq!(
        retry.store.history.find(&note.id).unwrap().raw_text,
        "Original"
    );
    note.revision = 3;
    note.rewrites = vec![AccountRewrite {
        id: "2026-10-12T00:00:00Z".into(),
        created_at: "2026-10-12T00:00:00Z".into(),
        instruction: "Polish".into(),
        text: "Complete reply".into(),
        model: "explicit choice".into(),
    }];
    retry.receive(&note).unwrap();
    assert_eq!(
        retry.store.replies(&note.id).unwrap()[0].text,
        "Complete reply"
    );
    assert!(retry.pending(&note.id).unwrap().is_none());
    retry.delete(&note.id).unwrap();
    assert!(!retry.receive(&note).unwrap());
    assert!(retry.store.history.find(&note.id).is_err());
}
#[test]
fn account_partition_and_sign_out_preserve_owned_recordings_but_clear_downloads() {
    let (_dir, l) = fixture();
    let local = l
        .store
        .history
        .add(HistoryInput::dictation("Local original", "Local output"))
        .unwrap();
    l.track(&local.identifier).unwrap();
    let note = AccountNote {
        id: uuid::Uuid::new_v4().to_string(),
        raw_text: "Remote original".into(),
        text: "Remote".into(),
        delivered_text: "Remote".into(),
        mode: "dictation".into(),
        revision: 1,
        ..AccountNote::default()
    };
    l.receive(&note).unwrap();
    let bob = AccountLedger::new(l.store.clone(), "bob".into()).unwrap();
    assert!(bob.ids().unwrap().is_empty());
    l.sign_out().unwrap();
    assert!(l.store.history.find(&local.identifier).is_ok());
    assert!(l.store.history.find(&note.id).is_err());
}

#[test]
fn merged_downloads_cannot_steal_private_notes_and_signout_preserves_local_originals() {
    use mluva_core::account::AccountSegment;
    let (_dir, l) = fixture();
    let local = l
        .store
        .history
        .add(HistoryInput::dictation("Local original", "Local original"))
        .unwrap();
    let mut remote = AccountNote {
        id: uuid::Uuid::new_v4().to_string(),
        raw_text: "Remote original".into(),
        text: "Remote".into(),
        delivered_text: "Remote".into(),
        mode: "dictation".into(),
        revision: 1,
        ..AccountNote::default()
    };
    l.receive(&remote).unwrap();
    remote.revision = 2;
    remote.segments.push(AccountSegment {
        id: local.identifier.clone(),
        raw_text: local.raw_text.clone(),
    });
    assert!(
        l.receive(&remote).is_err(),
        "private local note must not become an account segment"
    );
    assert_eq!(
        l.store
            .history
            .recording_conversation(&local.identifier)
            .unwrap()
            .identifier,
        local.identifier
    );
    l.track(&local.identifier).unwrap();
    let mut baseline = l.pending(&local.identifier).unwrap().unwrap().note;
    baseline.revision = 1;
    l.acknowledge(&baseline).unwrap();
    l.receive(&remote).unwrap();
    l.delete(&local.identifier).unwrap(); // Cloud merge hides its former standalone history row.
    assert!(l.store.history.find(&local.identifier).is_ok());
    l.sign_out().unwrap();
    assert!(l.store.history.find(&remote.id).is_err());
    assert_eq!(
        l.store
            .history
            .recording_conversation(&local.identifier)
            .unwrap()
            .raw_text,
        "Local original"
    );
}
