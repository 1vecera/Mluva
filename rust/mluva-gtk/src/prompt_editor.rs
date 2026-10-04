//! Native prompt discovery and lossless Save/Cancel editing, shared across application surfaces.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use mluva_core::database::StoreResult;
use mluva_core::prompts::{MAX_PROMPT_CHARACTERS, PromptState, PromptStore};

pub type OpenEditor = Rc<dyn Fn(&str)>;
pub type Writable = Rc<dyn Fn() -> bool>;
pub type Changed = Rc<dyn Fn()>;
pub type Message = Rc<dyn Fn(&str)>;

pub fn reveal_prompt_button(row: &impl IsA<gtk::Widget>, button: &gtk::Button) {
    let motion = gtk::EventControllerMotion::new();
    let focus = gtk::EventControllerFocus::new();
    let weak_button = button.downgrade();
    let weak_motion = motion.downgrade();
    let weak_focus = focus.downgrade();
    let update: Changed = Rc::new(move || {
        if let (Some(button), Some(motion), Some(focus)) = (
            weak_button.upgrade(),
            weak_motion.upgrade(),
            weak_focus.upgrade(),
        ) {
            button.set_opacity(if motion.contains_pointer() || focus.contains_focus() {
                1.0
            } else {
                0.0
            });
        }
    });
    let changed = update.clone();
    motion.connect_contains_pointer_notify(move |_| changed());
    focus.connect_contains_focus_notify(move |_| update());
    row.add_controller(motion);
    row.add_controller(focus);
    button.set_opacity(0.0);
}

pub fn prompt_control(
    widget: &impl IsA<gtk::Widget>,
    name: &str,
    open_editor: Changed,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    row.add_css_class("ml-prompt-control");
    widget.set_hexpand(true);
    row.append(widget);
    let edit = gtk::Button::builder()
        .icon_name("emblem-system-symbolic")
        .build();
    edit.set_css_classes(&["flat", "ml-prompt-settings"]);
    let label = format!("Edit prompt · {name}");
    edit.set_tooltip_text(Some(&label));
    edit.update_property(&[gtk::accessible::Property::Label(&label)]);
    edit.connect_clicked(move |_| open_editor());
    row.append(&edit);
    reveal_prompt_button(&row, &edit);
    row
}

pub struct PromptEditor {
    pub dialog: adw::Dialog,
    store: Rc<RefCell<PromptStore>>,
    identifier: String,
    changed: Changed,
    writable: Writable,
    state: PromptState,
    original: String,
    reset_pending: Cell<bool>,
    discard_dialog: RefCell<Option<adw::AlertDialog>>,
    editor: gtk::TextView,
    save_button: gtk::Button,
    reset_button: gtk::Button,
    status: gtk::Label,
    count: gtk::Label,
}

impl PromptEditor {
    pub fn new(
        store: Rc<RefCell<PromptStore>>,
        identifier: &str,
        changed: Changed,
        writable: Writable,
    ) -> StoreResult<Rc<Self>> {
        let (prompt, state, path) = {
            let store = store.borrow();
            (
                store.prompt(identifier)?.clone(),
                store.read(identifier)?,
                store.path(identifier)?,
            )
        };
        let dialog = adw::Dialog::builder()
            .title(&prompt.name)
            .content_width(720)
            .content_height(640)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .build();
        let cancel = gtk::Button::with_label("Cancel");
        let weak = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(dialog) = weak.upgrade() {
                dialog.close();
            }
        });
        header.pack_start(&cancel);
        let save_button = gtk::Button::with_label("Save");
        save_button.add_css_class("suggested-action");
        header.pack_end(&save_button);
        toolbar.add_top_bar(&header);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.set_margin_top(20);
        body.set_margin_bottom(20);
        body.set_margin_start(20);
        body.set_margin_end(20);
        body.append(&label(&prompt.purpose, &[]));
        let identity = label(
            if state.overridden {
                "Local override"
            } else if prompt.built_in {
                "Built-in default"
            } else {
                "Original saved text"
            },
            &["caption", "dim-label"],
        );
        body.append(&identity);
        let location = gtk::Label::builder()
            .label(path.to_string_lossy())
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .selectable(true)
            .build();
        location.set_tooltip_text(Some(&path.to_string_lossy()));
        location.add_css_class("caption");
        location.add_css_class("dim-label");
        body.append(&location);
        let editor = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .accepts_tab(true)
            .monospace(true)
            .top_margin(16)
            .bottom_margin(16)
            .left_margin(16)
            .right_margin(16)
            .build();
        editor.add_css_class("ml-prompt-editor");
        let original = if !state.error.is_empty() {
            state
                .token
                .as_deref()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_else(|| state.text.clone())
        } else {
            state.text.clone()
        };
        editor.buffer().set_text(&original);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&editor)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(100)
            .build();
        body.append(&scroll);
        let status = label("", &["caption"]);
        body.append(&status);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let reset_button = gtk::Button::with_label(if prompt.built_in {
            "Restore default"
        } else {
            "Restore original"
        });
        footer.append(&reset_button);
        let count = gtk::Label::builder().hexpand(true).xalign(1.0).build();
        count.add_css_class("caption");
        count.add_css_class("dim-label");
        footer.append(&count);
        body.append(&footer);
        body.append(&label(
            "Save applies to the next request. A running Live session keeps its instructions until the next recording. Response format and source-integrity rules remain fixed.",
            &["caption", "dim-label"],
        ));
        toolbar.set_content(Some(&body));
        dialog.set_child(Some(&toolbar));
        dialog.set_focus(Some(&editor));
        let controller = Rc::new(Self {
            dialog,
            store,
            identifier: identifier.into(),
            changed,
            writable,
            state,
            original,
            reset_pending: Cell::new(false),
            discard_dialog: RefCell::new(None),
            editor,
            save_button,
            reset_button,
            status,
            count,
        });
        let weak = Rc::downgrade(&controller);
        controller.editor.buffer().connect_changed(move |_| {
            if let Some(editor) = weak.upgrade() {
                editor.edited();
            }
        });
        let weak = Rc::downgrade(&controller);
        controller.save_button.connect_clicked(move |_| {
            if let Some(editor) = weak.upgrade() {
                editor.save();
            }
        });
        let weak = Rc::downgrade(&controller);
        controller.reset_button.connect_clicked(move |_| {
            if let Some(editor) = weak.upgrade() {
                editor.reset();
            }
        });
        let weak = Rc::downgrade(&controller);
        controller.dialog.connect_close_attempt(move |_| {
            if let Some(editor) = weak.upgrade() {
                editor.close_attempt();
            }
        });
        controller.edited();
        Ok(controller)
    }

    pub fn text(&self) -> String {
        let buffer = self.editor.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .into()
    }

    fn default_text(&self) -> StoreResult<String> {
        Ok(self
            .store
            .borrow()
            .prompt(&self.identifier)?
            .default
            .clone())
    }

    fn edited(&self) {
        let text = self.text();
        if self.reset_pending.get()
            && self
                .default_text()
                .as_deref()
                .is_ok_and(|default| text != default)
        {
            self.reset_pending.set(false);
        }
        let dirty = text != self.original || self.reset_pending.get();
        self.dialog.set_can_close(!dirty);
        self.count.set_label(&format!(
            "{} / {}",
            grouped_count(text.chars().count()),
            grouped_count(MAX_PROMPT_CHARACTERS)
        ));
        let mut error = self
            .store
            .borrow()
            .validate(&self.identifier, &text)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        let writable = (self.writable)();
        if !writable {
            error = "Incognito · viewing only. Prompt changes are not saved.".into();
        }
        self.save_button
            .set_sensitive((dirty || !self.state.overridden) && error.is_empty());
        self.reset_button.set_sensitive(writable);
        self.status.set_label(if !error.is_empty() {
            &error
        } else if self.reset_pending.get() {
            "Default staged · Save to apply"
        } else if !self.state.error.is_empty() {
            &self.state.error
        } else if dirty {
            "Unsaved changes"
        } else {
            ""
        });
    }

    fn reset(&self) {
        match self.default_text() {
            Ok(text) => {
                self.editor.buffer().set_text(&text);
                self.reset_pending.set(true);
                self.edited();
            }
            Err(error) => self.status.set_label(&error.to_string()),
        }
    }

    fn save(&self) {
        if !(self.writable)() {
            self.edited();
            return;
        }
        let text = self.text();
        let result = if self.reset_pending.get()
            && self
                .default_text()
                .as_deref()
                .is_ok_and(|default| text == default)
        {
            self.store
                .borrow()
                .reset(&self.identifier, self.state.token.as_deref())
        } else {
            self.store
                .borrow()
                .save(&self.identifier, &text, self.state.token.as_deref())
        };
        if let Err(error) = result {
            self.status.set_label(&error.to_string());
            return;
        }
        (self.changed)();
        self.dialog.force_close();
    }

    fn close_attempt(self: &Rc<Self>) {
        if self.discard_dialog.borrow().is_some() {
            return;
        }
        let confirm = adw::AlertDialog::new(
            Some("Discard prompt changes?"),
            Some("Your saved prompt will stay unchanged."),
        );
        confirm.add_response("keep", "Keep editing");
        confirm.add_response("discard", "Discard");
        confirm.set_default_response(Some("keep"));
        confirm.set_close_response("keep");
        confirm.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        confirm.connect_response(None, move |_, choice| {
            if let Some(editor) = weak.upgrade() {
                editor.discard_dialog.take();
                if choice == "discard" {
                    editor.dialog.force_close();
                }
            }
        });
        self.discard_dialog.replace(Some(confirm.clone()));
        confirm.present(Some(&self.dialog));
    }
}

pub struct PromptsPage {
    pub widget: adw::PreferencesPage,
    store: Rc<RefCell<PromptStore>>,
    open_editor: OpenEditor,
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
}

impl PromptsPage {
    pub fn new(store: Rc<RefCell<PromptStore>>, open_editor: OpenEditor) -> StoreResult<Rc<Self>> {
        let widget = adw::PreferencesPage::builder()
            .name("prompts")
            .title("Prompts")
            .icon_name("document-edit-symbolic")
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Prompts")
            .description("Edit task instructions and Markdown structures. Local .md files reload when opened or used; active Live sessions keep a snapshot.")
            .build();
        widget.add(&group);
        let page = Rc::new(Self {
            widget,
            store,
            open_editor,
            group,
            rows: RefCell::new(Vec::new()),
        });
        page.refresh()?;
        Ok(page)
    }

    pub fn set_load_error(&self, message: &str) {
        self.group.set_description(Some(message));
    }

    pub fn refresh(&self) -> StoreResult<()> {
        // Resolve before changing the displayed catalog so failed reads do not erase discovery.
        let contents = {
            let store = self.store.borrow();
            store
                .catalog()
                .iter()
                .map(|prompt| Ok((prompt.clone(), store.read(&prompt.identifier)?)))
                .collect::<StoreResult<Vec<_>>>()?
        };
        for row in self.rows.take() {
            self.group.remove(&row);
        }
        for (prompt, state) in contents {
            let row = adw::ActionRow::builder()
                .title(&prompt.name)
                .subtitle(if state.error.is_empty() {
                    &prompt.purpose
                } else {
                    &state.error
                })
                .activatable(true)
                .build();
            row.add_suffix(&gtk::Image::from_icon_name("document-edit-symbolic"));
            let callback = self.open_editor.clone();
            row.connect_activated(move |_| callback(&prompt.identifier));
            self.group.add(&row);
            self.rows.borrow_mut().push(row);
        }
        Ok(())
    }
}

/// Keep a single active edit when callers deep-link from different parts of the application.
pub struct PromptSession {
    store: Rc<RefCell<PromptStore>>,
    writable: Writable,
    changed: Changed,
    message: Message,
    active: RefCell<Option<Rc<PromptEditor>>>,
}

impl PromptSession {
    pub fn new(
        store: Rc<RefCell<PromptStore>>,
        writable: Writable,
        changed: Changed,
        message: Message,
    ) -> Rc<Self> {
        Rc::new(Self {
            store,
            writable,
            changed,
            message,
            active: RefCell::new(None),
        })
    }

    pub fn open(self: &Rc<Self>, parent: &impl IsA<gtk::Widget>, identifier: &str) {
        if self.active.borrow().is_some() {
            (self.message)("Finish or cancel the open prompt edit first.");
            return;
        }
        let editor = match PromptEditor::new(
            self.store.clone(),
            identifier,
            self.changed.clone(),
            self.writable.clone(),
        ) {
            Ok(editor) => editor,
            Err(error) => {
                (self.message)(&error.to_string());
                return;
            }
        };
        let weak = Rc::downgrade(self);
        editor.dialog.connect_closed(move |_| {
            if let Some(session) = weak.upgrade() {
                session.active.take();
            }
        });
        self.active.replace(Some(editor.clone()));
        editor.dialog.present(Some(parent));
    }
}

fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .build();
    for class in classes {
        label.add_css_class(class);
    }
    label
}

fn grouped_count(count: usize) -> String {
    let digits = count.to_string();
    let mut result = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            result.push(',');
        }
        result.push(digit);
    }
    result
}
