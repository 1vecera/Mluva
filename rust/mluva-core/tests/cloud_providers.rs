//! Settings and History for the Microsoft and xAI routes and the realtime/batch choice.
use mluva_core::{
    config::AppConfig,
    history::{HistoryInput, HistoryStore},
};
use serde_json::json;

fn config(provider: &str, mode: &str, region: &str) -> AppConfig {
    AppConfig {
        transcription_provider: provider.into(),
        transcription_mode: mode.into(),
        microsoft_region: region.into(),
        ..AppConfig::default()
    }
}

#[test]
fn defaults_leave_existing_behavior_unchanged() {
    let defaults = AppConfig::default();
    assert_eq!(defaults.transcription_provider, "elevenlabs");
    assert_eq!(defaults.transcription_mode, "realtime");
    assert_eq!(defaults.microsoft_region, "");
    defaults.validate().unwrap();
}

#[test]
fn every_route_and_both_modes_validate() {
    for provider in [
        "elevenlabs",
        "microsoft",
        "xai",
        "mistral",
        "litellm",
        "local",
    ] {
        for mode in ["realtime", "batch"] {
            config(provider, mode, "swedencentral").validate().unwrap();
        }
    }
    assert!(config("deepgram", "realtime", "").validate().is_err());
    assert!(config("xai", "streaming", "").validate().is_err());
}

#[test]
fn the_region_is_a_bounded_azure_name_because_it_becomes_part_of_a_host() {
    for region in ["", "swedencentral", "eastus2", "southeastasia", "abc"] {
        config("microsoft", "realtime", region).validate().unwrap();
    }
    for region in [
        "ab",
        "Sweden",
        "sweden central",
        "evil.example.com",
        "x/y",
        "a@b",
        "eastus2/",
        "swedencentral#",
        "ünicode",
        &"a".repeat(31),
    ] {
        assert!(
            config("microsoft", "realtime", region).validate().is_err(),
            "{region}"
        );
    }
}

#[test]
fn batch_applies_to_cloud_providers_only() {
    for (provider, mode, expected) in [
        ("elevenlabs", "batch", true),
        ("microsoft", "batch", true),
        ("xai", "batch", true),
        ("mistral", "batch", true),
        ("elevenlabs", "realtime", false),
        ("microsoft", "realtime", false),
        ("xai", "realtime", false),
        ("mistral", "realtime", false),
        ("local", "batch", false),
        ("litellm", "batch", false),
    ] {
        assert_eq!(
            config(provider, mode, "").prefers_batch(),
            expected,
            "{provider} {mode}"
        );
    }
}

#[test]
fn an_existing_file_without_the_new_settings_keeps_its_choice_and_gains_the_defaults() {
    let saved = json!({
        "transcription_provider": "local",
        "local_model": "parakeet-v3",
        "language_code": "ces",
        "welcome_completed": true,
    });
    let loaded = AppConfig::from_persisted_json(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(loaded.transcription_provider, "local");
    assert_eq!(loaded.local_model, "parakeet-v3");
    assert_eq!(loaded.transcription_mode, "realtime");
    assert_eq!(loaded.microsoft_region, "");
    // A complete older file keeps ElevenLabs.
    let older = serde_json::to_value(AppConfig::default()).unwrap();
    let mut older = older.as_object().unwrap().clone();
    older.remove("transcription_mode");
    older.remove("microsoft_region");
    let loaded = AppConfig::from_persisted_json(&serde_json::to_vec(&older).unwrap()).unwrap();
    assert_eq!(loaded.transcription_provider, "elevenlabs");
    assert_eq!(loaded.transcription_mode, "realtime");
}

#[test]
fn choices_survive_a_save_and_load() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    config("microsoft", "batch", "eastus2").save(&path).unwrap();
    let loaded = AppConfig::load(&path).unwrap();
    assert_eq!(loaded.transcription_provider, "microsoft");
    assert_eq!(loaded.transcription_mode, "batch");
    assert_eq!(loaded.microsoft_region, "eastus2");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"transcription_mode\": \"batch\""));
    // Only the region name is stored; no credential setting exists for the cloud providers.
    assert!(!text.to_lowercase().contains("subscription"));
}

#[test]
fn history_accepts_the_new_routes_and_ties_a_fallback_reason_to_a_batch_route() {
    let directory = tempfile::tempdir().unwrap();
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let input = |route: &str, reason: Option<&str>| HistoryInput {
        recognition_route: Some(route.into()),
        recognition_fallback_reason: reason.map(str::to_owned),
        ..HistoryInput::dictation("raw", "delivered")
    };
    for route in [
        "mai-realtime",
        "mai-batch",
        "mai-batch-retry",
        "grok-realtime",
        "grok-batch",
        "grok-batch-retry",
        "mistral-realtime",
        "mistral-batch",
        "mistral-batch-retry",
    ] {
        history.add(input(route, None)).unwrap();
    }
    for route in [
        "scribe-v2-batch",
        "mai-batch",
        "grok-batch",
        "mistral-batch",
    ] {
        let entry = history
            .add(input(route, Some("realtime-stream-failed")))
            .unwrap();
        assert_eq!(entry.recognition_route.as_deref(), Some(route));
    }
    for route in [
        "mai-realtime",
        "grok-realtime",
        "mistral-realtime",
        "mai-batch-retry",
        "managed-local",
    ] {
        assert!(
            history
                .add(input(route, Some("realtime-stream-failed")))
                .is_err(),
            "{route}"
        );
    }
    assert!(history.add(input("deepgram-batch", None)).is_err());
}
