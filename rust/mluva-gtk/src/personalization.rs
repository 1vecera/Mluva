//! The same nested native editors for vocabulary, snippets and output styles.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use mluva_core::history::HistoryStore;
use mluva_core::personalization::{DictionaryCaseBehavior, PersonalizationStore};
use mluva_core::prompt_catalog::SavedStyle;
use mluva_core::vocabulary::{self, VocabularySuggestion};

use crate::prompt_editor::reveal_prompt_button;

type Message = Rc<dyn Fn(&str)>;
pub type EditPrompt = Rc<dyn Fn(&str)>;

struct DictionaryEditor {
    row: adw::ExpanderRow,
    spoken: adw::EntryRow,
    written: adw::EntryRow,
    application: adw::EntryRow,
    case: adw::ComboRow,
    save: gtk::Button,
}
struct SnippetEditor {
    row: adw::ExpanderRow,
    trigger: adw::EntryRow,
    typed: adw::EntryRow,
    application: adw::EntryRow,
    expansion: gtk::TextView,
    save: gtk::Button,
}
struct StyleEditor {
    row: adw::ExpanderRow,
    name: adw::EntryRow,
    instructions: gtk::TextView,
    save: gtk::Button,
    cancel: gtk::Button,
}

pub struct PersonalizationPage {
    pub widget: gtk::Box,
    pub view_stack: adw::ViewStack,
    store: Rc<RefCell<PersonalizationStore>>,
    history: HistoryStore,
    message: Message,
    styles_changed: Rc<dyn Fn()>,
    edit_prompt: Option<EditPrompt>,
    editing_style: RefCell<Option<String>>,
    dictionary: DictionaryEditor,
    snippet: SnippetEditor,
    style: StyleEditor,
    dictionary_list: gtk::ListBox,
    suggestion_list: gtk::ListBox,
    snippet_list: gtk::ListBox,
    style_list: gtk::ListBox,
    maturity_badge: gtk::Label,
    maturity_detail: gtk::Label,
}

impl PersonalizationPage {
    pub fn new(
        store: Rc<RefCell<PersonalizationStore>>,
        history: HistoryStore,
        message: Message,
        styles_changed: Rc<dyn Fn()>,
        edit_prompt: Option<EditPrompt>,
    ) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let header = content(12);
        header.set_margin_bottom(8);
        if let Some(error) = &store.borrow().persistence_error {
            let warning = gtk::Label::builder().label(format!("The personalization document is malformed and was preserved. Repair it before saving changes: {error}")).xalign(0.0).wrap(true).build();
            warning.add_css_class("error");
            header.append(&warning);
        }
        let view_stack = adw::ViewStack::builder().vexpand(true).build();
        let switcher = adw::ViewSwitcher::builder()
            .stack(&view_stack)
            .policy(adw::ViewSwitcherPolicy::Narrow)
            .hexpand(true)
            .build();
        header.append(&switcher);
        let maturity = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        maturity.add_css_class("vs-maturity-notice");
        let maturity_badge = gtk::Label::builder().valign(gtk::Align::Center).build();
        maturity_badge.add_css_class("vs-maturity-badge");
        let maturity_detail = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .hexpand(true)
            .build();
        maturity_detail.add_css_class("vs-maturity-detail");
        maturity.append(&maturity_badge);
        maturity.append(&maturity_detail);
        header.append(&maturity);
        widget.append(&clamp(&header));
        let dictionary = dictionary_editor();
        let snippet = snippet_editor();
        let style = style_editor();
        let dictionary_list = list();
        let suggestion_list = list();
        let snippet_list = list();
        let style_list = list();
        for (name, title, icon, editor, list_title, description, list) in [
            (
                "dictionary",
                "Dictionary",
                "accessories-dictionary-symbolic",
                Some(&dictionary.row),
                "Saved dictionary entries",
                "Exact whole-phrase replacements run locally before optional Codex enhancement.",
                &dictionary_list,
            ),
            (
                "suggestions",
                "Suggestions",
                "dialog-information-symbolic",
                None,
                "Suggestions from your edits",
                "Only small corrections explicitly saved in History appear here. Add or dismiss each one; Mluva never learns automatically.",
                &suggestion_list,
            ),
            (
                "snippets",
                "Snippets",
                "insert-text-symbolic",
                Some(&snippet.row),
                "Saved snippets",
                "Exact spoken and optional typed triggers are expanded locally.",
                &snippet_list,
            ),
            (
                "styles",
                "Styles",
                "document-edit-symbolic",
                Some(&style.row),
                "Output styles",
                "Edit prompt instructions in the shared editor; styles apply only when selected.",
                &style_list,
            ),
        ] {
            view_stack.add_titled_with_icon(
                &subpage(editor, list_title, description, list),
                Some(name),
                title,
                icon,
            );
        }
        widget.append(&view_stack);
        if store.borrow().persistence_error.is_some() {
            for editor in [&dictionary.row, &snippet.row, &style.row] {
                editor.set_sensitive(false);
            }
        }
        let page = Rc::new(Self {
            widget,
            view_stack,
            store,
            history,
            message,
            styles_changed,
            edit_prompt,
            editing_style: RefCell::new(None),
            dictionary,
            snippet,
            style,
            dictionary_list,
            suggestion_list,
            snippet_list,
            style_list,
            maturity_badge,
            maturity_detail,
        });
        page.connect();
        page.category_changed();
        page.refresh();
        page
    }

    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.view_stack.connect_visible_child_name_notify(move |_| {
            if let Some(page) = weak.upgrade() {
                page.category_changed();
            }
        });
        let weak = Rc::downgrade(self);
        self.dictionary.save.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.save_dictionary();
            }
        });
        let weak = Rc::downgrade(self);
        self.snippet.save.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.save_snippet();
            }
        });
        let weak = Rc::downgrade(self);
        self.style.save.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.save_style();
            }
        });
        let weak = Rc::downgrade(self);
        self.style.cancel.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                page.clear_style_editor();
                page.style.row.set_expanded(false);
                (page.message)("Custom style edit cancelled.");
            }
        });
    }

    fn category_changed(&self) {
        let category = self
            .view_stack
            .visible_child_name()
            .unwrap_or_else(|| "dictionary".into());
        let identifier = match category.as_str() {
            "styles" => "saved_styles",
            "snippets" => "snippets",
            _ => "dictionary_suggestions",
        };
        let features: serde_json::Value =
            serde_json::from_str(include_str!("../resources/feature-capabilities.json"))
                .expect("reviewed capability labels");
        let capability = features
            .as_array()
            .unwrap()
            .iter()
            .find(|feature| feature["identifier"] == identifier)
            .unwrap();
        let verified = capability["maturity"] == "verified";
        self.maturity_badge.set_label(if verified {
            "Verified on Omarchy"
        } else {
            "Experimental"
        });
        self.maturity_badge.remove_css_class("vs-verified");
        self.maturity_badge.remove_css_class("vs-experimental");
        self.maturity_badge.add_css_class(if verified {
            "vs-verified"
        } else {
            "vs-experimental"
        });
        self.maturity_detail
            .set_label(capability["summary"].as_str().unwrap());
    }

    pub fn refresh(self: &Rc<Self>) {
        for list in [
            &self.dictionary_list,
            &self.suggestion_list,
            &self.snippet_list,
            &self.style_list,
        ] {
            clear(list);
        }
        match self
            .history
            .recent(vocabulary::MAX_SUGGESTION_HISTORY_ENTRIES as i64)
        {
            Err(error) => self.suggestion_list.append(&action(
                "Vocabulary suggestions are unavailable",
                &error.to_string(),
            )),
            Ok(entries) => {
                let state = self.store.borrow();
                let suggestions = vocabulary::suggestions(
                    &entries,
                    &state.state().dictionary,
                    &state.state().dismissed_vocabulary_suggestion_identifiers,
                );
                drop(state);
                if suggestions.is_empty() {
                    self.suggestion_list.append(&action(
                        "No suggestions awaiting review",
                        "Save a small explicit correction in History to propose one here.",
                    ));
                }
                for suggestion in suggestions {
                    self.suggestion_list
                        .append(&self.suggestion_row(suggestion));
                }
            }
        }
        let state = self.store.borrow().state().clone();
        if state.dictionary.is_empty() {
            self.dictionary_list.append(&action(
                "No dictionary entries yet",
                "Add one only when Mluva repeatedly writes an exact phrase incorrectly.",
            ));
        }
        for rule in state.dictionary {
            let scope = rule
                .application_identifier
                .as_deref()
                .unwrap_or("Every application");
            let behavior = if rule.case_behavior == DictionaryCaseBehavior::Fixed {
                "fixed"
            } else {
                "match spoken case"
            };
            let row = action(
                &format!("{} → {}", rule.spoken, rule.written),
                &format!("{scope} · {behavior}"),
            );
            let delete = button("Delete", "destructive-action");
            row.add_suffix(&delete);
            let weak = Rc::downgrade(self);
            delete.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    let result = page
                        .store
                        .borrow_mut()
                        .delete_dictionary_replacement(&rule.identifier);
                    if let Err(error) = result {
                        (page.message)(&format!("Dictionary entry could not be deleted: {error}"));
                        return;
                    }
                    page.refresh();
                    (page.message)("Dictionary entry deleted.");
                }
            });
            self.dictionary_list.append(&row);
        }
        if state.snippets.is_empty() {
            self.snippet_list.append(&action(
                "No snippets yet",
                "Add a reusable exact expansion behind a spoken trigger.",
            ));
        }
        for item in state.snippets {
            let scope = item
                .application_identifier
                .as_deref()
                .unwrap_or("Every application");
            let typed = item
                .typed_trigger
                .as_ref()
                .map(|value| format!(" · typed {value}"))
                .unwrap_or_default();
            let row = adw::ExpanderRow::builder()
                .title(format!("snippet {}", item.trigger))
                .subtitle(format!("{scope}{typed}"))
                .build();
            let expansion = gtk::Label::builder()
                .label(&item.expansion)
                .xalign(0.0)
                .selectable(true)
                .wrap(true)
                .build();
            margins(&expansion, 12);
            row.add_row(&expansion);
            let actions = action("Remove this exact snippet", "");
            let delete = button("Delete", "destructive-action");
            actions.add_suffix(&delete);
            row.add_row(&actions);
            let weak = Rc::downgrade(self);
            delete.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    let result = page.store.borrow_mut().delete_snippet(&item.identifier);
                    if let Err(error) = result {
                        (page.message)(&format!("Snippet could not be deleted: {error}"));
                        return;
                    }
                    page.refresh();
                    (page.message)("Snippet deleted.");
                }
            });
            self.snippet_list.append(&row);
        }
        let styles = self.store.borrow().styles();
        match styles {
            Ok(styles) => {
                for style in styles {
                    self.style_list.append(&self.style_row(style));
                }
            }
            Err(error) => (self.message)(&error.to_string()),
        }
    }

    fn suggestion_row(self: &Rc<Self>, suggestion: VocabularySuggestion) -> adw::ActionRow {
        let count = if suggestion.occurrences == 1 {
            "One correction".into()
        } else {
            format!("{} corrections", suggestion.occurrences)
        };
        let row = action(
            &format!("{} → {}", suggestion.spoken, suggestion.written),
            &format!(
                "{count} · {}",
                suggestion
                    .application_identifier
                    .as_deref()
                    .unwrap_or("Every application")
            ),
        );
        let add = button("Add", "suggested-action");
        let dismiss = button("Dismiss", "");
        row.add_suffix(&add);
        row.add_suffix(&dismiss);
        let weak = Rc::downgrade(self);
        let chosen = suggestion.clone();
        add.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                let result = page.store.borrow_mut().save_dictionary_replacement(
                    &chosen.spoken,
                    &chosen.written,
                    chosen.application_identifier.as_deref(),
                    DictionaryCaseBehavior::Fixed,
                );
                if let Err(error) = result {
                    (page.message)(&format!(
                        "Vocabulary suggestion could not be added: {error}"
                    ));
                    return;
                }
                page.refresh();
                (page.message)("Reviewed vocabulary suggestion added locally.");
            }
        });
        let weak = Rc::downgrade(self);
        dismiss.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                let result = page
                    .store
                    .borrow_mut()
                    .dismiss_vocabulary_suggestion(&suggestion.identifier);
                if let Err(error) = result {
                    (page.message)(&format!(
                        "Vocabulary suggestion could not be dismissed: {error}"
                    ));
                    return;
                }
                page.refresh();
                (page.message)("Vocabulary suggestion dismissed.");
            }
        });
        row
    }

    fn style_row(self: &Rc<Self>, style: SavedStyle) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::builder()
            .title(&style.name)
            .subtitle(if style.is_built_in {
                "Built-in output mode"
            } else {
                "Custom output mode"
            })
            .build();
        let instructions = gtk::Label::builder()
            .label(&style.instructions)
            .xalign(0.0)
            .selectable(true)
            .wrap(true)
            .build();
        margins(&instructions, 12);
        row.add_row(&instructions);
        if let Some(callback) = &self.edit_prompt {
            row.add_css_class("ml-prompt-control");
            let edit = gtk::Button::builder()
                .icon_name("emblem-system-symbolic")
                .valign(gtk::Align::Center)
                .build();
            edit.add_css_class("ml-prompt-settings");
            let label = format!("Edit prompt · {}", style.name);
            edit.set_tooltip_text(Some(&label));
            edit.update_property(&[gtk::accessible::Property::Label(&label)]);
            let callback = callback.clone();
            let identifier = format!("style-{}", style.identifier.to_ascii_lowercase());
            edit.connect_clicked(move |_| callback(&identifier));
            row.add_suffix(&edit);
            reveal_prompt_button(&row, &edit);
        }
        if !style.is_built_in {
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            if self.edit_prompt.is_none() {
                let edit = button("Edit", "");
                let weak = Rc::downgrade(self);
                let chosen = style.clone();
                edit.connect_clicked(move |_| {
                    if let Some(page) = weak.upgrade() {
                        page.begin_style_edit(&chosen);
                    }
                });
                actions.append(&edit);
            }
            let delete = button("Delete", "destructive-action");
            let weak = Rc::downgrade(self);
            delete.connect_clicked(move |_| {
                if let Some(page) = weak.upgrade() {
                    let result = page.store.borrow_mut().delete_style(&style.identifier);
                    if let Err(error) = result {
                        (page.message)(&format!("Custom style could not be deleted: {error}"));
                        return;
                    }
                    if page.editing_style.borrow().as_deref() == Some(&style.identifier) {
                        page.clear_style_editor();
                    }
                    page.refresh();
                    (page.styles_changed)();
                    (page.message)("Custom output style deleted.");
                }
            });
            actions.append(&delete);
            let action = action("Custom mode actions", "");
            action.add_suffix(&actions);
            row.add_row(&action);
        }
        row
    }

    fn save_dictionary(self: &Rc<Self>) {
        let application = self.dictionary.application.text();
        let result = self.store.borrow_mut().save_dictionary_replacement(
            &self.dictionary.spoken.text(),
            &self.dictionary.written.text(),
            optional(&application),
            if self.dictionary.case.selected() == 0 {
                DictionaryCaseBehavior::Fixed
            } else {
                DictionaryCaseBehavior::MatchSpoken
            },
        );
        if let Err(error) = result {
            (self.message)(&format!("Dictionary entry could not be saved: {error}"));
            return;
        }
        self.dictionary.spoken.set_text("");
        self.dictionary.written.set_text("");
        self.dictionary.row.set_expanded(false);
        self.refresh();
        (self.message)("Dictionary entry saved locally.");
    }

    fn save_snippet(self: &Rc<Self>) {
        let application = self.snippet.application.text();
        let typed = self.snippet.typed.text();
        let result = self.store.borrow_mut().save_snippet(
            &self.snippet.trigger.text(),
            &view_text(&self.snippet.expansion),
            optional(&typed),
            optional(&application),
        );
        if let Err(error) = result {
            (self.message)(&format!("Snippet could not be saved: {error}"));
            return;
        }
        self.snippet.trigger.set_text("");
        self.snippet.typed.set_text("");
        self.snippet.expansion.buffer().set_text("");
        self.snippet.row.set_expanded(false);
        self.refresh();
        (self.message)("Snippet saved locally.");
    }

    fn save_style(self: &Rc<Self>) {
        let name = self.style.name.text();
        let instructions = view_text(&self.style.instructions);
        let result = if let Some(identifier) = self.editing_style.borrow().as_deref() {
            self.store
                .borrow_mut()
                .update_style(identifier, &name, &instructions)
        } else {
            let styles = self.store.borrow().styles();
            if self.edit_prompt.is_some()
                && styles.as_ref().is_ok_and(|styles| {
                    styles.iter().any(|style| {
                        caseless_name(&style.name) == caseless_name(mluva_core::text::trim(&name))
                    })
                })
            {
                Err(mluva_core::database::StoreError::Invalid(
                    "Choose a new name, or edit the existing prompt with its settings button."
                        .into(),
                ))
            } else {
                self.store.borrow_mut().save_style(&name, &instructions)
            }
        };
        if let Err(error) = result {
            (self.message)(&format!("Custom style could not be saved: {error}"));
            return;
        }
        self.clear_style_editor();
        self.style.row.set_expanded(false);
        self.refresh();
        (self.styles_changed)();
        (self.message)("Custom output style saved locally.");
    }

    fn begin_style_edit(&self, style: &SavedStyle) {
        self.editing_style.replace(Some(style.identifier.clone()));
        self.style.name.set_text(&style.name);
        self.style
            .instructions
            .buffer()
            .set_text(&style.instructions);
        self.style.save.set_label("Save custom style");
        self.style.cancel.set_visible(true);
        self.style.row.set_expanded(true);
        (self.message)(&format!("Editing custom style “{}”.", style.name));
    }

    fn clear_style_editor(&self) {
        self.editing_style.replace(None);
        self.style.name.set_text("");
        self.style.instructions.buffer().set_text("");
        self.style.save.set_label("Create custom style");
        self.style.cancel.set_visible(false);
    }
}

fn caseless_name(value: &str) -> String {
    caseless::default_case_fold_str(value)
}
fn optional(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}
fn view_text(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .into()
}
fn margins(widget: &impl IsA<gtk::Widget>, size: i32) {
    widget.set_margin_top(size);
    widget.set_margin_bottom(size);
    widget.set_margin_start(size);
    widget.set_margin_end(size);
}
fn content(spacing: i32) -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, spacing);
    margins(&content, 16);
    content
}
fn clamp(widget: &impl IsA<gtk::Widget>) -> adw::Clamp {
    adw::Clamp::builder()
        .maximum_size(680)
        .tightening_threshold(640)
        .child(widget)
        .build()
}
fn list() -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    list.add_css_class("boxed-list");
    list
}
fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
fn action(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build()
}
fn button(label: &str, class: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(label)
        .valign(gtk::Align::Center)
        .build();
    if !class.is_empty() {
        button.add_css_class(class);
    }
    button
}
fn subpage(
    editor: Option<&adw::ExpanderRow>,
    title: &str,
    description: &str,
    list: &gtk::ListBox,
) -> gtk::ScrolledWindow {
    let content = content(16);
    if let Some(editor) = editor {
        let group = adw::PreferencesGroup::new();
        group.add(editor);
        content.append(&group);
    }
    let saved = adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .build();
    saved.add(list);
    content.append(&saved);
    gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .child(&clamp(&content))
        .build()
}
fn text_editor(title: &str, view: &gtk::TextView, height: i32, description: &str) -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    margins(&content, 12);
    let label = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .use_underline(true)
        .mnemonic_widget(view)
        .build();
    label.add_css_class("heading");
    content.append(&label);
    let help = gtk::Label::builder()
        .label(description)
        .xalign(0.0)
        .wrap(true)
        .build();
    help.add_css_class("caption");
    help.add_css_class("dim-label");
    content.append(&help);
    content.append(
        &gtk::ScrolledWindow::builder()
            .min_content_height(height)
            .child(view)
            .build(),
    );
    content
}
fn dictionary_editor() -> DictionaryEditor {
    let row = adw::ExpanderRow::builder()
        .title("Add dictionary entry")
        .subtitle("Map one exact spoken phrase to its written form")
        .build();
    let spoken = adw::EntryRow::builder().title("Spoken phrase").build();
    let written = adw::EntryRow::builder()
        .title("Written replacement")
        .build();
    let application = adw::EntryRow::builder()
        .title("Application identifier (optional)")
        .build();
    application.set_tooltip_text(Some("Leave blank for every application, or use the local executable identity shown by your application."));
    let case = adw::ComboRow::builder()
        .title("Capitalization")
        .model(&gtk::StringList::new(&[
            "Fixed written form",
            "Match spoken pattern",
        ]))
        .build();
    row.add_row(&spoken);
    row.add_row(&written);
    row.add_row(&application);
    row.add_row(&case);
    let save = button("Save entry", "suggested-action");
    let action = action("Create or update exact phrase", "");
    action.add_suffix(&save);
    row.add_row(&action);
    DictionaryEditor {
        row,
        spoken,
        written,
        application,
        case,
        save,
    }
}
fn snippet_editor() -> SnippetEditor {
    let row = adw::ExpanderRow::builder()
        .title("Add snippet")
        .subtitle("Expand a spoken trigger; optional typed triggers remain stored but inactive")
        .build();
    let trigger = adw::EntryRow::builder().title("Spoken trigger").build();
    let typed = adw::EntryRow::builder()
        .title("Exact typed trigger (optional)")
        .build();
    typed.set_tooltip_text(Some("Desktop-wide typed expansion stays disabled until a secure Wayland input boundary is available."));
    let application = adw::EntryRow::builder()
        .title("Application identifier (optional)")
        .build();
    let expansion = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    row.add_row(&trigger);
    row.add_row(&typed);
    row.add_row(&application);
    row.add_row(&text_editor(
        "_Expansion",
        &expansion,
        110,
        "Variables: {{date}}, {{time}}, {{datetime}}, and {{weekday}}.",
    ));
    let save = button("Save snippet", "suggested-action");
    let action = action("Create or update exact trigger", "");
    action.add_suffix(&save);
    row.add_row(&action);
    SnippetEditor {
        row,
        trigger,
        typed,
        application,
        expansion,
        save,
    }
}
fn style_editor() -> StyleEditor {
    let row = adw::ExpanderRow::builder()
        .title("Create custom style")
        .subtitle("Custom instructions are sent with dictated text only when selected")
        .build();
    let name = adw::EntryRow::builder().title("Custom style name").build();
    let instructions = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    row.add_row(&name);
    row.add_row(&text_editor(
        "_Full custom instructions",
        &instructions,
        140,
        "The target application identity is never sent to Codex.",
    ));
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let save = button("Create custom style", "suggested-action");
    let cancel = button("Cancel edit", "");
    cancel.set_visible(false);
    actions.append(&save);
    actions.append(&cancel);
    let action = action("Custom output mode", "");
    action.add_suffix(&actions);
    row.add_row(&action);
    StyleEditor {
        row,
        name,
        instructions,
        save,
        cancel,
    }
}
