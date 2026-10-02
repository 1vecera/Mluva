//! The actual editable conversation and capture workspace, backed by compatible stores.

use crate::document_layout::{DocumentResources, TailFollower, document_scroll, margins};
use crate::markdown_view::MarkdownTextView;
use crate::mermaid::MermaidPreview;
use crate::prompt_editor::prompt_control;
use crate::recording_light::RecordingLight;
use crate::screenshot_shelf::ScreenshotShelf;
use adw::prelude::*;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use mluva_core::config::AppConfig;
use mluva_core::conversation::{ConversationStore, MERGE_MODEL, Rewrite};
use mluva_core::database::{StoreError, StoreResult};
use mluva_core::history::HistoryEntry;
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;
use mluva_core::screenshots::ScreenshotStore;
use mluva_core::{text, titles::fallback_title};
use regex::Regex;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::LazyLock;

pub type TextAction = Rc<dyn Fn(&str)>;
pub type NamedAction = Rc<dyn Fn(&str, &str) -> bool>;
pub type SingleAction = Rc<dyn Fn(&str) -> bool>;
pub type Action = Rc<dyn Fn()>;
type EditKey = (String, Option<i64>);
fn invalid(message: &str) -> StoreError {
    StoreError::Invalid(message.into())
}

fn display_title(entry: &HistoryEntry) -> String {
    entry
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .map_or_else(|| fallback_title(&entry.raw_text), str::to_owned)
}

pub struct ConversationCallbacks {
    pub copy: TextAction,
    pub rewrite: TextAction,
    pub paste: TextAction,
    pub open_archive: Action,
    pub save_prompt: TextAction,
    pub cancel_rewrite: Action,
    pub rename: NamedAction,
    pub delete: SingleAction,
    pub merge: NamedAction,
    pub continue_recording: TextAction,
    pub capture_screenshot: Option<Action>,
    pub edit_screenshot: TextAction,
    pub remove_screenshot: TextAction,
    pub edit_prompt: TextAction,
}

pub fn history_timestamp(value: &str, format: &str) -> StoreResult<String> {
    let local = if let Ok(time) = DateTime::parse_from_rfc3339(value) {
        time.with_timezone(&Local)
    } else {
        let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
            .map_err(|_| invalid("History contains an invalid timestamp"))?;
        Local
            .from_local_datetime(&naive)
            .earliest()
            .ok_or_else(|| invalid("History contains an invalid timestamp"))?
    };
    let clock = if format == "24h" {
        local.format("%H:%M").to_string()
    } else {
        local
            .format("%I:%M %p")
            .to_string()
            .trim_start_matches('0')
            .into()
    };
    Ok(format!("{} · {clock}", local.format("%a %-d %b")))
}

pub fn split_grilling_draft(source: &str) -> (&str, &str) {
    static HEADER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)\A[\s\x1c-\x1f]*#{1,2} Questions[\s\x1c-\x1f]*\n").unwrap()
    });
    static ARCHITECTURE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^#{1,2} Architecture[\s\x1c-\x1f]*$").unwrap());
    let Some(header) = HEADER.find(source) else {
        return ("", source);
    };
    let end = ARCHITECTURE
        .find_at(source, header.end())
        .map_or(source.len(), |m| m.start());
    source.split_at(end)
}

fn builtin_prompt(identifier: &str) -> &'static str {
    &DEFAULTS
        .prompts
        .iter()
        .find(|p| p.identifier == identifier)
        .expect("reviewed built-in prompt")
        .default
}

#[derive(Default)]
struct WorkspaceState {
    entry: Option<HistoryEntry>,
    busy: bool,
    private: bool,
    renaming: Option<String>,
    live_active: bool,
    viewing_live: bool,
    live_draft_available: bool,
    live_questions_source: String,
    live_capture: Option<String>,
    live_conversation: Option<String>,
    drafts: BTreeMap<String, String>,
    edit_drafts: BTreeMap<EditKey, String>,
    editors: BTreeMap<EditKey, MarkdownTextView>,
    documents: Vec<MarkdownTextView>,
    copy_buttons: Vec<gtk::Button>,
    save_buttons: Vec<gtk::Button>,
    saved_prompt_buttons: Vec<gtk::Button>,
    rows: Vec<(gtk::ListBoxRow, String)>,
    row_titles: BTreeMap<String, gtk::Label>,
    row_menus: Vec<gtk::MenuButton>,
    previews: Vec<Rc<MermaidPreview>>,
    preview_identifier: Option<String>,
    preview_text: String,
    preview_box: Option<gtk::Box>,
    preview_editor: Option<MarkdownTextView>,
    search_limit: i64,
}

pub struct ConversationWorkspace {
    pub widget: gtk::Box,
    pub split: adw::OverlaySplitView,
    pub content: gtk::Box,
    pub history_list: gtk::ListBox,
    pub search: gtk::SearchEntry,
    pub more: gtk::Button,
    pub live_navigation: gtk::Button,
    pub conversation_title: gtk::Label,
    pub title_button: gtk::Button,
    pub title_stack: gtk::Stack,
    pub title_entry: gtk::Entry,
    pub title_save: gtk::Button,
    pub continue_button: gtk::Button,
    pub heading: gtk::Box,
    pub messages: gtk::Box,
    pub scroll: gtk::ScrolledWindow,
    pub conversation_follower: Rc<TailFollower>,
    pub live_header: gtk::Box,
    pub live_light: Rc<RecordingLight>,
    pub live_title: gtk::Label,
    pub live_cancel: gtk::Button,
    pub live_cancel_slot: gtk::Stack,
    pub source_toggle: gtk::ToggleButton,
    pub draft_toggle: gtk::ToggleButton,
    pub live_box: gtk::Box,
    pub live_source_box: gtk::Box,
    pub live_text: MarkdownTextView,
    pub live_scroll: gtk::ScrolledWindow,
    pub live_follower: Rc<TailFollower>,
    pub live_divider: gtk::Separator,
    pub live_draft_box: gtk::Box,
    pub live_draft_status: gtk::Label,
    pub live_questions: MarkdownTextView,
    pub live_questions_scroll: gtk::ScrolledWindow,
    pub live_draft_text: MarkdownTextView,
    pub live_draft_scroll: gtk::ScrolledWindow,
    pub live_draft_follower: Rc<TailFollower>,
    pub screenshot_shelf: ScreenshotShelf,
    pub screenshot_buttons: [gtk::Button; 2],
    pub composer: gtk::Box,
    pub actions: gtk::FlowBox,
    pub quick_polish: gtk::Button,
    pub structured_note: gtk::Button,
    pub saved_prompts: gtk::MenuButton,
    pub prompt_label: gtk::Label,
    pub prompt: gtk::TextView,
    pub prompt_placeholder: gtk::Label,
    pub notice: gtk::Label,
    pub save: gtk::Button,
    pub cancel: gtk::Button,
    pub send: gtk::Button,
    pub store: ConversationStore,
    callbacks: ConversationCallbacks,
    resources: DocumentResources,
    config: RefCell<AppConfig>,
    state: RefCell<WorkspaceState>,
    prompt_store: RefCell<Option<Rc<RefCell<PromptStore>>>>,
    screenshot_store: ScreenshotStore,
    live_source_visible: Cell<bool>,
    live_draft_visible: Cell<bool>,
    _live_diagrams: Rc<MermaidPreview>,
}

impl ConversationWorkspace {
    pub fn new(
        store: ConversationStore,
        callbacks: ConversationCallbacks,
        resources: DocumentResources,
    ) -> StoreResult<Rc<Self>> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let config = AppConfig::default();
        let split = adw::OverlaySplitView::builder()
            .vexpand(true)
            .min_sidebar_width(200.0)
            .max_sidebar_width(232.0)
            .sidebar_width_fraction(0.23)
            .show_sidebar(config.history_sidebar_visible)
            .build();
        widget.append(&split);
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        sidebar.add_css_class("ml-history-sidebar");
        margins(&sidebar, 16);
        let brand = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        brand.set_size_request(-1, 32);
        brand.append(&resources.brand_mark(20));
        brand.append(
            &gtk::Label::builder()
                .label("Mluva")
                .xalign(0.0)
                .css_classes(["ml-wordmark"])
                .build(),
        );
        sidebar.append(&brand);
        let new_conversation = gtk::Button::builder()
            .label("New conversation")
            .tooltip_text("Start with dictation or pasted text")
            .css_classes(["ml-new-conversation"])
            .build();
        sidebar.append(&new_conversation);
        let live_navigation = gtk::Button::builder()
            .label("Live conversation")
            .has_frame(false)
            .visible(false)
            .tooltip_text("Return to the recording and its live draft")
            .build();
        sidebar.append(&live_navigation);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search history")
            .build();
        sidebar.append(&search);
        let history_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();
        sidebar.append(&document_scroll(&history_list, true));
        let more = gtk::Button::with_label("Show more");
        sidebar.append(&more);
        let archive = gtk::Button::builder()
            .label("Manage history")
            .has_frame(false)
            .height_request(32)
            .tooltip_text("Rename, export, recover or delete dictations")
            .build();
        sidebar.append(&archive);
        let sidebar_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sidebar_pane.add_css_class("ml-history-pane");
        sidebar_pane.append(&sidebar);
        split.set_sidebar(Some(&sidebar_pane));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.add_css_class("ml-conversation");
        split.set_content(Some(&content));
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        margins(&heading, 16);
        heading.set_margin_bottom(8);
        heading.set_size_request(-1, 32);
        let conversation_title = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["ml-conversation-title"])
            .build();
        let title_button = gtk::Button::builder()
            .child(&conversation_title)
            .has_frame(false)
            .hexpand(true)
            .tooltip_text("Rename conversation")
            .build();
        let title_stack = gtk::Stack::builder()
            .hexpand(true)
            .hhomogeneous(false)
            .vhomogeneous(false)
            .build();
        title_stack.add_named(&title_button, Some("title"));
        let title_editor = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let title_entry = gtk::Entry::builder().hexpand(true).width_chars(1).build();
        title_entry.update_property(&[gtk::accessible::Property::Label("Conversation title")]);
        title_editor.append(&title_entry);
        let title_save = gtk::Button::builder()
            .icon_name("object-select-symbolic")
            .tooltip_text("Save title")
            .build();
        title_editor.append(&title_save);
        let title_cancel = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Cancel rename")
            .build();
        title_editor.append(&title_cancel);
        title_stack.add_named(&title_editor, Some("edit"));
        heading.append(&title_stack);
        let continue_button = gtk::Button::builder()
            .label("Continue recording")
            .tooltip_text("Continue recording · Add speech to this conversation")
            .build();
        continue_button.update_property(&[gtk::accessible::Property::Label("Continue recording")]);
        heading.append(&continue_button);
        let screenshot_buttons = std::array::from_fn(|_| {
            let button = gtk::Button::builder()
                .icon_name("camera-photo-symbolic")
                .tooltip_text("Add screenshot · F10 · Experimental")
                .build();
            button.update_property(&[gtk::accessible::Property::Label("Add screenshot")]);
            button.set_sensitive(callbacks.capture_screenshot.is_some());
            button
        });
        heading.append(&screenshot_buttons[0]);
        content.append(&heading);
        let screenshot_shelf = ScreenshotShelf::new(
            callbacks.edit_screenshot.clone(),
            callbacks.remove_screenshot.clone(),
        );
        content.append(&screenshot_shelf.widget);
        let messages = gtk::Box::new(gtk::Orientation::Vertical, 16);
        margins(&messages, 16);
        messages.set_margin_top(0);
        let scroll = document_scroll(&messages, true);
        content.append(&scroll);
        let conversation_follower = TailFollower::new(&scroll, None);
        let live_header = gtk::Box::builder()
            .spacing(8)
            .valign(gtk::Align::Center)
            .visible(false)
            .build();
        margins(&live_header, 16);
        live_header.set_margin_bottom(0);
        let live_light = RecordingLight::new();
        live_header.append(&live_light.widget);
        let live_title = gtk::Label::builder()
            .xalign(1.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["caption"])
            .build();
        let live_cancel = gtk::Button::builder()
            .label("Cancel")
            .has_frame(false)
            .valign(gtk::Align::Center)
            .tooltip_text("Cancel the final rewrite; keep the original dictation")
            .build();
        live_cancel.update_property(&[gtk::accessible::Property::Label("Cancel final rewrite")]);
        let live_cancel_slot = gtk::Stack::builder()
            .hhomogeneous(false)
            .vhomogeneous(true)
            .visible(false)
            .build();
        live_cancel_slot.add_named(
            &gtk::Box::new(gtk::Orientation::Horizontal, 0),
            Some("idle"),
        );
        live_cancel_slot.add_named(&live_cancel, Some("cancel"));
        live_header.append(&live_cancel_slot);
        live_header.append(&live_title);
        live_header.append(&screenshot_buttons[1]);
        let source_toggle = gtk::ToggleButton::builder()
            .label("Original")
            .active(true)
            .has_frame(false)
            .tooltip_text("Show or hide original · Ctrl+1")
            .build();
        let draft_toggle = gtk::ToggleButton::builder()
            .label("Live draft")
            .active(true)
            .has_frame(false)
            .tooltip_text("Show or hide draft · Ctrl+2")
            .build();
        live_header.append(&source_toggle);
        live_header.append(&draft_toggle);
        content.append(&live_header);
        let live_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .vexpand(true)
            .visible(false)
            .css_classes(["ml-live"])
            .build();
        margins(&live_box, 16);
        let live_source_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .hexpand(true)
            .css_classes(["ml-live-original"])
            .build();
        let live_text = MarkdownTextView::new("", false, false);
        live_text.update_property(&[gtk::accessible::Property::Label("Dictation")]);
        let live_scroll = document_scroll(&live_text, true);
        live_source_box.append(&live_scroll);
        live_box.append(&live_source_box);
        let live_follower = TailFollower::new(&live_scroll, Some(live_text.upcast_ref()));
        let live_divider = gtk::Separator::builder()
            .orientation(gtk::Orientation::Vertical)
            .visible(false)
            .css_classes(["ml-pane-divider"])
            .build();
        live_box.append(&live_divider);
        let live_draft_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .hexpand(true)
            .visible(false)
            .css_classes(["ml-live-draft"])
            .build();
        let live_draft_status = gtk::Label::builder()
            .label("Live draft")
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["heading"])
            .visible(false)
            .build();
        live_draft_box.append(&live_draft_status);
        let live_questions = MarkdownTextView::document("", true, false);
        live_questions.add_css_class("ml-live-questions");
        live_questions.set_top_margin(0);
        live_questions.set_bottom_margin(0);
        live_questions.set_visible(false);
        live_questions.update_property(&[gtk::accessible::Property::Label(
            "Questions to consider as you speak",
        )]);
        let live_questions_scroll = document_scroll(&live_questions, false);
        live_questions_scroll.set_propagate_natural_height(true);
        live_questions_scroll.set_max_content_height(180);
        live_questions_scroll.set_visible(false);
        live_draft_box.append(&live_questions_scroll);
        let live_draft_text = MarkdownTextView::document("", true, true);
        live_draft_text.update_property(&[gtk::accessible::Property::Label("Live draft")]);
        let live_draft_document = gtk::Box::new(gtk::Orientation::Vertical, 0);
        live_draft_document.append(&live_draft_text);
        let live_diagrams = MermaidPreview::new(&live_draft_text, &resources);
        live_draft_document.append(&live_diagrams.widget);
        let live_draft_scroll = document_scroll(&live_draft_document, true);
        live_draft_box.append(&live_draft_scroll);
        live_box.append(&live_draft_box);
        let live_draft_follower =
            TailFollower::new(&live_draft_scroll, Some(live_draft_text.upcast_ref()));
        content.append(&live_box);
        let composer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        composer.add_css_class("ml-composer");
        margins(&composer, 16);
        composer.set_margin_top(8);
        composer.set_margin_bottom(8);
        let actions = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(8)
            .row_spacing(8)
            .max_children_per_line(4)
            .min_children_per_line(1)
            .halign(gtk::Align::Start)
            .build();
        let quick_polish = gtk::Button::builder()
            .label("Polish")
            .tooltip_text("Remove filler words and fix phrasing while keeping your meaning")
            .build();
        let structured_note = gtk::Button::builder()
            .label("Structure")
            .tooltip_text("A concise summary followed by organized bullet points")
            .build();
        let edit = callbacks.edit_prompt.clone();
        actions.insert(
            &prompt_control(
                &quick_polish,
                "Polish",
                Rc::new(move || edit("rewrite-polish")),
            ),
            -1,
        );
        let edit = callbacks.edit_prompt.clone();
        actions.insert(
            &prompt_control(
                &structured_note,
                "Structure",
                Rc::new(move || edit("rewrite-structure")),
            ),
            -1,
        );
        let saved_prompts = gtk::MenuButton::builder().label("More").build();
        actions.insert(&saved_prompts, -1);
        composer.append(&actions);
        let prompt_label = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["caption"])
            .build();
        composer.append(&prompt_label);
        let prompt = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .accepts_tab(false)
            .css_classes(["ml-prompt"])
            .tooltip_text("Write a custom instruction, then press Ctrl+Enter to send")
            .build();
        let prompt_scroll = document_scroll(&prompt, false);
        prompt_scroll.set_min_content_height(40);
        prompt_scroll.set_max_content_height(120);
        prompt_scroll.set_propagate_natural_height(true);
        let overlay = gtk::Overlay::builder().child(&prompt_scroll).build();
        let prompt_placeholder = gtk::Label::builder()
            .label("Ask for a rewrite…")
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Center)
            .margin_start(10)
            .can_target(false)
            .css_classes(["dim-label"])
            .build();
        overlay.add_overlay(&prompt_placeholder);
        composer.append(&overlay);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let notice = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .hexpand(true)
            .accessible_role(gtk::AccessibleRole::Status)
            .css_classes(["caption"])
            .max_width_chars(28)
            .build();
        footer.append(&notice);
        let save = gtk::Button::builder()
            .icon_name("bookmark-new-symbolic")
            .tooltip_text("Save this prompt for later")
            .build();
        footer.append(&save);
        let cancel = gtk::Button::builder()
            .label("Cancel")
            .visible(false)
            .build();
        footer.append(&cancel);
        let send = gtk::Button::builder()
            .label("Rewrite")
            .css_classes(["suggested-action", "ml-primary"])
            .build();
        footer.append(&send);
        composer.append(&footer);
        content.append(&composer);
        let screenshot_store = ScreenshotStore::new(&store.history.database.path);
        let workspace = Rc::new(Self {
            widget,
            split,
            content,
            history_list,
            search,
            more,
            live_navigation,
            conversation_title,
            title_button,
            title_stack,
            title_entry,
            title_save,
            continue_button,
            heading,
            messages,
            scroll,
            conversation_follower,
            live_header,
            live_light,
            live_title,
            live_cancel,
            live_cancel_slot,
            source_toggle,
            draft_toggle,
            live_box,
            live_source_box,
            live_text,
            live_scroll,
            live_follower,
            live_divider,
            live_draft_box,
            live_draft_status,
            live_questions,
            live_questions_scroll,
            live_draft_text,
            live_draft_scroll,
            live_draft_follower,
            screenshot_shelf,
            screenshot_buttons,
            composer,
            actions,
            quick_polish,
            structured_note,
            saved_prompts,
            prompt_label,
            prompt,
            prompt_placeholder,
            notice,
            save,
            cancel,
            send,
            store,
            callbacks,
            resources,
            config: RefCell::new(config),
            state: RefCell::new(WorkspaceState {
                search_limit: 80,
                ..WorkspaceState::default()
            }),
            prompt_store: RefCell::new(None),
            screenshot_store,
            live_source_visible: Cell::new(true),
            live_draft_visible: Cell::new(true),
            _live_diagrams: live_diagrams,
        });
        workspace.connect(&new_conversation, &archive, &title_cancel);
        workspace.show_conversation(None, &[], false)?;
        workspace.refresh_history()?;
        Ok(workspace)
    }

    fn connect(
        self: &Rc<Self>,
        new_conversation: &gtk::Button,
        archive: &gtk::Button,
        title_cancel: &gtk::Button,
    ) {
        macro_rules! clicked {
            ($button:expr, $action:expr) => {{
                let weak = Rc::downgrade(self);
                $button.connect_clicked(move |_| {
                    if let Some(w) = weak.upgrade() {
                        ($action)(&w);
                    }
                });
            }};
        }
        clicked!(new_conversation, |w: &Rc<Self>| {
            w.report(w.show_conversation(None, &[], false));
        });
        clicked!(&self.live_navigation, |w: &Rc<Self>| w.show_live());
        clicked!(archive, |w: &Rc<Self>| (w.callbacks.open_archive)());
        clicked!(&self.more, |w: &Rc<Self>| {
            w.state.borrow_mut().search_limit += 80;
            w.report(w.refresh_history());
        });
        clicked!(&self.title_button, |w: &Rc<Self>| {
            if let Some(id) = w.entry().map(|e| e.identifier) {
                w.report(w.begin_rename(&id));
            }
        });
        clicked!(&self.title_save, |w: &Rc<Self>| w.save_title());
        clicked!(title_cancel, |w: &Rc<Self>| w.cancel_title());
        clicked!(&self.continue_button, |w: &Rc<Self>| {
            if let Some(entry) = w.entry() {
                (w.callbacks.continue_recording)(&entry.identifier);
            }
        });
        clicked!(&self.live_cancel, |w: &Rc<Self>| (w
            .callbacks
            .cancel_rewrite)(
        ));
        clicked!(&self.source_toggle, |w: &Rc<Self>| w.toggle_live_pane(true));
        clicked!(&self.draft_toggle, |w: &Rc<Self>| w.toggle_live_pane(false));
        clicked!(&self.quick_polish, |w: &Rc<Self>| w.request_prompt(
            "rewrite-polish",
            builtin_prompt("rewrite-polish")
        ));
        clicked!(&self.structured_note, |w: &Rc<Self>| w.request_prompt(
            "rewrite-structure",
            builtin_prompt("rewrite-structure")
        ));
        clicked!(&self.save, |w: &Rc<Self>| (w.callbacks.save_prompt)(
            &w.prompt_text()
        ));
        clicked!(&self.cancel, |w: &Rc<Self>| (w.callbacks.cancel_rewrite)());
        clicked!(&self.send, |w: &Rc<Self>| w.submit());
        for button in &self.screenshot_buttons {
            clicked!(button, |w: &Rc<Self>| {
                if let Some(capture) = &w.callbacks.capture_screenshot {
                    capture();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.title_entry.connect_activate(move |_| {
            if let Some(w) = weak.upgrade() {
                w.save_title();
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape
                && let Some(w) = weak.upgrade()
            {
                w.cancel_title();
                w.title_button.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.title_entry.add_controller(keys);
        let weak = Rc::downgrade(self);
        self.search.connect_search_changed(move |_| {
            if let Some(w) = weak.upgrade() {
                w.state.borrow_mut().search_limit = 80;
                w.report(w.refresh_history());
            }
        });
        let weak = Rc::downgrade(self);
        self.history_list.connect_row_activated(move |_, row| {
            if let Some(w) = weak.upgrade() {
                let id = w
                    .state
                    .borrow()
                    .rows
                    .iter()
                    .find(|(r, _)| r == row)
                    .map(|(_, id)| id.clone());
                if let Some(id) = id {
                    let result = w.store.history.find(&id).and_then(|e| {
                        w.store
                            .replies(&id)
                            .and_then(|replies| w.show_conversation(Some(e), &replies, false))
                    });
                    w.report(result);
                }
            }
        });
        let placeholder = self.prompt_placeholder.downgrade();
        self.prompt.buffer().connect_changed(move |b| {
            if let Some(label) = placeholder.upgrade() {
                label.set_visible(b.char_count() == 0);
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            if key == gtk::gdk::Key::Return
                && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && let Some(w) = weak.upgrade()
            {
                w.submit();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.prompt.add_controller(keys);
        let divider = self.live_divider.downgrade();
        self.live_box.connect_orientation_notify(move |b| {
            if let Some(divider) = divider.upgrade() {
                divider.set_orientation(if b.orientation() == gtk::Orientation::Horizontal {
                    gtk::Orientation::Vertical
                } else {
                    gtk::Orientation::Horizontal
                });
            }
        });
        let draft_toggle = self.draft_toggle.downgrade();
        self.live_draft_status.connect_label_notify(move |label| {
            label.set_tooltip_text(Some(&label.label()));
            if let Some(toggle) = draft_toggle.upgrade() {
                toggle.set_tooltip_text(Some(&format!(
                    "Show or hide draft · Ctrl+2 · {}",
                    label.label()
                )));
            }
        });
        let focus = gtk::EventControllerFocus::new();
        let follower = Rc::downgrade(&self.live_draft_follower);
        focus.connect_enter(move |_| {
            if let Some(f) = follower.upgrade() {
                f.stop();
            }
        });
        self.live_draft_text.add_controller(focus);
    }

    fn report<T>(&self, result: StoreResult<T>) {
        if result.is_err() {
            self.notice
                .set_label("Could not open this conversation. Try again.");
        }
    }
    pub fn entry(&self) -> Option<HistoryEntry> {
        self.state.borrow().entry.clone()
    }
    pub fn config(&self) -> AppConfig {
        self.config.borrow().clone()
    }
    pub fn set_prompt_store(&self, store: Option<Rc<RefCell<PromptStore>>>) {
        self.prompt_store.replace(store);
    }
    pub fn set_capture_controls(&self, controls: &impl IsA<gtk::Widget>) {
        self.content.append(controls);
    }
    pub fn set_rewrite_settings(&self, settings: &impl IsA<gtk::Widget>) {
        self.actions.insert(settings, -1);
    }
    pub fn set_compact(&self, compact: bool) {
        if compact {
            self.continue_button
                .set_icon_name("audio-input-microphone-symbolic");
            self.live_cancel.set_icon_name("process-stop-symbolic");
        } else {
            self.continue_button.set_label("Continue recording");
            self.live_cancel.set_label("Cancel");
        }
        self.actions.set_halign(if compact {
            gtk::Align::Fill
        } else {
            gtk::Align::Start
        });
        self.live_header.set_spacing(if compact { 4 } else { 8 });
        self.draft_toggle
            .set_label(if compact { "Draft" } else { "Live draft" });
    }

    pub fn set_config(self: &Rc<Self>, config: AppConfig) -> StoreResult<()> {
        let previous = self.config.replace(config.clone());
        self.split
            .set_show_sidebar(config.history_sidebar_visible && !self.split.is_collapsed());
        if previous.time_format != config.time_format {
            self.refresh_history()?;
        }
        for follower in [
            &self.conversation_follower,
            &self.live_follower,
            &self.live_draft_follower,
        ] {
            follower.smooth.set(config.smooth_scrolling);
            follower.duration_ms.set(config.scroll_duration_ms as u32);
            follower.lookahead_lines.set(config.scroll_lookahead_lines);
        }
        self.messages
            .set_margin_bottom(16 + config.scroll_lookahead_lines as i32 * 20);
        self.live_follower.queue();
        self.live_draft_follower.queue();
        let s = self.state.borrow();
        for button in &s.copy_buttons {
            button.set_visible(config.show_copy_action);
        }
        for button in &s.save_buttons {
            button.set_visible(config.show_save_action);
        }
        drop(s);
        self.update_actions();
        Ok(())
    }

    fn message(self: &Rc<Self>, title: &str, text: &str, source: bool, reply: Option<i64>) {
        let message = gtk::Box::new(gtk::Orientation::Vertical, 8);
        message.add_css_class(if source { "ml-source" } else { "ml-reply" });
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.append(
            &gtk::Label::builder()
                .label(title)
                .xalign(0.0)
                .hexpand(true)
                .css_classes(["heading"])
                .build(),
        );
        let key = self.entry().map(|e| (e.identifier, reply));
        let draft = key
            .as_ref()
            .and_then(|key| self.state.borrow().edit_drafts.get(key).cloned())
            .unwrap_or_else(|| text.into());
        let view = MarkdownTextView::document(&draft, !source, true);
        let save = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .has_frame(false)
            .tooltip_text("Save edits")
            .visible(self.config.borrow().show_save_action)
            .sensitive(false)
            .build();
        self.state.borrow_mut().save_buttons.push(save.clone());
        if let Some(key) = key {
            self.state
                .borrow_mut()
                .editors
                .insert(key.clone(), view.clone());
            let weak = Rc::downgrade(self);
            let edit = view.downgrade();
            let button = save.downgrade();
            let original = text.to_owned();
            let changed_key = key.clone();
            view.buffer().connect_changed(move |_| {
                if let (Some(w), Some(view), Some(button)) =
                    (weak.upgrade(), edit.upgrade(), button.upgrade())
                {
                    let text = view.text();
                    button.set_sensitive(text != original);
                    w.state
                        .borrow_mut()
                        .edit_drafts
                        .insert(changed_key.clone(), text);
                }
            });
            let weak = Rc::downgrade(self);
            save.connect_clicked(move |_| {
                if let Some(w) = weak.upgrade() {
                    w.save_edits(Some(&key));
                }
            });
            save.set_sensitive(draft != text);
        }
        header.append(&save);
        let copy = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .has_frame(false)
            .tooltip_text(format!("Copy {}", title.to_lowercase()))
            .visible(self.config.borrow().show_copy_action)
            .build();
        let callback = self.callbacks.copy.clone();
        let weak_view = view.downgrade();
        copy.connect_clicked(move |_| {
            if let Some(view) = weak_view.upgrade() {
                callback(&view.text());
            }
        });
        self.state.borrow_mut().copy_buttons.push(copy.clone());
        header.append(&copy);
        message.append(&header);
        message.append(&view);
        if !source {
            let preview = MermaidPreview::new(&view, &self.resources);
            message.append(&preview.widget);
            self.state.borrow_mut().previews.push(preview);
        }
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            if key == gtk::gdk::Key::s
                && state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && let Some(w) = weak.upgrade()
            {
                w.save_edits(None);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        view.add_controller(keys);
        self.messages.append(&message);
        self.state.borrow_mut().documents.push(view);
    }

    pub fn documents(&self) -> Vec<MarkdownTextView> {
        self.state.borrow().documents.clone()
    }
    pub fn save_edits(self: &Rc<Self>, key: Option<&EditKey>) -> bool {
        let keys = key.map_or_else(
            || {
                let s = self.state.borrow();
                s.edit_drafts
                    .keys()
                    .filter(|k| s.entry.as_ref().is_some_and(|e| e.identifier == k.0))
                    .cloned()
                    .collect::<Vec<_>>()
            },
            |key| vec![key.clone()],
        );
        for key in keys {
            let draft = self.state.borrow().edit_drafts.get(&key).cloned();
            if let Some(text) = draft {
                if self.store.save_text(&key.0, &text, key.1).is_err() {
                    self.notice
                        .set_label("Could not save edits. Keep the document open and try again.");
                    return false;
                }
                self.state.borrow_mut().edit_drafts.remove(&key);
            }
        }
        self.notice.set_label("Edits saved");
        self.report(self.refresh_history());
        true
    }

    pub fn save_conversation_edits(self: &Rc<Self>, identifier: &str) -> bool {
        let keys = self
            .state
            .borrow()
            .edit_drafts
            .keys()
            .filter(|key| key.0 == identifier)
            .cloned()
            .collect::<Vec<_>>();
        keys.iter().all(|key| self.save_edits(Some(key)))
    }

    pub fn can_copy_current_output(&self) -> bool {
        !self.state.borrow().documents.is_empty() && !self.live_box.get_visible()
    }
    pub fn copy_current_output(&self) -> bool {
        if !self.can_copy_current_output() {
            return false;
        }
        let text = self.state.borrow().documents.last().map(|view| view.text());
        if let Some(text) = text {
            (self.callbacks.copy)(&text);
        }
        true
    }
    pub fn focus_prompt(&self) {
        if self.composer.get_visible() {
            self.prompt.grab_focus();
        }
    }
    pub fn prompt_text(&self) -> String {
        let b = self.prompt.buffer();
        text::trim(&b.text(&b.start_iter(), &b.end_iter(), false)).into()
    }
    pub fn live_draft(&self) -> String {
        self.state.borrow().live_questions_source.clone() + &self.live_draft_text.text()
    }
    pub fn set_live_draft_available(&self, available: bool) {
        self.state.borrow_mut().live_draft_available = available;
        self.sync_live_panes();
    }

    pub fn show_live_draft(self: &Rc<Self>, source: &str, status: &str) {
        let follower = &self.live_draft_follower;
        let following = follower.following.get();
        let position = self.live_draft_scroll.vadjustment().value();
        self.state.borrow_mut().live_draft_available = true;
        self.sync_live_panes();
        follower.writing.set(true);
        let (questions, body) = split_grilling_draft(source);
        self.state.borrow_mut().live_questions_source = questions.into();
        self.live_questions
            .replace_text(questions.trim_end_matches(text::whitespace));
        self.live_questions.set_visible(!questions.is_empty());
        self.live_questions_scroll
            .set_visible(!questions.is_empty());
        follower.prepare_update(body, false);
        self.live_draft_text.replace_text(body);
        follower.updated();
        self.live_draft_status.set_label(status);
        if following {
            follower.follow(false);
        } else {
            let weak = Rc::downgrade(follower);
            glib::idle_add_local_once(move || {
                if let Some(f) = weak.upgrade() {
                    f.write(position);
                }
            });
        }
    }

    pub fn show_conversation(
        self: &Rc<Self>,
        entry: Option<HistoryEntry>,
        replies: &[Rewrite],
        preserve_live: bool,
    ) -> StoreResult<()> {
        self.cancel_title();
        if !preserve_live {
            self.set_live_visibility(false);
        }
        let draft_key = self.entry().map_or_else(|| "new".into(), |e| e.identifier);
        self.state
            .borrow_mut()
            .drafts
            .insert(draft_key, self.prompt_text());
        let same_entry = self
            .entry()
            .as_ref()
            .zip(entry.as_ref())
            .is_some_and(|(before, after)| before.identifier == after.identifier);
        let position = self.scroll.vadjustment().value();
        let following = self.conversation_follower.following.get();
        {
            let mut s = self.state.borrow_mut();
            s.editors.clear();
            s.copy_buttons.clear();
            s.save_buttons.clear();
            s.documents.clear();
            s.previews.clear();
            s.preview_box = None;
            s.preview_editor = None;
            s.entry = entry.clone();
        }
        self.refresh_screenshots();
        self.conversation_title.set_label("New conversation");
        self.conversation_title.set_tooltip_text(None);
        while let Some(child) = self.messages.first_child() {
            self.messages.remove(&child);
        }
        if let Some(entry) = &entry {
            let title = display_title(entry);
            self.conversation_title.set_label(&title);
            self.conversation_title.set_tooltip_text(Some(&title));
            self.message(
                "Original",
                &self.store.source_text(entry, false)?,
                true,
                None,
            );
            for reply in replies {
                let instruction = if reply.instruction == builtin_prompt("rewrite-polish") {
                    "Quick Polish"
                } else if reply.instruction == builtin_prompt("rewrite-structure") {
                    "Structured Note"
                } else {
                    &reply.instruction
                };
                self.messages.append(
                    &gtk::Label::builder()
                        .label(instruction)
                        .xalign(1.0)
                        .wrap(true)
                        .selectable(true)
                        .halign(gtk::Align::End)
                        .max_width_chars(64)
                        .css_classes(["ml-instruction"])
                        .build(),
                );
                self.message(
                    if reply.model == MERGE_MODEL {
                        "Merged text"
                    } else {
                        "Rewrite"
                    },
                    &reply.text,
                    false,
                    Some(reply.identifier),
                );
            }
            self.render_rewrite_preview();
        } else {
            let empty = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(12)
                .valign(gtk::Align::Start)
                .css_classes(["ml-empty"])
                .build();
            let mark = self.resources.brand_mark(40);
            mark.set_halign(gtk::Align::Start);
            empty.append(&mark);
            empty.append(
                &gtk::Label::builder()
                    .label("Your words, ready to use")
                    .xalign(0.0)
                    .css_classes(["title-2"])
                    .build(),
            );
            empty.append(&gtk::Label::builder().label("Press F9 to dictate. Or paste text below.\nPolish it, structure it, make it yours.").xalign(0.0).wrap(true).css_classes(["dim-label"]).build());
            self.messages.append(&empty);
        }
        let key = entry.as_ref().map_or("new", |e| &e.identifier);
        let prompt = self
            .state
            .borrow()
            .drafts
            .get(key)
            .cloned()
            .unwrap_or_default();
        self.prompt.buffer().set_text(&prompt);
        self.prompt_label.set_label(if entry.is_some() {
            "Ask for a rewrite or a follow-up"
        } else {
            "Paste or type text to get started"
        });
        self.prompt_label.set_visible(false);
        self.prompt_placeholder.set_label(if entry.is_some() {
            "Ask for a rewrite…"
        } else {
            "Paste or type text to start…"
        });
        self.send.set_label(if entry.is_some() {
            "Rewrite"
        } else {
            "Start conversation"
        });
        self.notice.set_label(if entry.is_some() {
            "Edit either document. Save edits or rewrite to keep them."
        } else {
            "Dictation copies automatically."
        });
        self.update_actions();
        let selected = {
            let s = self.state.borrow();
            if s.viewing_live {
                None
            } else {
                s.rows
                    .iter()
                    .find(|(_, id)| entry.as_ref().is_some_and(|e| e.identifier == *id))
                    .map(|(row, _)| row.clone())
            }
        };
        if let Some(row) = selected {
            self.history_list.select_row(Some(&row));
        }
        if self.split.is_collapsed() {
            self.split.set_show_sidebar(false);
        }
        if !same_entry {
            self.scroll_to_latest();
        } else if following {
            self.conversation_follower.follow(false);
        } else {
            self.conversation_follower.stop();
            let weak = Rc::downgrade(&self.conversation_follower);
            glib::idle_add_local_once(move || {
                if let Some(f) = weak.upgrade() {
                    f.write(position);
                }
            });
        }
        Ok(())
    }

    pub fn set_rewrite_preview(self: &Rc<Self>, identifier: &str, source: &str) {
        {
            let mut s = self.state.borrow_mut();
            s.preview_identifier = Some(identifier.into());
            s.preview_text = source.into();
        }
        self.render_rewrite_preview();
    }
    fn render_rewrite_preview(self: &Rc<Self>) {
        {
            let s = self.state.borrow();
            if s.private
                || s.entry
                    .as_ref()
                    .is_none_or(|e| Some(&e.identifier) != s.preview_identifier.as_ref())
            {
                return;
            }
        }
        if self.state.borrow().preview_box.is_none() {
            let panel = gtk::Box::new(gtk::Orientation::Vertical, 8);
            panel.add_css_class("ml-reply");
            panel.append(
                &gtk::Label::builder()
                    .label("Rewriting…")
                    .xalign(0.0)
                    .css_classes(["heading"])
                    .build(),
            );
            let view = MarkdownTextView::document("", true, false);
            view.set_focusable(false);
            view.set_can_target(false);
            panel.append(&view);
            self.messages.append(&panel);
            let mut s = self.state.borrow_mut();
            s.preview_box = Some(panel);
            s.preview_editor = Some(view);
        }
        let (editor, text) = {
            let s = self.state.borrow();
            (s.preview_editor.clone(), s.preview_text.clone())
        };
        if let Some(editor) = editor {
            editor.replace_text(&text);
        }
        self.conversation_follower.queue();
    }
    pub fn clear_rewrite_preview(&self) {
        let panel = {
            let mut s = self.state.borrow_mut();
            s.preview_identifier = None;
            s.preview_text.clear();
            s.preview_editor = None;
            s.preview_box.take()
        };
        if let Some(panel) = panel {
            self.messages.remove(&panel);
        }
    }
    pub fn scroll_to_latest(self: &Rc<Self>) {
        self.conversation_follower.follow(true);
    }

    pub fn refresh_history(self: &Rc<Self>) -> StoreResult<()> {
        {
            let mut s = self.state.borrow_mut();
            s.rows.clear();
            s.row_titles.clear();
            s.row_menus.clear();
        }
        while let Some(child) = self.history_list.first_child() {
            self.history_list.remove(&child);
        }
        let entries = self.store.search(
            self.search.text().as_str(),
            self.state.borrow().search_limit,
            None,
        )?;
        for entry in &entries {
            let row_content = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            margins(&row_content, 6);
            let body = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .hexpand(true)
                .build();
            row_content.append(&body);
            let title = gtk::Label::builder()
                .label(display_title(entry))
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(23)
                .build();
            body.append(&title);
            body.append(
                &gtk::Label::builder()
                    .label(history_timestamp(
                        &entry.created_at,
                        &self.config.borrow().time_format,
                    )?)
                    .xalign(0.0)
                    .css_classes(["caption", "dim-label"])
                    .build(),
            );
            let menu = self.conversation_menu(entry);
            row_content.append(&menu);
            let row = gtk::ListBoxRow::builder().child(&row_content).build();
            self.history_list.append(&row);
            {
                let mut s = self.state.borrow_mut();
                s.rows.push((row.clone(), entry.identifier.clone()));
                s.row_titles.insert(entry.identifier.clone(), title);
                s.row_menus.push(menu);
            }
            if !self.state.borrow().viewing_live
                && self
                    .entry()
                    .is_some_and(|e| e.identifier == entry.identifier)
            {
                self.history_list.select_row(Some(&row));
            }
        }
        if entries.is_empty() {
            self.history_list.append(
                &gtk::Label::builder()
                    .label(if self.search.text().is_empty() {
                        "Your history will appear here."
                    } else {
                        "No matching conversations"
                    })
                    .wrap(true)
                    .css_classes(["dim-label"])
                    .build(),
            );
        }
        self.more
            .set_visible(entries.len() as i64 == self.state.borrow().search_limit);
        Ok(())
    }

    pub fn refresh_title(self: &Rc<Self>, identifier: &str) -> StoreResult<()> {
        let entry = self.store.history.find(identifier)?;
        let title = display_title(&entry);
        if self.entry().is_some_and(|e| e.identifier == identifier) {
            self.state.borrow_mut().entry = Some(entry);
            self.conversation_title.set_label(&title);
            self.conversation_title.set_tooltip_text(Some(&title));
        }
        if !self.search.text().is_empty() {
            self.refresh_history()?;
        } else if let Some(label) = self.state.borrow().row_titles.get(identifier) {
            label.set_label(&title);
        }
        Ok(())
    }

    fn conversation_menu(self: &Rc<Self>, entry: &HistoryEntry) -> gtk::MenuButton {
        let menu = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .has_frame(false)
            .valign(gtk::Align::Center)
            .tooltip_text("Conversation actions")
            .build();
        {
            let s = self.state.borrow();
            menu.set_sensitive(!s.private && !s.busy && !s.live_active);
        }
        let popover = gtk::Popover::new();
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
        for (index, label) in ["Rename", "Merge with…", "Delete…"].into_iter().enumerate() {
            let button = gtk::Button::builder().label(label).has_frame(false).build();
            if index == 2 {
                button.add_css_class("destructive-action");
            }
            if index == 1 {
                button.set_sensitive(entry.mode == "dictation");
            }
            let weak = Rc::downgrade(self);
            let popover = popover.downgrade();
            let id = entry.identifier.clone();
            button.connect_clicked(move |_| {
                if let (Some(w), Some(popover)) = (weak.upgrade(), popover.upgrade()) {
                    popover.popdown();
                    match index {
                        0 => w.report(w.begin_rename(&id)),
                        1 => w.report(w.choose_merge(&id)),
                        _ => w.report(w.confirm_delete(&id)),
                    }
                }
            });
            actions.append(&button);
        }
        popover.set_child(Some(&actions));
        menu.set_popover(Some(&popover));
        menu
    }

    pub fn begin_rename(self: &Rc<Self>, identifier: &str) -> StoreResult<()> {
        if self.state.borrow().private {
            return Ok(());
        }
        let entry = match self.store.history.find(identifier) {
            Ok(entry) => entry,
            Err(mluva_core::database::StoreError::NotFound) => return self.refresh_history(),
            Err(e) => return Err(e),
        };
        if self.entry().is_none_or(|e| e.identifier != identifier) {
            self.show_conversation(Some(entry.clone()), &self.store.replies(identifier)?, false)?;
        }
        self.state.borrow_mut().renaming = Some(identifier.into());
        self.title_entry.set_text(&display_title(&entry));
        self.title_entry.remove_css_class("error");
        self.title_stack.set_visible_child_name("edit");
        self.continue_button.set_visible(false);
        self.title_entry.grab_focus();
        self.title_entry.select_region(0, -1);
        Ok(())
    }

    pub fn save_title(&self) {
        let title = self.title_entry.text();
        let title = text::trim(&title);
        if title.is_empty() {
            self.title_entry.add_css_class("error");
            self.title_entry
                .set_tooltip_text(Some("Enter a title before saving."));
            return;
        }
        let id = self.state.borrow().renaming.clone();
        if let Some(id) = id
            && (self.callbacks.rename)(&id, title)
        {
            self.cancel_title();
            self.title_button.grab_focus();
        }
    }
    pub fn cancel_title(&self) {
        self.state.borrow_mut().renaming = None;
        self.title_stack.set_visible_child_name("title");
        self.title_entry.set_tooltip_text(None);
        self.continue_button.set_visible(
            self.entry().is_some_and(|e| e.mode == "dictation") && !self.state.borrow().private,
        );
    }

    pub fn confirm_delete(self: &Rc<Self>, identifier: &str) -> StoreResult<()> {
        let entry = match self.store.history.find(identifier) {
            Ok(entry) => entry,
            Err(StoreError::NotFound) => return self.refresh_history(),
            Err(error) => return Err(error),
        };
        let title = display_title(&entry);
        let dialog = adw::AlertDialog::new(
            Some("Delete conversation?"),
            Some(&format!(
                "“{title}” and all its originals, saved rewrites and retained recordings will be permanently deleted."
            )),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        let id = identifier.to_owned();
        dialog.choose(
            Some(&self.widget),
            gio::Cancellable::NONE,
            move |response| {
                if response == "delete"
                    && let Some(w) = weak.upgrade()
                {
                    (w.callbacks.delete)(&id);
                }
            },
        );
        Ok(())
    }

    pub fn choose_merge(self: &Rc<Self>, identifier: &str) -> StoreResult<()> {
        let source = match self.store.history.find(identifier) {
            Ok(entry) => entry,
            Err(StoreError::NotFound) => return self.refresh_history(),
            Err(error) => return Err(error),
        };
        let title = display_title(&source);
        let dialog = adw::AlertDialog::new(
            Some("Merge conversations"),
            Some(&format!(
                "Choose the chat to keep. “{title}” is added after it, using its title. Originals and saved rewrites are kept. This cannot be undone."
            )),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("merge", "Merge")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("merge", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("merge", false);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Find a conversation")
            .build();
        content.append(&search);
        let choices = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["boxed-list"])
            .build();
        content.append(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .min_content_height(180)
                .child(&choices)
                .build(),
        );
        let empty = gtk::Label::builder()
            .label("No other matching conversations")
            .wrap(true)
            .css_classes(["dim-label"])
            .build();
        content.append(&empty);
        dialog.set_extra_child(Some(&content));
        let targets = Rc::new(RefCell::new(Vec::<(gtk::ListBoxRow, String)>::new()));
        let weak = Rc::downgrade(self);
        let weak_choices = choices.downgrade();
        let weak_dialog = dialog.downgrade();
        let weak_empty = empty.downgrade();
        let shared_targets = targets.clone();
        let id = identifier.to_owned();
        let populate = Rc::new(move |query: &str| {
            let (Some(w), Some(choices), Some(dialog), Some(empty)) = (
                weak.upgrade(),
                weak_choices.upgrade(),
                weak_dialog.upgrade(),
                weak_empty.upgrade(),
            ) else {
                return;
            };
            shared_targets.borrow_mut().clear();
            while let Some(row) = choices.first_child() {
                choices.remove(&row);
            }
            let Ok(entries) = w.store.search(query, 80, Some(&id)) else {
                dialog.set_response_enabled("merge", false);
                return;
            };
            for entry in entries {
                let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
                margins(&body, 8);
                body.append(
                    &gtk::Label::builder()
                        .label(display_title(&entry))
                        .xalign(0.0)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .max_width_chars(28)
                        .build(),
                );
                if let Ok(stamp) =
                    history_timestamp(&entry.created_at, &w.config.borrow().time_format)
                {
                    body.append(
                        &gtk::Label::builder()
                            .label(stamp)
                            .xalign(0.0)
                            .css_classes(["caption", "dim-label"])
                            .build(),
                    );
                }
                let row = gtk::ListBoxRow::builder().child(&body).build();
                choices.append(&row);
                shared_targets.borrow_mut().push((row, entry.identifier));
            }
            empty.set_visible(shared_targets.borrow().is_empty());
            dialog.set_response_enabled("merge", false);
        });
        let rows = targets.clone();
        let weak_dialog = dialog.downgrade();
        choices.connect_row_selected(move |_, row| {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.set_response_enabled(
                    "merge",
                    row.is_some_and(|r| rows.borrow().iter().any(|(target, _)| target == r)),
                );
            }
        });
        let update = populate.clone();
        search.connect_search_changed(move |s| update(s.text().as_str()));
        populate("");
        let weak = Rc::downgrade(self);
        let weak_choices = choices.downgrade();
        let id = identifier.to_owned();
        dialog.choose(
            Some(&self.widget),
            gio::Cancellable::NONE,
            move |response| {
                if response == "merge"
                    && let (Some(w), Some(choices)) = (weak.upgrade(), weak_choices.upgrade())
                {
                    let selected = choices.selected_row().and_then(|r| {
                        targets
                            .borrow()
                            .iter()
                            .find(|(target, _)| *target == r)
                            .map(|(_, id)| id.clone())
                    });
                    if let Some(target) = selected {
                        (w.callbacks.merge)(&id, &target);
                    }
                }
            },
        );
        Ok(())
    }

    pub fn show_transient(self: &Rc<Self>, original: &str, text: &str) -> StoreResult<()> {
        self.show_conversation(None, &[], false)?;
        if let Some(empty) = self.messages.first_child() {
            self.messages.remove(&empty);
        }
        self.message("Original", original, true, None);
        if text != original {
            self.message("Dictation", text, false, None);
        }
        self.notice.set_label("This conversation is not saved.");
        self.scroll_to_latest();
        Ok(())
    }
    pub fn submit(&self) {
        let text = self.prompt_text();
        if text.is_empty() || self.state.borrow().busy {
            return;
        }
        if self.entry().is_none() {
            (self.callbacks.paste)(&text);
        } else {
            (self.callbacks.rewrite)(&text);
        }
    }
    pub fn request_prompt(&self, identifier: &str, fallback: &str) {
        let text = self
            .prompt_store
            .borrow()
            .as_ref()
            .and_then(|s| {
                let store = s.borrow();
                if store.prompt(identifier).is_ok() {
                    store.read(identifier).ok().map(|s| s.text)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| fallback.into());
        (self.callbacks.rewrite)(&text);
    }
    pub fn set_busy(&self, busy: bool, message: &str) {
        self.state.borrow_mut().busy = busy;
        self.notice.set_label(message);
        self.update_actions();
    }
    fn update_actions(&self) {
        let s = self.state.borrow();
        let config = self.config.borrow();
        let has_entry = s.entry.is_some();
        for button in &self.screenshot_buttons {
            button.set_sensitive(
                self.callbacks.capture_screenshot.is_some() && !s.private && !s.busy,
            );
        }
        self.title_button.set_sensitive(has_entry && !s.private);
        self.title_save.set_sensitive(!s.private);
        for menu in &s.row_menus {
            menu.set_sensitive(!s.private && !s.busy && !s.live_active);
        }
        self.continue_button.set_visible(
            s.entry.as_ref().is_some_and(|e| e.mode == "dictation")
                && !s.private
                && s.renaming.is_none(),
        );
        self.continue_button
            .set_sensitive(!s.busy && !s.live_active);
        // Changing GtkTextView properties can invoke document formatting, but never changes stored source.
        let editors = s.editors.values().cloned().collect::<Vec<_>>();
        self.cancel.set_visible(s.busy);
        self.live_cancel_slot
            .set_visible_child_name(if s.busy { "cancel" } else { "idle" });
        for button in [&self.quick_polish, &self.structured_note]
            .into_iter()
            .chain(s.saved_prompt_buttons.iter())
        {
            button.set_sensitive(
                config.rewrite_provider != "none" && has_entry && !s.busy && !s.private,
            );
        }
        self.send.set_sensitive(
            !s.busy && (!has_entry || (!s.private && config.rewrite_provider != "none")),
        );
        self.save.set_sensitive(has_entry && !s.private && !s.busy);
        if s.private {
            self.notice
                .set_label("Incognito: no saved conversation. Rewriting is unavailable.");
        }
        let editable = !s.busy;
        drop(s);
        drop(config);
        for editor in editors {
            editor.set_editable(editable);
        }
    }
    pub fn set_private(&self, private: bool) {
        self.state.borrow_mut().private = private;
        if private {
            self.screenshot_shelf.show_images(&[]);
            self.cancel_title();
            self.state.borrow_mut().drafts.clear();
            self.clear_rewrite_preview();
        }
        self.update_actions();
    }
    pub fn set_screenshot_context(&self, capture: Option<String>, conversation: Option<String>) {
        let mut s = self.state.borrow_mut();
        s.live_capture = capture;
        s.live_conversation = conversation;
        drop(s);
        self.refresh_screenshots();
    }
    pub fn refresh_screenshots(&self) {
        let s = self.state.borrow();
        let mut images = Vec::new();
        if !s.private {
            if s.viewing_live
                && let Some(capture) = &s.live_capture
            {
                if let Some(conversation) = &s.live_conversation {
                    images.extend(
                        self.screenshot_store
                            .recent(conversation, false)
                            .unwrap_or_default(),
                    );
                }
                images.extend(
                    self.screenshot_store
                        .recent(capture, true)
                        .unwrap_or_default(),
                );
            } else if let Some(entry) = &s.entry {
                images = self
                    .screenshot_store
                    .recent(&entry.identifier, false)
                    .unwrap_or_default();
            }
        }
        drop(s);
        self.screenshot_shelf.show_images(&images);
    }

    pub fn toggle_live_pane(&self, source: bool) {
        {
            let s = self.state.borrow();
            if !s.viewing_live || !s.live_draft_available {
                drop(s);
                self.sync_live_panes();
                return;
            }
        }
        if source {
            self.live_source_visible
                .set(!self.live_source_visible.get());
            if !self.live_source_visible.get() {
                self.live_draft_visible.set(true);
            }
        } else {
            self.live_draft_visible.set(!self.live_draft_visible.get());
            if !self.live_draft_visible.get() {
                self.live_source_visible.set(true);
            }
        }
        self.sync_live_panes();
    }
    fn sync_live_panes(&self) {
        let available = self.state.borrow().live_draft_available;
        let source = self.live_source_visible.get() || !available;
        let draft = self.live_draft_visible.get() && available;
        self.live_source_box.set_visible(source);
        self.live_draft_box.set_visible(draft);
        self.live_divider.set_visible(source && draft);
        self.source_toggle.set_active(source);
        self.draft_toggle.set_active(draft);
        self.source_toggle.set_sensitive(available);
        self.draft_toggle.set_sensitive(available);
    }
    pub fn set_live(self: &Rc<Self>, phase: &str, text: &str, recording: bool) {
        let starting = !self.state.borrow().live_active;
        self.state.borrow_mut().live_active = true;
        self.update_actions();
        self.live_navigation.set_visible(true);
        if starting {
            self.sync_live_panes();
            self.show_live();
        }
        self.set_live_status(phase, recording);
        self.live_cancel_slot.set_visible(true);
        self.live_follower.prepare_update(text, starting);
        self.live_text.replace_text(text);
        self.live_follower.updated();
    }
    pub fn show_live(&self) {
        if self.state.borrow().live_active {
            self.set_live_visibility(true);
            self.history_list.unselect_all();
            if self.split.is_collapsed() {
                self.split.set_show_sidebar(false);
            }
        }
    }

    pub fn is_viewing_live(&self) -> bool {
        self.state.borrow().viewing_live
    }
    fn set_live_visibility(&self, visible: bool) {
        self.state.borrow_mut().viewing_live = visible;
        self.refresh_screenshots();
        self.live_box.set_visible(visible);
        self.live_header.set_visible(visible);
        self.scroll.set_visible(!visible);
        self.composer.set_visible(!visible);
        self.heading.set_visible(!visible);
        self.live_navigation.set_css_classes(if visible {
            &["flat", "ml-live-current"]
        } else {
            &["flat"]
        });
    }
    pub fn set_live_status(&self, phase: &str, recording: bool) {
        self.live_light.set_recording(recording);
        self.live_title.set_label(phase);
        self.live_title
            .update_property(&[gtk::accessible::Property::Label(&if recording {
                format!("Recording {phase}")
            } else {
                phase.into()
            })]);
    }
    pub fn finish_live(&self) {
        self.state.borrow_mut().live_active = false;
        self.update_actions();
        self.live_navigation.set_visible(false);
        self.live_follower.stop();
        self.live_draft_follower.stop();
        self.live_draft_box.set_visible(false);
        self.state.borrow_mut().live_draft_available = false;
        self.live_divider.set_visible(false);
        self.live_text.buffer().set_text("");
        self.set_live_visibility(false);
        self.live_light.set_recording(false);
        self.live_header.set_visible(false);
        self.live_cancel_slot.set_visible(false);
    }

    pub fn set_saved_prompts(self: &Rc<Self>, prompts: &[(String, String)]) {
        let popover = gtk::Popover::new();
        let choices = gtk::Box::new(gtk::Orientation::Vertical, 4);
        margins(&choices, 8);
        self.state.borrow_mut().saved_prompt_buttons.clear();
        for (name, instruction) in prompts {
            let button = gtk::Button::builder().label(name).has_frame(false).build();
            let weak = Rc::downgrade(self);
            let instruction = instruction.clone();
            let clicked_instruction = instruction.clone();
            let weak_popover = popover.downgrade();
            button.connect_clicked(move |_| {
                if let (Some(w), Some(popover)) = (weak.upgrade(), weak_popover.upgrade()) {
                    popover.popdown();
                    let s = w.state.borrow();
                    let allowed = s.entry.is_some() && !s.busy && !s.private;
                    drop(s);
                    if allowed {
                        w.request_prompt(&clicked_instruction, &clicked_instruction);
                    }
                }
            });
            self.state
                .borrow_mut()
                .saved_prompt_buttons
                .push(button.clone());
            let weak_popover = popover.downgrade();
            let edit = self.callbacks.edit_prompt.clone();
            choices.append(&prompt_control(
                &button,
                name,
                Rc::new(move || {
                    if let Some(p) = weak_popover.upgrade() {
                        p.popdown();
                    }
                    edit(&instruction);
                }),
            ));
        }
        if prompts.is_empty() {
            choices.append(
                &gtk::Label::builder()
                    .label("Write a prompt below, then save it.")
                    .wrap(true)
                    .build(),
            );
        }
        popover.set_child(Some(&choices));
        self.saved_prompts.set_popover(Some(&popover));
        self.update_actions();
    }
}
