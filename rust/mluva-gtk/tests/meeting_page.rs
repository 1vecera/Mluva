//! Actual Meeting widgets, edits, private exports and confirmation dialogs.

use adw::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use glib::translate::IntoGlib;
use mluva_core::meeting::{MeetingRecord, MeetingStore};
use mluva_gtk::{
    document_layout::DocumentResources,
    meeting_view::{MeetingCallbacks, MeetingPage},
    theme::ThemeController,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn walk(widget: gtk::Widget, result: &mut Vec<gtk::Widget>) {
        result.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(widget) = child {
            walk(widget.clone(), result);
            child = widget.next_sibling();
        }
    }
    let mut result = vec![];
    walk(widget.as_ref().clone(), &mut result);
    result
}
fn settle() {
    let until = Instant::now() + Duration::from_millis(110);
    while Instant::now() < until {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn normalize(value: Value, root: &Path) -> Value {
    match value {
        Value::String(value) if value.starts_with("Meeting title could not be saved: ") => {
            Value::String("Meeting title could not be saved: I/O failure".into())
        }
        Value::String(value) => Value::String(value.replace(root.to_str().unwrap(), "$ROOT")),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| normalize(value, root))
                .collect(),
        ),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalize(value, root)))
                .collect(),
        ),
        value => value,
    }
}
fn files(root: &Path) -> BTreeMap<String, String> {
    fn walk(directory: &Path, root: &Path, files: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, files);
            } else if path.is_file() {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    STANDARD.encode(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn observe(page: &MeetingPage, window: &adw::Window, events: &[Value], root: &Path) -> Value {
    let rows=page.meeting_rows.borrow().iter().map(|(identifier,row)|{
        let widgets=descendants(row);
        json!({"id":identifier,"title":row.title().to_string(),"subtitle":row.subtitle().to_string(),"expanded":row.is_expanded(),
            "labels":widgets.iter().filter_map(|widget|widget.clone().downcast::<gtk::Label>().ok()).map(|label|label.label().to_string()).collect::<Vec<_>>(),
            "entries":widgets.iter().filter_map(|widget|widget.clone().downcast::<gtk::Entry>().ok()).map(|entry|entry.text().to_string()).collect::<Vec<_>>(),
            "expanders":widgets.iter().filter_map(|widget|widget.clone().downcast::<adw::ExpanderRow>().ok()).map(|row|json!({"title":row.title().to_string(),"subtitle":row.subtitle().to_string(),"expanded":row.is_expanded()})).collect::<Vec<_>>(),
            "buttons":widgets.iter().filter_map(|widget|widget.clone().downcast::<gtk::Button>().ok()).filter_map(|button|button.label().map(|label|json!({"label":label.to_string(),"visible":button.is_visible(),"sensitive":button.is_sensitive(),"destructive":button.has_css_class("destructive-action")}))).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    let dialog=window.visible_dialog().map(|dialog|{
        let dialog=dialog.downcast::<adw::AlertDialog>().unwrap();
        let responses=["cancel","delete"].iter().map(|name|json!({"id":name,"label":dialog.response_label(name).to_string(),"appearance":dialog.response_appearance(name).into_glib()})).collect::<Vec<_>>();
        json!({"heading":dialog.heading().map(String::from),"body":dialog.body().to_string(),"default":dialog.default_response().map(String::from),"close":dialog.close_response().to_string(),
            "responses":responses})
    });
    let widgets = descendants(&page.record_button);
    normalize(
        json!({"status":page.status.label().to_string(),"status_role":page.status.accessible_role().into_glib(),
        "audio":[page.audio_routes.title.label().to_string(),page.audio_routes.subtitle.label().to_string()],
        "privacy":[page.privacy.title.label().to_string(),page.privacy.subtitle.label().to_string()],
        "record":{"labels":widgets.iter().filter_map(|widget|widget.clone().downcast::<gtk::Label>().ok()).map(|label|label.label().to_string()).collect::<Vec<_>>(),
            "icons":widgets.iter().filter_map(|widget|widget.clone().downcast::<gtk::Image>().ok()).map(|image|image.icon_name().map(String::from)).collect::<Vec<_>>(),
            "sensitive":page.record_button.is_sensitive(),"suggested":page.record_button.has_css_class("suggested-action"),"destructive":page.record_button.has_css_class("destructive-action")},
        "count":page.count.label().to_string(),"stack":page.archive.visible_child_name().map(String::from),"error_description":page.error_page.description().map(String::from),
        "rows":rows,"dialog":dialog,"events":events,"records":page.store.lock().unwrap().meetings().iter().map(MeetingRecord::document).collect::<Vec<_>>(),"files":files(root)}),
        root,
    )
}
fn compare(actual: &Value, expected: &Value, label: &str) {
    if actual != expected {
        let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
        fs::write(
            root.join("meeting-page-actual.json"),
            serde_json::to_vec_pretty(actual).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("meeting-page-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let changed = actual
            .as_object()
            .unwrap()
            .iter()
            .filter(|(key, value)| expected.get(*key) != Some(*value))
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>();
        panic!(
            "{label}: changed fields {changed:?}; complete synthetic observations saved in private evidence"
        );
    }
}
#[test]
#[ignore = "Requires the guarded private display, accessibility/session buses, network/PID and device isolation"]
fn released_meeting_archive_widgets_and_actions_match() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&private));
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
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
    gtk::init().unwrap();
    adw::init().unwrap();
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
        private.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-meeting-page.json")).unwrap();
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(
        reference["adw"],
        json!([
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        ])
    );
    let mut states = 0;
    for case in reference["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir_in(&private).unwrap();
        let root = directory.path();
        let path = root.join("meetings.json");
        if let Some(files) = case["files"].as_object() {
            for (relative, contents) in files {
                let path = root.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, contents.as_str().unwrap()).unwrap();
            }
        }
        if case["malformed"] == true {
            fs::write(&path, "{broken private archive").unwrap();
        }
        let mut archive = MeetingStore::new(path.clone(), None);
        if let Some(records) = case["records"].as_array() {
            for record in records {
                archive
                    .save(MeetingRecord::decode(record).unwrap())
                    .unwrap();
            }
        }
        let store = Arc::new(Mutex::new(archive));
        let events = Rc::new(RefCell::new(vec![]));
        let denied = Rc::new(Cell::new(false));
        let toggles = events.clone();
        let copied = events.clone();
        let retried = events.clone();
        let deleted = events.clone();
        let messages = events.clone();
        let removed = store.clone();
        let delete_denied = denied.clone();
        let page = MeetingPage::new(
            store,
            root.join("exports"),
            MeetingCallbacks {
                toggle_capture: Rc::new(move || toggles.borrow_mut().push(json!(["toggle"]))),
                copy_text: Rc::new(move |value| copied.borrow_mut().push(json!(["copy", value]))),
                retry_recognition: Rc::new(move |meeting| {
                    retried
                        .borrow_mut()
                        .push(json!(["retry", meeting.identifier]))
                }),
                delete_meeting: Rc::new(move |meeting| {
                    deleted
                        .borrow_mut()
                        .push(json!(["delete", meeting.identifier]));
                    if delete_denied.get() {
                        false
                    } else {
                        removed.lock().unwrap().delete(&meeting.identifier).unwrap();
                        true
                    }
                }),
                show_message: Rc::new(move |message| {
                    messages.borrow_mut().push(json!(["message", message]))
                }),
            },
            "24h".into(),
        )
        .unwrap();
        let window = adw::Window::builder()
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .content(&page.widget)
            .build();
        window.present();
        settle();
        for stage in case["stages"].as_array().unwrap() {
            let action = &stage["action"];
            let operation = action["op"].as_str().unwrap();
            let identifier = action["id"].as_str().unwrap_or("");
            match operation {
                "observe" => {}
                "privacy" => page.set_privacy(action["value"] == true),
                "routes" => page.set_audio_routes(
                    action["microphone"].as_str().unwrap(),
                    action["system"].as_str().unwrap(),
                ),
                "status" => page.set_status(action["value"].as_str().unwrap()),
                "recording" => page.set_capture_state(true, false),
                "processing" => page.set_capture_state(false, true),
                "idle" => page.set_capture_state(false, false),
                "toggle" => page.record_button.emit_clicked(),
                "refresh" => page.refresh().unwrap(),
                "time" => {
                    page.set_time_format(action["value"].as_str().unwrap().into());
                    page.refresh().unwrap();
                }
                "deny_delete" => denied.set(true),
                "write_failure" => {
                    let bytes = fs::read(&path).unwrap();
                    fs::remove_file(&path).unwrap();
                    fs::create_dir(&path).unwrap();
                    fs::write(path.join("preserved"), bytes).unwrap();
                }
                "title" => descendants(&page.row(identifier).unwrap())
                    .into_iter()
                    .find_map(|widget| widget.downcast::<gtk::Entry>().ok())
                    .unwrap()
                    .set_text(action["value"].as_str().unwrap()),
                "expand" | "expand_notes" | "expand_transcript" => {
                    let row = page.row(identifier).unwrap();
                    if operation == "expand" {
                        row.set_expanded(true);
                    } else {
                        descendants(&row)
                            .into_iter()
                            .filter_map(|widget| widget.downcast::<adw::ExpanderRow>().ok())
                            .find(|row| {
                                row.title()
                                    == if operation == "expand_notes" {
                                        "Summary and actions"
                                    } else {
                                        "Transcript"
                                    }
                            })
                            .unwrap()
                            .set_expanded(true);
                    }
                }
                "cancel" | "confirm" => window.visible_dialog().unwrap().emit_by_name::<()>(
                    "response",
                    &[&if operation == "cancel" {
                        "cancel"
                    } else {
                        "delete"
                    }],
                ),
                other => {
                    let label = match other {
                        "copy" => "Copy transcript",
                        "retry" => "Retry transcription",
                        "save_title" => "Save title",
                        "export_markdown" => "Export Markdown",
                        "export_json" => "Export JSON",
                        "delete" => "Delete",
                        _ => panic!("unknown page action {other}"),
                    };
                    descendants(&page.row(identifier).unwrap())
                        .into_iter()
                        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
                        .find(|button| button.label().as_deref() == Some(label))
                        .unwrap()
                        .emit_clicked();
                }
            }
            settle();
            compare(
                &observe(&page, &window, &events.borrow(), root),
                &stage["observed"],
                &format!("{} {action}", case["name"]),
            );
            states += 1;
        }
        window.destroy();
        drop(page);
        settle();
    }
    println!(
        "Matched {} actual Meeting pages/{states} GTK states",
        reference["cases"].as_array().unwrap().len()
    );
}
