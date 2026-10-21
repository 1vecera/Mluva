use mluva_providers::{ProviderError, elevenlabs, models, realtime};
use serde::Serialize;
use serde_json::{Value, json};

mod support;

fn released() -> Value {
    serde_json::from_str(include_str!("fixtures/released-providers.json")).unwrap()
}

fn compare<T: Serialize>(actual: Result<T, ProviderError>, expected: &Value, context: &str) {
    let observed = match actual {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":error.to_string()}),
    };
    let mut expected = expected.clone();
    expected.as_object_mut().unwrap().remove("exception");
    assert_eq!(observed, expected, "{context}");
}

#[test]
fn model_identifiers_and_catalogs_match_released_observations() {
    let fixture = released();
    for row in fixture["identifiers"].as_array().unwrap() {
        let value = row["value"].as_str().unwrap();
        assert_eq!(
            models::valid_identifier(value),
            row["valid"].as_bool().unwrap(),
            "identifier {value:?}"
        );
    }
    for (index, row) in fixture["catalogs"].as_array().unwrap().iter().enumerate() {
        if row["capability"] != "speech" || [142, 145, 148, 151, 154, 166, 169].contains(&index) {
            continue;
        }
        let mut expected = row["result"].clone();
        if let Some(models) = expected.get_mut("ok").and_then(Value::as_array_mut) {
            for model in models {
                for key in ["rewrite_effort", "fast_tier", "reasoning_efforts"] {
                    model.as_object_mut().unwrap().remove(key);
                }
            }
        }
        compare(
            models::compatible_catalog(&row["payload"], row["selected"].as_str()),
            &expected,
            &format!("catalog {index}"),
        );
    }
}

#[test]
fn batch_text_and_speaker_metadata_match_released_observations() {
    for (index, row) in released()["metadata"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        // Word timing is a native addition the released observations predate.
        let result =
            elevenlabs::transcription_result(&row["payload"], row["speakers"].as_bool().unwrap())
                .map(|value| {
                    let mut value = serde_json::to_value(value).unwrap();
                    value.as_object_mut().unwrap().remove("words");
                    value
                });
        compare(result, &row["result"], &format!("metadata {index}"));
    }
}

#[test]
fn batch_word_times_are_kept_without_spacing_or_audio_events() {
    let result = elevenlabs::transcription_result(
        &json!({"text": "Look here.", "language_code": "eng", "words": [
            {"text": "Look", "start": 0.4, "end": 0.7, "type": "word"},
            {"text": " ", "start": 0.7, "end": 0.75, "type": "spacing"},
            {"text": "(click)", "start": 0.75, "end": 0.8, "type": "audio_event"},
            {"text": "here.", "start": 0.8, "end": 1.1, "type": "word"},
            {"text": "late", "start": 2.0, "end": 1.0, "type": "word"}
        ]}),
        false,
    )
    .unwrap();
    let words: Vec<_> = result
        .words
        .iter()
        .map(|w| (w.text.as_str(), w.start_seconds, w.end_seconds))
        .collect();
    assert_eq!(
        words,
        [("Look", 0.4, 0.7), ("here.", 0.8, 1.1), ("late", 2.0, 2.0)]
    );
    assert_eq!(result.text, "Look here.");
    assert!(result.speaker_segments.is_empty());
}

#[test]
fn realtime_protocol_and_sanitized_errors_match_released_observations() {
    let fixture = released();
    for (index, row) in fixture["decode_events"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        compare(
            realtime::decode_event(&support::unhex(row["message_hex"].as_str().unwrap())),
            &row["result"],
            &format!("event {index}"),
        );
    }
    for row in fixture["event_errors"].as_array().unwrap() {
        assert_eq!(json!(realtime::event_error(&row["event"])), row["error"]);
    }
    for row in fixture["uris"].as_array().unwrap() {
        assert_eq!(
            realtime::realtime_uri(
                row["endpoint"].as_str().unwrap(),
                row["language"].as_str().unwrap()
            )
            .unwrap(),
            row["uri"].as_str().unwrap()
        );
    }
    for row in fixture["audio_events"].as_array().unwrap() {
        assert_eq!(
            realtime::audio_event(
                &support::unhex(row["pcm_hex"].as_str().unwrap()),
                row["commit"].as_bool().unwrap()
            ),
            row["event"].as_str().unwrap()
        );
    }
}
