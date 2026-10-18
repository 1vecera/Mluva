//! Current product acceptance through real application actions, PCM, HTTP and SQLite.
use adw::prelude::*;
use mluva_core::{
    config::{AppConfig, AppPaths},
    history::HistoryInput,
};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_workflows::{
    capture::CapturePhase,
    services::{ApplicationServices, NativeBinaries},
};
use serde_json::{Value, json};
use std::{cell::Cell, fs, os::unix::fs::symlink, path::PathBuf, rc::Rc};
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
#[path = "support/dictation_ui.rs"]
mod ui;

#[test]
#[ignore = "requires the private display, network, device and clipboard runner"]
fn dictation_editor_navigation_settings_recovery_and_shutdown() {
    let root = ui::isolated();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let tools = root.join("application-tools");
    fs::create_dir_all(&tools).unwrap();
    symlink(
        target.join("audio-fixture-peer").canonicalize().unwrap(),
        tools.join("pw-record"),
    )
    .unwrap();
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-application.json")).unwrap();
    for (index, scenario) in [
        "dictation",
        "incognito",
        "cancel",
        "failure",
        "quit-recording",
    ]
    .iter()
    .enumerate()
    {
        let dir = tempfile::tempdir_in(&root).unwrap();
        let paths = AppPaths {
            config: dir.path().join("config"),
            data: dir.path().join("data"),
            runtime: dir.path().join("runtime"),
        };
        let _ = fs::remove_file(tools.join("raw.ready.json"));
        fs::write(
            tools.join("test-config.json"),
            serde_json::to_vec(&fixture["cases"][0]["pcm"]).unwrap(),
        )
        .unwrap();
        let response = if *scenario == "failure" {
            json!({"route":"speech","status":503,"payload":{"error":"synthetic provider failure"}})
        } else {
            json!({"route":"speech","status":200,"payload":{"text":"Narration keeps 12 files.","language_code":"eng"}})
        };
        let mut peer = http::Peer::new(&if ["cancel", "quit-recording"].contains(scenario) {
            vec![]
        } else if *scenario == "failure" {
            vec![response.clone(), response]
        } else if *scenario == "dictation" {
            vec![
                response,
                json!({"route":"speech","status":200,"payload":{"text":"A continued thought.","language_code":"eng"}}),
            ]
        } else {
            vec![response]
        });
        AppConfig {
            transcription_provider: "litellm".into(),
            transcription_base_url: format!("{}/v1", peer.address),
            transcription_api_key_env: "MLUVA_UNUSED_TEST_KEY".into(),
            incognito_mode: *scenario == "incognito",
            welcome_completed: true,
            ..Default::default()
        }
        .save(&paths.config.join("config.json"))
        .unwrap();
        let services = ApplicationServices::open(paths.clone()).unwrap();
        let app = adw::Application::builder()
            .application_id(format!("com.mluva.DictationOnly{index}"))
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let closed = Rc::new(Cell::new(false));
        let close = closed.clone();
        let owner = ApplicationDesktop::new(
            &app,
            services.clone(),
            DesktopRuntime::new().unwrap(),
            resources.clone(),
            NativeBinaries {
                asr_worker: target.join("mluva-asr-worker"),
                audio_cleanup: target.join("mluva-audio-cleanup"),
                screenshot_editor: target.join("mluva-screenshot-editor"),
            },
            ApplicationPlatform {
                compact_recording: Rc::new(|_| {}),
                close: Rc::new(move || close.set(true)),
            },
        )
        .unwrap();
        owner.shell.present();
        ui::until(|| owner.capture.page.record_button.get_sensitive());
        if *scenario == "dictation" {
            for widget in ui::widgets(&owner.shell.window) {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    assert!(
                        !["Polish", "Rewrite", "Notes", "Command", "Live Rewrite"]
                            .contains(&button.label().as_deref().unwrap_or_default())
                    );
                }
            }
            ui::paint(&owner.shell.window, "native-empty-wide");
            app.activate_action("settings", None);
            ui::settle(0.1);
            assert!(
                owner
                    .settings
                    .view
                    .pages()
                    .iter()
                    .all(|p| p.name().as_deref() != Some("prompts"))
            );
            ui::paint(&owner.shell.window, "native-settings-wide");
            app.activate_action("latest", None);
        }
        app.activate_action("record", None);
        ui::until(|| {
            owner.capture.phase() == Some(CapturePhase::Recording)
                && tools.join("raw.ready.json").exists()
        });
        let audio = fs::read_dir(paths.data.join("recordings"))
            .ok()
            .and_then(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|ext| ext == "wav"))
            })
            .unwrap_or_else(|| paths.data.join("no-persistent-incognito-audio"));
        assert!(*scenario == "incognito" || audio.exists());
        let ready: Value =
            serde_json::from_slice(&fs::read(tools.join("raw.ready.json")).unwrap()).unwrap();
        let pid = ready["pid"].as_u64().unwrap();
        if *scenario == "dictation" {
            ui::paint(&owner.shell.window, "native-recording-wide");
        }
        if *scenario != "quit-recording" {
            app.activate_action(
                if *scenario == "cancel" {
                    "cancel"
                } else {
                    "record"
                },
                None,
            );
            ui::until(|| owner.capture.phase().is_none());
            let entries = services.history.recent(10).unwrap();
            match *scenario {
                "cancel" | "incognito" => {
                    assert!(entries.is_empty());
                    assert!(!audio.exists());
                }
                "failure" => {
                    assert_eq!(entries.len(), 1);
                    assert!(audio.exists());
                    assert_eq!(entries[0].delivery_outcome, "recognition-failed");
                    assert!(
                        owner
                            .capture
                            .page
                            .status
                            .label()
                            .starts_with("Capture failed.")
                    );
                    ui::paint(&owner.shell.window, "native-recovery-wide");
                }
                _ => {
                    assert_eq!(entries.len(), 1);
                    assert_eq!(entries[0].raw_text, "Narration keeps 12 files.");
                    assert_eq!(entries[0].delivered_text, entries[0].raw_text);
                    assert_eq!(entries[0].mode, "dictation");
                    assert!(entries[0].enhancement_provider_id.is_none());
                    assert!(!audio.exists());
                    let workspace = &owner.capture.page.workspace;
                    ui::button(&workspace.messages, "Copy").emit_clicked();
                    assert!(ui::button(&workspace.messages, "Save edits").is_visible());
                    workspace.documents()[0]
                        .buffer()
                        .set_text("My working edit");
                    ui::button(&workspace.messages, "Save edits").emit_clicked();
                    assert_eq!(
                        services
                            .conversations
                            .source_text(&entries[0], false)
                            .unwrap(),
                        "My working edit"
                    );
                    assert_eq!(
                        services
                            .history
                            .find(&entries[0].identifier)
                            .unwrap()
                            .raw_text,
                        "Narration keeps 12 files."
                    );
                    ui::paint(&owner.shell.window, "native-editing-wide");
                    owner.shell.window.set_default_size(520, 820);
                    ui::settle(0.2);
                    ui::paint(&owner.shell.window, "native-editing-narrow");
                    for action in [
                        "history",
                        "settings",
                        "meeting",
                        "personalization",
                        "latest",
                    ] {
                        app.activate_action(action, None);
                        ui::settle(0.05);
                        assert_eq!(
                            owner.shell.stack.visible_child_name().as_deref(),
                            Some(if action == "latest" {
                                "capture"
                            } else {
                                action
                            })
                        );
                    }
                    fs::remove_file(tools.join("raw.ready.json")).unwrap();
                    workspace.continue_button.emit_clicked();
                    ui::until(|| {
                        owner.capture.phase() == Some(CapturePhase::Recording)
                            && tools.join("raw.ready.json").exists()
                    });
                    app.activate_action("record", None);
                    ui::until(|| owner.capture.phase().is_none());
                    assert_eq!(workspace.entry().unwrap().identifier, entries[0].identifier);
                    assert_eq!(
                        services
                            .conversations
                            .source_text(&entries[0], false)
                            .unwrap(),
                        "My working edit\n\nA continued thought."
                    );
                    assert_eq!(
                        services
                            .history
                            .find(&entries[0].identifier)
                            .unwrap()
                            .raw_text,
                        "Narration keeps 12 files."
                    );
                    assert_eq!(
                        services
                            .history
                            .continuations(&entries[0].identifier)
                            .unwrap()
                            .len(),
                        1
                    );
                    // Historical replies remain editable; renaming cannot change the recognized source.
                    let old = services
                        .history
                        .add(HistoryInput::dictation("Old recognition", "Old delivery"))
                        .unwrap();
                    services
                        .conversations
                        .append(
                            &old.identifier,
                            "Old polish",
                            "Saved historical rewrite",
                            "old-model",
                        )
                        .unwrap();
                    workspace
                        .show_conversation(
                            Some(old.clone()),
                            &services.conversations.replies(&old.identifier).unwrap(),
                            false,
                        )
                        .unwrap();
                    assert_eq!(workspace.documents().len(), 2);
                    workspace.begin_rename(&old.identifier).unwrap();
                    workspace.title_entry.set_text("Retained history");
                    workspace.title_save.emit_clicked();
                    assert_eq!(
                        services
                            .history
                            .find(&old.identifier)
                            .unwrap()
                            .title
                            .as_deref(),
                        Some("Retained history")
                    );
                }
            }
        }
        let done = Rc::new(Cell::new(false));
        let completed = done.clone();
        let task = owner.shutdown();
        glib::MainContext::default().spawn_local(async move {
            task.await.unwrap();
            completed.set(true);
        });
        ui::until(|| done.get());
        assert!(closed.get());
        assert!(!PathBuf::from(format!("/proc/{pid}")).exists());
        assert!(!audio.exists() || *scenario == "failure");
        let requests = peer.finish();
        assert_eq!(
            requests.len(),
            usize::from(!["cancel", "quit-recording"].contains(scenario))
                + usize::from(["dictation", "failure"].contains(scenario))
        );
        assert!(
            requests
                .iter()
                .all(|request| request["path"] == "/v1/audio/transcriptions")
        );
        drop(owner);
        drop(app);
        ui::settle(0.3);
        println!("{scenario}: private application PASS");
    }
}
