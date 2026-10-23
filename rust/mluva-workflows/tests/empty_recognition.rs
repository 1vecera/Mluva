//! An empty recognition on a route that expects text keeps its recording; ordinary dictation does not.
use mluva_core::{
    config::{AppConfig, AudioRetentionPolicy},
    history::HistoryStore,
};
use mluva_providers::{TranscriptionResult, compatible::CompatibleClient, speech::SpeechClient};
use mluva_workflows::dictation::{Completion, DictationWorkflow, WorkflowResult};
use std::{path::Path, time::Duration};

fn workflow(root: &Path) -> DictationWorkflow {
    let history = HistoryStore::new(root.join("history.sqlite3"));
    history.initialize().unwrap();
    let speech = SpeechClient::Compatible(
        CompatibleClient::new(
            "http://127.0.0.1:1/v1",
            "UNUSED_KEY",
            None,
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    DictationWorkflow::new(AppConfig::default(), speech, history, root.into())
}

fn silence() -> TranscriptionResult {
    TranscriptionResult {
        text: " \n".into(),
        language_code: "eng".into(),
        language_probability: None,
        transcription_id: None,
        speaker_segments: vec![],
        audio_duration_seconds: Some(12.0),
        words: vec![],
    }
}

async fn complete_silence(
    workflow: &DictationWorkflow,
    audio: &Path,
    policy: AudioRetentionPolicy,
    incognito: bool,
    keep_empty_recognition: bool,
) -> WorkflowResult {
    std::fs::write(audio, b"RIFF").unwrap();
    workflow
        .complete(
            audio,
            Completion {
                recognized_transcription: Some(silence()),
                recognition_duration_seconds: Some(0.1),
                audio_retention_policy: policy,
                incognito,
                defer_delivery: keep_empty_recognition,
                keep_empty_recognition,
                ..Default::default()
            },
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn silence_on_an_agent_route_is_kept_with_its_audio_to_recognise_again() {
    for policy in [AudioRetentionPolicy::Failures, AudioRetentionPolicy::Always] {
        let root = tempfile::tempdir().unwrap();
        let workflow = workflow(root.path());
        let audio = root.path().join("capture.wav");
        let result = complete_silence(&workflow, &audio, policy, false, true).await;
        let entry = result.history_entry.expect("a row for the empty capture");
        assert_eq!(entry.delivery_outcome, "no-speech");
        assert_eq!(entry.mode, "dictation");
        // Raw recognition stays raw: nothing was recognised, nothing is invented.
        assert!(entry.raw_text.trim().is_empty() && entry.delivered_text.is_empty());
        assert_eq!(
            entry.retained_audio_path.as_deref(),
            audio.to_str(),
            "{policy:?}"
        );
        assert!(audio.exists());
        assert_eq!(result.retained_audio_path.as_deref(), Some(audio.as_path()));
        assert!(result.delivery.guidance.contains("kept in History"));
        assert_eq!(workflow.history.recent(10).unwrap().len(), 1);
    }
}

#[tokio::test]
async fn incognito_and_never_keep_neither_a_row_nor_the_audio() {
    for (policy, incognito) in [
        (AudioRetentionPolicy::Failures, true),
        (AudioRetentionPolicy::Always, true),
        (AudioRetentionPolicy::Never, false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let workflow = workflow(root.path());
        let audio = root.path().join("capture.wav");
        let result = complete_silence(&workflow, &audio, policy, incognito, true).await;
        assert!(result.history_entry.is_none(), "{policy:?} {incognito}");
        assert!(result.retained_audio_path.is_none());
        assert!(!audio.exists(), "{policy:?} {incognito}");
        assert!(workflow.history.recent(10).unwrap().is_empty());
    }
}

#[tokio::test]
async fn ordinary_dictation_keeps_dropping_silence() {
    let root = tempfile::tempdir().unwrap();
    let workflow = workflow(root.path());
    let audio = root.path().join("capture.wav");
    let result = complete_silence(
        &workflow,
        &audio,
        AudioRetentionPolicy::Failures,
        false,
        false,
    )
    .await;
    assert!(result.history_entry.is_none());
    assert!(!audio.exists());
    assert!(workflow.history.recent(10).unwrap().is_empty());
}
