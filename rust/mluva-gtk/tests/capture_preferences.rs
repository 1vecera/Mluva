//! Actual native preference graph against unchanged released handlers and stores.
#![recursion_limit = "512"]
use adw::prelude::*;
use mluva_audio::catalog::PipeWireDeviceCatalog;
use mluva_core::config::{AppConfig, AppPaths};
use mluva_gtk::{
    application_settings::{ApplicationSettings, ApplicationSettingsCallbacks},
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferenceCallbacks, InlineEffect, PreferenceActivity},
    capture_view::{CaptureCallbacks, CapturePage},
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    rewrite_settings::RewriteSettings,
    theme::ThemeController,
};
use mluva_workflows::services::{ApplicationServices, SettingsActivity};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

#[path = "support/text_transport.rs"]
pub mod transport;

fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn walk(widget: gtk::Widget, result: &mut Vec<gtk::Widget>) {
        result.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(widget) = child {
            walk(widget.clone(), result);
            child = widget.next_sibling();
        }
    }
    let mut result = Vec::new();
    walk(widget.as_ref().clone(), &mut result);
    result
}
fn maturity(widget: &adw::PreferencesPage) -> Value {
    json!(descendants(widget).into_iter().filter_map(|widget|widget.downcast::<adw::ActionRow>().ok()).map(|row| {
        let badges=descendants(&row).into_iter().filter_map(|widget|widget.downcast::<gtk::Label>().ok()).filter(|label|label.has_css_class("vs-maturity-badge")).map(|label| {
            let mut classes:Vec<_>=label.css_classes().into_iter().map(String::from).collect();classes.sort();json!([label.label().to_string(),classes])
        }).collect::<Vec<_>>();
        json!({"title":row.title().to_string(),"subtitle":row.subtitle().map(String::from),"badges":badges})
    }).collect::<Vec<_>>())
}

const FIELDS: &[&str] = &[
    "language_code",
    "default_mode",
    "global_recording_key",
    "auto_paste",
    "automatic_titles",
    "spoken_commands_enabled",
    "remember_per_application",
    "microphone_target",
    "system_audio_target",
    "incognito_mode",
    "audio_retention_policy",
    "history_retention_days",
    "welcome_completed",
    "widget_opacity",
    "rewrite_provider",
    "live_rewrite_enabled",
];
fn selected_config(config: &AppConfig) -> Value {
    selected_document(&serde_json::to_value(config).unwrap())
}
fn selected_document(config: &Value) -> Value {
    Value::Object(
        FIELDS
            .iter()
            .map(|field| ((*field).into(), config[*field].clone()))
            .collect(),
    )
}
fn update_config(config: &AppConfig, changes: &Value) -> AppConfig {
    let mut value = serde_json::to_value(config).unwrap();
    for (key, value_new) in changes.as_object().unwrap() {
        value[key] = value_new.clone();
    }
    serde_json::from_value(value).unwrap()
}
fn settle() {
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(3));
    }
}
fn error_text(text: &str) -> String {
    for prefix in [
        "Transcription language could not be saved:",
        "Default capture mode could not be saved:",
        "Application mode could not be saved:",
        "Global recording key could not be saved:",
        "General settings could not be saved:",
        "Audio routing could not be saved:",
        "Output style could not be saved:",
        "Privacy settings could not be saved:",
        "Privacy settings saved, but history retention failed:",
    ] {
        if text.starts_with(prefix) {
            return format!("{prefix} $WRITE_ERROR");
        }
    }
    text.into()
}
fn button(widget: &gtk::Button) -> Value {
    json!({"label":widget.label().map(String::from),"tooltip":widget.tooltip_text().map(String::from),"visible":widget.get_visible(),"sensitive":widget.get_sensitive()})
}
fn combo(widget: &adw::ComboRow) -> Value {
    let labels = widget
        .model()
        .map(|model| {
            (0..model.n_items())
                .map(|index| {
                    model
                        .item(index)
                        .unwrap()
                        .downcast::<gtk::StringObject>()
                        .unwrap()
                        .string()
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"title":widget.title().to_string(),"subtitle":widget.subtitle().map(String::from),"visible":widget.get_visible(),"sensitive":widget.get_sensitive(),"selected":widget.selected(),"labels":labels,"tooltip":widget.tooltip_text().map(String::from)})
}
fn switch(widget: &adw::SwitchRow) -> Value {
    json!({"title":widget.title().to_string(),"subtitle":widget.subtitle().map(String::from),"active":widget.is_active(),"sensitive":widget.get_sensitive()})
}
fn capture(services: &ApplicationServices, resources: &DocumentResources) -> Rc<CapturePage> {
    let workspace = ConversationWorkspace::new(
        services.conversations.clone(),
        ConversationCallbacks {
            copy: Rc::new(|_| {}),
            rewrite: Rc::new(|_| {}),
            paste: Rc::new(|_| {}),
            open_archive: Rc::new(|| {}),
            save_prompt: Rc::new(|_| {}),
            cancel_rewrite: Rc::new(|| {}),
            rename: Rc::new(|_, _| true),
            delete: Rc::new(|_| true),
            merge: Rc::new(|_, _| true),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: Some(Rc::new(|| {})),
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
            edit_prompt: Rc::new(|_| {}),
        },
        resources.clone(),
    )
    .unwrap();
    let config = services.config();
    CapturePage::new(
        workspace,
        RewriteSettings::new(config.clone(), Rc::new(|| {}), Rc::new(|_, _, _| {})),
        config,
        CaptureCallbacks {
            toggle_recording: Rc::new(|| {}),
            apply_live_settings: Rc::new(|_| false),
            toast: Rc::new(|_| {}),
            open_prompt: Rc::new(|_| {}),
            retry_initialization: Rc::new(|| {}),
            accept_command: Rc::new(|| {}),
            discard_command: Rc::new(|| {}),
            copy_scratchpad: Rc::new(|| {}),
            delete_scratchpad: Rc::new(|| {}),
            output_changed: Rc::new(|_| {}),
            live_draft_edited: Rc::new(|| {}),
            announce: Rc::new(|_| {}),
        },
    )
    .unwrap()
}
fn generated_styles(value: Value, styles: &[mluva_core::prompt_catalog::SavedStyle]) -> Value {
    fn string(mut value: String, styles: &[mluva_core::prompt_catalog::SavedStyle]) -> String {
        for style in styles {
            value = value
                .replace(&style.identifier, &format!("$STYLE:{}", style.name))
                .replace(
                    &style.identifier.to_lowercase(),
                    &format!("$STYLE:{}", style.name),
                );
        }
        value
    }
    match value {
        Value::String(value) => string(value, styles).into(),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| generated_styles(value, styles))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (string(key, styles), generated_styles(value, styles)))
                .collect(),
        ),
        value => value,
    }
}
fn observe(
    owner: &ApplicationSettings,
    services: &ApplicationServices,
    page: &CapturePage,
    stack: &adw::ViewStack,
    events: &[Value],
) -> Value {
    let form = &owner.capture;
    let personal = services.personalization.borrow();
    let personal = personal.state();
    let disk = fs::read(services.paths.config.join("config.json"))
        .ok()
        .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap());
    generated_styles(
        json!({
            "config":selected_config(&services.config()),"disk":disk.as_ref().map(selected_document),"status":error_text(&page.status.label()),
            "combos":{
                "mode":combo(&form.mode),"language":combo(&form.language),"output_style":combo(&form.output_style),
                "global_recording_key":combo(&form.recording_key),"microphone_device":combo(&form.microphone),
                "system_audio_device":combo(&form.system_audio),"audio_retention":combo(&form.audio_retention),"history_retention":combo(&form.history_retention),
            },
            "switches":{
                "remember_application_switch":switch(&form.remember_application),"cleanup_switch":switch(&form.cleanup),"automatic_titles_switch":switch(&form.automatic_titles),
                "spoken_commands_switch":switch(&form.spoken_commands),"auto_paste_switch":switch(&form.auto_paste),"incognito_switch":switch(&form.incognito),
            },
            "instructions":form.style_instructions.subtitle().map(String::from),"shortcut":form.shortcut_status.subtitle().map(String::from),"latest":form.latest_shortcut_status.subtitle().map(String::from),
            "refresh":button(&form.refresh_audio),"profile":form.profile(),
            "personal":{"default":personal.default_style_identifier,"styles":personal.application_style_identifiers,"disabled":personal.application_style_disabled_identifiers,"modes":personal.application_modes,"custom":personal.custom_styles.iter().map(|style|json!({"id":style.identifier,"name":style.name,"instructions":style.instructions})).collect::<Vec<_>>()},
            "events":events,"pages":owner.view.pages().iter().map(|page|json!([page.name().map(String::from),page.title().to_string(),page.icon_name().map(String::from)])).collect::<Vec<_>>(),
            "visible_page":stack.visible_child_name().map(String::from),"settings_page":owner.view.visible_page_name().map(String::from),
            "welcome_step":owner.welcome.step(),"workspace_values":owner.workspace.values(),
        }),
        &personal.custom_styles,
    )
}
fn assert_state(actual: &Value, expected: &Value, name: &str) {
    fn diff(actual: &Value, expected: &Value, prefix: &str, out: &mut Vec<String>) {
        if let (Value::Object(a), Value::Object(b)) = (actual, expected) {
            for (key, a) in a {
                diff(
                    a,
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{prefix}.{key}"),
                    out,
                );
            }
            for key in b.keys().filter(|key| !a.contains_key(*key)) {
                out.push(format!("{prefix}.{key}: missing"));
            }
        } else if actual != expected {
            out.push(format!("{prefix}: actual={actual}, expected={expected}"));
        }
    }
    if actual != expected {
        let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
        fs::write(
            root.join("inline-actual.json"),
            serde_json::to_vec_pretty(actual).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("inline-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let mut differences = Vec::new();
        diff(actual, expected, "state", &mut differences);
        panic!("{name}: {}", differences.join("\n"));
    }
}
fn dump(tools: &Path, graph: &Value, exit: i64) {
    let _ = fs::remove_file(tools.join("dump.ready.json"));
    let bytes = serde_json::to_vec(graph).unwrap();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(
        tools.join("test-config.json"),
        serde_json::to_vec(&json!({"dump_hex":hex,"dump_exit":exit})).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "requires the private GTK/network/device runner and native PipeWire metadata peer"]
fn released_capture_preferences_and_application_settings() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("isolated runner"));
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
    }
    for node in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(node).exists());
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let tools = root.join("preferences-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone())
    );
    fs::create_dir(&tools).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    symlink(target.join("audio-fixture-peer"), tools.join("pw-dump")).unwrap();
    gtk::init().unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let runtime = DesktopRuntime::new().unwrap();
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let _status_peer = transport::Peer::new(root.join("preferences-status"), "text_target_peer");
    assert!(mluva_gtk::text_target::system_accessibility_enabled());
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-preferences.json")).unwrap();
    let mut states = 0;
    for row in fixture["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let directory = root.join(name);
        let paths = AppPaths {
            config: directory.join("config"),
            data: directory.join("data"),
            runtime: directory.join("runtime"),
        };
        fs::create_dir_all(&paths.config).unwrap();
        let config = update_config(
            &AppConfig::default(),
            row.get("initial").unwrap_or(&json!({})),
        );
        config.save(&paths.config.join("config.json")).unwrap();
        let services = ApplicationServices::open(paths).unwrap();
        let page = capture(&services, &resources);
        dump(&tools, &fixture["graph"], 0);
        let catalog = PipeWireDeviceCatalog::from_system(None).unwrap();
        let stack = adw::ViewStack::builder()
            .hhomogeneous(false)
            .vhomogeneous(false)
            .build();
        let events = Rc::new(RefCell::new(Vec::new()));
        let activity = Rc::new(RefCell::new(SettingsActivity::default()));
        let settings = ApplicationSettings::new(
            services.clone(),
            runtime.clone(),
            catalog,
            None,
            row["tracking"].as_bool().unwrap_or(false),
            ApplicationSettingsCallbacks {
                activity: {
                    let activity = activity.clone();
                    Rc::new(move || {
                        let a = activity.borrow();
                        SettingsActivity {
                            preparing: a.preparing,
                            processing: a.processing,
                            recording: a.recording,
                            pending_incognito: a.pending_incognito,
                            pending_mode: a.pending_mode.clone(),
                            final_live: a.final_live,
                            rewriting: a.rewriting,
                            live_schedule: a.live_schedule,
                        }
                    })
                },
                committed: {
                    let page = page.clone();
                    Rc::new(move |update| {
                        page.set_config(update.config).unwrap();
                    })
                },
                navigate: {
                    let stack = stack.clone();
                    Rc::new(move |name| stack.set_visible_child_name(name))
                },
                prompts_changed: Rc::new(|| {}),
                message: {
                    let events = events.clone();
                    Rc::new(move |message| events.borrow_mut().push(json!(["toast", message])))
                },
                inline: CapturePreferenceCallbacks {
                    changed: {
                        let services = services.clone();
                        let page = page.clone();
                        let events = events.clone();
                        let shortcuts = row["shortcuts"].as_bool().unwrap_or(false);
                        Rc::new(move |effect| {
                            if effect == InlineEffect::Shortcut && shortcuts {
                                events
                                    .borrow_mut()
                                    .push(json!(["key", services.config().global_recording_key]));
                            }
                            if matches!(effect, InlineEffect::Privacy { .. }) {
                                page.workspace.set_private(services.config().incognito_mode);
                            }
                        })
                    },
                    summary_changed: Rc::new(|| {}),
                    routes_changed: {
                        let events = events.clone();
                        Rc::new(move |mic, system| {
                            events.borrow_mut().push(json!(["routes", mic, system]))
                        })
                    },
                    history_changed: {
                        let page = page.clone();
                        Rc::new(move || {
                            page.workspace.refresh_history().unwrap();
                        })
                    },
                    status: {
                        let page = page.clone();
                        Rc::new(move |message| page.set_status(message))
                    },
                    toast: {
                        let events = events.clone();
                        Rc::new(move |message| events.borrow_mut().push(json!(["toast", message])))
                    },
                    manage_styles: {
                        let events = events.clone();
                        Rc::new(move || events.borrow_mut().push(json!(["manage"])))
                    },
                    export_diagnostics: {
                        let events = events.clone();
                        Rc::new(move || events.borrow_mut().push(json!(["export"])))
                    },
                },
            },
        )
        .unwrap();
        settings
            .capture
            .set_shortcuts_available(row["shortcuts"].as_bool().unwrap_or(false));
        assert_eq!(maturity(&settings.capture.maturity), fixture["maturity"]);
        stack.add_named(&page.widget, Some("capture"));
        stack.add_named(&settings.view.widget, Some("settings"));
        stack.add_named(&settings.welcome.widget, Some("welcome"));
        stack.set_visible_child_name("settings");
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .content(&stack)
            .build();
        window.present();
        settle();
        page.status.set_label("");
        events.borrow_mut().clear();
        for stage in row["stages"].as_array().unwrap() {
            let action = &stage["action"];
            let form = &settings.capture;
            match action["op"].as_str().unwrap() {
                "observe" => {}
                "mode" => form
                    .mode
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "language" => form
                    .language
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "key" => form
                    .recording_key
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "style" => form
                    .output_style
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "audio_policy" => form
                    .audio_retention
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "retention" => form
                    .history_retention
                    .set_selected(action["index"].as_u64().unwrap() as u32),
                "cleanup" => form.cleanup.set_active(action["value"].as_bool().unwrap()),
                "incognito" => form
                    .incognito
                    .set_active(action["value"].as_bool().unwrap()),
                "titles" => form
                    .automatic_titles
                    .set_active(action["value"].as_bool().unwrap()),
                "spoken" => form
                    .spoken_commands
                    .set_active(action["value"].as_bool().unwrap()),
                "paste" => form
                    .auto_paste
                    .set_active(action["value"].as_bool().unwrap()),
                "remember" => form
                    .remember_application
                    .set_active(action["value"].as_bool().unwrap()),
                "audio" => {
                    form.microphone
                        .set_selected(action["microphone"].as_u64().unwrap() as u32);
                    form.system_audio
                        .set_selected(action["system"].as_u64().unwrap() as u32);
                }
                "profile" => form.set_profile(action["value"].as_str().map(str::to_owned)),
                "fail" => {
                    let path = services.paths.config.join("config.json");
                    fs::remove_file(&path).unwrap();
                    fs::create_dir(path).unwrap();
                }
                "repair" => {
                    let path = services.paths.config.join("config.json");
                    fs::remove_dir(&path).unwrap();
                    services.config().save(&path).unwrap();
                }
                "fail_personal" => {
                    let path = services.paths.config.join("personalization.json");
                    if path.exists() {
                        fs::remove_file(&path).unwrap();
                    }
                    fs::create_dir(path).unwrap();
                }
                "repair_personal" => {
                    fs::remove_dir(services.paths.config.join("personalization.json")).unwrap()
                }
                "refresh" => {
                    let _ = fs::remove_file(tools.join("dump.ready.json"));
                    form.refresh_audio.emit_clicked();
                }
                "graph" => dump(&tools, &action["payload"], action["exit"].as_i64().unwrap()),
                "busy" => {
                    let field = action["field"].as_str().unwrap();
                    activity.borrow_mut().preparing = field == "preparing";
                    activity.borrow_mut().processing = field == "processing";
                    activity.borrow_mut().recording = field == "recording";
                    form.set_activity(PreferenceActivity {
                        preparing: field == "preparing",
                        processing: field == "processing",
                        recording: field == "recording",
                        meeting_processing: field == "meeting_processing",
                        meeting_retrying: field == "meeting_retrying",
                        meeting_recording: field == "meeting_recording",
                        retrying: field == "retrying",
                        ..Default::default()
                    });
                }
                "idle" => {
                    *activity.borrow_mut() = SettingsActivity::default();
                    form.set_activity(PreferenceActivity::default());
                }
                "manage" => {
                    settings.view.set_visible_page_name("capture");
                    form.manage_styles.emit_clicked();
                }
                "export" => {
                    settings.view.set_visible_page_name("advanced");
                    form.export_diagnostics.emit_clicked();
                }
                "workspace" => {
                    assert!(settings.apply(action["changes"].as_object().unwrap()));
                }
                "show_settings" => settings.show(),
                "show_welcome" => settings.show_welcome(),
                "open_style_prompt" => settings.open_prompt(&format!(
                    "style-{}",
                    form.selected_style().unwrap().to_lowercase()
                )),
                "create_style" => {
                    services
                        .personalization
                        .borrow_mut()
                        .save_style(
                            action["name"].as_str().unwrap(),
                            action["instructions"].as_str().unwrap(),
                        )
                        .unwrap();
                    form.refresh_styles();
                }
                "custom_style" => form
                    .output_style
                    .set_selected(form.output_style.model().unwrap().n_items() - 1),
                "restore_prompt" => {
                    let dialog = window.visible_dialog().unwrap();
                    descendants(&dialog)
                        .into_iter()
                        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
                        .find(|button| button.label().as_deref() == Some("Restore original"))
                        .unwrap()
                        .emit_clicked();
                }
                "edit_prompt" => {
                    let dialog = window.visible_dialog().unwrap();
                    let editor = descendants(&dialog)
                        .into_iter()
                        .find_map(|widget| widget.downcast::<gtk::TextView>().ok())
                        .unwrap();
                    editor.buffer().set_text(action["value"].as_str().unwrap());
                }
                "save_prompt" => {
                    let dialog = window.visible_dialog().unwrap();
                    descendants(&dialog)
                        .into_iter()
                        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
                        .find(|button| button.label().as_deref() == Some("Save"))
                        .unwrap()
                        .emit_clicked();
                }
                "welcome_next" => settings.welcome.next.emit_clicked(),
                "back_settings" => settings.view.close(),
                other => panic!("unsupported action {other}"),
            }
            settle();
            states += 1;
            assert_state(
                &observe(&settings, &services, &page, &stack, &events.borrow()),
                &stage["observed"],
                &format!("{name} {}", action),
            );
        }
        if name == "config-write-failure" {
            let before = services.config();
            let path = services.paths.config.join("config.json");
            fs::remove_file(&path).unwrap();
            fs::create_dir(&path).unwrap();
            let began = Instant::now();
            settings.capture.microphone.set_selected(1);
            settings.capture.system_audio.set_selected(1);
            settle();
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "Audio settings re-entered a failed save"
            );
            assert_eq!(services.config(), before);
            assert_eq!(settings.capture.microphone.selected(), 0);
            assert_eq!(settings.capture.system_audio.selected(), 0);
            assert!(
                page.status
                    .label()
                    .starts_with("Audio routing could not be saved:")
            );
            assert!(path.is_dir());
            println!("native unwritable audio route returns with previous routes");
        }
        window.destroy();
        drop(settings);
        settle();
        println!("native preferences {name}");
    }
    println!(
        "native preferences cases={} states={states}",
        fixture["cases"].as_array().unwrap().len()
    );
}
