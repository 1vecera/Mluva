//! Outputs captured from the unchanged release, using only synthetic inputs and private temporary paths.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;

use mluva_core::config::{AppConfig, AppPaths, AudioRetentionPolicy};
use mluva_core::transcript::normalize_spoken_structure;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct ConfigCase {
    label: String,
    json: String,
    accepted: bool,
    #[serde(default)]
    expected_changes: BTreeMap<String, Value>,
}

#[test]
fn defaults_and_persisted_migrations_match_release_with_authorized_model_retirement() {
    let defaults: Value =
        serde_json::from_str(include_str!("fixtures/config-defaults.json")).unwrap();
    assert_eq!(
        serde_json::to_value(AppConfig::default()).unwrap(),
        defaults
    );
    let cases: Vec<ConfigCase> =
        serde_json::from_str(include_str!("fixtures/config-cases.json")).unwrap();
    let mut differences = Vec::new();
    for case in cases {
        let actual = AppConfig::from_persisted_json(case.json.as_bytes());
        if actual.is_ok() != case.accepted {
            differences.push(format!(
                "{}: accepted {}, expected {} ({:?})",
                case.label,
                actual.is_ok(),
                case.accepted,
                actual.err()
            ));
        } else if let Ok(config) = actual {
            let mut expected = defaults.clone();
            expected
                .as_object_mut()
                .unwrap()
                .extend(case.expected_changes);
            // Daniel reduced the native lineup on 1 October 2026. Keep the
            // original oracle and apply only this explicitly authorized delta.
            let persisted: Value = serde_json::from_str(&case.json).unwrap();
            if matches!(
                persisted["local_model"].as_str(),
                Some("whisper-base" | "whisper-small")
            ) {
                expected["local_model"] = Value::String("qwen3-1.7b".into());
                expected["welcome_completed"] = Value::Bool(false);
            }
            if serde_json::to_value(config).unwrap() != expected {
                differences.push(format!("{}: settings changed", case.label));
            }
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn retired_models_load_and_save_without_losing_other_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    for retired in ["whisper-base", "whisper-small", "whisper-turbo"] {
        let original = serde_json::to_vec(&serde_json::json!({
            "local_model": retired,
            "transcription_provider": "local",
            "local_device": "cuda",
            "language_code": "ces",
            "incognito_mode": true,
            "auto_paste": true,
            "welcome_completed": true
        }))
        .unwrap();
        fs::write(&path, &original).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        let expected = AppConfig {
            transcription_provider: "local".into(),
            local_device: "cuda".into(),
            language_code: "ces".into(),
            incognito_mode: true,
            auto_paste: true,
            ..AppConfig::default()
        };
        assert_eq!(loaded, expected, "migration of {retired}");
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "load never rewrites settings"
        );
        loaded.save(&path).unwrap();
        assert_eq!(AppConfig::load(&path).unwrap(), expected);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let rejected = AppConfig {
            local_model: retired.into(),
            ..expected
        };
        assert_eq!(
            rejected.validate().unwrap_err().to_string(),
            "Choose a supported local speech model"
        );
    }
}

#[derive(Deserialize)]
struct TranscriptCase {
    raw: String,
    expected: String,
}

#[test]
fn spoken_structure_matches_the_release_including_unicode_boundaries_and_corrections() {
    let cases: Vec<TranscriptCase> =
        serde_json::from_str(include_str!("fixtures/transcript-cases.json")).unwrap();
    for case in cases {
        assert_eq!(
            normalize_spoken_structure(&case.raw),
            case.expected,
            "raw input: {:?}",
            case.raw
        );
    }
}

#[derive(Deserialize)]
struct PathCase {
    environ: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
}

#[test]
fn xdg_roots_and_explicit_empty_roots_match_existing_installations() {
    let cases: Vec<PathCase> =
        serde_json::from_str(include_str!("fixtures/xdg-paths.json")).unwrap();
    for case in cases {
        let paths = AppPaths::from_environ(&case.environ).unwrap();
        assert_eq!(paths.config.to_string_lossy(), case.expected["config"]);
        assert_eq!(paths.data.to_string_lossy(), case.expected["data"]);
        assert_eq!(paths.runtime.to_string_lossy(), case.expected["runtime"]);
    }
}

#[test]
fn first_launch_and_existing_installation_keep_distinct_onboarding_choices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    assert!(!AppConfig::load(&path).unwrap().welcome_completed);
    fs::write(&path, "{}").unwrap();
    assert!(AppConfig::load(&path).unwrap().welcome_completed);
}

#[test]
fn saving_preserves_all_choices_and_tightens_owner_only_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("existing/config.json");
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&path, "{}").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let config = AppConfig {
        language_code: "ces".into(),
        auto_paste: true,
        global_recording_key: "F11".into(),
        audio_retention_policy: AudioRetentionPolicy::Never,
        rewrite_provider: "none".into(),
        microphone_target: Some("alsa_input.test".into()),
        ..AppConfig::default()
    };
    config.save(&path).unwrap();
    assert_eq!(AppConfig::load(&path).unwrap(), config);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn damaged_settings_and_unknown_fields_are_never_silently_replaced() {
    for content in ["{broken", "[]", r#"{"private_future_setting":true}"#] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(&path, content).unwrap();
        let error = AppConfig::default().save(&path).unwrap_err();
        assert_eq!(
            error.to_string(),
            "config.json needs repair; refusing to overwrite it"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn capture_recovery_policy_keeps_the_existing_success_and_failure_behavior() {
    for (policy, success, failure) in [
        (AudioRetentionPolicy::Never, false, false),
        (AudioRetentionPolicy::Failures, false, true),
        (AudioRetentionPolicy::Always, true, true),
    ] {
        assert_eq!(policy.should_retain(true), success);
        assert_eq!(policy.should_retain(false), failure);
    }
}
