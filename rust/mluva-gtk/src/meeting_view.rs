//! Explicit Meeting capture controls and the separate private review archive.

use crate::{
    conversation_view::history_timestamp, document_layout::margins, prompt_editor::Message,
};
use adw::prelude::*;
use mluva_core::{
    database::{StoreError, StoreResult},
    feature_maturity,
    meeting::{MeetingRecognitionStatus, MeetingRecord, MeetingStore, meeting_timestamp},
    text,
};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
};

pub type MeetingAction = Rc<dyn Fn(&MeetingRecord)>;
pub type DeleteMeeting = Rc<dyn Fn(&MeetingRecord) -> bool>;
pub struct MeetingCallbacks {
    pub toggle_capture: Rc<dyn Fn()>,
    pub copy_text: Message,
    pub retry_recognition: MeetingAction,
    pub delete_meeting: DeleteMeeting,
    pub show_message: Message,
}
pub struct SummaryRow {
    pub widget: gtk::Box,
    pub title: gtk::Label,
    pub subtitle: gtk::Label,
}
impl SummaryRow {
    fn new(title: &str) -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.set_margin_top(8);
        widget.set_margin_bottom(8);
        let title = gtk::Label::builder()
            .label(title)
            .xalign(0.0)
            .wrap(true)
            .build();
        let subtitle = gtk::Label::builder().xalign(0.0).wrap(true).build();
        subtitle.add_css_class("caption");
        subtitle.add_css_class("dim-label");
        widget.append(&title);
        widget.append(&subtitle);
        Self {
            widget,
            title,
            subtitle,
        }
    }
}
pub struct MeetingPage {
    pub widget: gtk::Box,
    pub status: gtk::Label,
    pub audio_routes: SummaryRow,
    pub privacy: SummaryRow,
    pub record_button: gtk::Button,
    pub count: gtk::Label,
    pub archive: gtk::Stack,
    pub error_page: adw::StatusPage,
    pub list: gtk::ListBox,
    pub scroll: gtk::ScrolledWindow,
    pub meeting_rows: RefCell<Vec<(String, adw::ExpanderRow)>>,
    pub store: Arc<Mutex<MeetingStore>>,
    export_directory: PathBuf,
    time_format: RefCell<String>,
    callbacks: MeetingCallbacks,
}
impl MeetingPage {
    pub fn new(
        store: Arc<Mutex<MeetingStore>>,
        export_directory: PathBuf,
        callbacks: MeetingCallbacks,
        time_format: String,
    ) -> StoreResult<Rc<Self>> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        margins(&content, 16);
        content.append(&maturity_notice());
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("card");
        let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
        margins(&body, 16);
        card.append(&body);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let icon = gtk::Image::from_icon_name("system-users-symbolic");
        icon.set_pixel_size(32);
        icon.add_css_class("accent");
        icon.set_valign(gtk::Align::Start);
        heading.append(&icon);
        let status_copy = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = gtk::Label::builder()
            .label("Ready for an explicit Meeting")
            .xalign(0.0)
            .wrap(true)
            .build();
        title.add_css_class("title-2");
        status_copy.append(&title);
        let status = gtk::Label::builder()
            .label("Meeting is idle")
            .xalign(0.0)
            .wrap(true)
            .selectable(false)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        status.add_css_class("dim-label");
        status_copy.append(&status);
        heading.append(&status_copy);
        body.append(&heading);
        let audio_routes = SummaryRow::new("Audio routes");
        let privacy = SummaryRow::new("");
        let summaries = gtk::Box::new(gtk::Orientation::Vertical, 0);
        summaries.append(&audio_routes.widget);
        summaries.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        summaries.append(&privacy.widget);
        body.append(&summaries);
        let boundary = gtk::Expander::new(Some("Capture boundary and cloud processing"));
        let detail = gtk::Label::builder().label("System output is desktop-wide sink audio, not per-window capture. After Stop, audio is uploaded to ElevenLabs Scribe v2 for transcription and speaker diarization. Meeting never pastes text.").xalign(0.0).wrap(true).margin_top(8).build();
        boundary.set_child(Some(&detail));
        body.append(&boundary);
        content.append(&card);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::builder()
            .label("Meeting archive")
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
        let empty = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        empty.add_css_class("boxed-list");
        let row = adw::ActionRow::builder()
            .title("No saved meetings yet")
            .subtitle("Only explicit non-Incognito Meeting captures appear here.")
            .build();
        let icon = gtk::Image::from_icon_name("system-users-symbolic");
        icon.set_pixel_size(32);
        row.add_prefix(&icon);
        empty.append(&row);
        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.set_valign(gtk::Align::Start);
        wrapper.append(&empty);
        archive.add_named(&wrapper, Some("empty"));
        let error_page = adw::StatusPage::builder()
            .title("Meeting archive needs repair")
            .description(
                "The malformed archive was preserved; retained Meeting writes remain disabled.",
            )
            .icon_name("dialog-warning-symbolic")
            .vexpand(true)
            .build();
        error_page.add_css_class("compact");
        error_page.set_size_request(-1, 160);
        archive.add_named(&error_page, Some("error"));
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list.add_css_class("boxed-list");
        archive.add_named(&list, Some("meetings"));
        content.append(&archive);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&clamp(&content))
            .build();
        widget.append(&scroll);
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("vs-dock");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        margins(&content, 12);
        let record_button = gtk::Button::builder().hexpand(true).build();
        record_button.add_css_class("vs-record");
        record_button.add_css_class("suggested-action");
        button_content(
            &record_button,
            "system-users-symbolic",
            "Start Meeting capture",
        );
        record_button.set_size_request(-1, 56);
        content.append(&record_button);
        let hint = gtk::Label::builder()
            .label("Always explicit · microphone + system output · never auto-pastes")
            .xalign(0.5)
            .wrap(true)
            .build();
        hint.add_css_class("caption");
        content.append(&hint);
        bar.append(&clamp(&content));
        widget.append(&bar);
        let owner = Rc::new(Self {
            widget,
            status,
            audio_routes,
            privacy,
            record_button,
            count,
            archive,
            error_page,
            list,
            scroll,
            meeting_rows: RefCell::new(vec![]),
            store,
            export_directory,
            time_format: RefCell::new(time_format),
            callbacks,
        });
        let weak = Rc::downgrade(&owner);
        owner.record_button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.toggle_capture)();
            }
        });
        owner.set_privacy(false);
        owner.set_audio_routes("Default (automatic)", "Default (automatic)");
        owner.refresh()?;
        Ok(owner)
    }
    pub fn set_audio_routes(&self, microphone: &str, system: &str) {
        self.audio_routes.subtitle.set_label(&format!(
            "{microphone} + everything playing through {system}"
        ));
    }
    pub fn set_privacy(&self, incognito: bool) {
        let (title, subtitle) = if incognito {
            (
                "Incognito for the next Meeting",
                "Uploaded to ElevenLabs after Stop; local audio and Meeting metadata are erased",
            )
        } else {
            (
                "Private local archive",
                "Uploaded to ElevenLabs after Stop; mixed audio is retained for explicit retry",
            )
        };
        self.privacy.title.set_label(title);
        self.privacy.subtitle.set_label(subtitle);
    }
    pub fn set_status(&self, message: &str) {
        self.status.set_label(message);
    }
    pub fn set_time_format(&self, value: String) {
        self.time_format.replace(value);
    }
    pub fn set_capture_state(&self, recording: bool, processing: bool) {
        let (icon, label) = if processing {
            ("content-loading-symbolic", "Transcribing Meeting…")
        } else if recording {
            (
                "media-playback-stop-symbolic",
                "Stop and transcribe Meeting",
            )
        } else {
            ("system-users-symbolic", "Start Meeting capture")
        };
        button_content(&self.record_button, icon, label);
        self.record_button.set_sensitive(!processing);
        self.record_button.remove_css_class("destructive-action");
        self.record_button.remove_css_class("suggested-action");
        if !processing {
            self.record_button.add_css_class(if recording {
                "destructive-action"
            } else {
                "suggested-action"
            });
        }
    }
    pub fn row(&self, identifier: &str) -> Option<adw::ExpanderRow> {
        self.meeting_rows
            .borrow()
            .iter()
            .find(|(id, _)| id == identifier)
            .map(|(_, row)| row.clone())
    }
    pub fn refresh(self: &Rc<Self>) -> StoreResult<()> {
        let expanded: BTreeSet<_> = self
            .meeting_rows
            .borrow()
            .iter()
            .filter(|(_, row)| row.is_expanded())
            .map(|(id, _)| id.clone())
            .collect();
        let position = self.scroll.vadjustment().value();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        self.meeting_rows.borrow_mut().clear();
        let (meetings, damaged) = {
            let archive = self
                .store
                .lock()
                .map_err(|_| StoreError::Invalid("Meeting archive owner is unavailable.".into()))?;
            (
                archive.recent(100).to_vec(),
                archive.persistence_error.is_some(),
            )
        };
        self.count.set_label(&format!("{} saved", meetings.len()));
        if damaged {
            self.error_page.set_description(Some("The existing file was preserved unchanged. Repair it before retaining another Meeting, or use Incognito for a non-persistent session."));
            self.archive.set_visible_child_name("error");
        } else if meetings.is_empty() {
            self.archive.set_visible_child_name("empty");
        } else {
            self.archive.set_visible_child_name("meetings");
            for meeting in meetings {
                let row = self.build_meeting(&meeting)?;
                row.set_expanded(expanded.contains(&meeting.identifier));
                self.list.append(&row);
                self.meeting_rows
                    .borrow_mut()
                    .push((meeting.identifier.clone(), row));
            }
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
    fn build_meeting(self: &Rc<Self>, meeting: &MeetingRecord) -> StoreResult<adw::ExpanderRow> {
        let preview = meeting
            .transcript
            .split(text::whitespace)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let title = meeting.title.clone().unwrap_or_else(|| {
            if preview.is_empty() {
                "Transcription failed — audio retained".into()
            } else {
                preview.chars().take(80).collect()
            }
        });
        let captured = history_timestamp(&meeting.timestamp, &self.time_format.borrow())
            .unwrap_or_else(|_| meeting.timestamp.clone());
        let retained = self
            .store
            .lock()
            .map_err(|_| StoreError::Invalid("Meeting archive owner is unavailable.".into()))?
            .recording_path(meeting)
            .is_some();
        let status = match meeting.recognition_status {
            MeetingRecognitionStatus::Completed => "Completed",
            MeetingRecognitionStatus::Failed => "Failed",
        };
        let subtitle = format!(
            "{status} · {}{} · {captured}",
            duration(meeting.duration_seconds),
            if retained { " · recovery audio" } else { "" }
        );
        let row = adw::ExpanderRow::builder()
            .title(&title)
            .subtitle(&subtitle)
            .build();
        let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        margins(&title_box, 12);
        let entry = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text("Optional title")
            .text(meeting.title.as_deref().unwrap_or(""))
            .build();
        title_box.append(&entry);
        let save = gtk::Button::with_label("Save title");
        title_box.append(&save);
        row.add_row(&title_box);
        let weak = Rc::downgrade(self);
        let identifier = meeting.identifier.clone();
        save.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                let title = text::trim(&entry.text()).to_owned();
                let result = owner
                    .store
                    .lock()
                    .map_err(|_| {
                        StoreError::Invalid("Meeting archive owner is unavailable.".into())
                    })
                    .and_then(|mut store| {
                        store.rename(&identifier, (!title.is_empty()).then_some(title))
                    });
                if let Err(error) = result {
                    (owner.callbacks.show_message)(&format!(
                        "Meeting title could not be saved: {error}"
                    ));
                    return;
                }
                if let Err(error) = owner.refresh() {
                    (owner.callbacks.show_message)(&error.to_string());
                    return;
                }
                (owner.callbacks.show_message)("Meeting title saved.");
            }
        });
        let notes = adw::ExpanderRow::builder()
            .title("Summary and actions")
            .subtitle("Review generated notes, decisions, action items, and speaker turns")
            .build();
        notes.add_row(&text_block(
            "Summary",
            if meeting.insights.summary.is_empty() {
                "No summary available."
            } else {
                &meeting.insights.summary
            },
        ));
        notes.add_row(&text_block(
            "Decisions",
            &line_list(&meeting.insights.decisions),
        ));
        notes.add_row(&text_block(
            "Action items",
            &line_list(&meeting.insights.action_items),
        ));
        let speakers = meeting
            .speakers
            .iter()
            .map(|segment| {
                format!(
                    "[{}] {}: {}",
                    meeting_timestamp(segment.started_at_seconds),
                    segment.speaker,
                    segment.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        notes.add_row(&text_block(
            "Speakers",
            if speakers.is_empty() {
                "Speaker labels unavailable."
            } else {
                &speakers
            },
        ));
        row.add_row(&notes);
        let transcript = adw::ExpanderRow::builder()
            .title("Transcript")
            .subtitle(if preview.is_empty() {
                "Transcription has not completed".into()
            } else {
                preview.chars().take(100).collect::<String>()
            })
            .build();
        transcript.add_row(&text_block(
            "Full transcript",
            if meeting.transcript.is_empty() {
                "Transcription has not completed."
            } else {
                &meeting.transcript
            },
        ));
        row.add_row(&transcript);
        if !meeting.warnings.is_empty() {
            let warnings = adw::ExpanderRow::builder()
                .title("Capture warnings")
                .subtitle(format!(
                    "{} item{}",
                    meeting.warnings.len(),
                    if meeting.warnings.len() == 1 { "" } else { "s" }
                ))
                .build();
            warnings.add_row(&text_block("Warnings", &meeting.warnings.join("\n")));
            row.add_row(&warnings);
        }
        let actions = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .row_spacing(8)
            .column_spacing(8)
            .max_children_per_line(3)
            .build();
        margins(&actions, 12);
        let copy = gtk::Button::with_label("Copy transcript");
        copy.set_sensitive(!meeting.transcript.is_empty());
        actions.append(&copy);
        let weak = Rc::downgrade(self);
        let snapshot = meeting.clone();
        copy.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.copy_text)(&snapshot.transcript);
            }
        });
        let retry = gtk::Button::with_label("Retry transcription");
        retry.set_visible(
            meeting.recognition_status == MeetingRecognitionStatus::Failed && retained,
        );
        actions.append(&retry);
        let weak = Rc::downgrade(self);
        let snapshot = meeting.clone();
        retry.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.retry_recognition)(&snapshot);
            }
        });
        for (format, title) in [("markdown", "Export Markdown"), ("json", "Export JSON")] {
            let button = gtk::Button::with_label(title);
            actions.append(&button);
            let weak = Rc::downgrade(self);
            let snapshot = meeting.clone();
            button.connect_clicked(move |_| {
                if let Some(owner) = weak.upgrade() {
                    let result = owner
                        .store
                        .lock()
                        .map_err(|_| {
                            StoreError::Invalid("Meeting archive owner is unavailable.".into())
                        })
                        .and_then(|store| store.export(&snapshot, &owner.export_directory, format));
                    match result {
                        Ok(path) => (owner.callbacks.show_message)(&format!(
                            "Exported Meeting to {}",
                            path.display()
                        )),
                        Err(error) => (owner.callbacks.show_message)(&format!(
                            "Meeting export failed: {error}"
                        )),
                    }
                }
            });
        }
        let delete = gtk::Button::with_label("Delete");
        delete.add_css_class("destructive-action");
        actions.append(&delete);
        let weak = Rc::downgrade(self);
        let snapshot = meeting.clone();
        delete.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.confirm_delete(&snapshot);
            }
        });
        row.add_row(&actions);
        Ok(row)
    }
    fn confirm_delete(self: &Rc<Self>, meeting: &MeetingRecord) {
        let dialog = adw::AlertDialog::new(
            Some("Delete this Meeting?"),
            Some(
                "The transcript, notes, metadata, and retained recording will be permanently deleted.",
            ),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete permanently")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        let snapshot = meeting.clone();
        dialog.choose(
            Some(&self.widget),
            gio::Cancellable::NONE,
            move |response| {
                if response == "delete"
                    && let Some(owner) = weak.upgrade()
                    && (owner.callbacks.delete_meeting)(&snapshot)
                {
                    if let Err(error) = owner.refresh() {
                        (owner.callbacks.show_message)(&error.to_string());
                        return;
                    }
                    (owner.callbacks.show_message)(
                        "Meeting and its retained recording permanently deleted.",
                    );
                }
            },
        );
    }
}
fn text_block(title: &str, text: &str) -> gtk::Box {
    let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
    margins(&widget, 12);
    let heading = gtk::Label::builder().label(title).xalign(0.0).build();
    heading.add_css_class("heading");
    widget.append(&heading);
    widget.append(
        &gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build(),
    );
    widget
}
fn line_list(values: &[String]) -> String {
    if values.is_empty() {
        "None recorded.".into()
    } else {
        values
            .iter()
            .map(|value| format!("• {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn duration(seconds: f64) -> String {
    let seconds = (seconds + 0.5).max(0.0) as u64;
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    if hours == 0 {
        format!("{minutes}:{:02}", seconds % 60)
    } else {
        format!("{hours}:{minutes:02}:{:02}", seconds % 60)
    }
}
fn clamp(widget: &impl IsA<gtk::Widget>) -> adw::Clamp {
    adw::Clamp::builder()
        .maximum_size(680)
        .tightening_threshold(640)
        .child(widget)
        .build()
}
fn button_content(button: &gtk::Button, icon: &str, label: &str) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
}
fn maturity_notice() -> gtk::Box {
    let capability =
        feature_maturity::capability("meeting_mode").expect("released Meeting registry");
    let widget = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    widget.add_css_class("vs-maturity-notice");
    let badge = gtk::Label::new(Some(capability.maturity.label()));
    badge.add_css_class("vs-maturity-badge");
    badge.add_css_class("vs-experimental");
    badge.set_valign(gtk::Align::Center);
    let detail = gtk::Label::builder()
        .label(capability.summary)
        .xalign(0.0)
        .wrap(true)
        .hexpand(true)
        .build();
    detail.add_css_class("vs-maturity-detail");
    widget.append(&badge);
    widget.append(&detail);
    widget
}
