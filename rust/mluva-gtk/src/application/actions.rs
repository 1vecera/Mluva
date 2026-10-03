use super::*;
use crate::{command_palette::Command, commands::CommandAction};
use mluva_core::{
    database::{StoreError, StoreResult},
    delivery::{DeliveryOptions, deliver_text},
    history::HistoryInput,
    text,
};
use serde_json::json;
use std::collections::BTreeSet;

impl ApplicationDesktop {
    pub fn dispatch(self: &Rc<Self>, action: ApplicationAction) {
        if self.closed.get() {
            return;
        }
        match action {
            ApplicationAction::Latest => {
                if let Err(error) = self.open_latest(true) {
                    self.shell.show_message(&error.to_string());
                }
            }
            ApplicationAction::Record => self.toggle_recording(CaptureOrigin::Manual),
            ApplicationAction::GlobalRecord => {
                self.toggle_recording(CaptureOrigin::ApprovedShortcut)
            }
            ApplicationAction::Screenshot => self.request_screenshot(),
            ApplicationAction::Cancel => {
                self.cancel();
            }
            ApplicationAction::Status => {
                if let Some(publisher) = self.overlay.borrow().as_ref() {
                    publisher.replay();
                }
            }
            ApplicationAction::History => {
                let id = self.workspace().entry().map(|entry| entry.identifier);
                if let Err(error) = self.history.page.focus_entry(id.as_deref()) {
                    self.shell.show_message(&error.to_string());
                }
                self.shell.navigate("history");
                self.shell.present();
            }
            ApplicationAction::Settings => {
                self.settings.show();
                self.shell.present();
            }
            ApplicationAction::Commands => self.shell.show_commands(),
            ApplicationAction::Meeting => {
                self.shell.navigate("meeting");
                self.shell.present();
            }
            ApplicationAction::Personalization => {
                self.personalization.refresh();
                self.shell.navigate("personalization");
                self.shell.present();
            }
            ApplicationAction::Review {
                operation,
                identifier,
                option,
            } => self.review.action(&operation, &identifier, &option),
            ApplicationAction::Quit => {
                let application = self.application.clone();
                let shutdown = self.shutdown();
                glib::MainContext::default().spawn_local(async move {
                    let _ = shutdown.await;
                    application.quit();
                });
            }
        }
    }

    pub(super) fn open_latest(&self, present: bool) -> StoreResult<()> {
        if let Some(entry) = self
            .services
            .conversations
            .search("", 1, None)?
            .into_iter()
            .next()
        {
            let replies = self.services.conversations.replies(&entry.identifier)?;
            self.workspace()
                .show_conversation(Some(entry), &replies, false)?;
        }
        if present {
            self.shell.navigate("capture");
            self.shell.present();
        }
        self.workspace().prompt.grab_focus();
        Ok(())
    }
    pub(super) fn copy_text(&self, value: &str) {
        let message = match deliver_text(value, false, DeliveryOptions::default()) {
            Ok(receipt) => receipt.guidance,
            Err(error) => format!("Copy failed: {error}"),
        };
        self.shell.show_message(&message);
    }
    pub(super) fn import_text(&self, value: &str) {
        if self.closed.get() || text::trim(value).is_empty() {
            return;
        }
        if self.services.config().incognito_mode {
            let _ = self.workspace().show_transient(value, value);
            return;
        }
        let saved = (|| {
            let entry = self.services.history.add(HistoryInput {
                language_code: self.services.config().language_code,
                delivery_outcome: "imported".into(),
                ..HistoryInput::dictation(value, value)
            })?;
            self.workspace().prompt.buffer().set_text("");
            self.workspace()
                .show_conversation(Some(entry.clone()), &[], false)?;
            self.history_changed();
            self.titles.enqueue(&entry);
            Ok::<_, StoreError>(())
        })();
        if saved.is_err() {
            self.workspace().set_busy(
                false,
                "Could not save this conversation. Your text is still in the editor.",
            );
        }
    }
    pub(super) fn refresh_title(&self, id: &str) {
        let _ = self.workspace().refresh_title(id);
        let _ = self.history.page.refresh_title(id);
    }
    pub(super) fn rename(&self, id: &str, title: &str) -> bool {
        let title = text::trim(title);
        if self.closed.get() || self.services.config().incognito_mode || title.is_empty() {
            return false;
        }
        if self.services.history.update_title(id, Some(title)).is_err() {
            self.shell
                .show_message("Could not rename this conversation. Try again.");
            return false;
        }
        self.refresh_title(id);
        true
    }
    fn can_manage(&self) -> bool {
        let state = self.workspace().command_state();
        !self.closed.get()
            && !self.services.config().incognito_mode
            && self.capture.phase().is_none()
            && self.history.retry_identifier().is_none()
            && !self.live.finalizing()
            && !self.review.rewriting()
            && !state.busy
            && !state.live_active
    }
    pub(super) fn merge_conversations(self: &Rc<Self>, source: &str, target: &str) -> bool {
        if !self.can_manage() {
            self.shell
                .show_message("Finish the current operation before merging conversations.");
            return false;
        }
        match self.workspace().merge_documents(source, target) {
            Ok(false) => return false,
            Ok(true) => {}
            Err(StoreError::Invalid(message)) => {
                self.shell.show_message(&message);
                return false;
            }
            Err(_) => {
                self.shell.show_message(
                    "Could not merge these conversations. Reopen them and try again.",
                );
                return false;
            }
        }
        let _ = self.history.page.refresh();
        if self
            .review
            .current()
            .is_some_and(|id| id == source || id == target)
        {
            self.review.publish(target, "ready", "");
        }
        self.shell
            .show_message("Conversations merged. Originals remain in History.");
        true
    }
    pub(super) fn delete_conversation(&self, id: &str) -> bool {
        if !self.can_manage() {
            self.shell
                .show_message("Finish the current operation before deleting conversations.");
            return false;
        }
        let removed = (|| {
            let entry = self.services.history.find(id)?;
            let mut identifiers = BTreeSet::from([id.to_owned()]);
            identifiers.extend(
                self.services
                    .history
                    .continuations(id)?
                    .into_iter()
                    .map(|entry| entry.identifier),
            );
            Ok::<_, StoreError>((entry, identifiers))
        })();
        let Ok((entry, identifiers)) = removed else {
            return false;
        };
        if !self.history.delete(&entry) {
            return false;
        }
        self.history_changed();
        let _ = self.history.page.refresh();
        self.workspace().forget_documents(&identifiers);
        for id in identifiers {
            self.history.forget_target(&id);
        }
        self.shell
            .show_message("Conversation and retained recordings deleted.");
        true
    }
    pub(super) fn history_changed(&self) {
        if self.closed.get() {
            return;
        }
        if let Err(error) = self.workspace().refresh_history() {
            self.shell.show_message(&error.to_string());
        }
        self.personalization.refresh();
    }
    pub(super) fn remove_screenshot(&self, id: &str) {
        if self.closed.get() {
            return;
        }
        if self
            .close_screenshot_editor(id)
            .and_then(|()| self.services.screenshots.delete(id))
            .is_err()
        {
            self.shell.show_message("Could not remove the screenshot.");
        }
        self.workspace().refresh_screenshots();
    }
    pub(super) fn save_prompt(self: &Rc<Self>, instruction: &str) {
        if self.closed.get()
            || self.services.config().incognito_mode
            || text::trim(instruction).is_empty()
        {
            return;
        }
        let dialog = adw::AlertDialog::builder()
            .heading("Save prompt")
            .body("Give this rewrite prompt a short name.")
            .default_response("save")
            .build();
        let name = gtk::Entry::builder()
            .placeholder_text("Prompt name")
            .activates_default(true)
            .build();
        dialog.set_extra_child(Some(&name));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("save", "Save");
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        let weak = Rc::downgrade(self);
        let instruction = instruction.to_owned();
        dialog.connect_response(None, move |_, response| {
            let Some(owner) = weak
                .upgrade()
                .filter(|owner| !owner.closed.get() && !owner.services.config().incognito_mode)
            else {
                return;
            };
            if response != "save" {
                return;
            }
            let result = (|| {
                let mut store = owner.services.personalization.borrow_mut();
                let folded = caseless::default_case_fold_str(text::trim(&name.text()));
                if store
                    .styles()?
                    .iter()
                    .any(|style| caseless::default_case_fold_str(&style.name) == folded)
                {
                    return Err(StoreError::Invalid("Choose a new prompt name.".into()));
                }
                store.save_style(&name.text(), &instruction)?;
                Ok::<_, StoreError>(())
            })();
            if result.is_err() {
                owner.shell.show_message(
                    "Could not save the prompt. Use a unique name and a shorter instruction.",
                );
                return;
            }
            owner.refresh_style_controls();
            owner.shell.show_message("Prompt saved.");
        });
        dialog.present(Some(&self.shell.window));
    }
    pub(super) fn refresh_style_controls(self: &Rc<Self>) {
        let refreshed = (|| {
            self.services.synchronize_style_prompts()?;
            let prompts = self
                .services
                .personalization
                .borrow()
                .styles()?
                .into_iter()
                .map(|style| {
                    (
                        style.name,
                        format!("style-{}", style.identifier.to_lowercase()),
                    )
                })
                .collect::<Vec<_>>();
            self.workspace().set_saved_prompts(&prompts);
            self.settings.capture.refresh_styles();
            self.settings.prompts.refresh()?;
            self.personalization.refresh();
            Ok::<_, StoreError>(())
        })();
        if let Err(error) = refreshed {
            self.shell.show_message(&error.to_string());
        }
        self.refresh_review();
    }
    pub(super) fn refresh_review(self: &Rc<Self>) {
        if let Some(id) = self.review.current() {
            self.review.publish(
                &id,
                if self.review.rewrite_identifier().as_deref() == Some(&id) {
                    "rewriting"
                } else {
                    "ready"
                },
                "",
            );
        }
    }
    pub(super) fn export_diagnostics(&self) {
        match self.services.diagnostics.export(
            &self.services.paths.data.join("exports"),
            &self.services.config(),
        ) {
            Ok(path) => self.page().set_status(&format!(
                "Privacy-safe diagnostics exported to {}",
                path.display()
            )),
            Err(error) => self
                .page()
                .set_status(&format!("Diagnostics export failed: {error}")),
        }
    }
    pub(super) fn announce(&self, message: &str) {
        self.shell
            .window
            .announce(message, gtk::AccessibleAnnouncementPriority::High);
    }
    pub(super) fn toggle_sidebar(self: &Rc<Self>) {
        self.shell.navigate("capture");
        let visible = !self.workspace().split.shows_sidebar();
        if self.settings.apply(
            json!({"history_sidebar_visible":visible})
                .as_object()
                .unwrap(),
        ) {
            self.workspace().split.set_show_sidebar(visible);
        }
    }
    pub(super) fn adapt(&self, compact: bool) {
        let workspace = self.workspace().clone();
        let page = self.page().clone();
        let services = self.services.clone();
        AdaptiveWorkspace {
            split: workspace.split.clone(),
            live_panes: workspace.live_box.clone(),
            capture_actions: page.action_bar.clone(),
            capture_buttons: page.buttons.clone(),
            record_button: page.record_button.clone(),
            sidebar_visible: Rc::new(move || services.config().history_sidebar_visible),
            compact_documents: Rc::new(move |value| workspace.set_compact(value)),
            compact_recording: self.platform.compact_recording.clone(),
        }
        .set_compact(compact);
    }
    pub(super) fn commands(self: &Rc<Self>) -> Vec<Command> {
        let state = Rc::downgrade(self);
        let action = state.clone();
        let context = Rc::new(CommandContext {
            state: Rc::new(move || {
                state
                    .upgrade()
                    .map_or_else(CommandState::default, |app| app.command_state())
            }),
            invoke: Rc::new(move |command| {
                if let Some(app) = action.upgrade() {
                    app.command(command);
                }
            }),
        });
        application_commands(
            &context,
            &self.settings.view,
            self.services.prompts.borrow().catalog(),
        )
    }
    fn command_state(&self) -> CommandState {
        let mut state = self.workspace().command_state();
        let page = self.page();
        let view = page.view_state();
        let config = self.services.config();
        state.page = self
            .shell
            .stack
            .visible_child_name()
            .unwrap_or_default()
            .to_string();
        state.preparing = view.preparing;
        state.processing = view.processing;
        state.recording = view.recording;
        state.record_sensitive = page.record_button.is_sensitive();
        state.live_enabled = config.live_rewrite_enabled;
        state.live_sensitive = page.live_mode.is_sensitive();
        state.live_final_entry = self.live.finalizing();
        state.rewriting = self.review.rewriting();
        state.incognito = config.incognito_mode;
        if let Some(current) = self.current.borrow().as_ref() {
            state.pending_incognito = current.options.incognito;
            state.pending_mode = current.options.mode.clone();
        }
        state
    }
    fn command(self: &Rc<Self>, command: CommandAction) {
        if self.closed.get() {
            return;
        }
        let workspace = self.workspace();
        match command {
            CommandAction::ToggleRecording => self.toggle_recording(CaptureOrigin::Manual),
            CommandAction::CycleLive => self.page().live_mode.emit_clicked(),
            CommandAction::Polish => workspace.quick_polish.emit_clicked(),
            CommandAction::FocusRewrite => workspace.focus_prompt(),
            CommandAction::CopyOutput => {
                workspace.copy_current_output();
            }
            CommandAction::SaveEdits => {
                workspace.save_edits(None);
            }
            CommandAction::History => self.dispatch(ApplicationAction::History),
            CommandAction::ShowLive => {
                self.shell.navigate("capture");
                workspace.show_live();
            }
            CommandAction::ToggleSidebar => self.toggle_sidebar(),
            CommandAction::ToggleSourcePane => workspace.toggle_live_pane(true),
            CommandAction::ToggleDraftPane => workspace.toggle_live_pane(false),
            CommandAction::Settings => self.settings.show(),
            CommandAction::Welcome => self.settings.show_welcome(),
            CommandAction::WidgetPositionSetting => {
                self.settings.show();
                self.settings.view.focus_row(
                    &self.settings.workspace.widget,
                    &self.settings.workspace.appearance.position.widget,
                );
            }
            CommandAction::WorkspaceSetting { name, value } => {
                self.settings
                    .apply(json!({name:value}).as_object().unwrap());
            }
            CommandAction::EditPrompt(id) => self.settings.open_prompt(&id),
        }
    }
    pub(super) fn apply_live_change(&self, change: LiveSettingsChange) -> bool {
        let changes = match change {
            LiveSettingsChange::Mode {
                enabled,
                continuous,
            } => json!({"live_rewrite_enabled":enabled,"live_rewrite_continuous":continuous}),
            LiveSettingsChange::Template(template) => json!({"live_rewrite_template":template}),
        };
        self.settings.apply(changes.as_object().unwrap())
    }
}
