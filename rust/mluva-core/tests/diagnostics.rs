//! Released bounded ledger and export observations, including privacy and rounding.
use mluva_core::{
    config::AppConfig,
    diagnostics::{DiagnosticOutcome, DiagnosticProvider, DiagnosticStage, DiagnosticsStore},
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt};

fn events(store: &DiagnosticsStore, limit: i64) -> Value {
    let mut value = serde_json::to_value(store.recent(limit).unwrap()).unwrap();
    for event in value.as_array_mut().unwrap() {
        event["created_at"] = json!("generated");
    }
    value
}

#[test]
fn compatible_ledger_is_bounded_and_exports_only_reviewed_metadata() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-diagnostics.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("data/diagnostics.sqlite3");
    let store = DiagnosticsStore::new(&path, 3).unwrap();
    store.initialize().unwrap();
    let session = fixture["session"].as_str().unwrap();
    let mut recorded_timings = vec![];
    for input in fixture["inputs"].as_array().unwrap() {
        store
            .record(
                session,
                input["mode"].as_str().unwrap(),
                serde_json::from_value(input["stage"].clone()).unwrap(),
                serde_json::from_value(input["provider"].clone()).unwrap(),
                serde_json::from_value(input["outcome"].clone()).unwrap(),
                input["duration_seconds"].as_f64().unwrap(),
            )
            .unwrap();
        recorded_timings.push(store.recent(1).unwrap()[0].duration_ms);
    }
    assert_eq!(json!(recorded_timings), fixture["recorded_timings"]);
    assert_eq!(events(&store, 1000), fixture["recent"]);
    assert_eq!(events(&store, 0), fixture["zero"]);
    assert_eq!(events(&store, -1), fixture["negative_limit"]);
    for input in fixture["invalid"].as_array().unwrap() {
        let before = store.recent(100).unwrap();
        assert!(
            store
                .record(
                    input["session"].as_str().unwrap(),
                    input["mode"].as_str().unwrap(),
                    DiagnosticStage::Capture,
                    DiagnosticProvider::Local,
                    DiagnosticOutcome::Failed,
                    input["duration_seconds"].as_f64().unwrap()
                )
                .is_err()
        );
        assert_eq!(store.recent(100).unwrap(), before, "{}", input["name"]);
    }
    for duration in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            store
                .record(
                    session,
                    "dictation",
                    DiagnosticStage::Capture,
                    DiagnosticProvider::Local,
                    DiagnosticOutcome::Failed,
                    duration
                )
                .is_err()
        );
    }
    let config: AppConfig = serde_json::from_value(fixture["configuration"].clone()).unwrap();
    let output = store
        .export(&directory.path().join("export"), &config)
        .unwrap();
    let bytes = fs::read(&output).unwrap();
    let mut exported: Value = serde_json::from_slice(&bytes).unwrap();
    let generated =
        chrono::DateTime::parse_from_rfc3339(exported["generated_at"].as_str().unwrap()).unwrap();
    let expected_filename = format!(
        "mluva-diagnostics-{}.json",
        generated.format("%Y%m%dT%H%M%S%6fZ")
    );
    assert_eq!(
        json!(output.file_name().unwrap().to_str().unwrap() == expected_filename),
        fixture["filename_matches_generated_at"],
        "the export filename and contents must identify the same instant"
    );
    exported["generated_at"] = json!("generated");
    for event in exported["events"].as_array_mut().unwrap() {
        event["created_at"] = json!("generated");
    }
    assert_eq!(exported, fixture["exported"]);
    assert!(!String::from_utf8(bytes).unwrap().contains("private-"));
    let columns = Connection::open(&path)
        .unwrap()
        .prepare("PRAGMA table_info(diagnostic_events)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(json!(columns), fixture["columns"]);
    for (path, field) in [
        (path.as_path(), "file_mode"),
        (output.as_path(), "export_mode"),
        (output.parent().unwrap(), "directory_mode"),
    ] {
        assert_eq!(
            u64::from(fs::metadata(path).unwrap().permissions().mode() & 0o777),
            fixture[field].as_u64().unwrap()
        );
    }
    for maximum in [0, -1] {
        assert_eq!(
            DiagnosticsStore::new(&path, maximum)
                .unwrap_err()
                .to_string(),
            "maximum_events must be positive"
        );
    }
}
