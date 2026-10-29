//! Capture page/session ownership; no microphone work runs on the GTK thread.

use crate::{
    async_runtime::DesktopRuntime,
    capture_view::{CapturePage, CaptureViewState},
};
use gtk::prelude::*;
use mluva_core::{
    delivery::{DeliveryOptions, DeliveryReceipt, deliver_text},
    text,
};
use mluva_workflows::{
    capture::{CaptureError, CapturePhase, CaptureSession},
    dictation::{
        DeliveryAppendix, DeliveryTarget, WorkflowError, WorkflowOutcome, WorkflowResult,
        with_appendix,
    },
};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone, Debug)]
pub enum CaptureOrigin {
    Manual,
    Transcript,
    Phone(String),
    ApprovedShortcut,
    AgentVoice(String),
    Continuation(String),
}

pub struct CaptureLaunch {
    pub session: Rc<CaptureSession>,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}

pub type PrepareCapture = Rc<dyn Fn(CaptureOrigin) -> WorkflowOutcome<CaptureLaunch>>;
/// Text delivered after the dictation, such as a screen annotation summary, decided at Stop.
pub type CaptureAppendix = Rc<dyn Fn(&CaptureSession) -> Option<DeliveryAppendix>>;

pub struct CaptureCompletion {
    pub result: WorkflowResult,
    /// Frozen settings needed by acceptance and continuation owners.
    pub session: Rc<CaptureSession>,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}
pub struct CaptureFailure {
    pub phase: CapturePhase,
    pub error: WorkflowError,
    pub session_identifier: String,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}

/// Application services own continuation and retained-target caches.
pub struct CaptureControllerCallbacks {
    pub appendix: CaptureAppendix,
    /// Reconcile saved history before selecting the completed conversation.
    pub refresh_history: Rc<dyn Fn(&mut WorkflowResult)>,
    pub completed: Rc<dyn Fn(CaptureCompletion)>,
    pub failed: Rc<dyn Fn(CaptureFailure)>,
    pub cancelled: Rc<dyn Fn(&str)>,
    pub phase_changed: Rc<dyn Fn(CapturePhase)>,
    pub preview_changed: Rc<dyn Fn()>,
}

pub struct CaptureController {
    pub page: Rc<CapturePage>,
    runtime: Rc<DesktopRuntime>,
    prepare: PrepareCapture,
    callbacks: CaptureControllerCallbacks,
    active: RefCell<Option<CaptureLaunch>>,
    timer: RefCell<Option<glib::SourceId>>,
    preview_task: RefCell<Option<glib::JoinHandle<()>>>,
    keys: gtk::EventControllerKey,
    key_root: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
    continuation: RefCell<Option<String>>,
    continuation_source: RefCell<String>,
}

impl CaptureController {
    pub fn attach(
        page: Rc<CapturePage>,
        runtime: Rc<DesktopRuntime>,
        prepare: PrepareCapture,
        callbacks: CaptureControllerCallbacks,
    ) -> Rc<Self> {
        let controller = Rc::new(Self {
            page,
            runtime,
            prepare,
            callbacks,
            active: RefCell::new(None),
            timer: RefCell::new(None),
            preview_task: RefCell::new(None),
            keys: gtk::EventControllerKey::new(),
            key_root: RefCell::new(None),
            continuation: RefCell::new(None),
            continuation_source: RefCell::new(String::new()),
        });
        let weak = Rc::downgrade(&controller);
        controller.page.set_recording_action(Rc::new(move || {
            if let Some(controller) = weak.upgrade() {
                controller.toggle(CaptureOrigin::Manual);
            }
        }));
        let weak = Rc::downgrade(&controller);
        controller.keys.connect_key_pressed(move |_, key, _, _| {
            if key != gtk::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            let Some(controller) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !controller.page.widget.is_mapped() {
                return glib::Propagation::Proceed;
            }
            let modal = controller.page.widget.root().is_some_and(|root| {
                root.find_property("visible-dialog").is_some()
                    && root
                        .property::<Option<adw::Dialog>>("visible-dialog")
                        .is_some()
            });
            if !modal && controller.cancel() {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = Rc::downgrade(&controller);
        controller.page.widget.connect_root_notify(move |_| {
            if let Some(controller) = weak.upgrade() {
                controller.bind_keys();
            }
        });
        controller.bind_keys();
        controller
    }

    fn bind_keys(&self) {
        self.unbind_keys();
        if let Some(root) = self
            .page
            .widget
            .root()
            .and_then(|root| root.upcast::<glib::Object>().downcast::<gtk::Widget>().ok())
        {
            root.add_controller(self.keys.clone());
            self.key_root.replace(Some(root.downgrade()));
        }
    }
    fn unbind_keys(&self) {
        if let Some(root) = self
            .key_root
            .borrow_mut()
            .take()
            .and_then(|root| root.upgrade())
        {
            root.remove_controller(&self.keys);
        }
    }

    pub fn phase(&self) -> Option<CapturePhase> {
        self.active
            .borrow()
            .as_ref()
            .map(|active| active.session.phase())
    }
    pub fn session_identifier(&self) -> Option<String> {
        self.active
            .borrow()
            .as_ref()
            .map(|active| active.session.identifier.clone())
    }

    /// Called after the application accepts and persists a Live preference edit.
    /// Capture readiness, Stop and Incognito keep their immutable session rules.
    pub fn toggle(self: &Rc<Self>, origin: CaptureOrigin) {
        if matches!(origin, CaptureOrigin::Continuation(_)) && self.phase().is_some() {
            return;
        }
        match self.phase() {
            Some(CapturePhase::Preparing) => {
                self.cancel();
            }
            Some(CapturePhase::Recording) => self.stop(),
            Some(_) => self
                .page
                .set_status("The stopped recording is still being processed."),
            None => self.start(origin),
        }
    }

    fn start(self: &Rc<Self>, origin: CaptureOrigin) {
        let view = self.page.view_state();
        if view.meeting_busy {
            return;
        }
        let manual = matches!(
            origin,
            CaptureOrigin::Manual | CaptureOrigin::Transcript | CaptureOrigin::Continuation(_)
        );
        let continuation = match &origin {
            CaptureOrigin::Continuation(identifier) => {
                if self.page.config().incognito_mode {
                    return;
                }
                let prepared = (|| {
                    let entry = self
                        .page
                        .workspace
                        .store
                        .history
                        .recording_conversation(identifier)?;
                    if entry.mode != "dictation" {
                        return Err(mluva_core::database::StoreError::Invalid(
                            "Choose a dictation conversation.".into(),
                        ));
                    }
                    if !self
                        .page
                        .workspace
                        .save_conversation_edits(&entry.identifier)
                    {
                        return Err(mluva_core::database::StoreError::Invalid(
                            "Could not save the existing document.".into(),
                        ));
                    }
                    let source = self.page.workspace.store.source_text(&entry, false)?;
                    let replies = self.page.workspace.store.replies(&entry.identifier)?;
                    let identifier = entry.identifier.clone();
                    self.page
                        .workspace
                        .show_conversation(Some(entry), &replies, false)?;
                    Ok::<_, mluva_core::database::StoreError>((identifier, source))
                })();
                let (identifier, source) = match prepared {
                    Ok(prepared) => prepared,
                    Err(_) => {
                        self.page
                            .set_status("This conversation is no longer available.");
                        return;
                    }
                };
                self.continuation_source.replace(source);
                Some(identifier)
            }
            _ => {
                self.continuation_source.borrow_mut().clear();
                None
            }
        };
        let origin = continuation.as_ref().map_or(origin, |identifier| {
            CaptureOrigin::Continuation(identifier.clone())
        });
        let launch = match (self.prepare)(origin) {
            Ok(launch) => launch,
            Err(error) => {
                self.page.set_status(&error.to_string());
                return;
            }
        };
        // Manual capture has no reason to read or restore another application's
        // text target. The factory must honor that boundary before returning.
        if manual
            && (launch.session.options.allow_auto_paste
                || launch.delivery_target.is_some()
                || launch.session.options.application_identifier.is_some())
        {
            self.page
                .set_error("Manual recording cannot capture an external delivery target.");
            return;
        }
        let session = launch.session.clone();
        // Keep only a wakeup, never a queue of transcript revisions. Snapshot
        // the current session on GTK and reject late results by its identity.
        let (preview_tx, mut preview_rx) = tokio::sync::watch::channel(());
        session.set_preview_callback(Box::new(move |_| {
            let _ = preview_tx.send(());
            Ok(())
        }));
        self.continuation.replace(continuation.clone());
        self.page.workspace.set_private(session.options.incognito);
        self.page.set_view_state(CaptureViewState {
            preparing: true,
            ..view
        });
        self.page
            .set_status("Preparing recognition before microphone capture…");
        if continuation.is_some() {
            self.page
                .workspace
                .set_live("Continuing…", &self.continuation_source.borrow(), false);
        } else {
            self.page
                .workspace
                .set_live("Preparing…", "Preparing recognition…", false);
        }
        self.active.replace(Some(launch));
        (self.callbacks.phase_changed)(CapturePhase::Preparing);
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let ready = session.prepare_and_start().await;
            let Some(controller) = weak
                .upgrade()
                .filter(|controller| controller.owns(&session.identifier))
            else {
                return;
            };
            match ready {
                Ok(ready) => {
                    controller.page.set_view_state(CaptureViewState {
                        recording: true,
                        ..Default::default()
                    });
                    controller.page.set_status(ready.status());
                    controller.start_clock();
                    (controller.callbacks.phase_changed)(CapturePhase::Recording);
                    let weak = Rc::downgrade(&controller);
                    let identifier = session.identifier.clone();
                    let task = controller.runtime.spawn(async move {
                        while preview_rx.changed().await.is_ok() {
                            let Some(controller) = weak.upgrade().filter(|owner| {
                                owner.owns(&identifier)
                                    && owner.phase() == Some(CapturePhase::Recording)
                            }) else {
                                break;
                            };
                            controller.update_clock();
                            (controller.callbacks.preview_changed)();
                        }
                    });
                    controller.preview_task.replace(Some(task));
                }
                Err(CaptureError::Cancelled(cancelled)) => {
                    controller.cancel_finished(&session.identifier, cancelled.status())
                }
                Err(CaptureError::Failed(message)) => {
                    controller.release(&session.identifier);
                    controller.page.set_error(&message);
                    (controller.callbacks.phase_changed)(CapturePhase::Failed);
                    (controller.callbacks.failed)(CaptureFailure {
                        phase: CapturePhase::Preparing,
                        error: WorkflowError::Invalid(message),
                        session_identifier: session.identifier.clone(),
                        delivery_target: None,
                    });
                }
            }
        });
    }

    pub fn cancel(self: &Rc<Self>) -> bool {
        let Some(session) = self
            .active
            .borrow()
            .as_ref()
            .map(|active| active.session.clone())
        else {
            return false;
        };
        if !session.request_cancel() {
            return false;
        }
        self.clear_clock();
        self.page.record_button.set_sensitive(false);
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let cancelled = session.cancel().await;
            if let Some(controller) = weak
                .upgrade()
                .filter(|controller| controller.owns(&session.identifier))
                && let Some(cancelled) = cancelled
            {
                controller.cancel_finished(&session.identifier, cancelled.status());
            }
        });
        true
    }

    /// Release the current identity synchronously, then acknowledge audio and
    /// provider cleanup before the application exits its main context.
    pub fn shutdown(&self) -> glib::JoinHandle<()> {
        self.unbind_keys();
        self.clear_clock();
        let active = self.active.borrow_mut().take();
        if let Some(active) = &active {
            active.session.request_shutdown();
        }
        self.runtime.spawn(async move {
            if let Some(active) = active {
                active.session.shutdown().await;
            }
        })
    }

    fn cancel_finished(&self, identifier: &str, status: &str) {
        if !self.owns(identifier) {
            return;
        }
        self.release(identifier);
        self.page.set_status(status);
        (self.callbacks.phase_changed)(CapturePhase::Cancelled);
        (self.callbacks.cancelled)(identifier);
    }

    pub fn stop(self: &Rc<Self>) {
        let (session, target) = {
            let active = self.active.borrow();
            let Some(active) = active.as_ref() else {
                return;
            };
            (active.session.clone(), active.delivery_target.clone())
        };
        if !session.begin_stop() {
            return;
        }
        self.clear_clock();
        let status_title = self.page.status_title.label();
        self.page.set_view_state(CaptureViewState {
            processing: true,
            ..Default::default()
        });
        // The released stop callback retains the recording title until a later
        // status-row refresh. Preserve that observed transition independently of
        // the generic view projection used by settings and initialization.
        self.page.status_title.set_label(&status_title);
        self.page.set_status(if session.realtime_healthy() {
            "Finalizing recognized text…"
        } else {
            "Transcribing with the selected speech provider…"
        });
        self.page.workspace.set_live_status("Processing…", false);
        (self.callbacks.phase_changed)(CapturePhase::Processing);
        let appendix = (self.callbacks.appendix)(&session);
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let mut result = session
                .complete_with_appendix(target.as_deref(), appendix)
                .await;
            let Some(controller) = weak
                .upgrade()
                .filter(|controller| controller.owns(&session.identifier))
            else {
                return;
            };
            let viewing_live = controller.page.workspace.is_viewing_live();
            let continuation = controller.continuation.borrow().clone();
            if let Some(identifier) = continuation.as_deref()
                && let Ok(result) = &mut result
                && let Some(segment) = result.history_entry.as_ref().filter(|_| !result.incognito)
            {
                let appended = (|| {
                    if !controller
                        .page
                        .workspace
                        .save_conversation_edits(identifier)
                    {
                        return Err(mluva_core::database::StoreError::Invalid(
                            "Could not save the existing document.".into(),
                        ));
                    }
                    let output = controller
                        .page
                        .workspace
                        .store
                        .append_recording(identifier, segment)?;
                    let entry = controller.page.workspace.store.history.find(identifier)?;
                    Ok::<_, mluva_core::database::StoreError>((output, entry))
                })();
                let receipt = |guidance: &str| DeliveryReceipt {
                    copied: false,
                    pasted: false,
                    guidance: guidance.into(),
                    paste_dispatched: false,
                    paste_confirmed: None,
                };
                match appended {
                    Ok((output, entry)) => {
                        result.delivery = deliver_text(
                            &with_appendix(&output, result.delivery_appendix.as_deref()),
                            false,
                            DeliveryOptions::default(),
                        )
                        .unwrap_or_else(|_| {
                            receipt("Recording appended. Automatic copy failed; use Copy.")
                        });
                        result.output_text = output;
                        result.history_entry = Some(entry);
                    }
                    Err(_) => {
                        result.delivery = receipt(
                            "Could not append. This recording remains separately in History.",
                        );
                        controller.continuation.borrow_mut().take();
                        controller.continuation_source.borrow_mut().clear();
                    }
                }
            }
            controller.release(&session.identifier);
            match result {
                Ok(mut result) => {
                    (controller.callbacks.refresh_history)(&mut result);
                    if !text::trim(&result.transcription.text).is_empty()
                        || result.history_entry.is_some()
                    {
                        let displayed = if let Some(entry) = result.history_entry.clone() {
                            controller.page.workspace.refresh_history().and_then(|()| {
                                if viewing_live {
                                    let replies = controller
                                        .page
                                        .workspace
                                        .store
                                        .replies(&entry.identifier)?;
                                    controller.page.workspace.show_conversation(
                                        Some(entry),
                                        &replies,
                                        false,
                                    )
                                } else {
                                    Ok(())
                                }
                            })
                        } else {
                            controller
                                .page
                                .workspace
                                .show_transient(&result.transcription.text, &result.output_text)
                        };
                        if let Err(error) = displayed {
                            controller
                                .page
                                .workspace
                                .notice
                                .set_label(&error.to_string());
                        }
                    }
                    controller.page.set_status(&result.delivery.guidance);
                    (controller.callbacks.phase_changed)(CapturePhase::Completed);
                    (controller.callbacks.completed)(CaptureCompletion {
                        result,
                        session: session.clone(),
                        delivery_target: target,
                    });
                }
                Err(error) => {
                    controller
                        .page
                        .set_status("Capture failed. Open History to recover retained audio.");
                    if let WorkflowError::Failure(failure) = &error {
                        let recovery = if let Some(path) = &failure.retained_audio_path {
                            format!("The recording was retained at {}.", path.display())
                        } else {
                            "The recording was erased.".into()
                        };
                        controller.page.set_error(&format!(
                            "Mluva could not complete: {}. {recovery}",
                            failure.message
                        ));
                        if let Some(entry) = failure.history_entry.clone() {
                            if let Err(error) = controller
                                .page
                                .workspace
                                .show_conversation(Some(entry), &[], false)
                                .and_then(|()| controller.page.workspace.refresh_history())
                            {
                                controller
                                    .page
                                    .workspace
                                    .notice
                                    .set_label(&error.to_string());
                            }
                        } else if !failure.output_text.is_empty()
                            && let Err(error) = controller
                                .page
                                .workspace
                                .show_transient(&failure.output_text, &failure.output_text)
                        {
                            controller
                                .page
                                .workspace
                                .notice
                                .set_label(&error.to_string());
                        }
                    } else {
                        controller.page.set_error(&error.to_string());
                    }
                    (controller.callbacks.phase_changed)(CapturePhase::Failed);
                    (controller.callbacks.failed)(CaptureFailure {
                        phase: CapturePhase::Processing,
                        error,
                        session_identifier: session.identifier.clone(),
                        delivery_target: target,
                    });
                }
            }
        });
    }

    fn start_clock(self: &Rc<Self>) {
        self.clear_clock();
        self.update_clock();
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(250), move || {
            let Some(controller) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let session = controller
                .active
                .borrow()
                .as_ref()
                .map(|active| active.session.clone());
            let Some(session) =
                session.filter(|session| session.phase() == CapturePhase::Recording)
            else {
                controller.timer.borrow_mut().take();
                return glib::ControlFlow::Break;
            };
            controller.update_clock_for(&session);
            glib::ControlFlow::Continue
        });
        self.timer.replace(Some(source));
    }
    fn update_clock(&self) {
        if let Some(session) = self
            .active
            .borrow()
            .as_ref()
            .map(|active| active.session.clone())
        {
            self.update_clock_for(&session);
        }
    }
    fn update_clock_for(&self, session: &CaptureSession) {
        let elapsed = session.elapsed_seconds().unwrap_or(0.0) as u64;
        let continued = self.continuation.borrow().is_some();
        let preview = if session.realtime_healthy() {
            session
                .preview()
                .map(|preview| preview.display_text())
                .filter(|preview| !preview.is_empty())
                .unwrap_or_else(|| {
                    if continued {
                        String::new()
                    } else {
                        "Waiting for speech…".into()
                    }
                })
        } else {
            if continued {
                String::new()
            } else {
                "Realtime preview unavailable; finalized local audio will use batch recognition."
                    .into()
            }
        };
        let elapsed = format!("{:02}:{:02}", elapsed / 60, elapsed % 60);
        let preview = if continued {
            if preview.is_empty() {
                self.continuation_source.borrow().clone()
            } else {
                format!(
                    "{}\n\n{preview}",
                    self.continuation_source
                        .borrow()
                        .trim_end_matches(text::whitespace)
                )
            }
        } else {
            preview
        };
        self.page.workspace.set_live(
            &if continued {
                format!("Continuing · {elapsed}")
            } else {
                elapsed
            },
            &preview,
            true,
        );
    }
    fn clear_clock(&self) {
        if let Some(task) = self.preview_task.borrow_mut().take() {
            task.abort();
        }
        if let Some(source) = self.timer.borrow_mut().take() {
            source.remove();
        }
    }
    fn owns(&self, identifier: &str) -> bool {
        self.active
            .borrow()
            .as_ref()
            .is_some_and(|active| active.session.identifier == identifier)
    }
    fn release(&self, identifier: &str) {
        if !self.owns(identifier) {
            return;
        }
        self.clear_clock();
        self.active.borrow_mut().take();
        self.continuation.borrow_mut().take();
        self.continuation_source.borrow_mut().clear();
        self.page.workspace.finish_live();
        self.page.set_view_state(CaptureViewState::default());
    }
}

impl Drop for CaptureController {
    fn drop(&mut self) {
        self.unbind_keys();
        self.clear_clock();
        if let Some(active) = self.active.get_mut().take() {
            active.session.request_shutdown();
            self.runtime.spawn(async move {
                active.session.shutdown().await;
            });
        }
    }
}
