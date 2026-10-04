//! Actual capture and model/thinking widgets against immutable released GTK observations.

#![recursion_limit = "512"]

use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_core::conversation::ConversationStore;
use mluva_core::history::HistoryStore;
use mluva_gtk::capture_view::{
    CaptureCallbacks, CapturePage, CaptureShortcutState, LiveSettingsChange,
};
use mluva_gtk::conversation_view::{ConversationCallbacks, ConversationWorkspace};
use mluva_gtk::document_layout::DocumentResources;
use mluva_gtk::rewrite_settings::RewriteSettings;
use mluva_gtk::theme::ThemeController;
use mluva_providers::models::Model;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::thread;
use std::time::{Duration, Instant};

fn settle() {
    let end = Instant::now() + Duration::from_millis(100);
    while Instant::now() < end {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(3));
    }
}

fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn append(widget: gtk::Widget, result: &mut Vec<gtk::Widget>) {
        result.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            append(current.clone(), result);
            child = current.next_sibling();
        }
    }
    let mut result = vec![];
    append(widget.as_ref().clone(), &mut result);
    result
}

fn button(button: &gtk::Button) -> Value {
    json!({"label":button.label().map(String::from),"tooltip":button.tooltip_text().map(String::from),"visible":button.get_visible(),"sensitive":button.get_sensitive()})
}

fn combo(row: &adw::ComboRow) -> Value {
    let labels = row
        .model()
        .and_downcast::<gtk::StringList>()
        .map(|model| {
            (0..model.n_items())
                .map(|i| model.string(i).unwrap().to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"title":row.title().to_string(),"subtitle":row.subtitle().map(String::from),"visible":row.get_visible(),"sensitive":row.get_sensitive(),"selected":row.selected(),"labels":labels})
}

fn settings_observation(w: &RewriteSettings, events: &RefCell<Vec<Value>>) -> Value {
    json!({"caption":w.caption.label().to_string(),"tooltip":w.widget.tooltip_text().map(String::from),"sensitive":w.widget.get_sensitive(),"model":combo(&w.model_row),"thinking":combo(&w.thinking_row.widget),"fast":{"active":w.fast_row.is_active(),"visible":w.fast_row.get_visible(),"sensitive":w.fast_row.get_sensitive(),"subtitle":w.fast_row.subtitle().map(String::from)},"refresh":button(&w.refresh),"status":w.status.label().to_string(),"events":*events.borrow()})
}

fn capture_observation(w: &CapturePage, events: &RefCell<Vec<Value>>) -> Value {
    let record = widgets(&w.record_button);
    let record_labels = record
        .iter()
        .filter_map(|widget| {
            widget
                .downcast_ref::<gtk::Label>()
                .map(|label| label.label().to_string())
        })
        .collect::<Vec<_>>();
    let record_icons = record
        .iter()
        .filter_map(|widget| {
            widget
                .downcast_ref::<gtk::Image>()
                .and_then(|image| image.icon_name())
                .map(String::from)
        })
        .collect::<Vec<_>>();
    let labels = widgets(&w.output_section)
        .iter()
        .filter_map(|widget| {
            widget
                .downcast_ref::<gtk::Button>()
                .and_then(|button| button.label())
                .map(String::from)
        })
        .collect::<Vec<_>>();
    let templates = w
        .live_templates
        .iter()
        .map(|(id, button)| json!([id, button.label().map(String::from), button.is_active()]))
        .collect::<Vec<_>>();
    let buffer = w.output_view.buffer();
    json!({"status":w.status.label().to_string(),"status_tooltip":w.status.tooltip_text().map(String::from),"status_title":w.status_title.label().to_string(),"status_title_visible":w.status_title.get_visible(),"hint":w.action_hint.get_visible(),"hint_label":w.action_hint.label().to_string(),"record":{"labels":record_labels,"icons":record_icons,"sensitive":w.record_button.get_sensitive(),"suggested":w.record_button.has_css_class("suggested-action"),"destructive":w.record_button.has_css_class("destructive-action"),"tooltip":w.record_button.tooltip_text().map(String::from)},"dock":{"visible":w.action_bar.get_visible(),"orientation":if w.action_bar.orientation()==gtk::Orientation::Horizontal {"horizontal"} else {"vertical"},"spacing":w.action_bar.spacing()},"live":{"active":w.live_mode.is_active(),"button":button(w.live_mode.upcast_ref()),"menu_sensitive":w.live_menu.get_sensitive(),"menu_tooltip":w.live_menu.tooltip_text().map(String::from),"templates":templates},"callout":{"revealed":w.setup_callout.reveals_child(),"title":w.setup_title.label().to_string(),"body":w.setup_body.label().to_string(),"tooltip":w.setup_body.tooltip_text().map(String::from),"button":button(&w.setup_button)},"output":{"visible":w.output_section.get_visible(),"text":buffer.text(&buffer.start_iter(),&buffer.end_iter(),true).to_string(),"tooltip":w.output_view.tooltip_text().map(String::from),"editable":w.output_view.is_editable(),"source":[w.command_source.label().to_string(),w.command_source.get_visible()],"command":w.command_actions.get_visible(),"scratchpad":w.scratchpad_actions.get_visible(),"buttons":labels},"settings_caption":w.rewrite_settings.caption.label().to_string(),"events":*events.borrow()})
}

fn changed_config(config: &AppConfig, changes: &Value) -> AppConfig {
    let mut value = serde_json::to_value(config).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .extend(changes.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn check(root: &std::path::Path, surface: &str, index: usize, actual: Value, expected: &Value) {
    if &actual != expected {
        std::fs::write(
            root.join("capture-control-mismatch.json"),
            serde_json::to_vec_pretty(
                &json!({"surface":surface,"index":index,"actual":actual,"expected":expected}),
            )
            .unwrap(),
        )
        .unwrap();
        let differences = expected
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, value)| actual[*key] != **value)
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>();
        panic!("{surface} case {index}, differing fields: {differences:?}");
    }
    eprintln!("native_{surface}={index} PASS");
}

#[test]
#[ignore = "requires the private display/network/PID/device runner and the recorded GTK renderer"]
fn capture_page_and_catalog_controls_match_released_widgets() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!std::path::Path::new(path).exists());
    }
    for name in ["HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(name).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-capture-controls.json")).unwrap();
    assert_eq!(
        reference["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    adw::init().unwrap();
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(reference["pango"], gtk::pango::version_string().as_str());
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let models: Vec<Model> = serde_json::from_value(reference["models"].clone()).unwrap();
    let remote: Vec<Model> = serde_json::from_value(reference["remote"].clone()).unwrap();
    let events = Rc::new(RefCell::new(vec![]));
    let loading = events.clone();
    let saving = events.clone();
    let mut config = AppConfig {
        rewrite_provider: "codex".into(),
        ..Default::default()
    };
    let settings = RewriteSettings::new(
        config.clone(),
        Rc::new(move || loading.borrow_mut().push(json!(["load"]))),
        Rc::new(move |model, fast, effort| {
            saving
                .borrow_mut()
                .push(json!(["save", model, fast, effort]))
        }),
    );
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&settings.widget);
    window.set_content(Some(&content));
    window.present();
    settle();
    for (index, case) in reference["settings"].as_array().unwrap().iter().enumerate() {
        let action = &case["action"];
        match action["op"].as_str().unwrap() {
            "observe" => {}
            "loading" => settings.set_loading(),
            "models" => settings.set_models(match action["kind"].as_str().unwrap() {
                "none" => None,
                "empty" => Some(vec![]),
                "remote" => Some(remote.clone()),
                _ => Some(models.clone()),
            }),
            "model" => settings
                .model_row
                .set_selected(action["index"].as_u64().unwrap() as u32),
            "effort" => settings
                .thinking_row
                .widget
                .set_selected(action["index"].as_u64().unwrap() as u32),
            "fast" => settings
                .fast_row
                .set_active(action["value"].as_bool().unwrap()),
            "config" => {
                config = changed_config(&config, &action["changes"]);
                settings.set_config(config.clone());
            }
            "refresh" => settings.refresh.emit_clicked(),
            "show" => settings.popover.emit_by_name::<()>("show", &[]),
            op => panic!("unknown reference settings action: {op}"),
        }
        settle();
        check(
            &root,
            "settings",
            index,
            settings_observation(&settings, &events),
            &case["observed"],
        );
    }
    window.close();
    settle();
    drop(settings);

    let store = ConversationStore::new(HistoryStore::new(root.join("capture.sqlite3")));
    store.initialize().unwrap();
    let workspace = ConversationWorkspace::new(
        store,
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
        resources,
    )
    .unwrap();
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let event = |name: &'static str| {
        let events = events.clone();
        Rc::new(move || events.borrow_mut().push(json!([name]))) as Rc<dyn Fn()>
    };
    let text_event = |name: &'static str| {
        let events = events.clone();
        Rc::new(move |text: &str| events.borrow_mut().push(json!([name, text]))) as Rc<dyn Fn(&str)>
    };
    let loading = events.clone();
    let saving = events.clone();
    let config = AppConfig {
        rewrite_provider: "codex".into(),
        ..Default::default()
    };
    let settings = RewriteSettings::new(
        config.clone(),
        Rc::new(move || loading.borrow_mut().push(json!(["load"]))),
        Rc::new(move |model, fast, effort| {
            saving
                .borrow_mut()
                .push(json!(["save", model, fast, effort]))
        }),
    );
    let owner = Rc::new(RefCell::new(Weak::<CapturePage>::new()));
    let live_owner = owner.clone();
    let reject = Rc::new(Cell::new(false));
    let rejection = reject.clone();
    let changes = events.clone();
    let callbacks = CaptureCallbacks {
        toggle_recording: event("record"),
        apply_live_settings: Rc::new(move |change| {
            let patch = match change {
                LiveSettingsChange::Mode {
                    enabled,
                    continuous,
                } => json!({"live_rewrite_enabled":enabled,"live_rewrite_continuous":continuous}),
                LiveSettingsChange::Template(id) => json!({"live_rewrite_template":id}),
            };
            changes.borrow_mut().push(json!(["live-settings", patch]));
            if rejection.get() {
                return false;
            }
            let page = live_owner.borrow().upgrade().unwrap();
            page.set_config(changed_config(&page.config(), &patch))
                .unwrap();
            true
        }),
        toast: text_event("toast"),
        open_prompt: text_event("open-prompt"),
        retry_initialization: event("retry"),
        accept_command: event("accept-command"),
        discard_command: event("discard-command"),
        copy_scratchpad: event("copy-scratchpad"),
        delete_scratchpad: event("delete-scratchpad"),
        output_changed: text_event("output"),
        announce: Rc::new(|_| {}),
    };
    let page = CapturePage::new(workspace, settings, config, callbacks).unwrap();
    *owner.borrow_mut() = Rc::downgrade(&page);
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .build();
    window.set_content(Some(&page.widget));
    window.present();
    settle();
    for (index, case) in reference["capture"].as_array().unwrap().iter().enumerate() {
        let action = &case["action"];
        match action["op"].as_str().unwrap() {
            "observe" => {}
            "record" => page.record_button.emit_clicked(),
            "live-cycle" => page.live_mode.emit_clicked(),
            "template" => page.live_templates[action["id"].as_str().unwrap()].set_active(true),
            "reject" => reject.set(action["value"].as_bool().unwrap()),
            "config" => page
                .set_config(changed_config(&page.config(), &action["changes"]))
                .unwrap(),
            "shortcuts" => page.set_shortcuts(CaptureShortcutState {
                recording_trigger: action["record"].as_str().map(str::to_owned),
                rewrite_trigger: action["rewrite"].as_str().map(str::to_owned),
                shortcut_service_available: action["service"].as_bool().unwrap(),
                target_tracking_available: action["tracking"].as_bool().unwrap(),
            }),
            "status" => page.set_status(action["text"].as_str().unwrap()),
            "error" => page.set_error(action["text"].as_str().unwrap()),
            "initialization" => page.set_initialization_error(action["text"].as_str().unwrap()),
            "callout" => page.setup_button.emit_clicked(),
            "mode" => page.set_pending_mode(action["mode"].as_str().unwrap()),
            "output" => page
                .output_view
                .buffer()
                .set_text(action["text"].as_str().unwrap()),
            "command-actions" => {
                page.command_actions
                    .set_visible(action["value"].as_bool().unwrap());
                page.refresh_output_visibility();
            }
            "scratchpad-actions" => {
                page.scratchpad_actions
                    .set_visible(action["value"].as_bool().unwrap());
                page.refresh_output_visibility();
            }
            "accept" => page.accept_command.emit_clicked(),
            "discard" => page.discard_command.emit_clicked(),
            "copy-scratchpad" => page.copy_scratchpad.emit_clicked(),
            "delete-scratchpad" => page.delete_scratchpad.emit_clicked(),
            op => panic!("unknown reference capture action: {op}"),
        }
        settle();
        check(
            &root,
            "capture",
            index,
            capture_observation(&page, &events),
            &case["observed"],
        );
    }
    window.close();
    settle();
}
