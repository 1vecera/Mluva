use super::*;
use crate::{command_palette::Command, commands::CommandAction};
use mluva_core::{
    database::{StoreError, StoreResult},
    delivery::{DeliveryOptions, deliver_text},
    text,
};
use serde_json::json;
use std::collections::BTreeSet;

impl ApplicationDesktop {
    /// Show the result where sign-in started, even if the user navigated away.
    pub fn account_callback(self: &Rc<Self>, uri: &str) {
        self.dispatch(ApplicationAction::Account);
        self.account.callback(uri);
    }

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
            ApplicationAction::Account => {
                self.settings.show();
                self.settings.view.set_visible_page(&self.account.page);
                self.shell.present();
                if self.account.sign_in.is_visible() {
                    self.account.sign_in.grab_focus();
                }
            }
            ApplicationAction::Commands => self.shell.show_commands(),
            ApplicationAction::Meeting => {
                self.shell.navigate("meeting");
            }
            ApplicationAction::Personalization => {
                self.personalization.refresh();
                self.shell.navigate("personalization");
            }
            ApplicationAction::Quit => {
                if !self.workspace().save_edits(None) {
                    self.shell.show_message("Edits could not be saved. Keep Mluva open to retry, or copy your text before quitting.");
                    return;
                }
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
        Ok(())
    }
    pub(super) fn copy_text(&self, value: &str) {
        let message = match deliver_text(value, false, DeliveryOptions::default()) {
            Ok(receipt) => receipt.guidance,
            Err(error) => format!("Copy failed: {error}"),
        };
        self.shell.show_message(&message);
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
        let workspace = self.workspace();
        let refreshed = workspace.refresh_history().and_then(|()| {
            let Some(selected) = workspace.entry() else {
                return Ok(());
            };
            match self.services.history.find(&selected.identifier) {
                Ok(entry) => {
                    let replies = self.services.conversations.replies(&entry.identifier)?;
                    workspace.show_conversation(Some(entry), &replies, false)
                }
                Err(StoreError::NotFound) => workspace.show_conversation(None, &[], false),
                Err(error) => Err(error),
            }
        });
        if let Err(error) = refreshed {
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
            &self.settings.command_pages(),
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
        state.incognito = config.incognito_mode;
        state
    }
    fn command(self: &Rc<Self>, command: CommandAction) {
        if self.closed.get() {
            return;
        }
        let workspace = self.workspace();
        match command {
            CommandAction::ToggleRecording => self.toggle_recording(CaptureOrigin::Manual),
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
        }
    }
}
