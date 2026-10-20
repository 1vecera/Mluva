//! The real capture page, recovery editor and recording controls around the conversation workspace.

use crate::conversation_view::ConversationWorkspace;
use crate::document_layout::margins;
use adw::prelude::*;
use mluva_core::{config::AppConfig, database::StoreResult, text};
use std::cell::RefCell;
use std::rc::Rc;

pub struct CaptureCallbacks {
    pub toggle_recording: Rc<dyn Fn()>,
    pub toast: Rc<dyn Fn(&str)>,
    pub retry_initialization: Rc<dyn Fn()>,
    pub announce: Rc<dyn Fn(&str)>,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureViewState {
    pub preparing: bool,
    pub recording: bool,
    pub processing: bool,
    pub initialization_failed: bool,
    pub meeting_busy: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct CaptureShortcutState {
    pub recording_trigger: Option<String>,
    pub latest_trigger: Option<String>,
    pub shortcut_service_available: bool,
    pub target_tracking_available: bool,
}

pub struct CapturePage {
    pub widget: adw::ToolbarView,
    pub workspace: Rc<ConversationWorkspace>,
    pub setup_callout: gtk::Revealer,
    pub setup_title: gtk::Label,
    pub setup_body: gtk::Label,
    pub setup_button: gtk::Button,
    pub status: gtk::Label,
    pub status_title: gtk::Label,
    pub action_hint: gtk::Label,
    pub action_bar: gtk::Box,
    pub buttons: gtk::Box,
    pub record_button: gtk::Button,
    config: RefCell<AppConfig>,
    state: RefCell<CaptureViewState>,
    shortcuts: RefCell<CaptureShortcutState>,
    callbacks: CaptureCallbacks,
    recording_action: RefCell<Rc<dyn Fn()>>,
}

fn button_content(button: &gtk::Button, icon: &str, label: &str) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.append(&gtk::Image::from_icon_name(icon));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
}

impl CapturePage {
    pub fn new(
        workspace: Rc<ConversationWorkspace>,
        config: AppConfig,
        callbacks: CaptureCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let widget = adw::ToolbarView::new();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let setup_callout = gtk::Revealer::builder().reveal_child(false).build();
        let callout_card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        callout_card.add_css_class("vs-callout");
        margins(&callout_card, 12);
        let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
        icon.add_css_class("warning");
        icon.set_valign(gtk::Align::Start);
        callout_card.append(&icon);
        let callout_copy = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();
        let setup_title = gtk::Label::builder()
            .label("Setup required")
            .xalign(0.0)
            .build();
        setup_title.add_css_class("vs-callout-title");
        callout_copy.append(&setup_title);
        let setup_body = gtk::Label::builder().xalign(0.0).wrap(true).build();
        setup_body.add_css_class("vs-callout-body");
        callout_copy.append(&setup_body);
        callout_card.append(&callout_copy);
        let setup_button = gtk::Button::builder()
            .label("Dismiss")
            .valign(gtk::Align::Center)
            .build();
        callout_card.append(&setup_button);
        setup_callout.set_child(Some(&callout_card));
        body.append(&setup_callout);
        workspace.widget.set_vexpand(true);
        body.append(&workspace.widget);

        widget.set_content(Some(&body));

        let status = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .valign(gtk::Align::End)
            .accessible_role(gtk::AccessibleRole::Status)
            .build();
        status.add_css_class("ml-capture-status");
        let status_title = gtk::Label::builder()
            .label("Ready to dictate")
            .xalign(0.0)
            .visible(false)
            .build();
        status_title.add_css_class("heading");
        let action_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        action_bar.add_css_class("ml-recording-dock");
        margins(&action_bar, 16);
        action_bar.set_margin_top(8);
        let status_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .valign(gtk::Align::End)
            .build();
        status_box.append(&status_title);
        status_box.append(&status);
        let action_hint = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .visible(false)
            .build();
        action_hint.add_css_class("caption");
        status_box.append(&action_hint);
        action_bar.append(&status_box);
        let buttons = gtk::Box::builder()
            .spacing(8)
            .halign(gtk::Align::End)
            .build();
        action_bar.append(&buttons);
        let record_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .tooltip_text("Start or stop dictation · F9")
            .build();
        record_button.add_css_class("suggested-action");
        record_button.add_css_class("ml-record-toggle");
        button_content(&record_button, "audio-input-microphone-symbolic", "Dictate");
        buttons.append(&record_button);
        workspace.set_capture_controls(&action_bar);
        let page = Rc::new(Self {
            widget,
            workspace,
            setup_callout,
            setup_title,
            setup_body,
            setup_button,
            status,
            status_title,
            action_hint,
            action_bar,
            buttons,
            record_button,
            config: RefCell::new(config.clone()),
            state: RefCell::new(CaptureViewState::default()),
            shortcuts: RefCell::new(CaptureShortcutState::default()),
            recording_action: RefCell::new(callbacks.toggle_recording.clone()),
            callbacks,
        });
        page.bind();
        page.set_config(config)?;
        Ok(page)
    }

    fn bind(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.record_button.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                let action = page.recording_action.borrow().clone();
                action();
            }
        });
        let weak = Rc::downgrade(self);
        self.setup_button.connect_clicked(move |_| {
            if let Some(page) = weak.upgrade() {
                if page.state.borrow().initialization_failed {
                    (page.callbacks.retry_initialization)();
                } else {
                    page.setup_callout.set_reveal_child(false);
                }
            }
        });
    }

    pub fn config(&self) -> AppConfig {
        self.config.borrow().clone()
    }

    pub fn set_recording_action(&self, action: Rc<dyn Fn()>) {
        self.recording_action.replace(action);
    }
    pub fn toast(&self, message: &str) {
        (self.callbacks.toast)(message);
    }
    pub fn view_state(&self) -> CaptureViewState {
        self.state.borrow().clone()
    }

    pub fn set_config(self: &Rc<Self>, config: AppConfig) -> StoreResult<()> {
        self.config.replace(config.clone());
        self.workspace.set_config(config)?;
        self.refresh_action_hint();
        Ok(())
    }

    pub(crate) fn shortcut_state(&self) -> CaptureShortcutState {
        self.shortcuts.borrow().clone()
    }

    pub fn set_shortcuts(&self, state: CaptureShortcutState) {
        self.shortcuts.replace(state);
        self.refresh_action_hint();
    }

    fn refresh_action_hint(&self) {
        let shortcuts = self.shortcuts.borrow();
        let config = self.config.borrow();
        let mut hint = match shortcuts.recording_trigger.as_deref() {
            None if shortcuts.shortcut_service_available => format!(
                "{} awaits desktop approval · this button always copies",
                config.global_recording_key
            ),
            None => "Use the on-screen button for copy-only capture".into(),
            Some(trigger) if shortcuts.target_tracking_available => format!(
                "{trigger} toggles global capture and can insert into a text field or terminal · this button always copies"
            ),
            Some(trigger) => {
                format!("{trigger} toggles capture · text is copied when paste is unavailable")
            }
        };
        if let Some(trigger) = &shortcuts.latest_trigger {
            hint += &format!(" · {trigger} opens the latest dictation");
        }
        self.action_hint.set_label(&hint);
    }

    pub fn set_status(&self, message: &str) {
        self.status.remove_css_class("error");
        self.status.set_label(message);
        self.status.set_tooltip_text(Some(message));
    }

    pub fn set_view_state(&self, state: CaptureViewState) {
        let title = if state.initialization_failed {
            "Capture unavailable"
        } else if state.processing {
            "Finishing capture"
        } else if state.preparing {
            "Preparing capture"
        } else if state.recording {
            "Recording"
        } else {
            "Ready to dictate"
        };
        self.status_title.set_label(title);
        if state.preparing || state.recording {
            button_content(
                &self.record_button,
                if state.preparing {
                    "process-stop-symbolic"
                } else {
                    "media-playback-stop-symbolic"
                },
                if state.preparing {
                    "Cancel preparation"
                } else {
                    "Stop"
                },
            );
            self.record_button.remove_css_class("suggested-action");
            self.record_button.add_css_class("destructive-action");
            self.record_button.set_sensitive(true);
        } else if state.processing {
            self.record_button.set_sensitive(false);
        } else {
            button_content(
                &self.record_button,
                "audio-input-microphone-symbolic",
                "Dictate",
            );
            self.record_button.remove_css_class("destructive-action");
            self.record_button.add_css_class("suggested-action");
            self.record_button.set_sensitive(!state.meeting_busy);
        }
        if state.initialization_failed {
            self.record_button.set_sensitive(false);
        }
        self.state.replace(state);
    }

    pub fn set_error(&self, message: &str) {
        self.status.remove_css_class("error");
        let failed = self.state.borrow().initialization_failed;
        self.setup_title.set_label(if failed {
            "Setup required"
        } else {
            "Capture problem"
        });
        let summary = text::trim(message.split(". ").next().unwrap());
        self.setup_body
            .set_label(if summary.is_empty() { message } else { summary });
        self.setup_body.set_tooltip_text(Some(message));
        self.setup_button.set_label(if failed {
            "Retry initialization"
        } else {
            "Dismiss"
        });
        self.setup_callout.set_reveal_child(true);
        (self.callbacks.announce)(message);
    }

    pub fn set_initialization_error(&self, message: &str) {
        let mut state = self.state.borrow().clone();
        state.initialization_failed = true;
        self.set_view_state(state);
        self.set_error(message);
        self.set_status("Set up the missing capture dependency, then retry from the callout.");
    }
}
