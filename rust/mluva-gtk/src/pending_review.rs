//! Explicit Command decisions and durable, editable Notes recovery.

use crate::{
    capture_preferences::CapturePreferences,
    capture_view::CapturePage,
    history_controller::HistoryController,
    text_target::{DeliveryTargetSnapshot, TextTargetSnapshot},
};
use adw::prelude::*;
use mluva_core::{
    config::AudioRetentionPolicy,
    database::{StoreError, StoreResult},
    delivery::{DeliveryOptions, deliver_text},
    diagnostics::{DiagnosticOutcome, DiagnosticProvider, DiagnosticStage},
    scratchpad::ScratchpadDraft,
    text,
};
use mluva_workflows::{
    dictation::WorkflowResult,
    services::{ApplicationServices, ScratchpadRecovery},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
    time::Instant,
};

pub struct PendingReviewCallbacks {
    pub changed: Rc<dyn Fn()>,
    pub excluded_history: Rc<dyn Fn() -> BTreeSet<String>>,
}
struct CommandPreview {
    result: WorkflowResult,
    target: Option<TextTargetSnapshot>,
    retention: AudioRetentionPolicy,
}
pub struct PendingReview {
    services: Rc<ApplicationServices>,
    capture: Rc<CapturePage>,
    preferences: Rc<CapturePreferences>,
    history: Rc<HistoryController>,
    callbacks: PendingReviewCallbacks,
    command: RefCell<Option<Rc<CommandPreview>>>,
    editing_notes: Cell<bool>,
    closed: Cell<bool>,
}
impl PendingReview {
    pub fn new(
        services: Rc<ApplicationServices>,
        capture: Rc<CapturePage>,
        preferences: Rc<CapturePreferences>,
        history: Rc<HistoryController>,
        callbacks: PendingReviewCallbacks,
    ) -> Rc<Self> {
        Rc::new(Self {
            services,
            capture,
            preferences,
            history,
            callbacks,
            command: RefCell::new(None),
            editing_notes: Cell::new(false),
            closed: Cell::new(false),
        })
    }
    pub fn command_identifier(&self) -> Option<String> {
        self.command.borrow().as_ref().and_then(|preview| {
            preview
                .result
                .history_entry
                .as_ref()
                .map(|entry| entry.identifier.clone())
        })
    }
    pub fn has_command(&self) -> bool {
        self.command.borrow().is_some()
    }
    pub fn busy(&self) -> bool {
        self.has_command() || self.services.scratchpad.borrow().draft.is_some()
    }
    pub fn show_command(
        &self,
        result: WorkflowResult,
        target: Option<TextTargetSnapshot>,
        retention: AudioRetentionPolicy,
    ) {
        if self.closed.get() {
            return;
        }
        self.editing_notes.set(false);
        let description = match &target {
            None => {
                "No supported external text target was captured. Acceptance will copy the result."
                    .into()
            }
            Some(target) => match target.selected_text() {
                None => {
                    "No text was selected. Acceptance will insert at the captured caret.".into()
                }
                Some(selected) => format!("Selected text:\n{selected}"),
            },
        };
        let label = match &target {
            None => "Copy result",
            Some(_) if !self.services.config().auto_paste => "Copy result",
            Some(target) if target.has_selection() => "Replace selection",
            Some(_) => "Insert at captured caret",
        };
        self.capture.command_source.set_label(&description);
        self.capture.command_source.set_visible(true);
        self.capture.accept_command.set_label(label);
        self.capture.command_actions.set_visible(true);
        self.capture.output_view.set_editable(false);
        self.capture
            .output_view
            .buffer()
            .set_text(&result.output_text);
        self.capture.scratchpad_actions.set_visible(false);
        let status = result.delivery.guidance.clone();
        self.command.replace(Some(Rc::new(CommandPreview {
            result,
            target,
            retention,
        })));
        self.finished_result(status);
    }
    pub fn show_notes(&self, result: WorkflowResult, retention: AudioRetentionPolicy) {
        if self.closed.get() {
            return;
        }
        self.editing_notes.set(true);
        let draft = ScratchpadDraft {
            identifier: glib::uuid_string_random().into(),
            history_identifier: result
                .history_entry
                .as_ref()
                .map(|entry| entry.identifier.clone()),
            created_at: result
                .history_entry
                .as_ref()
                .map(|entry| entry.created_at.clone())
                .unwrap_or_else(|| {
                    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
                }),
            raw_text: result.transcription.text,
            text: result.output_text.clone(),
            audio_path: result
                .retained_audio_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            incognito: result.incognito,
            audio_retention_policy: match retention {
                AudioRetentionPolicy::Never => "never",
                AudioRetentionPolicy::Failures => "failures",
                AudioRetentionPolicy::Always => "always",
            }
            .into(),
            session_identifier: Some(result.session_identifier),
        };
        let mut status = result.delivery.guidance;
        if let Err(error) = self
            .services
            .scratchpad
            .borrow_mut()
            .save(draft, !result.incognito)
        {
            self.editing_notes.set(false);
            let recovery = if result.history_entry.is_some() {
                "The text and audio remain in History.".into()
            } else {
                format!(
                    "The source audio remains at {}.",
                    result
                        .retained_audio_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "None".into())
                )
            };
            status = format!("Scratchpad draft persistence failed: {error}. {recovery}");
        }
        self.capture
            .output_view
            .buffer()
            .set_text(&result.output_text);
        self.capture
            .scratchpad_actions
            .set_visible(self.editing_notes.get());
        self.finished_result(status);
    }
    fn finished_result(&self, mut status: String) {
        if let Err(error) = self.prune() {
            status.push_str(&format!(" History retention failed: {error}"));
        }
        self.refresh();
        (self.callbacks.changed)();
        self.status(&status);
        self.history.reset();
        self.capture.refresh_output_visibility();
    }
    pub fn restore_notes(&self) {
        if self.closed.get() {
            return;
        }
        match self.services.recover_scratchpad() {
            Ok(ScratchpadRecovery::Restored(draft)) => {
                self.editing_notes.set(true);
                self.preferences.mode.set_selected(2);
                self.capture.output_view.buffer().set_text(&draft.text);
                self.capture.scratchpad_actions.set_visible(true);
                self.preferences.mode.set_sensitive(false);
                self.capture.output_view.set_editable(true);
                self.history.reset();
                self.capture.refresh_output_visibility();
                self.status("Recovered an unresolved Scratchpad draft and its source audio.");
            }
            Ok(ScratchpadRecovery::Empty) => {}
            Ok(recovery) => self.status(recovery.message()),
            Err(error) => self.status(&error.to_string()),
        }
    }
    pub fn edit_notes(&self, value: &str) {
        if self.closed.get() || !self.editing_notes.get() {
            return;
        }
        let Some(mut draft) = self.services.scratchpad.borrow().draft.clone() else {
            return;
        };
        draft.text = value.into();
        let persist = !draft.incognito;
        if let Err(error) = self.services.scratchpad.borrow_mut().save(draft, persist) {
            self.status(&format!("Scratchpad edit could not be persisted: {error}"));
        }
    }
    pub fn accept_command(&self) {
        if self.closed.get() {
            return;
        }
        let Some(preview) = self.command.borrow().clone() else {
            return;
        };
        let result = &preview.result;
        let started = Instant::now();
        let auto = self.services.config().auto_paste;
        let restored = auto
            && preview
                .target
                .as_ref()
                .is_some_and(TextTargetSnapshot::restore);
        let receipt = if let Some(target) = preview.target.as_ref().filter(|_| restored) {
            deliver_text(
                &result.output_text,
                true,
                DeliveryOptions {
                    confirm_paste: Some(&mut || Ok(target.confirm_insertion(&result.output_text))),
                    insert_directly: Some(&mut |value| Ok(target.insert_text(value))),
                    authorize_keyboard_paste: Some(&mut || Ok(target.restore())),
                    application_identifier: target.application_identifier(),
                    ..Default::default()
                },
            )
        } else {
            deliver_text(&result.output_text, false, DeliveryOptions::default())
        };
        let mut receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                self.diagnostic(
                    &result.session_identifier,
                    &result.mode,
                    result.incognito,
                    DiagnosticOutcome::Failed,
                    started.elapsed().as_secs_f64(),
                );
                self.status(&format!(
                    "Command result remains in preview because delivery failed: {error}"
                ));
                return;
            }
        };
        let seconds = started.elapsed().as_secs_f64();
        self.diagnostic(
            &result.session_identifier,
            &result.mode,
            result.incognito,
            if receipt.paste_dispatched && !receipt.pasted {
                DiagnosticOutcome::SafeFallback
            } else {
                DiagnosticOutcome::Completed
            },
            seconds,
        );
        if preview.target.is_some() && auto && !restored {
            receipt
                .guidance
                .push_str(" The captured target could not be restored, so no paste was attempted.");
        }
        if let Some(entry) = &result.history_entry {
            if let Err(error) = self.services.history.mark_delivered(
                &entry.identifier,
                &result.output_text,
                receipt.history_outcome(),
                preview.retention == AudioRetentionPolicy::Always,
                Some(milliseconds(seconds)),
            ) {
                receipt.guidance.push_str(&format!(
                    " Local history could not record acceptance: {}",
                    history_error(&error, &entry.identifier)
                ));
            }
            if let Some(target) = &preview.target {
                self.history.remember_target(
                    &entry.identifier,
                    &DeliveryTargetSnapshot::Text(target.clone()),
                );
            }
        }
        self.clear_command(false);
        self.refresh();
        self.status(&receipt.guidance);
    }
    pub fn discard_command(&self) {
        if self.closed.get() {
            return;
        }
        let Some(preview) = self.command.borrow().clone() else {
            return;
        };
        let result = &preview.result;
        self.diagnostic(
            &result.session_identifier,
            &result.mode,
            result.incognito,
            DiagnosticOutcome::Cancelled,
            0.0,
        );
        let mut status = "Command preview discarded; the target was not changed.".to_owned();
        if let Some(entry) = &result.history_entry
            && let Err(error) = self.services.history.mark_delivered(
                &entry.identifier,
                &result.output_text,
                "discarded",
                preview.retention == AudioRetentionPolicy::Always,
                None,
            )
        {
            status.push_str(&format!(
                " Local history could not record the discard: {}",
                history_error(&error, &entry.identifier)
            ));
        }
        self.clear_command(true);
        self.refresh();
        self.status(&status);
    }
    fn clear_command(&self, clear_output: bool) {
        self.command.borrow_mut().take();
        self.capture.command_actions.set_visible(false);
        self.capture.command_source.set_visible(false);
        self.capture.command_source.set_label("");
        self.capture.output_view.set_editable(true);
        if clear_output {
            self.capture.output_view.buffer().set_text("");
        }
        self.history.reset();
        self.capture.refresh_output_visibility();
    }
    pub fn copy_notes(&self) {
        if self.closed.get() {
            return;
        }
        let Some(draft) = self.services.scratchpad.borrow().draft.clone() else {
            return;
        };
        let buffer = self.capture.output_view.buffer();
        let value = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        let value = text::trim(&value);
        if value.is_empty() {
            self.status("The Scratchpad is empty; edit it or delete the draft.");
            return;
        }
        let started = Instant::now();
        let resolved = (|| {
            let receipt = deliver_text(value, false, DeliveryOptions::default())
                .map_err(|error| StoreError::Invalid(error.to_string()))?;
            let seconds = started.elapsed().as_secs_f64();
            let policy = match draft.audio_retention_policy.as_str() {
                "always" => AudioRetentionPolicy::Always,
                "never" => AudioRetentionPolicy::Never,
                "failures" => AudioRetentionPolicy::Failures,
                invalid => {
                    return Err(StoreError::Invalid(format!(
                        "'{invalid}' is not a valid AudioRetentionPolicy"
                    )));
                }
            };
            if let Some(id) = &draft.history_identifier {
                self.services
                    .history
                    .mark_delivered(
                        id,
                        value,
                        "copied",
                        !draft.incognito && policy == AudioRetentionPolicy::Always,
                        Some(milliseconds(seconds)),
                    )
                    .map_err(|error| StoreError::Invalid(history_error(&error, id)))?;
            }
            self.services
                .scratchpad
                .borrow_mut()
                .clear(draft.history_identifier.is_none())?;
            Ok::<_, StoreError>((receipt, seconds))
        })();
        let (mut receipt, seconds) = match resolved {
            Ok(result) => result,
            Err(error) => {
                if let Some(id) = &draft.session_identifier {
                    self.diagnostic(
                        id,
                        "scratchpad",
                        draft.incognito,
                        DiagnosticOutcome::Failed,
                        started.elapsed().as_secs_f64(),
                    );
                }
                self.status(&format!(
                    "Scratchpad acceptance did not fully resolve: {error}"
                ));
                return;
            }
        };
        if let Some(id) = &draft.session_identifier {
            self.diagnostic(
                id,
                "scratchpad",
                draft.incognito,
                DiagnosticOutcome::Completed,
                seconds,
            );
        }
        if let Err(error) = self.prune() {
            receipt
                .guidance
                .push_str(&format!(" History retention failed: {error}"));
        }
        self.editing_notes.set(false);
        self.capture.scratchpad_actions.set_visible(false);
        self.history.reset();
        self.capture.refresh_output_visibility();
        self.refresh();
        (self.callbacks.changed)();
        self.status(&receipt.guidance);
    }
    pub fn delete_notes(&self) {
        if self.closed.get() {
            return;
        }
        let Some(draft) = self.services.scratchpad.borrow().draft.clone() else {
            return;
        };
        if let Err(error) = self.history.delete_scratchpad(&draft) {
            self.status(&format!("Scratchpad deletion failed: {error}"));
            return;
        }
        if let Some(id) = &draft.session_identifier {
            self.diagnostic(
                id,
                "scratchpad",
                draft.incognito,
                DiagnosticOutcome::Cancelled,
                0.0,
            );
        }
        self.editing_notes.set(false);
        self.capture.output_view.buffer().set_text("");
        self.capture.scratchpad_actions.set_visible(false);
        self.history.reset();
        self.capture.refresh_output_visibility();
        self.refresh();
        (self.callbacks.changed)();
        self.status("Scratchpad draft and recovery audio permanently deleted.");
    }
    pub fn confirm_delete_notes(self: &Rc<Self>) {
        if self.closed.get()
            || self.services.scratchpad.borrow().draft.is_none()
            || self.capture.widget.root().is_none()
        {
            return;
        }
        let dialog=adw::AlertDialog::builder().heading("Delete this Notes draft?").body("The draft, its history record, and retained recovery audio will be permanently deleted.").default_response("cancel").close_response("cancel").build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete permanently");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let weak = Rc::downgrade(self);
        dialog.choose(
            Some(&self.capture.widget),
            gio::Cancellable::NONE,
            move |response| {
                if response == "delete"
                    && let Some(owner) = weak.upgrade()
                {
                    owner.delete_notes();
                }
            },
        );
    }
    fn diagnostic(
        &self,
        id: &str,
        mode: &str,
        private: bool,
        outcome: DiagnosticOutcome,
        seconds: f64,
    ) {
        if !private {
            let _ = self.services.diagnostics.record(
                id,
                mode,
                DiagnosticStage::Delivery,
                DiagnosticProvider::Desktop,
                outcome,
                seconds,
            );
        }
    }
    fn prune(&self) -> StoreResult<usize> {
        let mut excluded = (self.callbacks.excluded_history)();
        excluded.extend(self.history.retry_identifier());
        excluded.extend(self.command_identifier());
        self.services.prune_history(&excluded)
    }
    fn refresh(&self) {
        if let Err(error) = self.history.page.refresh() {
            self.status(&error.to_string());
        }
    }
    fn status(&self, value: &str) {
        self.capture.set_status(value);
    }
    pub fn close(&self) {
        self.closed.set(true);
        self.command.borrow_mut().take();
        self.editing_notes.set(false);
    }
}
fn milliseconds(seconds: f64) -> i64 {
    (seconds * 1000.0).round_ties_even() as i64
}

fn history_error(error: &StoreError, identifier: &str) -> String {
    // The released acceptance warnings identify the missing History key.
    if matches!(error, StoreError::NotFound) {
        format!("'{identifier}'")
    } else {
        error.to_string()
    }
}
