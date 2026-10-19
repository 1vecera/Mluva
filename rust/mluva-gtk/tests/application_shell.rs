//! Palette commands bind the current editor and capture state at dispatch time.
use adw::prelude::*;
use mluva_gtk::{
    application_shell::register_actions,
    commands::{CommandContext, CommandState, application_commands},
    settings_view::SettingsView,
};
use std::{cell::RefCell, rc::Rc};
#[path = "support/dictation_ui.rs"]
mod ui;
#[test]
#[ignore = "requires a private GTK/session bus"]
fn current_commands_reject_stale_documents_and_removed_review_actions() {
    ui::isolated();
    let editor = gtk::TextView::new();
    let state = Rc::new(RefCell::new(CommandState {
        page: "capture".into(),
        document: Some("one".into()),
        editor: Some(editor.upcast()),
        can_copy: true,
        record_sensitive: true,
        edit_draft: true,
        ..Default::default()
    }));
    let current = state.clone();
    let invoked = Rc::new(RefCell::new(vec![]));
    let calls = invoked.clone();
    let context = Rc::new(CommandContext {
        state: Rc::new(move || current.borrow().clone()),
        invoke: Rc::new(move |action| calls.borrow_mut().push(action)),
    });
    let settings = SettingsView::new(Rc::new(|| {}));
    let commands = application_commands(&context, &settings, &[]);
    let command = |name: &str| commands.iter().find(|c| c.title == name).unwrap();
    for title in ["Copy current text", "Save edits", "Start dictation"] {
        assert!((command(title).enabled)());
    }
    state.borrow_mut().document = Some("two".into());
    assert!(!(command("Copy current text").enabled)());
    assert!(!(command("Save edits").enabled)());
    state.borrow_mut().recording = true;
    assert!(!(command("Start dictation").enabled)());
    for old in ["Polish", "Rewrite", "Notes", "Command", "Live Rewrite"] {
        assert!(!commands.iter().any(|c| c.title == old));
    }
    let app = adw::Application::builder()
        .application_id("com.mluva.CommandAcceptance")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    register_actions(&app, Rc::new(|_| {}));
    assert!(app.lookup_action("review").is_none());
    assert!(app.lookup_action("record").is_some());
    assert!(app.lookup_action("latest").is_some());
    app.quit();
}
