//! Capture page/session ownership; no microphone work runs on the GTK thread.

use crate::{
    async_runtime::DesktopRuntime,
    capture_view::{CapturePage, CaptureViewState},
};
use gtk::prelude::*;
use mluva_core::{screenshots::ImageInput, text};
use mluva_workflows::{
    capture::{CaptureError, CapturePhase, CaptureSession},
    dictation::{DeliveryTarget, WorkflowError, WorkflowOutcome, WorkflowResult},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone, Debug)]
pub enum CaptureOrigin {
    Manual,
    ApprovedShortcut,
    Continuation(String),
}

pub struct CaptureLaunch {
    pub session: Rc<CaptureSession>,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}

pub type PrepareCapture = Rc<dyn Fn(CaptureOrigin) -> WorkflowOutcome<CaptureLaunch>>;
pub type CaptureImages = Rc<dyn Fn(&str) -> WorkflowOutcome<Vec<ImageInput>>>;

pub struct CaptureCompletion {
    pub result: WorkflowResult,
    /// Frozen settings needed by acceptance and continuation owners.
    pub session: Rc<CaptureSession>,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}
pub struct CaptureFailure {
    pub error: WorkflowError,
    pub session_identifier: String,
    pub delivery_target: Option<Rc<dyn DeliveryTarget>>,
}

/// The remaining application services own review/continuation, screenshot picker
/// waiting and retained-target caches. They receive one terminal result here.
pub struct CaptureControllerCallbacks {
    pub images: CaptureImages,
    pub completed: Rc<dyn Fn(CaptureCompletion)>,
    pub failed: Rc<dyn Fn(CaptureFailure)>,
    pub cancelled: Rc<dyn Fn(&str)>,
    pub phase_changed: Rc<dyn Fn(CapturePhase)>,
}

pub struct CaptureController {
    pub page: Rc<CapturePage>,
    runtime: Rc<DesktopRuntime>,
    prepare: PrepareCapture,
    callbacks: CaptureControllerCallbacks,
    active: RefCell<Option<CaptureLaunch>>,
    timer: RefCell<Option<glib::SourceId>>,
    keys: gtk::EventControllerKey,
    key_root: RefCell<Option<glib::WeakRef<gtk::Widget>>>,
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
            keys: gtk::EventControllerKey::new(),
            key_root: RefCell::new(None),
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

    pub fn toggle(self: &Rc<Self>, origin: CaptureOrigin) {
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
        if view.initialization_failed || view.review_active || view.meeting_busy {
            return;
        }
        let manual = matches!(
            origin,
            CaptureOrigin::Manual | CaptureOrigin::Continuation(_)
        );
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
                || launch.session.options.application_identifier.is_some()
                || launch.session.options.selected_text.is_some())
        {
            self.page
                .set_error("Manual recording cannot capture an external delivery target.");
            return;
        }
        let session = launch.session.clone();
        self.page.set_pending_mode(&session.options.mode);
        self.page.workspace.set_private(session.options.incognito);
        self.page
            .workspace
            .set_screenshot_context(Some(session.identifier.clone()), None);
        self.page.set_view_state(CaptureViewState {
            preparing: true,
            ..view
        });
        self.page.set_status(
            if session.options.mode == "command"
                || session.options.use_cleanup
                || session.frozen_style.is_some()
            {
                "Preparing recognition and resolving the Codex model before microphone capture…"
            } else {
                "Preparing recognition before microphone capture…"
            },
        );
        self.page
            .workspace
            .set_live("Preparing…", "Preparing recognition…", false);
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
                    (controller.callbacks.phase_changed)(CapturePhase::Recording);
                    controller.start_clock();
                }
                Err(CaptureError::Cancelled(cancelled)) => {
                    controller.cancel_finished(&session.identifier, cancelled.status())
                }
                Err(CaptureError::Failed(message)) => {
                    controller.release(&session.identifier);
                    if message.starts_with("Codex preparation failed before microphone capture:") {
                        controller.page.set_status(&message);
                    } else {
                        controller.page.set_error(&message);
                    }
                    (controller.callbacks.phase_changed)(CapturePhase::Failed);
                    (controller.callbacks.failed)(CaptureFailure {
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

    fn cancel_finished(&self, identifier: &str, status: &str) {
        if !self.owns(identifier) {
            return;
        }
        self.release(identifier);
        self.page.set_status(status);
        (self.callbacks.phase_changed)(CapturePhase::Cancelled);
        (self.callbacks.cancelled)(identifier);
    }

    fn stop(self: &Rc<Self>) {
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
            "Finalizing committed ElevenLabs Scribe v2 Realtime text…"
        } else {
            "Transcribing with the selected speech provider…"
        });
        self.page.workspace.set_live_status("Processing…", false);
        (self.callbacks.phase_changed)(CapturePhase::Processing);
        let images = (self.callbacks.images)(&session.identifier);
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let mut visual_warning = false;
            let images = images.unwrap_or_else(|_| {
                visual_warning = true;
                vec![]
            });
            let result = session.complete(target.as_deref(), images).await;
            let Some(controller) = weak
                .upgrade()
                .filter(|controller| controller.owns(&session.identifier))
            else {
                return;
            };
            let viewing_live = controller.page.workspace.is_viewing_live();
            controller.release(&session.identifier);
            match result {
                Ok(mut result) => {
                    if visual_warning {
                        result.delivery.guidance.push_str(" Screenshot was not ready. Save it in the editor, then rewrite with its context.");
                    }
                    controller.page.output_view.buffer().set_text(&result.output_text);
                    controller.page.refresh_output_visibility();
                    if result.mode == "dictation" && !text::trim(&result.transcription.text).is_empty() {
                        let displayed = if let Some(entry) = result.history_entry.clone() {
                            controller.page.workspace.refresh_history().and_then(|()| {
                                if viewing_live && text::trim(&controller.page.workspace.prompt_text()).is_empty() {
                                    let replies = controller.page.workspace.store.replies(&entry.identifier)?;
                                    controller.page.workspace.show_conversation(Some(entry), &replies, false)
                                } else {
                                    Ok(())
                                }
                            })
                        } else {
                            controller.page.workspace.show_transient(&result.transcription.text, &result.output_text)
                        };
                        if let Err(error) = displayed {
                            controller.page.workspace.notice.set_label(&error.to_string());
                        }
                    }
                    controller.page.set_status(&result.delivery.guidance);
                    if result.mode == "dictation"
                        && !result.incognito
                        && !controller.page.config().incognito_mode
                        && let Some(entry) = result.history_entry.as_ref().filter(|entry| !text::trim(&entry.raw_text).is_empty())
                    {
                        let title = mluva_core::titles::fallback_title(&entry.raw_text);
                        if controller.page.workspace.store.history.save_generated_title(&entry.identifier, &title, None).unwrap_or(false) {
                            let _ = controller.page.workspace.refresh_title(&entry.identifier);
                        }
                    }
                    (controller.callbacks.phase_changed)(CapturePhase::Completed);
                    (controller.callbacks.completed)(CaptureCompletion {
                        result,
                        session: session.clone(),
                        delivery_target: target,
                    });
                }
                Err(error) => {
                    if let WorkflowError::Failure(failure) = &error {
                        if !failure.output_text.is_empty() {
                            controller.page.output_view.buffer().set_text(&failure.output_text);
                        }
                        let recovery = if let Some(path) = &failure.retained_audio_path {
                            format!("The recording was retained at {}.", path.display())
                        } else {
                            "The recording was erased.".into()
                        };
                        controller.page.set_error(&format!("Mluva could not complete: {}. {recovery}", failure.message));
                        if let Some(entry) = failure.history_entry.clone() {
                            if let Err(error) = controller.page.workspace.show_conversation(Some(entry), &[], false).and_then(|()| controller.page.workspace.refresh_history()) {
                                controller.page.workspace.notice.set_label(&error.to_string());
                            }
                        } else if !failure.output_text.is_empty()
                            && let Err(error) = controller.page.workspace.show_transient(&failure.output_text, &failure.output_text) {
                            controller.page.workspace.notice.set_label(&error.to_string());
                        }
                    } else {
                        controller.page.set_error(&error.to_string());
                    }
                    (controller.callbacks.phase_changed)(CapturePhase::Failed);
                    (controller.callbacks.failed)(CaptureFailure {
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
        let preview = if session.realtime_healthy() {
            session
                .preview()
                .map(|preview| preview.display_text())
                .filter(|preview| !preview.is_empty())
                .unwrap_or_else(|| "Waiting for speech…".into())
        } else {
            "Realtime preview unavailable; finalized local audio will use batch recognition.".into()
        };
        self.page.workspace.set_live(
            &format!("{:02}:{:02}", elapsed / 60, elapsed % 60),
            &preview,
            true,
        );
    }
    fn clear_clock(&self) {
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
        self.page.workspace.finish_live();
        self.page.workspace.set_screenshot_context(None, None);
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
