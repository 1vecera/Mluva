//! Compare the actual native workspace with independent released GTK observations.

#![recursion_limit = "512"]

use adw::prelude::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use mluva_core::conversation::{ConversationStore, MERGE_MODEL};
use mluva_core::history::{HistoryInput, HistoryStore};
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;
use mluva_core::screenshots::ScreenshotStore;
use mluva_gtk::conversation_view::{ConversationCallbacks, ConversationWorkspace};
use mluva_gtk::document_layout::DocumentResources;
use mluva_gtk::markdown_view::MarkdownTextView;
use mluva_gtk::theme::ThemeController;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};
#[path = "support/conversation_stability.rs"]
mod conversation_stability;
#[path = "support/fixture_states.rs"]
mod fixture_states;

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(120);
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(4));
    }
}
fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn collect(widget: gtk::Widget, result: &mut Vec<gtk::Widget>) {
        result.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(c) = child {
            collect(c.clone(), result);
            child = c.next_sibling();
        }
    }
    let mut result = Vec::new();
    collect(widget.as_ref().clone(), &mut result);
    result
}
fn button(b: &gtk::Button) -> Value {
    json!({"label": b.label().map(String::from), "icon": b.icon_name().map(String::from), "visible": b.get_visible(), "sensitive": b.get_sensitive(), "tooltip": b.tooltip_text().map(String::from)})
}
fn document(v: &MarkdownTextView) -> Value {
    let (a, b, c, d) = v.measure(gtk::Orientation::Horizontal, -1);
    let request = match v.request_mode() {
        gtk::SizeRequestMode::HeightForWidth => "height-for-width",
        gtk::SizeRequestMode::WidthForHeight => "width-for-height",
        _ => "constant-size",
    };
    let vertical = [320, 680, 1040].map(|width| {
        let (a, b, c, d) = v.measure(gtk::Orientation::Vertical, width);
        [a, b, c, d]
    });
    json!({"text": v.text(), "editable": v.is_editable(), "request": request, "horizontal": [a,b,c,d], "vertical": vertical})
}
fn labels(widget: &impl IsA<gtk::Widget>) -> Vec<String> {
    widgets(widget)
        .into_iter()
        .filter_map(|w| {
            w.downcast::<gtk::Label>()
                .ok()
                .map(|l| l.label().to_string())
        })
        .collect()
}
fn observe(
    w: &ConversationWorkspace,
    ids: &[String],
    events: &RefCell<Vec<Value>>,
    window: &adw::Window,
) -> Value {
    let docs = w.documents();
    let mut copy = Vec::new();
    let mut save = Vec::new();
    for widget in widgets(&w.messages) {
        if let Ok(b) = widget.downcast::<gtk::Button>() {
            if b.tooltip_text().as_deref() == Some("Save edits") {
                save.push(button(&b));
            } else if b.tooltip_text().is_some_and(|t| t.starts_with("Copy ")) {
                copy.push(button(&b));
            }
        }
    }
    let rows = widgets(&w.history_list)
        .into_iter()
        .filter(|w| w.is::<gtk::ListBoxRow>())
        .count();
    let menus = widgets(&w.history_list)
        .into_iter()
        .filter_map(|w| {
            w.downcast::<gtk::MenuButton>()
                .ok()
                .map(|b| b.get_sensitive())
        })
        .collect::<Vec<_>>();
    let preview = widgets(&w.messages)
        .into_iter()
        .filter_map(|w| w.downcast::<MarkdownTextView>().ok())
        .filter(|v| !v.is_editable() && !v.can_target())
        .map(|v| v.text())
        .collect::<Vec<_>>();
    let stored = ids.iter().filter_map(|id| w.store.history.find(id).ok()).map(|e| json!({"raw": e.raw_text, "source": w.store.source_text(&e, false).unwrap(), "replies": w.store.replies(&e.identifier).unwrap().into_iter().map(|r| r.text).collect::<Vec<_>>()})).collect::<Vec<_>>();
    let dialog = window.visible_dialog().and_downcast::<adw::AlertDialog>().map(|d| json!({"heading": d.heading().map(String::from), "body": d.body().to_string(), "enabled": if d.heading().as_deref() == Some("Merge conversations") { Some(d.is_response_enabled("merge")) } else { None }, "labels": d.extra_child().map(|w| labels(&w)).unwrap_or_default()}));
    json!({"title": w.conversation_title.label().to_string(), "title_tooltip": w.conversation_title.tooltip_text().map(String::from), "rename_page": w.title_stack.visible_child_name().map(String::from), "title_entry": w.title_entry.text().to_string(), "title_error": w.title_entry.has_css_class("error"), "title_entry_tooltip": w.title_entry.tooltip_text().map(String::from), "entry": w.entry().and_then(|e| ids.iter().position(|id| *id == e.identifier)),
        "prompt": w.prompt_text(), "placeholder": w.prompt_placeholder.label().to_string(), "placeholder_visible": w.prompt_placeholder.get_visible(), "notice": w.notice.label().to_string(), "send": button(&w.send), "save": button(&w.save), "cancel": button(&w.cancel), "polish": button(&w.quick_polish), "structure": button(&w.structured_note), "continuation": button(&w.continue_button), "title_button": button(&w.title_button), "screenshots": w.screenshot_buttons.iter().map(button).collect::<Vec<_>>(),
        "documents": docs.iter().map(document).collect::<Vec<_>>(), "copy": copy, "document_save": save, "can_copy": w.can_copy_current_output(),
        "live": {"visible": w.live_box.get_visible(), "header": w.live_header.get_visible(), "navigation": button(&w.live_navigation), "source_visible": w.live_source_box.get_visible(), "draft_visible": w.live_draft_box.get_visible(), "divider_visible": w.live_divider.get_visible(), "source_toggle": w.source_toggle.is_active(), "draft_toggle": w.draft_toggle.is_active(), "toggle_sensitive": w.draft_toggle.get_sensitive(), "source": w.live_text.text(), "draft": w.live_draft_text.text(), "complete": w.live_draft(), "questions": w.live_questions.text(), "questions_visible": w.live_questions_scroll.get_visible(), "status": w.live_draft_status.label().to_string(), "title": w.live_title.label().to_string(), "light": w.live_light.widget.tooltip_text().map(String::from), "cancel_slot": w.live_cancel_slot.visible_child_name().map(String::from), "cancel_visible": w.live_cancel_slot.get_visible()},
        "viewing": {"heading": w.heading.get_visible(), "scroll": w.scroll.get_visible(), "composer": w.composer.get_visible()}, "history": {"count": rows, "labels": labels(&w.history_list), "more": w.more.get_visible(), "menus": menus}, "shelf": {"visible": w.screenshot_shelf.widget.get_visible(), "labels": labels(&w.screenshot_shelf.widget)}, "preview": preview, "events": events.borrow().clone(), "stored": stored, "dialog": dialog})
}

fn event_callback(events: &Rc<RefCell<Vec<Value>>>, name: &'static str) -> Rc<dyn Fn(&str)> {
    let events = events.clone();
    Rc::new(move |text| events.borrow_mut().push(json!([name, text])))
}

#[test]
#[ignore = "requires a private desktop, UTC, network namespace and disabled microphone/input devices"]
fn conversation_page_matches_released_editing_navigation_privacy_and_live_observations() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for p in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!std::path::Path::new(p).exists());
    }
    for key in ["HOME", "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(root.canonicalize().unwrap())
        );
    }
    assert_eq!(std::env::var("TZ").unwrap(), "UTC");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    let reference = fixture_states::load(
        include_str!("fixtures/released-conversation-page.json"),
        &["cases"],
        "observed",
    );
    assert_eq!(
        reference["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    assert_eq!(
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ]),
        reference["gtk"]
    );
    assert_eq!(
        gtk::pango::version_string().as_str(),
        reference["pango"].as_str().unwrap()
    );
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let data = root.join("conversation-native");
    std::fs::create_dir_all(&data).unwrap();
    let history = HistoryStore::new(data.join("history.sqlite3"));
    let store = ConversationStore::new(history.clone());
    store.initialize().unwrap();
    let mut ids = Vec::new();
    for (index, seed) in reference["seed"].as_array().unwrap().iter().enumerate() {
        let entry = history
            .add(HistoryInput {
                raw_text: seed["raw"].as_str().unwrap().into(),
                delivered_text: seed["delivered"].as_str().unwrap().into(),
                mode: seed["mode"].as_str().unwrap().into(),
                language_code: "eng".into(),
                delivery_outcome: "copied".into(),
                ..Default::default()
            })
            .unwrap();
        let id = format!("00000000-0000-4000-8000-{:012}", index + 1);
        rusqlite::Connection::open(&history.database.path)
            .unwrap()
            .execute(
                "UPDATE transcription_history SET identifier=?,created_at=? WHERE identifier=?",
                rusqlite::params![id, "2026-03-17T13:45:00+00:00", entry.identifier],
            )
            .unwrap();
        history
            .update_title(&id, Some(seed["title"].as_str().unwrap()))
            .unwrap();
        ids.push(id);
    }
    let polish = &DEFAULTS
        .prompts
        .iter()
        .find(|p| p.identifier == "rewrite-polish")
        .unwrap()
        .default;
    store
        .append(
            &ids[1],
            polish,
            "# Saved rewrite\n**Keep** every character.",
            "fixture",
        )
        .unwrap();
    store
        .append(&ids[1], "Merged conversations", "Joined text", MERGE_MODEL)
        .unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let handle = Rc::new(RefCell::new(None::<std::rc::Weak<ConversationWorkspace>>));
    let new_callbacks = || ConversationCallbacks {
        copy: event_callback(&events, "copy"),
        rewrite: event_callback(&events, "rewrite"),
        paste: event_callback(&events, "paste"),
        open_archive: Rc::new(|| {}),
        save_prompt: event_callback(&events, "save-prompt"),
        cancel_rewrite: Rc::new(|| {}),
        rename: Rc::new(|_, _| false),
        delete: Rc::new(|_| false),
        merge: Rc::new(|_, _| false),
        continue_recording: Rc::new(|_| {}),
        capture_screenshot: Some(Rc::new(|| {})),
        edit_screenshot: event_callback(&events, "edit-image"),
        remove_screenshot: event_callback(&events, "remove-image"),
        edit_prompt: event_callback(&events, "edit-prompt"),
    };
    let mut callbacks = new_callbacks();
    let e = events.clone();
    callbacks.open_archive = Rc::new(move || e.borrow_mut().push(json!(["archive"])));
    let e = events.clone();
    callbacks.cancel_rewrite = Rc::new(move || e.borrow_mut().push(json!(["cancel"])));
    let e = events.clone();
    callbacks.capture_screenshot =
        Some(Rc::new(move || e.borrow_mut().push(json!(["screenshot"]))));
    let e = events.clone();
    let id_list = ids.clone();
    callbacks.continue_recording = Rc::new(move |id| {
        e.borrow_mut().push(json!([
            "continue",
            id_list.iter().position(|value| value == id).unwrap()
        ]))
    });
    let e = events.clone();
    let id_list = ids.clone();
    let owner = handle.clone();
    let h = history.clone();
    callbacks.rename = Rc::new(move |id, title| {
        e.borrow_mut().push(json!([
            "rename",
            id_list.iter().position(|value| value == id).unwrap(),
            title
        ]));
        h.update_title(id, Some(title)).unwrap();
        owner
            .borrow()
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .refresh_title(id)
            .unwrap();
        true
    });
    let e = events.clone();
    let id_list = ids.clone();
    let owner = handle.clone();
    let h = history.clone();
    callbacks.delete = Rc::new(move |id| {
        e.borrow_mut().push(json!([
            "delete",
            id_list.iter().position(|value| value == id).unwrap()
        ]));
        h.delete(id).unwrap();
        let w = owner.borrow().as_ref().unwrap().upgrade().unwrap();
        w.show_conversation(None, &[], false).unwrap();
        w.refresh_history().unwrap();
        true
    });
    let e = events.clone();
    let id_list = ids.clone();
    let owner = handle.clone();
    let conversations = store.clone();
    callbacks.merge = Rc::new(move |source, target| {
        e.borrow_mut().push(json!([
            "merge",
            id_list.iter().position(|value| value == source).unwrap(),
            id_list.iter().position(|value| value == target).unwrap()
        ]));
        let entry = conversations.merge(target, source).unwrap();
        let w = owner.borrow().as_ref().unwrap().upgrade().unwrap();
        w.show_conversation(Some(entry), &conversations.replies(target).unwrap(), false)
            .unwrap();
        w.refresh_history().unwrap();
        true
    });
    let workspace = ConversationWorkspace::new(store, callbacks, resources.clone()).unwrap();
    *handle.borrow_mut() = Some(Rc::downgrade(&workspace));
    let w = &workspace;
    assert!(!w.composer.get_visible(), "manual rewrite starts collapsed");
    assert!(
        !w.prompt.is_cursor_visible(),
        "an empty placeholder has no caret"
    );
    w.rewrite_toggle.set_active(true);
    assert!(w.composer.get_visible());
    w.prompt.buffer().set_text("Keep this unsent request.");
    assert!(w.prompt.is_cursor_visible());
    assert!(!w.prompt_placeholder.get_visible());
    for expanded in [false, true] {
        w.rewrite_toggle.set_active(expanded);
        assert_eq!(w.composer.get_visible(), expanded);
        assert_eq!(w.prompt_text(), "Keep this unsent request.");
        let reopened = ConversationWorkspace::new(
            ConversationStore::new(history.clone()),
            new_callbacks(),
            resources.clone(),
        )
        .unwrap();
        assert_eq!(reopened.rewrite_toggle.is_active(), expanded);
        assert_eq!(reopened.composer.get_visible(), expanded);
    }
    w.prompt.buffer().set_text("");
    assert!(!w.prompt.is_cursor_visible());
    assert!(w.prompt_placeholder.get_visible());
    w.rewrite_toggle.set_active(false);
    let idle_notice = w.notice.label();
    w.set_busy(true, "Rewriting…");
    assert!(w.composer.get_visible() && w.cancel.get_visible());
    assert!(!w.rewrite_toggle.is_sensitive());
    assert!(!w.rewrite_toggle.is_active());
    w.set_busy(false, &idle_notice);
    assert!(!w.composer.get_visible());
    w.focus_prompt();
    assert!(w.rewrite_toggle.is_active());
    assert!(w.composer.get_visible());
    let mut cfg = w.config();
    cfg.rewrite_provider = "codex".into();
    w.set_config(cfg.clone()).unwrap();
    let prompts = Rc::new(RefCell::new(
        PromptStore::new(data.join("prompts"), "", &[]).unwrap(),
    ));
    w.set_prompt_store(Some(prompts.clone()));
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .content(&w.widget)
        .build();
    window.present();
    for (index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let a = &case["action"];
        let op = a["op"].as_str().unwrap();
        let selected_id = || &ids[a["entry"].as_u64().unwrap() as usize];
        let value = || a["value"].as_bool().unwrap();
        let source = || a["text"].as_str().unwrap();
        match op {
            "observe" => {}
            "prompt" => w.prompt.buffer().set_text(source()),
            "submit" => w.send.emit_clicked(),
            "show" => w
                .show_conversation(
                    Some(history.find(selected_id()).unwrap()),
                    &w.store.replies(selected_id()).unwrap(),
                    false,
                )
                .unwrap(),
            "edit" => w.documents()[a["index"].as_u64().unwrap() as usize]
                .buffer()
                .set_text(source()),
            "copy" => {
                w.copy_current_output();
            }
            "save-edits" => {
                w.save_edits(None);
            }
            "polish" => w.quick_polish.emit_clicked(),
            "override-polish" => {
                let store = prompts.borrow();
                let state = store.read("rewrite-polish").unwrap();
                store
                    .save(
                        "rewrite-polish",
                        "Locally edited polish instructions",
                        state.token.as_deref(),
                    )
                    .unwrap();
            }
            "busy" => w.set_busy(value(), if value() { "Working" } else { "Ready" }),
            "continue" => w.continue_button.emit_clicked(),
            "rename" => w.begin_rename(selected_id()).unwrap(),
            "title" => w.title_entry.set_text(source()),
            "save-title" => w.title_entry.emit_activate(),
            "preview" => w.set_rewrite_preview(selected_id(), source()),
            "clear-preview" => w.clear_rewrite_preview(),
            "live" => w.set_live("Recording", source(), true),
            "draft" => w.show_live_draft(source(), "Live note"),
            "toggle-source" => w.source_toggle.emit_clicked(),
            "toggle-draft" => w.draft_toggle.emit_clicked(),
            "show-live" => w.show_live(),
            "draft-edit" => w.live_draft_text.buffer().set_text(source()),
            "finish-live" => w.finish_live(),
            "hide-actions" => {
                cfg.show_copy_action = false;
                cfg.show_save_action = false;
                cfg.time_format = "12h".into();
                w.set_config(cfg.clone()).unwrap();
            }
            "private" => w.set_private(value()),
            "transient" => w.show_transient("Original incognito", source()).unwrap(),
            "saved-prompts" => {
                w.set_saved_prompts(&[("Saved custom".into(), "Custom instructions".into())])
            }
            "saved-choice" => {
                let button = widgets(&w.saved_prompts.popover().unwrap())
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some("Saved custom"))
                    .unwrap();
                button.emit_clicked();
            }
            "no-rewrite" => {
                cfg.rewrite_provider = "none".into();
                w.set_config(cfg.clone()).unwrap();
            }
            "config-reset" => {
                cfg = Default::default();
                cfg.rewrite_provider = "codex".into();
                w.set_config(cfg.clone()).unwrap();
            }
            "compact" => w.set_compact(value()),
            "search" => {
                w.search.set_text(source());
                w.search.emit_by_name::<()>("search-changed", &[]);
            }
            "images" => {
                let png = STANDARD
                    .decode(reference["image_png"].as_str().unwrap())
                    .unwrap();
                let images = ScreenshotStore::new(&history.database.path);
                images.add(&ids[1], &png, false, Some(62.5)).unwrap();
                images
                    .add(
                        "00000000-0000-4000-8000-000000000099",
                        &png,
                        true,
                        Some(63.5),
                    )
                    .unwrap();
                w.refresh_screenshots();
            }
            "image-context" => w.set_screenshot_context(
                Some("00000000-0000-4000-8000-000000000099".into()),
                Some(ids[1].clone()),
            ),
            "delete-dialog" => w.confirm_delete(selected_id()).unwrap(),
            "merge-dialog" => w.choose_merge(selected_id()).unwrap(),
            "merge-search" => {
                let dialog = window
                    .visible_dialog()
                    .and_downcast::<adw::AlertDialog>()
                    .unwrap();
                let search = dialog
                    .extra_child()
                    .unwrap()
                    .first_child()
                    .and_downcast::<gtk::SearchEntry>()
                    .unwrap();
                search.set_text(source());
                search.emit_by_name::<()>("search-changed", &[]);
            }
            "merge-select" => {
                let dialog = window
                    .visible_dialog()
                    .and_downcast::<adw::AlertDialog>()
                    .unwrap();
                let choices = widgets(&dialog.extra_child().unwrap())
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::ListBox>().ok())
                    .next()
                    .unwrap();
                choices.select_row(choices.row_at_index(0).as_ref());
            }
            "respond" => {
                let dialog = window.visible_dialog().unwrap();
                let label = match a["response"].as_str().unwrap() {
                    "cancel" => "Cancel",
                    "merge" => "Merge",
                    _ => panic!("unknown response"),
                };
                widgets(&dialog)
                    .into_iter()
                    .filter_map(|w| w.downcast::<gtk::Button>().ok())
                    .find(|b| b.label().as_deref() == Some(label))
                    .unwrap()
                    .emit_clicked();
            }
            "pagination" => {
                for i in 0..82 {
                    let text = format!("Page {i:02}");
                    let entry = history
                        .add(HistoryInput {
                            raw_text: text.clone(),
                            delivered_text: text,
                            mode: "dictation".into(),
                            language_code: "eng".into(),
                            delivery_outcome: "copied".into(),
                            ..Default::default()
                        })
                        .unwrap();
                    rusqlite::Connection::open(&history.database.path)
                        .unwrap()
                        .execute(
                            "UPDATE transcription_history SET created_at=? WHERE identifier=?",
                            rusqlite::params![
                                format!("2026-03-18T14:{:02}:{:02}+00:00", i / 60, i % 60),
                                entry.identifier
                            ],
                        )
                        .unwrap();
                }
                w.refresh_history().unwrap();
            }
            "more" => w.more.emit_clicked(),
            "empty-title" => {
                history.update_title(selected_id(), Some("")).unwrap();
                w.refresh_title(selected_id()).unwrap();
            }
            _ => panic!("unknown reference action {op}"),
        }
        settle();
        if op == "merge-search" {
            settle();
            settle();
        }
        let actual = observe(w, &ids, &events, &window);
        if actual != case["observed"] {
            std::fs::write(
                root.join("conversation-mismatch.json"),
                serde_json::to_vec_pretty(
                    &json!({"index":index,"action":a,"actual":actual,"expected":case["observed"]}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        let differences = actual
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| actual[*key] != case["observed"][*key])
            .collect::<Vec<_>>();
        assert!(
            differences.is_empty(),
            "conversation action {index}: {op}; differing fields: {differences:?}; see conversation-mismatch.json"
        );
        eprintln!("conversation_action={index} {op} PASS");
    }
    exercise_scrolling(w);
    conversation_stability::exercise(w, &window, &root);
    window.close();
    drop(workspace);
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
        rewrite_provider: "codex".into(),
        ..Default::default()
    };
    w.set_config(cfg.clone()).unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let mut capture = None;
    for (index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let action = &case["action"];
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
                cfg.smooth_scrolling = op != "smooth-off";
                cfg.scroll_duration_ms = if op == "duration-zero" { 0 } else { 800 };
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
                    rewrite_provider: "codex".into(),
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
        let actual = scroll_observation(w, &positions, before);
        assert_eq!(actual, case["expected"], "scroll action {index} {op}");
        eprintln!("native_scroll={index} {op} PASS");
    }
    w.finish_live();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
}
