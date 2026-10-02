//! Actual application recovery and save gates against independently frozen release transactions.
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::{HistoryInput, HistoryStore},
    scratchpad::ScratchpadDraft,
    screenshots::ScreenshotStore,
};
use mluva_workflows::services::{ApplicationServices, SettingsActivity};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

const ID: &str = "11111111-1111-4111-8111-111111111111";
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAIAAAB7QOjdAAAAD0lEQVR4nGOQS7nzoUkOAAoRAu8yvJYrAAAAAElFTkSuQmCC";

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/released-application-services.json")).unwrap()
}
fn paths(directory: &Path) -> AppPaths {
    AppPaths {
        config: directory.join("config/mluva"),
        data: directory.join("data/mluva"),
        runtime: directory.join("runtime/mluva"),
    }
}
fn permission(path: &Path) -> String {
    format!(
        "{:o}",
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    )
}

#[test]
fn released_startup_and_recovery_transactions() {
    let fixture = fixture();
    for row in fixture["startup"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(directory.path());
        let params = &row["params"];
        let config = AppConfig {
            incognito_mode: params["incognito"].as_bool().unwrap_or(false),
            history_retention_days: params["retention"].as_i64().unwrap_or(0).into(),
            ..AppConfig::default()
        };
        config.save(&paths.config.join("config.json")).unwrap();
        let history = HistoryStore::new(paths.data.join("history.sqlite3"));
        history.initialize().unwrap();
        let old = history
            .add(HistoryInput {
                delivery_outcome: "ready".into(),
                ..HistoryInput::dictation(
                    "Old source remains 12 files.",
                    "Old working text remains 34 folders.",
                )
            })
            .unwrap();
        rusqlite::Connection::open(&history.database.path)
            .unwrap()
            .execute(
                "UPDATE transcription_history SET created_at='2001-01-01T00:00:00+00:00'",
                [],
            )
            .unwrap();
        if params["pending"] == true {
            ScreenshotStore::new(&history.database.path)
                .add(ID, &STANDARD.decode(PNG).unwrap(), true, Some(12.5))
                .unwrap();
        }
        let audio = if params["outside"] == true {
            directory.path().join("outside.wav")
        } else {
            paths.data.join("recordings").join(format!("{ID}.wav"))
        };
        if params["draft"] == true {
            fs::create_dir_all(audio.parent().unwrap()).unwrap();
            fs::write(&audio, b"synthetic source audio").unwrap();
            let draft = ScratchpadDraft {
                identifier: ID.into(),
                history_identifier: Some(old.identifier),
                created_at: "2001-01-01T00:00:00+00:00".into(),
                raw_text: "Raw draft has 56 notes.".into(),
                text: "Editable draft has 78 notes.".into(),
                audio_path: Some(audio.to_str().unwrap().into()),
                incognito: params["private_draft"].as_bool().unwrap_or(false),
                audio_retention_policy: "failures".into(),
                session_identifier: Some(ID.into()),
            };
            fs::write(
                paths.data.join("scratchpad-draft.json"),
                serde_json::to_vec(&draft).unwrap(),
            )
            .unwrap();
        }
        let broken = match params["broken"].as_str() {
            Some("config") => Some(paths.config.join("config.json")),
            Some("personalization") => Some(paths.config.join("personalization.json")),
            Some("scratchpad") => Some(paths.data.join("scratchpad-draft.json")),
            _ => None,
        };
        if let Some(path) = &broken {
            fs::write(path, b"{ preserved malformed document").unwrap();
        }
        if params["override"] == true {
            fs::create_dir(paths.config.join("prompts")).unwrap();
            fs::write(
                paths.config.join("prompts/cleanup.md"),
                "Current cleanup prompt from disk.",
            )
            .unwrap();
        }
        let services = ApplicationServices::open(paths.clone()).unwrap();
        let recovery = services.recover_scratchpad();
        let message = recovery
            .as_ref()
            .map(|recovery| recovery.message())
            .unwrap_or("");
        let records = services.history.recent(100).unwrap().iter().map(|entry| json!({
            "raw":entry.raw_text,"output":entry.delivered_text,"mode":entry.mode,"language":entry.language_code,"outcome":entry.delivery_outcome,
            "images":services.screenshots.snapshot(&entry.identifier,false).unwrap().iter().map(|image|json!({"png":STANDARD.encode(&image.data),"elapsed":image.captured_after_seconds})).collect::<Vec<_>>()
        })).collect::<Vec<_>>();
        let mut actual = json!({
            "config":services.config(),"config_error":services.config_load_error,
            "personal_error":services.personalization.borrow().persistence_error.is_some(),
            "scratch_error":services.scratchpad.borrow().persistence_error.is_some(),
            "records":records,"pending_count":services.screenshots.pending_captures().unwrap().len(),
            "recovery_error":recovery.is_err(),"recovery_message":message,
            "scratch_exists":paths.data.join("scratchpad-draft.json").exists(),"audio_exists":audio.exists(),
            "prompt":services.prompts.borrow().read("cleanup").unwrap().text,
            "styles_linked":services.personalization.borrow().prompt_store.as_ref().unwrap().directory==services.prompts.borrow().directory,
            "private_modes":{"workspace":permission(&services.cwd),"history":permission(&services.history.database.path),"diagnostics":permission(&services.diagnostics.path),"data":permission(&paths.data)},
        });
        if let Some(path) = broken {
            actual["broken_preserved"] = fs::read_to_string(path).unwrap().into();
        }
        assert_eq!(actual, row["expected"], "{}", row["name"]);
    }
    eprintln!(
        "Matched {} released startup/recovery transactions",
        fixture["startup"].as_array().unwrap().len()
    );
}

#[test]
fn released_settings_save_and_owner_busy_gates() {
    let fixture = fixture();
    for row in fixture["settings"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(directory.path());
        let config: AppConfig =
            serde_json::from_value(row.get("initial").cloned().unwrap_or_else(|| json!({})))
                .unwrap();
        let file = paths.config.join("config.json");
        config.save(&file).unwrap();
        let services = ApplicationServices::open(paths).unwrap();
        let source = &row["activity"];
        let activity = SettingsActivity {
            preparing: source["preparing"].as_bool().unwrap_or(false),
            processing: source["processing"].as_bool().unwrap_or(false),
            recording: source["recording"].as_bool().unwrap_or(false),
            rewriting: source["rewriting"].as_bool().unwrap_or(false),
            live_schedule: source["live_schedule"].as_bool().unwrap_or(false),
            final_live: source["final_live"].as_bool().unwrap_or(false),
            pending_incognito: source["pending_incognito"].as_bool().unwrap_or(false),
            pending_mode: source["pending_mode"]
                .as_str()
                .unwrap_or("dictation")
                .into(),
        };
        if row["write_failure"] == true {
            fs::remove_file(&file).unwrap();
            fs::create_dir(&file).unwrap();
            fs::write(file.join("preserved"), "original private settings").unwrap();
        }
        let document = if file.is_dir() {
            file.join("preserved")
        } else {
            file.clone()
        };
        let before = fs::read(&document).unwrap();
        let outcome = match services.apply_settings(row["changes"].as_object().unwrap(), &activity)
        {
            Ok(Some(_)) => "accepted",
            Ok(None) => "blocked",
            Err(_) => "invalid",
        };
        let actual = json!({"outcome":outcome,"config":services.config(),"file_unchanged":before==fs::read(document).unwrap()});
        assert_eq!(actual, row["expected"], "{}", row["name"]);
    }
    eprintln!(
        "Matched {} released settings-save transactions",
        fixture["settings"].as_array().unwrap().len()
    );
}
