//! An annotation summary joins the delivered message only; History keeps the dictation.
use mluva_core::{config::AppConfig, history::HistoryStore};
use mluva_providers::{TranscriptionResult, compatible::CompatibleClient, speech::SpeechClient};
use mluva_workflows::dictation::{Completion, DictationWorkflow, with_appendix};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn workflow(root: &std::path::Path) -> DictationWorkflow {
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

fn recognized(text: &str) -> TranscriptionResult {
    TranscriptionResult {
        text: text.into(),
        language_code: "eng".into(),
        language_probability: None,
        transcription_id: None,
        speaker_segments: vec![],
        audio_duration_seconds: Some(2.0),
        words: vec![],
    }
}

#[tokio::test]
async fn the_appendix_follows_final_recognition_and_stays_out_of_history() {
    let root = tempfile::tempdir().unwrap();
    let workflow = workflow(root.path());
    let audio = root.path().join("capture.wav");
    std::fs::write(&audio, b"RIFF").unwrap();
    let seen = Rc::new(RefCell::new(None));
    let observed = seen.clone();
    let result = workflow
        .complete(
            &audio,
            Completion {
                recognized_transcription: Some(recognized("Fix this button.")),
                recognition_duration_seconds: Some(0.1),
                defer_delivery: true,
                appendix: Some(Box::new(move |transcription| {
                    observed.replace(Some(transcription.text.clone()));
                    Box::pin(async { Some("[Screen annotation · 0:02 · 1 keyframe]\n".into()) })
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(seen.borrow().as_deref(), Some("Fix this button."));
    assert_eq!(
        result.delivery_appendix.as_deref(),
        Some("[Screen annotation · 0:02 · 1 keyframe]\n")
    );
    assert_eq!(result.output_text, "Fix this button.");
    let entry = result.history_entry.unwrap();
    assert_eq!(entry.raw_text, "Fix this button.");
    assert_eq!(entry.delivered_text, "Fix this button.");
    assert_eq!(
        with_appendix(&result.output_text, result.delivery_appendix.as_deref()),
        "Fix this button.\n\n[Screen annotation · 0:02 · 1 keyframe]"
    );
}

#[tokio::test]
async fn silence_completes_quietly_without_building_an_appendix() {
    let root = tempfile::tempdir().unwrap();
    let workflow = workflow(root.path());
    let audio = root.path().join("capture.wav");
    std::fs::write(&audio, b"RIFF").unwrap();
    let called = Rc::new(RefCell::new(false));
    let flag = called.clone();
    let result = workflow
        .complete(
            &audio,
            Completion {
                recognized_transcription: Some(recognized("  ")),
                recognition_duration_seconds: Some(0.1),
                appendix: Some(Box::new(move |_| {
                    flag.replace(true);
                    Box::pin(async { Some("unused".into()) })
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!*called.borrow());
    assert!(result.delivery_appendix.is_none());
    assert!(result.history_entry.is_none());
    assert_eq!(with_appendix("text", Some(" \n")), "text");
}
