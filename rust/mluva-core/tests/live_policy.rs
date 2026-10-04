//! Independent released prompt bytes and public scheduling state transitions.
use mluva_core::{
    config::AppConfig,
    live::{LiveRewriteSchedule, initial_draft, live_prompt},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn state(schedule: &LiveRewriteSchedule) -> Value {
    json!({"minimum_characters":schedule.minimum_characters,"interval_seconds":schedule.interval_seconds,
        "last_text":schedule.last_text,"last_started":schedule.last_started.is_finite().then_some(schedule.last_started),
        "last_final":schedule.last_final,"in_flight":schedule.in_flight,"failed":schedule.failed,"paused":schedule.paused,
        "observed_text":schedule.observed_text,"changed_at":schedule.changed_at})
}
fn outcome(value: Result<String, String>) -> Value {
    match value {
        Ok(text) => json!({"ok":text}),
        Err(error) => json!({"error":error}),
    }
}

#[test]
fn frozen_live_prompts_and_scheduler_match_release() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-live-policy.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        fixture["initial_minimum"],
        mluva_core::live::INITIAL_MINIMUM_CHARACTERS
    );
    for row in fixture["schedules"].as_array().unwrap() {
        let mut schedule = LiveRewriteSchedule::new(
            row["minimum"].as_i64().unwrap(),
            row["interval"].as_f64().unwrap(),
        );
        for (action, expected) in row["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(row["observations"].as_array().unwrap())
        {
            let mut result = None;
            if let Some(take) = action.get("take") {
                result = schedule.take(
                    take["text"].as_str().unwrap(),
                    take["now"].as_f64().unwrap(),
                    take["final"].as_bool().unwrap(),
                );
            }
            if let Some(success) = action["finish"].as_bool() {
                schedule.finish(success);
            }
            if let Some(settings) = action.get("set") {
                for (field, value) in settings.as_object().unwrap() {
                    match field.as_str() {
                        "last_text" => schedule.last_text = value.as_str().unwrap().into(),
                        "paused" => schedule.paused = value.as_bool().unwrap(),
                        other => panic!("Unknown public schedule field {other}"),
                    }
                }
            }
            let mut expected = expected.clone();
            // Python treats integral and floating seconds as equal. JSON's
            // arbitrary-precision number representation distinguishes them.
            for field in ["interval_seconds", "last_started", "changed_at"] {
                if let Some(seconds) = expected["state"][field].as_f64() {
                    expected["state"][field] = json!(seconds);
                }
            }
            assert_eq!(
                json!({"result":result,"state":state(&schedule)}),
                expected,
                "{}: {action}",
                row["name"]
            );
        }
    }
    for row in fixture["prompts"].as_array().unwrap() {
        let config: AppConfig = serde_json::from_value(row["config"].clone()).unwrap();
        let prompts: Option<BTreeMap<String, String>> =
            serde_json::from_value(row["prompts"].clone()).unwrap();
        assert_eq!(
            outcome(initial_draft(&config, prompts.as_ref())),
            row["initial"],
            "{} initial draft",
            row["name"]
        );
        let transcript = row
            .get("transcript_spec")
            .map(|spec| {
                spec["character"]
                    .as_str()
                    .unwrap()
                    .repeat(spec["count"].as_u64().unwrap() as usize)
            })
            .unwrap_or_else(|| row["transcript"].as_str().unwrap().to_owned());
        let mut actual = outcome(live_prompt(
            &config,
            &transcript,
            row["draft"].as_str().unwrap(),
            row["final"].as_bool().unwrap(),
            prompts.as_ref(),
        ));
        if row.get("transcript_spec").is_some()
            && let Some(value) = actual["ok"].as_str()
        {
            let digest = Sha256::digest(value.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            actual["ok"] = json!({"sha256":digest,"characters":value.chars().count(),"utf8_bytes":value.len()});
        }
        if actual != row["result"] {
            panic!("Live prompt differs from released bytes: {}", row["name"]);
        }
    }
}
