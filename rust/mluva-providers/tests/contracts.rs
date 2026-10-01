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
        compare(
            models::compatible_catalog(
                &row["payload"],
                row["capability"].as_str(),
                row["selected"].as_str(),
            ),
            &row["result"],
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
        compare(
            elevenlabs::transcription_result(&row["payload"], row["speakers"].as_bool().unwrap()),
            &row["result"],
            &format!("metadata {index}"),
        );
    }
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
