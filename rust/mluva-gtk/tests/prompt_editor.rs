//! Public GTK state and real private-file effects, compared with the unchanged released editor.

use std::cell::{Cell, RefCell};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use adw::prelude::*;
use mluva_core::prompt_catalog::{DEFAULTS, SavedStyle};
use mluva_core::prompts::PromptStore;
use mluva_gtk::prompt_editor::{PromptEditor, PromptSession};
use mluva_gtk::theme::ThemeController;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Reference {
    gtk: [u32; 3],
    libadwaita: [u32; 3],
    pango: String,
    cases: Vec<Case>,
    session: SessionCase,
}
#[derive(Deserialize)]
struct SessionCase {
    actions: Vec<SessionAction>,
    observations: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum SessionAction {
    Open { identifier: String },
    Text { value: String },
    Activate { label: String },
}
#[derive(Deserialize)]
struct Case {
    name: String,
    identifier: String,
    file: Option<Vec<u8>>,
    writable: bool,
    actions: Vec<Action>,
    observations: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
enum Action {
    Text { value: String },
    Activate { label: String },
    Writable { value: bool },
    External { value: Vec<u8> },
    Reopen,
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(400);
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(8));
    }
}

fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn collect(widget: gtk::Widget, output: &mut Vec<gtk::Widget>) {
        output.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(current.clone(), output);
            child = current.next_sibling();
        }
    }
    let mut output = Vec::new();
    collect(widget.as_ref().clone(), &mut output);
    output
}

fn text_view(dialog: &adw::Dialog) -> gtk::TextView {
    widgets(&dialog.child().unwrap())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk::TextView>().ok())
        .unwrap()
}

fn labels(widget: &impl IsA<gtk::Widget>, data: &Path) -> Vec<String> {
    widgets(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Label>().ok())
        .map(|label| label.text().replace(data.to_str().unwrap(), "<data>"))
        .collect()
}

fn buttons(widget: &impl IsA<gtk::Widget>) -> Vec<Value> {
    widgets(widget)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .filter_map(|button| {
            button
                .label()
                .filter(|label| !label.is_empty())
                .map(|label| json!({"label":label.as_str(),"sensitive":button.is_sensitive()}))
        })
        .collect()
}

fn activate(window: &adw::Window, label: &str) {
    let dialog = window.visible_dialog().unwrap();
    let button = widgets(&dialog.child().unwrap())
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk::Button>().ok())
        .find(|button| button.label().as_deref() == Some(label))
        .unwrap();
    // Match the reference's public GTK action, including a stale Save after privacy changes.
    button.emit_clicked();
}

fn observation(
    window: &adw::Window,
    editor: &PromptEditor,
    data: &Path,
    file: &Path,
    changed: usize,
    closed: usize,
) -> Value {
    let mut result = json!({
        "changed":changed,"closed":closed,"file":fs::read(file).ok(),
        "mode":fs::metadata(file).ok().map(|stat| stat.permissions().mode() & 0o777),
        "open":window.visible_dialog().is_some(),
    });
    let Some(top) = window.visible_dialog() else {
        return result;
    };
    let dialog = &editor.dialog;
    let view = text_view(dialog);
    let buffer = view.buffer();
    for (key, value) in [
        ("title", json!(dialog.title().as_str())),
        ("can_close", json!(dialog.can_close())),
        ("content_width", json!(dialog.content_width())),
        ("content_height", json!(dialog.content_height())),
        ("text", json!(editor.text())),
        ("insert", json!(buffer.iter_at_mark(&buffer.get_insert()).offset())),
        ("bound", json!(buffer.iter_at_mark(&buffer.selection_bound()).offset())),
        ("accepts_tab", json!(view.accepts_tab())),
        ("monospace", json!(view.is_monospace())),
        ("editable", json!(view.is_editable())),
        ("labels", json!(labels(&dialog.child().unwrap(), data))),
        ("buttons", json!(buttons(&dialog.child().unwrap()))),
        ("alert", top.downcast_ref::<adw::AlertDialog>().map(|alert| json!({
            "heading":alert.heading().as_deref(),"body":alert.body().as_str(),
            "default":alert.default_response().as_deref(),"close":alert.close_response().as_str(),
            "labels":labels(&alert.child().unwrap(),data),"buttons":buttons(&alert.child().unwrap()),
        })).unwrap_or(Value::Null)),
    ] {
        result[key] = value;
    }
    result
}

fn assert_observation(actual: Value, expected: &Value, case: &str, step: usize) {
    let differences = expected
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| actual[*key] != expected[*key])
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert!(
        actual == *expected,
        "{case} step {step}: fields {differences:?}; labels {:?} != {:?}; buttons {:?} != {:?}",
        actual["labels"],
        expected["labels"],
        actual["buttons"],
        expected["buttons"]
    );
}

#[test]
#[ignore = "requires disposable display, bus, accessibility registry and data directories"]
fn prompt_edit_save_cancel_reset_conflicts_and_privacy_match_the_released_native_editor() {
    let root = std::env::var_os("OFFSCREEN_SESSION_ROOT")
        .map(PathBuf::from)
        .expect("private desktop helper required")
        .canonicalize()
        .unwrap();
    let data_root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap()
        .canonicalize()
        .unwrap();
    assert!(data_root.starts_with(&root));
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    let portals =
        PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap()).join("xdg-desktop-portal");
    fs::create_dir_all(&portals).unwrap();
    fs::write(portals.join("portals.conf"), "[preferred]\ndefault=gtk\n").unwrap();
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let reference: Reference =
        serde_json::from_str(include_str!("fixtures/prompt-editor-cases.json")).unwrap();
    assert_eq!(
        [
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ],
        reference.gtk
    );
    assert_eq!(gtk::pango::version_string().as_str(), reference.pango);
    assert_eq!(
        [
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        ],
        reference.libadwaita,
        "recollect the unchanged release's observations when Libadwaita changes"
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .unwrap();
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .build();
    window.set_content(Some(&gtk::Label::new(Some("Editor reference"))));
    window.present();
    let mut styles = DEFAULTS.styles.clone();
    styles.push(SavedStyle {
        identifier: "00000000-0000-4000-8000-000000000003".into(),
        name: "Review".into(),
        instructions: "Keep every fact and technical term.".into(),
        is_built_in: false,
    });
    for case in reference.cases {
        eprintln!("prompt_case={}", case.name);
        let data = data_root.join("native-prompts").join(&case.name);
        let directory = data.join("mluva/prompts");
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let file = directory.join(format!("{}.md", case.identifier));
        if let Some(bytes) = &case.file {
            fs::write(&file, bytes).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let store = Rc::new(RefCell::new(
            PromptStore::new(&directory, "Legacy custom instructions.", &styles).unwrap(),
        ));
        let changed = Rc::new(Cell::new(0));
        let closed = Rc::new(Cell::new(0));
        let writable = Rc::new(Cell::new(case.writable));
        let open = || {
            let notified = changed.clone();
            let allowed = writable.clone();
            let editor = PromptEditor::new(
                store.clone(),
                &case.identifier,
                Rc::new(move || notified.set(notified.get() + 1)),
                Rc::new(move || allowed.get()),
            )
            .unwrap();
            let notified = closed.clone();
            editor
                .dialog
                .connect_closed(move |_| notified.set(notified.get() + 1));
            editor.dialog.present(Some(&window));
            settle();
            editor
        };
        let mut editor = open();
        assert_observation(
            observation(&window, &editor, &data, &file, changed.get(), closed.get()),
            &case.observations[0],
            &case.name,
            0,
        );
        for (index, action) in case.actions.into_iter().enumerate() {
            match action {
                Action::Text { value } => text_view(&editor.dialog).buffer().set_text(&value),
                Action::Activate { label } => activate(&window, &label),
                Action::Writable { value } => writable.set(value),
                Action::External { value } => fs::write(&file, value).unwrap(),
                Action::Reopen => editor = open(),
            }
            settle();
            assert_observation(
                observation(&window, &editor, &data, &file, changed.get(), closed.get()),
                &case.observations[index + 1],
                &case.name,
                index + 1,
            );
        }
    }
    let directory = data_root.join("native-prompt-session/prompts");
    let store = Rc::new(RefCell::new(
        PromptStore::new(&directory, "Legacy custom instructions.", &DEFAULTS.styles).unwrap(),
    ));
    let changed = Rc::new(Cell::new(0));
    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let recorded = messages.clone();
    let notified = changed.clone();
    let session = PromptSession::new(
        store,
        Rc::new(|| true),
        Rc::new(move || notified.set(notified.get() + 1)),
        Rc::new(move |message| recorded.borrow_mut().push(message.into())),
    );
    let observe_session = || {
        let dialog = window.visible_dialog();
        let view = dialog.as_ref().and_then(|dialog| {
            widgets(&dialog.child().unwrap())
                .into_iter()
                .find_map(|widget| widget.downcast::<gtk::TextView>().ok())
        });
        let text = view.map(|view| {
            let buffer = view.buffer();
            buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string()
        });
        json!({
            "dialog_count":window.dialogs().n_items(),
            "title":dialog.as_ref().map(|dialog|dialog.title().to_string()),
            "heading":dialog.as_ref().and_then(|dialog|dialog.downcast_ref::<adw::AlertDialog>()).and_then(|alert|alert.heading()).map(|heading|heading.to_string()),
            "can_close":dialog.as_ref().map(|dialog|dialog.can_close()),
            "text":text,"messages":*messages.borrow(),"changed":changed.get(),
            "files":{"cleanup":fs::read(directory.join("cleanup.md")).ok(),"title":fs::read(directory.join("title.md")).ok()},
        })
    };
    assert_eq!(observe_session(), reference.session.observations[0]);
    for (index, action) in reference.session.actions.into_iter().enumerate() {
        match action {
            SessionAction::Open { identifier } => session.open(&window, &identifier),
            SessionAction::Text { value } => text_view(&window.visible_dialog().unwrap())
                .buffer()
                .set_text(&value),
            SessionAction::Activate { label } => activate(&window, &label),
        }
        settle();
        assert_eq!(
            observe_session(),
            reference.session.observations[index + 1],
            "prompt session step {}",
            index + 1
        );
    }
    window.destroy();
}
