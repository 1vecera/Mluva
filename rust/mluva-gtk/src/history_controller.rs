//! Explicit archive recovery, exact retained targets and owned recognition retries.

use crate::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferences, PreferenceActivity},
    capture_view::CapturePage,
    history_view::{HistoryAction, HistoryCallbacks, HistoryPage},
    prompt_editor::Message,
    text_target::DeliveryTargetSnapshot,
};
use gtk::prelude::*;
use mluva_core::{
    database::{StoreError, StoreResult},
    delivery::{DeliveryOptions, deliver_text},
    history::HistoryEntry,
    scratchpad::ScratchpadDraft,
};
use mluva_workflows::{
    dictation::{DictationWorkflow, WorkflowOutcome},
    preparation::reprocess_history_entry,
    services::ApplicationServices,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::{Rc, Weak},
    time::Instant,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub type HistoryWorkflowFactory = Rc<dyn Fn() -> WorkflowOutcome<Rc<DictationWorkflow>>>;
pub type CloseScreenshot = Rc<dyn Fn(&str) -> StoreResult<()>>;
pub struct HistoryControllerCallbacks {
    pub activity: Rc<dyn Fn() -> PreferenceActivity>,
    pub pending_command: Rc<dyn Fn() -> Option<String>>,
    pub changed: Rc<dyn Fn()>,
    pub idle: Rc<dyn Fn()>,
    pub queue_title: HistoryAction,
    pub close_screenshot: CloseScreenshot,
    pub copy: Message,
}
struct Retry {
    identifier: String,
    workflow: Rc<DictationWorkflow>,
    cancelled: CancellationToken,
    finished: Cell<bool>,
    done: Notify,
}
impl Retry {
    async fn close(&self) {
        self.cancelled.cancel();
        self.workflow.cancel().await;
        loop {
            let done = self.done.notified();
            if self.finished.get() {
                break;
            }
            done.await;
        }
    }
}

pub struct HistoryController {
    pub page: Rc<HistoryPage>,
    services: Rc<ApplicationServices>,
    runtime: Rc<DesktopRuntime>,
    capture: Rc<CapturePage>,
    preferences: Rc<CapturePreferences>,
    factory: RefCell<Option<HistoryWorkflowFactory>>,
    retry: RefCell<Option<Rc<Retry>>>,
    targets: RefCell<VecDeque<(String, DeliveryTargetSnapshot)>>,
    closed: Cell<bool>,
    callbacks: HistoryControllerCallbacks,
}
impl HistoryController {
    pub fn new(
        services: Rc<ApplicationServices>,
        runtime: Rc<DesktopRuntime>,
        capture: Rc<CapturePage>,
        preferences: Rc<CapturePreferences>,
        callbacks: HistoryControllerCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let current = Rc::new(RefCell::new(Weak::<Self>::new()));
        let page = HistoryPage::new(
            services.history.clone(),
            Some(services.conversations.clone()),
            services.paths.data.join("exports"),
            services.config().time_format,
            HistoryCallbacks {
                copy: callbacks.copy.clone(),
                can_retry_delivery: {
                    let current = current.clone();
                    Rc::new(move |entry| {
                        current
                            .borrow()
                            .upgrade()
                            .is_some_and(|owner| owner.can_retry_delivery(entry))
                    })
                },
                retry_delivery: {
                    let current = current.clone();
                    Rc::new(move |entry| {
                        if let Some(owner) = current.borrow().upgrade() {
                            owner.retry_delivery(entry);
                        }
                    })
                },
                retry_recognition: {
                    let current = current.clone();
                    Rc::new(move |entry| {
                        if let Some(owner) = current.borrow().upgrade() {
                            owner.retry_recognition(entry);
                        }
                    })
                },
                reprocess: {
                    let current = current.clone();
                    Rc::new(move |entry| {
                        if let Some(owner) = current.borrow().upgrade() {
                            owner.reprocess(entry);
                        }
                    })
                },
                delete: {
                    let current = current.clone();
                    Rc::new(move |entry| {
                        current
                            .borrow()
                            .upgrade()
                            .is_some_and(|owner| owner.delete(entry))
                    })
                },
                changed: callbacks.changed.clone(),
                message: {
                    let capture = capture.clone();
                    Rc::new(move |message| capture.set_status(message))
                },
            },
        )?;
        let owner = Rc::new(Self {
            page,
            services,
            runtime,
            capture,
            preferences,
            callbacks,
            factory: RefCell::new(None),
            retry: RefCell::new(None),
            targets: RefCell::new(VecDeque::new()),
            closed: Cell::new(false),
        });
        current.replace(Rc::downgrade(&owner));
        Ok(owner)
    }
    pub fn set_workflow_factory(&self, factory: Option<HistoryWorkflowFactory>) -> bool {
        if self.closed.get() || self.retry.borrow().is_some() {
            return false;
        }
        self.factory.replace(factory);
        true
    }
    pub fn retry_identifier(&self) -> Option<String> {
        self.retry
            .borrow()
            .as_ref()
            .map(|retry| retry.identifier.clone())
    }

    pub(crate) fn forget_target(&self, identifier: &str) {
        self.targets.borrow_mut().retain(|(id, _)| id != identifier);
    }
    pub fn remember_target(&self, identifier: &str, target: &DeliveryTargetSnapshot) {
        if self.closed.get() {
            return;
        }
        let mut targets = self.targets.borrow_mut();
        targets.retain(|(id, _)| id != identifier);
        targets.push_back((identifier.into(), target.without_selected_text()));
        while targets.len() > 32 {
            targets.pop_front();
        }
    }
    pub fn can_retry_delivery(&self, entry: &HistoryEntry) -> bool {
        !self.closed.get()
            && !entry.delivered_text.is_empty()
            && entry.delivery_outcome != "draft"
            && self.services.config().auto_paste
            && self
                .targets
                .borrow()
                .iter()
                .any(|(id, _)| id == &entry.identifier)
    }
    pub fn retry_delivery(&self, entry: &HistoryEntry) {
        if self.closed.get() {
            return;
        }
        if entry.delivered_text.is_empty() {
            self.status("This history entry has no recognized text to paste or copy.");
            return;
        }
        let started = Instant::now();
        let target = self
            .targets
            .borrow()
            .iter()
            .find(|(id, _)| id == &entry.identifier)
            .map(|(_, target)| target.clone());
        let auto_paste = self.services.config().auto_paste;
        let restored = auto_paste && target.as_ref().is_some_and(DeliveryTargetSnapshot::restore);
        if auto_paste && target.is_some() && !restored {
            self.targets
                .borrow_mut()
                .retain(|(id, _)| id != &entry.identifier);
        }
        let receipt = if let Some(target) = target.as_ref().filter(|_| restored) {
            deliver_text(
                &entry.delivered_text,
                true,
                DeliveryOptions {
                    confirm_paste: Some(
                        &mut || Ok(target.confirm_insertion(&entry.delivered_text)),
                    ),
                    insert_directly: Some(&mut |value| Ok(target.insert_text(value))),
                    authorize_keyboard_paste: Some(&mut || Ok(target.restore())),
                    application_identifier: target.application_identifier(),
                    ..Default::default()
                },
            )
        } else {
            deliver_text(&entry.delivered_text, false, DeliveryOptions::default())
        };
        let mut receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                self.status(&format!("Delivery retry failed: {error}"));
                return;
            }
        };
        if target.is_some() && auto_paste && !restored {
            receipt
                .guidance
                .push_str(" The retained target was stale, so no paste was attempted.");
        }
        let result = (|| {
            let policy = entry
                .audio_retention_policy
                .as_deref()
                .unwrap_or("failures");
            if !["never", "failures", "always"].contains(&policy) {
                return Err(StoreError::Invalid(format!(
                    "'{policy}' is not a valid AudioRetentionPolicy"
                )));
            }
            self.services.history.mark_delivered(
                &entry.identifier,
                &entry.delivered_text,
                receipt.history_outcome(),
                policy == "always",
                Some((started.elapsed().as_secs_f64() * 1000.0).round_ties_even() as i64),
            )?;
            Ok(())
        })();
        self.refresh();
        match result {
            Ok(()) => self.status(&receipt.guidance),
            Err(error) => self.status(&format!(
                "{} History receipt update failed: {error}",
                receipt.guidance
            )),
        }
    }
    pub fn reprocess(&self, entry: &HistoryEntry) {
        if self.closed.get() {
            return;
        }
        if self.retry_identifier().as_deref() == Some(&entry.identifier) {
            self.status("Wait for the active transcription retry before reprocessing this entry.");
            return;
        }
        if (self.callbacks.pending_command)().as_deref() == Some(&entry.identifier) {
            self.status("Apply or discard the active Command preview before reprocessing its history entry.");
            return;
        }
        if self
            .services
            .scratchpad
            .borrow()
            .draft
            .as_ref()
            .and_then(|draft| draft.history_identifier.as_deref())
            == Some(&entry.identifier)
        {
            self.status(
                "Resolve or delete the active Notes draft before reprocessing its history entry.",
            );
            return;
        }
        let result = reprocess_history_entry(
            &self.services.config(),
            &self.services.history,
            Some(&self.services.personalization.borrow()),
            &entry.identifier,
        );
        match result {
            Ok(entry) => {
                self.capture
                    .output_view
                    .buffer()
                    .set_text(&entry.delivered_text);
                self.refresh();
                (self.callbacks.changed)();
                self.status(
                    "Raw transcript reprocessed locally. Review it, then paste or copy explicitly.",
                );
            }
            Err(error) => self.status(&format!("History reprocessing failed: {error}")),
        }
    }
    pub fn retry_recognition(self: &Rc<Self>, entry: &HistoryEntry) {
        if self.closed.get() {
            return;
        }
        let Some(factory) = self.factory.borrow().clone() else {
            self.status("Recognition retry is unavailable until capture services are configured.");
            return;
        };
        let activity = (self.callbacks.activity)();
        if activity.meeting_processing || activity.meeting_retrying || activity.meeting_recording {
            self.status(
                "Finish the explicit Meeting capture or retry before retrying dictation audio.",
            );
            return;
        }
        if activity.preparing || activity.processing || activity.recording {
            self.status("Finish the active recording before retrying retained audio.");
            return;
        }
        if self.retry.borrow().is_some() {
            self.status("A transcription retry is already in progress.");
            return;
        }
        let workflow = match factory() {
            Ok(workflow) => workflow,
            Err(error) => {
                self.retry_failed(&error.to_string());
                return;
            }
        };
        let retry = Rc::new(Retry {
            identifier: entry.identifier.clone(),
            workflow,
            cancelled: CancellationToken::new(),
            finished: Cell::new(false),
            done: Notify::new(),
        });
        self.retry.replace(Some(retry.clone()));
        let mut retry_activity = activity;
        retry_activity.retrying = true;
        self.preferences.set_activity(retry_activity);
        self.capture.record_button.set_sensitive(false);
        self.preferences.language.set_sensitive(false);
        self.preferences.microphone.set_sensitive(false);
        self.preferences.system_audio.set_sensitive(false);
        self.preferences.refresh_audio.set_sensitive(false);
        self.status("Retrying transcription from retained audio with ElevenLabs Scribe v2…");
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = tokio::select! {
                biased;
                _ = retry.cancelled.cancelled() => None,
                result = retry.workflow.retry_recognition(&retry.identifier) => Some(result),
            };
            retry.workflow.close().await;
            retry.finished.set(true);
            retry.done.notify_waiters();
            let Some(owner) = weak.upgrade().filter(|owner| {
                !owner.closed.get()
                    && owner
                        .retry
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| Rc::ptr_eq(current, &retry))
            }) else {
                return;
            };
            owner.retry.borrow_mut().take();
            owner.reset();
            match result {
                Some(Ok(entry)) => {
                    owner
                        .capture
                        .output_view
                        .buffer()
                        .set_text(&entry.delivered_text);
                    owner.refresh();
                    (owner.callbacks.changed)();
                    owner.status(
                        "Transcription recovered. Review it in History, then copy explicitly.",
                    );
                    if entry.mode == "dictation" {
                        (owner.callbacks.queue_title)(&entry);
                    }
                }
                Some(Err(error)) => owner.retry_failed(&error.to_string()),
                None => {}
            }
        });
    }
    fn retry_failed(&self, message: &str) {
        self.refresh();
        self.status(&format!(
            "Transcription retry failed: {message}. History and retained audio remain recoverable."
        ));
    }
    pub(crate) fn reset(&self) {
        let activity = (self.callbacks.activity)();
        self.preferences.set_activity(activity.clone());
        let available = !activity.pending_review
            && !activity.meeting_processing
            && !activity.meeting_retrying
            && !activity.meeting_recording;
        self.preferences.set_controls_available(available);
        let mut state = self.capture.view_state();
        state.preparing = false;
        state.review_active = activity.pending_review;
        state.meeting_busy =
            activity.meeting_processing || activity.meeting_retrying || activity.meeting_recording;
        self.capture.set_view_state(state);
        (self.callbacks.idle)();
    }
    pub fn delete_scratchpad(&self, draft: &ScratchpadDraft) -> StoreResult<()> {
        if let Some(id) = &draft.history_identifier {
            match self.services.history.delete(id) {
                Ok(()) => self.services.scratchpad.borrow_mut().clear(false),
                Err(StoreError::NotFound) => self.services.scratchpad.borrow_mut().clear(true),
                Err(error) => Err(error),
            }
        } else {
            self.services.scratchpad.borrow_mut().clear(true)
        }
    }
    pub fn delete(&self, entry: &HistoryEntry) -> bool {
        if self.closed.get() {
            return false;
        }
        if self.retry_identifier().as_deref() == Some(&entry.identifier) {
            self.status("Wait for the active transcription retry before deleting this entry.");
            return false;
        }
        if (self.callbacks.pending_command)().as_deref() == Some(&entry.identifier) {
            self.status(
                "Apply or discard the active Command preview before deleting its history entry.",
            );
            return false;
        }
        let result = (|| {
            let mut identifiers = vec![entry.identifier.clone()];
            identifiers.extend(
                self.services
                    .history
                    .continuations(&entry.identifier)?
                    .into_iter()
                    .map(|entry| entry.identifier),
            );
            for id in identifiers {
                for image in self.services.screenshots.recent(&id, false)? {
                    (self.callbacks.close_screenshot)(&image.identifier)?;
                }
            }
            let draft = self
                .services
                .scratchpad
                .borrow()
                .draft
                .clone()
                .filter(|draft| draft.history_identifier.as_deref() == Some(&entry.identifier));
            if let Some(draft) = draft {
                self.delete_scratchpad(&draft)?;
                self.capture.output_view.buffer().set_text("");
                self.capture.scratchpad_actions.set_visible(false);
                self.reset();
            } else {
                self.services.history.delete(&entry.identifier)?;
            }
            Ok::<_, StoreError>(())
        })();
        if let Err(error) = result {
            self.status(&format!("History deletion failed: {error}"));
            return false;
        }
        self.targets
            .borrow_mut()
            .retain(|(id, _)| id != &entry.identifier);
        true
    }
    fn refresh(&self) {
        if let Err(error) = self.page.refresh() {
            self.status(&error.to_string());
        }
    }
    fn status(&self, message: &str) {
        self.capture.set_status(message);
    }
    /// The root must await this before quitting its main context.
    pub fn shutdown(&self) -> glib::JoinHandle<()> {
        self.closed.set(true);
        self.factory.borrow_mut().take();
        self.targets.borrow_mut().clear();
        let retry = self.retry.borrow_mut().take();
        if let Some(retry) = &retry {
            retry.cancelled.cancel();
        }
        self.runtime.spawn(async move {
            if let Some(retry) = retry {
                retry.close().await;
            }
        })
    }
}
impl Drop for HistoryController {
    fn drop(&mut self) {
        self.shutdown();
    }
}
