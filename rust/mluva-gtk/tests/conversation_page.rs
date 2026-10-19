//! One editor: source-preserving saves, saved legacy replies and smooth live following.
use adw::prelude::*;
use mluva_core::{
    conversation::ConversationStore,
    history::{HistoryInput, HistoryStore},
};
use mluva_gtk::{
    conversation_view::{ConversationCallbacks, ConversationWorkspace},
    document_layout::DocumentResources,
    theme::ThemeController,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    thread,
    time::{Duration, Instant},
};
#[path = "support/dictation_ui.rs"]
mod ui;
fn settle() {
    ui::settle(0.1)
}

#[test]
#[ignore = "requires the private desktop, clipboard and network namespace"]
fn one_editor_preserves_unsaved_edits_legacy_replies_and_live_navigation() {
    let root = ui::isolated();
    let history = HistoryStore::new(root.join("editor.sqlite3"));
    history.initialize().unwrap();
    let store = ConversationStore::new(history.clone());
    let first = history
        .add(HistoryInput::dictation(
            "Immutable original",
            "Delivered result",
        ))
        .unwrap();
    let second = history
        .add(HistoryInput::dictation(
            "Another original",
            "Another original",
        ))
        .unwrap();
    let reply = store
        .append(
            &first.identifier,
            "Old rewrite instruction",
            "Historical result",
            "saved-model",
        )
        .unwrap();
    let copied = Rc::new(RefCell::new(Vec::<String>::new()));
    let copy = copied.clone();
    let rename_history = history.clone();
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let w = ConversationWorkspace::new(
        store.clone(),
        ConversationCallbacks {
            copy: Rc::new(move |text| copy.borrow_mut().push(text.into())),
            open_archive: Rc::new(|| {}),
            rename: Rc::new(move |id, title| rename_history.update_title(id, Some(title)).is_ok()),
            delete: Rc::new(|_| true),
            merge: Rc::new(|_, _| true),
            continue_recording: Rc::new(|_| {}),
            capture_screenshot: None,
            edit_screenshot: Rc::new(|_| {}),
            remove_screenshot: Rc::new(|_| {}),
        },
        resources,
    )
    .unwrap();
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .content(&w.widget)
        .build();
    window.present();
    w.show_conversation(Some(first.clone()), std::slice::from_ref(&reply), false)
        .unwrap();
    settle();
    assert_eq!(w.documents().len(), 2);
    for card in ui::widgets(&w.messages)
        .into_iter()
        .filter(|x| x.has_css_class("ml-source") || x.has_css_class("ml-reply"))
    {
        assert!(ui::button(&card, "Copy").get_visible());
    }
    w.documents()[0].buffer().set_text("Unsaved working edit");
    assert!(w.has_unsaved_edits(&first.identifier));
    w.show_conversation(Some(second.clone()), &[], false)
        .unwrap();
    w.show_conversation(Some(first.clone()), std::slice::from_ref(&reply), false)
        .unwrap();
    assert_eq!(w.documents()[0].text(), "Unsaved working edit");
    ui::until(|| !w.has_unsaved_edits(&first.identifier));
    assert!(!w.has_unsaved_edits(&first.identifier));
    assert_eq!(
        store.source_text(&first, false).unwrap(),
        "Unsaved working edit"
    );
    assert_eq!(
        history.find(&first.identifier).unwrap().raw_text,
        "Immutable original"
    );
    assert_eq!(
        history.find(&first.identifier).unwrap().delivered_text,
        "Delivered result"
    );
    assert!(w.copy_current_output());
    assert_eq!(copied.borrow().last().unwrap(), "Historical result");
    assert_eq!(
        store.replies(&first.identifier).unwrap(),
        std::slice::from_ref(&reply)
    );
    // Autosave updates the existing view without resetting the caret or raw recognition.
    let editor = w.documents()[0].clone();
    editor.buffer().set_text("Latest typing wins");
    editor
        .buffer()
        .place_cursor(&editor.buffer().iter_at_offset(6));
    ui::until(|| !w.has_unsaved_edits(&first.identifier));
    assert_eq!(editor.buffer().cursor_position(), 6);
    assert_eq!(w.documents()[0], editor);
    w.copy_path.emit_clicked();
    let path = PathBuf::from(copied.borrow().last().unwrap());
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("Latest typing wins")
    );
    // A real SQLite failure keeps the edit visible and retries it automatically.
    let connection = rusqlite::Connection::open(&history.database.path).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_edit BEFORE INSERT ON conversation_sources BEGIN SELECT RAISE(ABORT, 'disk failure'); END;").unwrap();
    editor.buffer().set_text("Kept through save failure");
    ui::until(|| w.notice.label().contains("Could not save"));
    assert!(w.has_unsaved_edits(&first.identifier));
    assert_eq!(editor.text(), "Kept through save failure");
    connection
        .execute_batch("DROP TRIGGER reject_edit;")
        .unwrap();
    ui::until(|| !w.has_unsaved_edits(&first.identifier));
    assert_eq!(
        store.source_text(&first, false).unwrap(),
        "Kept through save failure"
    );
    editor.buffer().set_text("");
    ui::until(|| !w.has_unsaved_edits(&first.identifier));
    assert_eq!(store.source_text(&first, false).unwrap(), "");
    assert_eq!(
        history.find(&first.identifier).unwrap().raw_text,
        "Immutable original"
    );
    // A pre-existing title edit survives failed storage and a temporary private view.
    connection.execute_batch("CREATE TRIGGER reject_title BEFORE UPDATE OF title ON transcription_history BEGIN SELECT RAISE(ABORT, 'disk failure'); END;").unwrap();
    w.begin_rename(&first.identifier).unwrap();
    w.title_entry.set_text("Title kept through private viewing");
    w.set_private(true);
    w.show_transient("Private text", "Private text").unwrap();
    assert!(w.has_unsaved_edits(&first.identifier));
    assert_eq!(history.find(&first.identifier).unwrap().title, None);
    connection
        .execute_batch("DROP TRIGGER reject_title;")
        .unwrap();
    w.set_private(false);
    ui::until(|| !w.has_unsaved_edits(&first.identifier));
    assert_eq!(
        history.find(&first.identifier).unwrap().title.as_deref(),
        Some("Title kept through private viewing")
    );
    w.begin_rename(&first.identifier).unwrap();
    w.title_entry.set_text("");
    w.set_private(true);
    w.show_transient("Another private thought", "Another private thought")
        .unwrap();
    w.set_private(false);
    assert!(!w.has_unsaved_edits(&first.identifier));
    assert_eq!(
        history.find(&first.identifier).unwrap().title.as_deref(),
        Some("Title kept through private viewing")
    );
    w.set_live("Recording", "New words arriving", true);
    settle();
    assert_eq!(w.live_text.text(), "New words arriving");
    assert!(!w.copy_current_output());
    w.show_conversation(Some(second), &[], false).unwrap();
    assert!(!w.is_viewing_live());
    w.show_live();
    assert!(w.is_viewing_live());
    assert_eq!(w.live_text.text(), "New words arriving");
    exercise_scrolling(&w);
    w.set_private(true);
    w.show_transient("Private recognition", "Private recognition")
        .unwrap();
    assert!(!w.copy_path.get_sensitive());
    assert!(ui::button(&w.messages, "Copy").get_visible());
    assert_eq!(history.recent(10).unwrap().len(), 2);
    window.close();
    settle();
}
fn scroll_frames(w: &ConversationWorkspace, seconds: f64) -> Vec<f64> {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    let context = glib::MainContext::default();
    let mut positions = Vec::new();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        positions.push(w.live_scroll.vadjustment().value());
        thread::sleep(Duration::from_millis(10));
    }
    positions
}

fn scroll_observation(w: &ConversationWorkspace, positions: &[f64], before: f64) -> Value {
    let a = w.live_scroll.vadjustment();
    json!({"at_end": (a.value()+a.page_size()-a.upper()).abs()<1.0,
        "at_origin": positions.iter().copied().fold(f64::NEG_INFINITY,f64::max)==0.0,
        "intermediate": positions.iter().any(|&value| before+1.0<value && value<a.value()-1.0),
        "manual_90": (a.value()-90.0).abs()<1.0,"top": w.live_text.top_margin(),
        "bottom_at_least_4": w.live_text.bottom_margin()>=4,"following": w.live_follower.following.get(),"text": w.live_text.text()})
}

fn exercise_scrolling(w: &Rc<ConversationWorkspace>) {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/released-conversation-scrolling.json"
    ))
    .unwrap();
    assert_eq!(
        reference["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    w.finish_live();
    w.set_screenshot_context(None, None);
    w.show_conversation(None, &[], false).unwrap();
    let mut cfg = mluva_core::config::AppConfig {
        ..Default::default()
    };
    w.set_config(cfg.clone()).unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let mut capture = None;
    for (index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let action = &case["action"];
        if matches!(action["op"].as_str(), Some("smooth-off" | "duration-zero")) {
            continue;
        }
        let op = action["op"].as_str().unwrap();
        let mut before = 0.0;
        let source = || action["text"].as_str().unwrap();
        let duration = match op {
            "short" => {
                let next = action["capture"].as_u64().unwrap();
                if capture != Some(next) {
                    w.finish_live();
                    w.set_live("Recording", "", true);
                    scroll_frames(w, 0.12);
                    capture = Some(next);
                }
                let text = reference["short"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .take(action["count"].as_u64().unwrap() as usize)
                    .collect::<String>();
                w.set_live("Recording", &text, true);
                0.15
            }
            "long" => {
                w.finish_live();
                scroll_frames(w, 0.12);
                w.set_live("Recording", source(), true);
                1.05
            }
            "append" => {
                before = w.live_scroll.vadjustment().value();
                w.set_live("Recording", source(), true);
                1.05
            }
            "manual" => {
                w.live_scroll.vadjustment().set_value(90.0);
                w.set_live("Recording", source(), true);
                1.05
            }
            "correction" => {
                w.set_live("Recording", source(), true);
                1.05
            }
            "large-correction" => {
                w.live_follower.follow(false);
                scroll_frames(w, 1.05);
                w.set_live("Recording", source(), true);
                1.05
            }
            "smooth-off" | "duration-zero" | "reduced-motion" => {
                w.set_config(cfg.clone()).unwrap();
                gtk::Settings::default()
                    .unwrap()
                    .set_gtk_enable_animations(op != "reduced-motion");
                w.live_follower.follow(true);
                scroll_frames(w, 0.1);
                w.set_live("Recording", source(), true);
                0.15
            }
            "fresh" => {
                w.finish_live();
                cfg = mluva_core::config::AppConfig {
                    ..Default::default()
                };
                w.set_config(cfg.clone()).unwrap();
                gtk::Settings::default()
                    .unwrap()
                    .set_gtk_enable_animations(true);
                w.set_live("Recording", source(), true);
                1.05
            }
            _ => panic!("unknown scrolling action {op}"),
        };
        let positions = scroll_frames(w, duration);
        let mut actual = scroll_observation(w, &positions, before);
        let mut expected = case["expected"].clone();
        if op == "reduced-motion" {
            // The removed smooth-off/duration-zero actions no longer precede this
            // case. Test arrival without animation, not their leftover spacer.
            actual.as_object_mut().unwrap().remove("top");
            expected.as_object_mut().unwrap().remove("top");
        }
        assert_eq!(actual, expected, "scroll action {index} {op}");
        eprintln!("native_scroll={index} {op} PASS");
    }
    w.finish_live();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
}
