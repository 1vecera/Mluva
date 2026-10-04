//! Durable recording/conversation recovery and explicit local archive actions.

use crate::{
    conversation_view::history_timestamp, document_layout::margins, prompt_editor::Message,
};
use adw::prelude::*;
use mluva_core::{
    conversation::ConversationStore,
    database::{StoreError, StoreResult},
    feature_maturity,
    history::{HistoryEntry, HistoryStore},
    text,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    rc::Rc,
};

pub type HistoryAction = Rc<dyn Fn(&HistoryEntry)>;
pub type HistoryPredicate = Rc<dyn Fn(&HistoryEntry) -> bool>;
pub struct HistoryCallbacks {
    pub copy: Message,
    pub can_retry_delivery: HistoryPredicate,
    pub retry_delivery: HistoryAction,
    pub retry_recognition: HistoryAction,
    pub reprocess: HistoryAction,
    pub delete: HistoryPredicate,
    pub changed: Rc<dyn Fn()>,
    pub message: Message,
}
pub struct HistoryPage {
    pub widget: gtk::Box,
    pub count: gtk::Label,
    pub archive: gtk::Stack,
    pub list: gtk::ListBox,
    pub scroll: gtk::ScrolledWindow,
    pub rows: RefCell<BTreeMap<String, adw::ExpanderRow>>,
    pub title_entries: RefCell<BTreeMap<String, (gtk::Entry, String)>>,
    pub store: HistoryStore,
    pub conversations: Option<ConversationStore>,
    focused: RefCell<Option<String>>,
    time_format: RefCell<String>,
    export_directory: PathBuf,
    callbacks: HistoryCallbacks,
}
impl HistoryPage {
    pub fn new(
        store: HistoryStore,
        conversations: Option<ConversationStore>,
        export_directory: PathBuf,
        time_format: String,
        callbacks: HistoryCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        margins(&content, 16);
        content.append(&maturity());
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::builder()
            .label("Recent transcriptions")
            .xalign(0.0)
            .hexpand(true)
            .build();
        title.add_css_class("title-2");
        heading.append(&title);
        let count = gtk::Label::builder().xalign(1.0).build();
        count.add_css_class("dim-label");
        heading.append(&count);
        content.append(&heading);
        let archive = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .vexpand(true)
            .build();
        let empty = adw::StatusPage::builder()
            .title("No transcriptions yet")
            .description("Completed non-Incognito captures will remain recoverable here.")
            .icon_name("document-open-recent-symbolic")
            .vexpand(true)
            .build();
        empty.add_css_class("compact");
        empty.set_size_request(-1, 160);
        archive.add_named(&empty, Some("empty"));
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list.add_css_class("boxed-list");
        archive.add_named(&list, Some("entries"));
        content.append(&archive);
        let clamp = adw::Clamp::builder()
            .maximum_size(680)
            .tightening_threshold(640)
            .child(&content)
            .build();
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&clamp)
            .build();
        widget.append(&scroll);
        let owner = Rc::new(Self {
            widget,
            count,
            archive,
            list,
            scroll,
            store,
            conversations,
            export_directory,
            time_format: RefCell::new(time_format),
            focused: RefCell::new(None),
            rows: RefCell::new(BTreeMap::new()),
            title_entries: RefCell::new(BTreeMap::new()),
            callbacks,
        });
        owner.refresh()?;
        Ok(owner)
    }
    pub fn set_time_format(&self, value: String) {
        self.time_format.replace(value);
    }
    pub fn focused_identifier(&self) -> Option<String> {
        self.focused.borrow().clone()
    }
    pub fn focus_entry(self: &Rc<Self>, identifier: Option<&str>) -> StoreResult<()> {
        let identifier = match identifier {
            Some(identifier) => match self.store.recording_conversation(identifier) {
                Ok(entry) => Some(entry.identifier),
                Err(StoreError::NotFound) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        self.focused.replace(identifier.clone());
        self.refresh()?;
        if let Some(row) = identifier.and_then(|id| self.rows.borrow().get(&id).cloned()) {
            row.set_expanded(true);
        }
        Ok(())
    }
    pub fn refresh(self: &Rc<Self>) -> StoreResult<()> {
        let expanded: BTreeSet<_> = self
            .rows
            .borrow()
            .iter()
            .filter(|(_, row)| row.is_expanded())
            .map(|(id, _)| id.clone())
            .collect();
        let position = self.scroll.vadjustment().value();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.rows.borrow_mut().clear();
        self.title_entries.borrow_mut().clear();
        let mut entries = if let Some(store) = &self.conversations {
            store.search("", 100, None)?
        } else {
            self.store.recent(100)?
        };
        let focused = self.focused.borrow().clone();
        if let Some(id) = focused {
            match self.store.find(&id) {
                Ok(selected) => {
                    entries.retain(|entry| entry.identifier != selected.identifier);
                    entries.insert(0, selected);
                }
                Err(StoreError::NotFound) => {
                    self.focused.borrow_mut().take();
                }
                Err(error) => return Err(error),
            }
        }
        self.count.set_label(&format!("{} shown", entries.len()));
        self.archive.set_visible_child_name(if entries.is_empty() {
            "empty"
        } else {
            "entries"
        });
        for entry in entries {
            let row = self.build_conversation(&entry)?;
            row.set_expanded(expanded.contains(&entry.identifier));
            self.list.append(&row);
            self.rows.borrow_mut().insert(entry.identifier, row);
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(owner) = weak.upgrade() {
                let adjustment = owner.scroll.vadjustment();
                let maximum = adjustment
                    .lower()
                    .max(adjustment.upper() - adjustment.page_size());
                adjustment.set_value(position.min(maximum));
            }
        });
        Ok(())
    }
    pub fn refresh_title(&self, identifier: &str) -> StoreResult<()> {
        let Some(row) = self.rows.borrow().get(identifier).cloned() else {
            return Ok(());
        };
        let entry = self.store.find(identifier)?;
        let title = entry
            .title
            .as_deref()
            .filter(|v| !v.is_empty())
            .unwrap_or("Empty transcript");
        row.set_title(title);
        let mut fields = self.title_entries.borrow_mut();
        if let Some((field, previous)) = fields.get_mut(identifier) {
            if field.text().as_str() == previous && !field.has_focus() {
                field.set_text(entry.title.as_deref().unwrap_or(""));
            }
            *previous = entry.title.unwrap_or_default();
        }
        Ok(())
    }
    fn build_entry(self: &Rc<Self>, entry: &HistoryEntry) -> adw::ExpanderRow {
        let preview = if entry.delivered_text.is_empty() {
            &entry.raw_text
        } else {
            &entry.delivered_text
        }
        .split(text::whitespace)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        let title = entry
            .title
            .clone()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                if preview.is_empty() {
                    "Empty transcript".into()
                } else {
                    preview.chars().take(80).collect()
                }
            });
        let created = history_timestamp(&entry.created_at, &self.time_format.borrow())
            .unwrap_or_else(|_| entry.created_at.clone());
        let mode = text::title(&entry.mode);
        let row = adw::ExpanderRow::builder()
            .title(title)
            .subtitle(format!(
                "{mode} · {}{} · {created}",
                entry.delivery_outcome,
                if entry.retained_audio_path.is_some() {
                    " · recovery audio"
                } else {
                    ""
                }
            ))
            .build();
        let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        margins(&title_box, 12);
        let field = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text("Optional title")
            .text(entry.title.as_deref().unwrap_or(""))
            .build();
        title_box.append(&field);
        self.title_entries.borrow_mut().insert(
            entry.identifier.clone(),
            (field.clone(), entry.title.clone().unwrap_or_default()),
        );
        let save = gtk::Button::with_label("Save title");
        title_box.append(&save);
        row.add_row(&title_box);
        let weak = Rc::downgrade(self);
        let snapshot = entry.clone();
        save.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                let title = text::trim(&field.text()).to_owned();
                if let Err(error) = owner.store.update_title(
                    &snapshot.identifier,
                    (!title.is_empty()).then_some(title.as_str()),
                ) {
                    (owner.callbacks.message)(&format!(
                        "History title could not be saved: {error}"
                    ));
                    return;
                }
                if owner.rebuild() {
                    (owner.callbacks.message)("History title saved.");
                    (owner.callbacks.changed)();
                }
            }
        });
        let delivered = action_row(
            "Final text",
            if entry.delivered_text.is_empty() {
                "No final text is available"
            } else {
                &entry.delivered_text
            },
        );
        delivered.set_subtitle_lines(3);
        let copy = gtk::Button::builder()
            .label("Copy")
            .valign(gtk::Align::Center)
            .sensitive(!entry.delivered_text.is_empty())
            .build();
        delivered.add_suffix(&copy);
        self.connect_entry(&copy, entry, |owner, entry| {
            (owner.callbacks.copy)(&entry.delivered_text)
        });
        let paste = gtk::Button::builder()
            .label("Paste again")
            .valign(gtk::Align::Center)
            .sensitive((self.callbacks.can_retry_delivery)(entry))
            .tooltip_text(
                "Available only while this Mluva process still owns the exact accessible target.",
            )
            .build();
        delivered.add_suffix(&paste);
        self.connect_entry(&paste, entry, |owner, entry| {
            (owner.callbacks.retry_delivery)(entry)
        });
        row.add_row(&delivered);
        if !entry.delivered_text.is_empty() {
            row.add_row(&self.correction(entry));
        }
        row.add_row(&self.technical(entry));
        let actions = actions();
        let restore = gtk::Button::builder()
            .label("Restore raw")
            .sensitive(!entry.raw_text.is_empty() && entry.raw_text != entry.delivered_text)
            .build();
        actions.append(&restore);
        self.connect_entry(&restore, entry, |owner, entry| {
            if let Err(error) = owner.store.restore_raw(&entry.identifier) {
                (owner.callbacks.message)(&format!(
                    "Raw transcript could not be restored: {error}"
                ));
                return;
            }
            if owner.rebuild() {
                (owner.callbacks.changed)();
                (owner.callbacks.message)("Raw transcript restored in history.");
            }
        });
        let retry = gtk::Button::builder()
            .label("Retry transcription")
            .visible(entry.retained_audio_path.is_some() && entry.raw_text.is_empty())
            .build();
        actions.append(&retry);
        self.connect_entry(&retry, entry, |owner, entry| {
            (owner.callbacks.retry_recognition)(entry)
        });
        let reprocess=gtk::Button::builder().label("Reprocess raw").sensitive(!entry.raw_text.is_empty()).tooltip_text("Apply current spoken-structure, dictionary, and snippet rules locally without Codex.").build();
        actions.append(&reprocess);
        self.connect_entry(&reprocess, entry, |owner, entry| {
            (owner.callbacks.reprocess)(entry)
        });
        self.export_actions(&actions, entry);
        row.add_row(&actions);
        row
    }
    fn connect_entry(
        self: &Rc<Self>,
        button: &gtk::Button,
        entry: &HistoryEntry,
        action: impl Fn(&Rc<Self>, &HistoryEntry) + 'static,
    ) {
        let weak = Rc::downgrade(self);
        let entry = entry.clone();
        button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                action(&owner, &entry);
            }
        });
    }
    fn build_conversation(self: &Rc<Self>, entry: &HistoryEntry) -> StoreResult<adw::ExpanderRow> {
        let first = self.build_entry(entry);
        let segments = self.store.continuations(&entry.identifier)?;
        let Some(store) = self.conversations.as_ref().filter(|_| !segments.is_empty()) else {
            return Ok(first);
        };
        let replies = store.replies(&entry.identifier)?;
        let current = match replies.last() {
            Some(reply) => reply.text.clone(),
            None => store.source_text(entry, false)?,
        };
        let row = adw::ExpanderRow::builder()
            .title(first.title())
            .subtitle(format!("Dictation · {} recordings", segments.len() + 1))
            .build();
        let text = action_row("Current text", &current);
        text.set_subtitle_lines(3);
        let copy = gtk::Button::builder()
            .label("Copy")
            .valign(gtk::Align::Center)
            .build();
        text.add_suffix(&copy);
        let weak = Rc::downgrade(self);
        copy.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.copy)(&current);
            }
        });
        row.add_row(&text);
        let recordings = adw::ExpanderRow::builder()
            .title("Original recordings")
            .subtitle("Raw captures and recovery tools")
            .build();
        recordings.add_row(&first);
        for entry in segments {
            recordings.add_row(&self.build_entry(&entry));
        }
        row.add_row(&recordings);
        let actions = actions();
        self.export_actions(&actions, entry);
        row.add_row(&actions);
        Ok(row)
    }
    fn correction(self: &Rc<Self>, entry: &HistoryEntry) -> gtk::Box {
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
        margins(&box_, 12);
        let label = gtk::Label::builder()
            .label("_Correct final text")
            .xalign(0.0)
            .use_underline(true)
            .build();
        label.add_css_class("heading");
        box_.append(&label);
        let explanation=gtk::Label::builder().label("Small saved edits can become review-only vocabulary suggestions; nothing is learned automatically.").xalign(0.0).wrap(true).build();
        explanation.add_css_class("caption");
        explanation.add_css_class("dim-label");
        box_.append(&explanation);
        let view = gtk::TextView::builder()
            .wrap_mode(gtk::WrapMode::WordChar)
            .build();
        view.buffer().set_text(&entry.delivered_text);
        label.set_mnemonic_widget(Some(&view));
        let scroll = gtk::ScrolledWindow::builder()
            .min_content_height(110)
            .child(&view)
            .build();
        box_.append(&scroll);
        let save = gtk::Button::builder()
            .label("Save correction")
            .halign(gtk::Align::End)
            .build();
        save.add_css_class("suggested-action");
        box_.append(&save);
        let weak = Rc::downgrade(self);
        let entry = entry.clone();
        save.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                let buffer = view.buffer();
                let value = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
                let value = text::trim(&value);
                if value == entry.delivered_text {
                    (owner.callbacks.message)("The final text is unchanged.");
                    return;
                }
                if let Err(error) = owner.store.correct_delivered_text(&entry.identifier, value) {
                    (owner.callbacks.message)(&format!("Correction could not be saved: {error}"));
                    return;
                }
                if owner.rebuild() {
                    (owner.callbacks.changed)();
                    (owner.callbacks.message)(
                        "Correction saved locally. Review derived suggestions in Personalization.",
                    );
                }
            }
        });
        box_
    }
    fn technical(self: &Rc<Self>, entry: &HistoryEntry) -> adw::ExpanderRow {
        let details = adw::ExpanderRow::builder()
            .title("Technical details")
            .subtitle("Raw transcript, recognition route, processing context, and timings")
            .build();
        let raw = action_row(
            "Raw transcript",
            if entry.raw_text.is_empty() {
                "No raw transcript"
            } else {
                &entry.raw_text
            },
        );
        raw.set_subtitle_lines(3);
        let copy = gtk::Button::builder()
            .label("Copy raw")
            .valign(gtk::Align::Center)
            .sensitive(!entry.raw_text.is_empty())
            .build();
        raw.add_suffix(&copy);
        self.connect_entry(&copy, entry, |owner, entry| {
            (owner.callbacks.copy)(&entry.raw_text)
        });
        details.add_row(&raw);
        if let Some(application) = &entry.application_identifier {
            details.add_row(&action_row("Captured application scope", application));
        }
        let route = match entry.recognition_route.as_deref() {
            Some("scribe-v2-realtime") => "Scribe v2 Realtime",
            Some("scribe-v2-batch") => "Scribe v2 batch",
            Some("scribe-v2-batch-retry") => "Scribe v2 batch retry",
            None => "Legacy/unknown route",
            _ => "Unknown controlled route",
        };
        details.add_row(&action_row("Recognition route", route));
        if let Some(reason) = &entry.recognition_fallback_reason {
            details.add_row(&action_row(
                "Realtime fallback",
                match reason.as_str() {
                    "realtime-unavailable" => "Realtime was unavailable before capture",
                    "realtime-startup-failed" => "Realtime session startup failed",
                    "realtime-stream-failed" => "Realtime stream did not produce committed text",
                    _ => "Unknown controlled fallback",
                },
            ));
        }
        if entry.enhancement_provider_id.is_some() {
            let outcome = match entry.enhancement_outcome.as_deref() {
                Some("completed") => "completed",
                Some("raw-fallback") => "raw fallback",
                Some("safe-fallback") => "partial safe fallback",
                Some("failed") => "failed",
                _ => "unknown outcome",
            };
            details.add_row(&action_row(
                "Enhancement provider",
                &format!(
                    "Codex app-server · {} · {outcome}",
                    entry
                        .enhancement_model_identifier
                        .as_deref()
                        .unwrap_or("None")
                ),
            ));
            let context = entry
                .enhancement_context_sources
                .iter()
                .map(|value| match value.as_str() {
                    "selected-text" => "explicit selected text",
                    "style-instructions" => "saved style instructions",
                    "screenshots" => "attached screenshots",
                    _ => "unknown controlled context",
                })
                .collect::<Vec<_>>()
                .join(", ");
            details.add_row(&action_row(
                "Disclosed enhancement context",
                if context.is_empty() { "None" } else { &context },
            ));
        }
        let timings = [
            ("recognition", entry.recognition_ms),
            ("enhancement", entry.enhancement_ms),
            ("delivery", entry.delivery_ms),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.map(|value| format!("{label} {value} ms")))
        .collect::<Vec<_>>()
        .join(" · ");
        if !timings.is_empty() {
            details.add_row(&action_row("Stage timings", &timings));
        }
        details
    }
    fn export_actions(self: &Rc<Self>, actions: &gtk::FlowBox, entry: &HistoryEntry) {
        for (format, label) in [("markdown", "Export Markdown"), ("json", "Export JSON")] {
            let button = gtk::Button::with_label(label);
            actions.append(&button);
            self.connect_entry(&button, entry, move |owner, entry| {
                let result = (|| {
                    let (replies, source) = if let Some(store) = &owner.conversations {
                        (
                            store.replies(&entry.identifier)?,
                            Some(store.source_text(entry, false)?),
                        )
                    } else {
                        (vec![], None)
                    };
                    owner.store.export(
                        entry,
                        &owner.export_directory,
                        format,
                        &replies,
                        source.as_deref(),
                    )
                })();
                match result {
                    Ok(path) => {
                        (owner.callbacks.message)(&format!("Exported to {}", path.display()))
                    }
                    Err(error) => {
                        (owner.callbacks.message)(&format!("History export failed: {error}"))
                    }
                }
            });
        }
        let delete = gtk::Button::with_label("Delete");
        delete.add_css_class("destructive-action");
        actions.append(&delete);
        self.connect_entry(&delete,entry,|owner,entry|{
            let dialog=adw::AlertDialog::builder().heading("Delete this history entry?").body("Its original, saved rewrites and retained recovery audio will be permanently deleted.").default_response("cancel").close_response("cancel").build();
            dialog.add_response("cancel","Cancel");dialog.add_response("delete","Delete permanently");dialog.set_response_appearance("delete",adw::ResponseAppearance::Destructive);
            let weak=Rc::downgrade(owner);let entry=entry.clone();dialog.choose(Some(&owner.widget),gio::Cancellable::NONE,move |response|{
                if response=="delete" && let Some(owner)=weak.upgrade() && (owner.callbacks.delete)(&entry) && owner.rebuild(){(owner.callbacks.changed)();(owner.callbacks.message)("History entry and retained recovery audio permanently deleted.");}
            });
        });
    }
    fn rebuild(self: &Rc<Self>) -> bool {
        match self.refresh() {
            Ok(()) => true,
            Err(error) => {
                (self.callbacks.message)(&error.to_string());
                false
            }
        }
    }
}
fn action_row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build()
}
fn actions() -> gtk::FlowBox {
    let row = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .row_spacing(8)
        .column_spacing(8)
        .max_children_per_line(3)
        .build();
    margins(&row, 12);
    row
}
fn maturity() -> gtk::Box {
    let value = feature_maturity::capability("history").expect("released History maturity");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("vs-maturity-notice");
    let badge = gtk::Label::new(Some(value.maturity.label()));
    badge.add_css_class("vs-maturity-badge");
    badge.add_css_class(if value.maturity == feature_maturity::Maturity::Verified {
        "vs-verified"
    } else {
        "vs-experimental"
    });
    badge.set_valign(gtk::Align::Center);
    row.append(&badge);
    let detail = gtk::Label::builder()
        .label(value.summary)
        .xalign(0.0)
        .wrap(true)
        .hexpand(true)
        .build();
    detail.add_css_class("vs-maturity-detail");
    row.append(&detail);
    row
}
