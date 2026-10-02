//! Actual archive widgets/SQLite actions against unchanged released observations.
use adw::prelude::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use glib::translate::IntoGlib;
use mluva_core::{
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
};
use mluva_gtk::{
    document_layout::DocumentResources,
    history_view::{HistoryCallbacks, HistoryPage},
    theme::ThemeController,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
const ID: &str = "11111111-1111-4111-8111-111111111111";
const SECOND: &str = "22222222-2222-4222-8222-222222222222";
fn settle() {
    let end = Instant::now() + Duration::from_millis(90);
    while Instant::now() < end {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(2));
    }
}
fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn walk(widget: gtk::Widget, out: &mut Vec<gtk::Widget>) {
        out.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(next) = child {
            walk(next.clone(), out);
            child = next.next_sibling();
        }
    }
    let mut out = vec![];
    walk(widget.as_ref().clone(), &mut out);
    out
}
fn normalize(value: Value, root: &Path) -> Value {
    match value {
        Value::String(value) => json!(value.replace(root.to_str().unwrap(), "$ROOT")),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(|v| normalize(v, root)).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(k, v)| (k, normalize(v, root)))
                .collect(),
        ),
        other => other,
    }
}
fn files(root: &Path) -> serde_json::Map<String, Value> {
    fn walk(directory: &Path, root: &Path, out: &mut serde_json::Map<String, Value>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_none_or(|value| value != "sqlite3") {
                out.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    json!(STANDARD.encode(fs::read(&path).unwrap())),
                );
            }
        }
    }
    let mut out = serde_json::Map::new();
    walk(root, root, &mut out);
    out
}
fn observe(page: &HistoryPage, window: &adw::Window, events: &[Value], root: &Path) -> Value {
    let mut rows = page
        .rows
        .borrow()
        .iter()
        .map(|(id, row)| (id.clone(), row.clone()))
        .collect::<Vec<_>>();
    rows.sort_by_key(|(_, row)| row.index());
    let rows=rows.iter().map(|(id,row)|{let children=widgets(row);json!({"id":id,"title":row.title().to_string(),"subtitle":row.subtitle().to_string(),"expanded":row.is_expanded(),
        "labels":children.iter().filter_map(|w|w.downcast_ref::<gtk::Label>()).map(|w|w.label().to_string()).collect::<Vec<_>>(),
        "entries":children.iter().filter_map(|w|w.downcast_ref::<gtk::Entry>()).map(|w|w.text().to_string()).collect::<Vec<_>>(),
        "texts":children.iter().filter_map(|w|w.downcast_ref::<gtk::TextView>()).map(|w|{let b=w.buffer();b.text(&b.start_iter(),&b.end_iter(),true).to_string()}).collect::<Vec<_>>(),
        "expanders":children.iter().filter_map(|w|w.downcast_ref::<adw::ExpanderRow>()).map(|w|json!({"title":w.title().to_string(),"subtitle":w.subtitle().to_string(),"expanded":w.is_expanded()})).collect::<Vec<_>>(),
        "buttons":children.iter().filter_map(|w|w.downcast_ref::<gtk::Button>()).filter_map(|w|w.label().map(|label|json!({"label":label.to_string(),"visible":w.is_visible(),"sensitive":w.is_sensitive(),"destructive":w.has_css_class("destructive-action")}))).collect::<Vec<_>>()})}).collect::<Vec<_>>();
    let dialog=window.visible_dialog().map(|dialog|{let dialog=dialog.downcast::<adw::AlertDialog>().unwrap();json!({"heading":dialog.heading().map(String::from),"body":dialog.body().to_string(),"default":dialog.default_response().map(String::from),"close":dialog.close_response().to_string(),"responses":(["cancel","delete"].into_iter().map(|name|json!({"id":name,"label":dialog.response_label(name).to_string(),"appearance":dialog.response_appearance(name).into_glib()})).collect::<Vec<_>>())})});
    normalize(
        json!({"count":page.count.label().to_string(),"stack":page.archive.visible_child_name().map(String::from),"focused":page.focused_identifier(),"rows":rows,"records":page.store.recent(200).unwrap(),"events":events,"dialog":dialog,"files":files(root)}),
        root,
    )
}
fn compare(actual: &Value, expected: &Value, label: &str, private: &Path) {
    if actual != expected {
        fs::write(
            private.join("history-page-actual.json"),
            serde_json::to_vec_pretty(actual).unwrap(),
        )
        .unwrap();
        fs::write(
            private.join("history-page-expected.json"),
            serde_json::to_vec_pretty(expected).unwrap(),
        )
        .unwrap();
        let changed = actual
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, v)| expected.get(*k) != Some(*v))
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>();
        panic!("{label}: changed {changed:?}; synthetic observations saved in private evidence");
    }
}
#[test]
#[ignore = "Requires the guarded private GTK/session/accessibility/network/PID/device environment"]
fn released_history_archive_widgets_actions_and_store_changes_match() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&private));
    }
    for node in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(node).exists());
    }
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
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
    let _theme =
        ThemeController::apply(private.join("theme"), resources.font.parent().unwrap()).unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-history-page.json")).unwrap();
    let mut states = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir_in(&private).unwrap();
        let root = directory.path();
        let store = HistoryStore::new(root.join("history.sqlite3"));
        store.initialize().unwrap();
        let conversations = ConversationStore::new(store.clone());
        conversations.initialize().unwrap();
        if let Some(files) = case["files"].as_object() {
            for (name, value) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, value.as_str().unwrap()).unwrap();
            }
        }
        if let Some(records) = case["records"].as_array() {
            for (index, record) in records.iter().enumerate() {
                let mut record = record.clone();
                if let Some(path) = record["retained_audio_path"].as_str() {
                    record["retained_audio_path"] = json!(root.join(path).to_str().unwrap());
                }
                let input: HistoryInput = serde_json::from_value(record).unwrap();
                let added = store.add(input).unwrap();
                rusqlite::Connection::open(root.join("history.sqlite3")).unwrap().execute("UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",rusqlite::params![[ID,SECOND][index],case["created_at"].as_str().map(str::to_owned).unwrap_or_else(||format!("2026-09-28T12:34:5{}.123456Z",6+index)),added.identifier]).unwrap();
            }
        }
        if case["group"] == true {
            conversations
                .append_recording(ID, &store.find(SECOND).unwrap())
                .unwrap();
        }
        if case["reply"] == true {
            conversations
                .append(
                    ID,
                    "Rewrite instruction.",
                    "Latest durable reply.",
                    "gpt-5.4-mini",
                )
                .unwrap();
            rusqlite::Connection::open(root.join("history.sqlite3"))
                .unwrap()
                .execute(
                    "UPDATE conversation_rewrites SET created_at=?",
                    ["2026-09-28T12:34:59.123456Z"],
                )
                .unwrap();
        }
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let denied = Rc::new(Cell::new(false));
        let page = HistoryPage::new(
            store.clone(),
            Some(conversations),
            root.join("exports"),
            "24h".into(),
            HistoryCallbacks {
                copy: {
                    let events = events.clone();
                    Rc::new(move |value| events.borrow_mut().push(json!(["copy", value])))
                },
                can_retry_delivery: {
                    let allowed = case["can_paste"].as_array().cloned().unwrap_or_default();
                    Rc::new(move |entry| allowed.contains(&json!(entry.identifier)))
                },
                retry_delivery: {
                    let events = events.clone();
                    Rc::new(move |entry| {
                        events.borrow_mut().push(json!(["paste", entry.identifier]))
                    })
                },
                retry_recognition: {
                    let events = events.clone();
                    Rc::new(move |entry| {
                        events.borrow_mut().push(json!(["retry", entry.identifier]))
                    })
                },
                reprocess: {
                    let events = events.clone();
                    Rc::new(move |entry| {
                        events
                            .borrow_mut()
                            .push(json!(["reprocess", entry.identifier]))
                    })
                },
                delete: {
                    let events = events.clone();
                    let denied = denied.clone();
                    let store = store.clone();
                    Rc::new(move |entry| {
                        events
                            .borrow_mut()
                            .push(json!(["delete", entry.identifier]));
                        if denied.get() {
                            return false;
                        }
                        store.delete(&entry.identifier).unwrap();
                        true
                    })
                },
                changed: {
                    let events = events.clone();
                    Rc::new(move || events.borrow_mut().push(json!(["changed"])))
                },
                message: {
                    let events = events.clone();
                    Rc::new(move |value| events.borrow_mut().push(json!(["message", value])))
                },
            },
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
            let op = action["op"].as_str().unwrap();
            let id = action["id"].as_str();
            match op {
                "observe" => {}
                "refresh" => page.refresh().unwrap(),
                "focus" => page.focus_entry(id).unwrap(),
                "time" => {
                    page.set_time_format(action["value"].as_str().unwrap().into());
                    page.refresh().unwrap();
                }
                "deny_delete" => denied.set(true),
                "external_title" => {
                    store
                        .update_title(id.unwrap(), Some(action["value"].as_str().unwrap()))
                        .unwrap();
                }
                "refresh_title" => page.refresh_title(id.unwrap()).unwrap(),
                "title" => page.title_entries.borrow()[id.unwrap()]
                    .0
                    .set_text(action["value"].as_str().unwrap()),
                "cancel" | "confirm" => window
                    .visible_dialog()
                    .unwrap()
                    .downcast::<adw::AlertDialog>()
                    .unwrap()
                    .emit_by_name::<()>(
                        "response",
                        &[&if op == "cancel" { "cancel" } else { "delete" }],
                    ),
                _ => {
                    let row = page.rows.borrow()[id.unwrap()].clone();
                    let children = widgets(&row);
                    match op {
                        "expand" => row.set_expanded(true),
                        "details" | "recordings" => children
                            .iter()
                            .filter_map(|w| w.downcast_ref::<adw::ExpanderRow>())
                            .find(|row| {
                                row.title().as_str()
                                    == if op == "details" {
                                        "Technical details"
                                    } else {
                                        "Original recordings"
                                    }
                            })
                            .unwrap()
                            .set_expanded(true),
                        "correction" => children
                            .iter()
                            .find_map(|w| w.downcast_ref::<gtk::TextView>())
                            .unwrap()
                            .buffer()
                            .set_text(action["value"].as_str().unwrap()),
                        _ => {
                            let label = match op {
                                "copy" | "copy_current" => "Copy",
                                "paste" => "Paste again",
                                "copy_raw" => "Copy raw",
                                "save_title" => "Save title",
                                "save_correction" => "Save correction",
                                "retry" => "Retry transcription",
                                "restore_raw" => "Restore raw",
                                "reprocess" => "Reprocess raw",
                                "export_markdown" => "Export Markdown",
                                "export_json" => "Export JSON",
                                "delete" => "Delete",
                                other => panic!("unknown History action {other}"),
                            };
                            children
                                .iter()
                                .filter_map(|w| w.downcast_ref::<gtk::Button>())
                                .find(|button| button.label().as_deref() == Some(label))
                                .unwrap()
                                .emit_clicked();
                        }
                    }
                }
            }
            settle();
            compare(
                &observe(&page, &window, &events.borrow(), root),
                &stage["observed"],
                &format!("{} {action}", case["name"]),
                &private,
            );
            states += 1;
        }
        window.destroy();
        settle();
    }
    println!(
        "Matched {} actual released History pages/{states} GTK/store states",
        fixture["cases"].as_array().unwrap().len()
    );
}
