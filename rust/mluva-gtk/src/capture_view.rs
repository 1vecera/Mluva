//! The real capture page, recovery editor and recording controls around the conversation workspace.

use crate::conversation_view::ConversationWorkspace;
use crate::document_layout::margins;
use crate::prompt_editor::prompt_control;
use crate::rewrite_settings::RewriteSettings;
use adw::prelude::*;
use mluva_core::{config::AppConfig, database::StoreResult, prompt_catalog::DEFAULTS, text};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiveSettingsChange {
    Mode { enabled: bool, continuous: bool },
    Template(String),
}

pub struct CaptureCallbacks {
    pub toggle_recording: Rc<dyn Fn()>,
    pub apply_live_settings: Rc<dyn Fn(LiveSettingsChange) -> bool>,
    pub toast: Rc<dyn Fn(&str)>,
    pub open_prompt: Rc<dyn Fn(&str)>,
    pub retry_initialization: Rc<dyn Fn()>,
    pub accept_command: Rc<dyn Fn()>,
    pub discard_command: Rc<dyn Fn()>,
    pub copy_scratchpad: Rc<dyn Fn()>,
    pub delete_scratchpad: Rc<dyn Fn()>,
    pub output_changed: Rc<dyn Fn(&str)>,
    pub live_draft_edited: Rc<dyn Fn()>,
    pub announce: Rc<dyn Fn(&str)>,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureViewState {
    pub preparing: bool,
    pub recording: bool,
    pub processing: bool,
    pub initialization_failed: bool,
    pub review_active: bool,
    pub meeting_busy: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureShortcutState {
    pub recording_trigger: Option<String>,
    pub rewrite_trigger: Option<String>,
    pub shortcut_service_available: bool,
    pub target_tracking_available: bool,
}

pub struct CapturePage {
    pub widget: adw::ToolbarView,
    pub workspace: Rc<ConversationWorkspace>,
    pub rewrite_settings: Rc<RewriteSettings>,
    pub setup_callout: gtk::Revealer,
    pub setup_title: gtk::Label,
    pub setup_body: gtk::Label,
    pub setup_button: gtk::Button,
    pub output_section: gtk::Box,
    pub output_view: gtk::TextView,
    pub command_source: gtk::Label,
    pub command_actions: gtk::Box,
    pub accept_command: gtk::Button,
    pub discard_command: gtk::Button,
    pub scratchpad_actions: gtk::Box,
    pub copy_scratchpad: gtk::Button,
    pub delete_scratchpad: gtk::Button,
    pub status: gtk::Label,
    pub status_title: gtk::Label,
    pub action_hint: gtk::Label,
    pub action_bar: gtk::Box,
    pub buttons: gtk::Box,
    pub record_button: gtk::Button,
    pub live_mode: gtk::ToggleButton,
    pub live_menu: gtk::MenuButton,
    pub live_templates: BTreeMap<String, gtk::CheckButton>,
    config: RefCell<AppConfig>,
    state: RefCell<CaptureViewState>,
    shortcuts: RefCell<CaptureShortcutState>,
    pending_mode: RefCell<String>,
    syncing_live: Cell<bool>,
    callbacks: CaptureCallbacks,
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
        rewrite_settings: Rc<RewriteSettings>,
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
        workspace.set_rewrite_settings(&rewrite_settings.widget);
        body.append(&workspace.widget);

        let output_section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        output_section.add_css_class("card");
        let recovery = gtk::Box::new(gtk::Orientation::Vertical, 16);
        margins(&recovery, 16);
        output_section.append(&recovery);
        let heading = gtk::Label::builder()
            .label("Latest result")
            .xalign(0.0)
            .build();
        heading.add_css_class("heading");
        recovery.append(&heading);
        let command_source = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .visible(false)
            .build();
        command_source.add_css_class("caption");
        recovery.append(&command_source);
        let output_view = gtk::TextView::builder()
            .editable(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .tooltip_text("Recognized text and unresolved Command or Notes output")
            .build();
        recovery.append(
            &gtk::ScrolledWindow::builder()
                .min_content_height(120)
                .child(&output_view)
                .build(),
        );
        let command_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let accept_command = gtk::Button::builder()
            .label("Apply command")
            .hexpand(true)
            .build();
        accept_command.add_css_class("suggested-action");
        command_actions.append(&accept_command);
        let discard_command = gtk::Button::with_label("Discard");
        command_actions.append(&discard_command);
        command_actions.set_visible(false);
        recovery.append(&command_actions);
        let scratchpad_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let copy_scratchpad = gtk::Button::builder()
            .label("Copy and resolve")
            .hexpand(true)
            .build();
        copy_scratchpad.add_css_class("suggested-action");
        scratchpad_actions.append(&copy_scratchpad);
        let delete_scratchpad = gtk::Button::with_label("Delete draft");
        delete_scratchpad.add_css_class("destructive-action");
        scratchpad_actions.append(&delete_scratchpad);
        scratchpad_actions.set_visible(false);
        recovery.append(&scratchpad_actions);
        output_section.set_visible(false);
        body.append(&output_section);
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
        let live_control = gtk::Box::builder()
            .valign(gtk::Align::Center)
            .css_classes(["linked"])
            .build();
        let live_mode = gtk::ToggleButton::builder()
            .label("Live rewrite")
            .active(config.live_rewrite_enabled)
            .build();
        live_control.append(&live_mode);
        let live_menu = gtk::MenuButton::builder()
            .icon_name("pan-down-symbolic")
            .build();
        let popover = gtk::Popover::new();
        let choices = gtk::Box::new(gtk::Orientation::Vertical, 4);
        margins(&choices, 8);
        let mut live_templates = BTreeMap::new();
        let mut group = None;
        for template in &DEFAULTS.live_templates {
            let choice = gtk::CheckButton::with_label(&template.name);
            if let Some(group) = &group {
                choice.set_group(Some(group));
            } else {
                group = Some(choice.clone());
            }
            let menu = live_menu.downgrade();
            let open = callbacks.open_prompt.clone();
            let id = format!("live-{}", template.identifier);
            choices.append(&prompt_control(
                &choice,
                &template.name,
                Rc::new(move || {
                    if let Some(menu) = menu.upgrade() {
                        menu.popdown();
                    }
                    open(&id);
                }),
            ));
            live_templates.insert(template.identifier.clone(), choice);
        }
        popover.set_child(Some(&choices));
        live_menu.set_popover(Some(&popover));
        live_control.append(&live_menu);
        buttons.append(&live_control);
        workspace.set_capture_controls(&action_bar);
        let page = Rc::new(Self {
            widget,
            workspace,
            rewrite_settings,
            setup_callout,
            setup_title,
            setup_body,
            setup_button,
            output_section,
            output_view,
            command_source,
            command_actions,
            accept_command,
            discard_command,
            scratchpad_actions,
            copy_scratchpad,
            delete_scratchpad,
            status,
            status_title,
            action_hint,
            action_bar,
            buttons,
            record_button,
            live_mode,
            live_menu,
            live_templates,
            config: RefCell::new(config.clone()),
            state: RefCell::new(CaptureViewState::default()),
            shortcuts: RefCell::new(CaptureShortcutState::default()),
            pending_mode: RefCell::new("dictation".into()),
            syncing_live: Cell::new(false),
            callbacks,
        });
        page.bind();
        page.set_config(config)?;
        Ok(page)
    }

    fn bind(self: &Rc<Self>) {
        let toggle = self.callbacks.toggle_recording.clone();
        self.record_button.connect_clicked(move |_| toggle());
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
        let weak = Rc::downgrade(self);
        self.live_mode.connect_toggled(move |_| {
            let Some(page) = weak.upgrade() else { return; };
            if page.syncing_live.get() { return; }
            let config = page.config.borrow().clone();
            let changes = LiveSettingsChange::Mode {
                enabled: !(config.live_rewrite_enabled && config.live_rewrite_continuous),
                continuous: config.live_rewrite_enabled && !config.live_rewrite_continuous,
            };
            if !(page.callbacks.apply_live_settings)(changes) {
                page.sync_live_mode();
                (page.callbacks.toast)("Live rewrite is available during dictation; wait for preparation or finalization to finish.");
            }
        });
        for (id, button) in &self.live_templates {
            let weak = Rc::downgrade(self);
            let id = id.clone();
            button.connect_toggled(move |button| {
                let Some(page) = weak.upgrade() else {
                    return;
                };
                if !button.is_active() || id == page.config.borrow().live_rewrite_template {
                    return;
                }
                if !(page.callbacks.apply_live_settings)(LiveSettingsChange::Template(id.clone())) {
                    page.sync_live_mode();
                    (page.callbacks.toast)(
                        "Finish active work before changing the Live rewrite template.",
                    );
                }
                page.live_menu.popdown();
            });
        }
        for (button, action) in [
            (&self.accept_command, &self.callbacks.accept_command),
            (&self.discard_command, &self.callbacks.discard_command),
            (&self.copy_scratchpad, &self.callbacks.copy_scratchpad),
            (&self.delete_scratchpad, &self.callbacks.delete_scratchpad),
        ] {
            let action = action.clone();
            button.connect_clicked(move |_| action());
        }
        let weak = Rc::downgrade(self);
        self.output_view.buffer().connect_changed(move |buffer| {
            if let Some(page) = weak.upgrade() {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
                (page.callbacks.output_changed)(&text);
                page.refresh_output_visibility();
            }
        });
        let changed = self.callbacks.live_draft_edited.clone();
        self.workspace
            .live_draft_text
            .buffer()
            .connect_changed(move |_| changed());
    }

    pub fn config(&self) -> AppConfig {
        self.config.borrow().clone()
    }

    pub fn set_config(self: &Rc<Self>, config: AppConfig) -> StoreResult<()> {
        self.config.replace(config.clone());
        self.rewrite_settings.set_config(config.clone());
        self.workspace.set_config(config)?;
        self.sync_live_mode();
        self.refresh_action_hint();
        Ok(())
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
            Some(trigger) if config.auto_paste && shortcuts.target_tracking_available => format!(
                "{trigger} toggles global capture and can insert into a text field or terminal · this button always copies"
            ),
            Some(trigger) => format!(
                "{trigger} toggles global capture · automatic paste is {}",
                if config.auto_paste {
                    "unavailable"
                } else {
                    "off"
                }
            ),
        };
        if let Some(trigger) = &shortcuts.rewrite_trigger {
            hint += &format!(" · {trigger} opens rewriting");
        }
        self.action_hint.set_label(&hint);
    }

    pub fn sync_live_mode(&self) {
        let config = self.config.borrow().clone();
        let available = config.rewrite_provider != "none";
        self.live_mode.set_sensitive(available);
        self.live_menu.set_sensitive(available);
        self.syncing_live.set(true);
        self.live_mode
            .set_active(config.live_rewrite_enabled && available);
        let (mode, suffix) = if config.live_rewrite_continuous {
            ("Continuous", " · ∞")
        } else {
            ("Once", " · 1×")
        };
        self.live_mode.set_label(&if config.live_rewrite_enabled {
            format!("Live rewrite{suffix}")
        } else {
            "Live rewrite".into()
        });
        self.live_mode
            .update_property(&[gtk::accessible::Property::Label(
                &if config.live_rewrite_enabled {
                    format!("Live rewrite · {mode}")
                } else {
                    "Live rewrite · Off".into()
                },
            )]);
        self.syncing_live.set(false);
        if let Some(button) = self.live_templates.get(&config.live_rewrite_template) {
            button.set_active(true);
        }
        let label = DEFAULTS
            .live_templates
            .iter()
            .find(|template| template.identifier == config.live_rewrite_template)
            .expect("validated live template")
            .name
            .clone();
        self.live_mode.set_tooltip_text(Some(&format!(
            "Off → Once → Continuous. Build a {} draft; reconcile at Stop.",
            text::lower(&label)
        )));
        self.live_menu
            .set_tooltip_text(Some(&format!("Live rewrite template: {label}")));
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
        } else if state.review_active {
            "Review required"
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
            self.record_button
                .set_sensitive(!state.review_active && !state.meeting_busy);
            self.action_bar.set_visible(!state.review_active);
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

    pub fn set_pending_mode(&self, mode: &str) {
        self.pending_mode.replace(mode.into());
        self.refresh_output_visibility();
    }

    pub fn refresh_output_visibility(&self) {
        let buffer = self.output_view.buffer();
        let has_text =
            !text::trim(&buffer.text(&buffer.start_iter(), &buffer.end_iter(), true)).is_empty();
        self.output_section.set_visible(
            (has_text && *self.pending_mode.borrow() != "dictation")
                || self.command_actions.get_visible()
                || self.scratchpad_actions.get_visible(),
        );
    }
}
