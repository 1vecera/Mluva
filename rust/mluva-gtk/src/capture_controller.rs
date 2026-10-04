//! Capture page/session ownership; no microphone work runs on the GTK thread.

use crate::{
    async_runtime::DesktopRuntime,
    capture_view::{CapturePage, CaptureViewState},
    live_controller::LiveController,
};
use gtk::prelude::*;
use mluva_core::{
    config::AppConfig,
    delivery::{DeliveryOptions, DeliveryReceipt, deliver_text},
    screenshots::ImageInput,
    text,
};
use mluva_workflows::{
    capture::{CaptureError, CapturePhase, CaptureSession},
    dictation::{DeliveryTarget, WorkflowError, WorkflowOutcome, WorkflowResult},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    time::Duration,
};

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
pub type WaitForCaptureImages =
    Rc<dyn Fn(&str) -> Pin<Box<dyn Future<Output = WorkflowOutcome<()>>>>>;
pub type PrepareCaptureResult = Rc<dyn Fn(&CaptureSession, &mut WorkflowOutcome<WorkflowResult>)>;

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

/// The remaining application services own review/continuation, screenshot picker
/// waiting and retained-target caches. They receive one terminal result here.
pub struct CaptureControllerCallbacks {
    pub images: CaptureImages,
    pub wait_for_images: WaitForCaptureImages,
    pub prepare_result: PrepareCaptureResult,
    /// Reconcile saved history before selecting the completed conversation.
    pub refresh_history: Rc<dyn Fn(&mut WorkflowResult)>,
    pub queue_title: Rc<dyn Fn(&mluva_core::history::HistoryEntry)>,
    pub completed: Rc<dyn Fn(CaptureCompletion)>,
    pub failed: Rc<dyn Fn(CaptureFailure)>,
    pub cancelled: Rc<dyn Fn(&str)>,
    pub phase_changed: Rc<dyn Fn(CapturePhase)>,
    /// Once is disarmed even if saving fails. Return whether it was persisted.
    pub live_config_changed: Rc<dyn Fn(&AppConfig) -> bool>,
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
    live: RefCell<Option<Rc<LiveController>>>,
    preview_task: RefCell<Option<glib::JoinHandle<()>>>,
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
            keys: gtk::EventControllerKey::new(),
            key_root: RefCell::new(None),
            live: RefCell::new(None),
            preview_task: RefCell::new(None),
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
    pub fn bind_live(&self, live: Rc<LiveController>) {
        assert!(Rc::ptr_eq(&self.page.workspace, &live.workspace));
        assert!(self.active.borrow().is_none(), "bind Live before capture");
        self.live.replace(Some(live));
    }

    /// Called after the application accepts and persists a Live preference edit.
    /// Capture readiness, Stop and Incognito keep their immutable session rules.
    pub fn apply_live_config(self: &Rc<Self>, config: AppConfig) -> Result<(), String> {
        let live = self
            .live
            .borrow()
            .clone()
            .ok_or("Live service is unavailable")?;
        let active = self
            .active
            .borrow()
            .as_ref()
            .map(|launch| launch.session.clone());
        if live.finalizing()
            || active.as_ref().is_some_and(|session| {
                session.phase() != CapturePhase::Recording
                    || session.options.incognito
                    || session.options.mode != "dictation"
            })
        {
            return Err("Live settings cannot change during this operation.".into());
        }
        self.page
            .set_config(config.clone())
            .map_err(|error| error.to_string())?;
        live.set_config(config.clone());
        if let Some(session) = active {
            session.set_preview_enabled(config.live_rewrite_enabled);
            if config.live_rewrite_enabled {
                live.begin(
                    &session.identifier,
                    &session.options.mode,
                    session.options.incognito,
                    self.continuation.borrow().as_deref(),
                    true,
                )?;
                if let Some(preview) = session.preview() {
                    live.offer(&preview.display_text(), false);
                }
            } else {
                live.pause();
            }
        }
        Ok(())
    }

    fn consume_live_once(&self) {
        let live = self.live.borrow().clone();
        if let Some(config) = live.and_then(|live| live.consume_once()) {
            let _ = self.page.set_config(config.clone());
            if !(self.callbacks.live_config_changed)(&config) {
                self.page.toast(
                    "Live rewrite is off for this session; the preference could not be saved.",
                );
            }
        }
    }

    pub fn toggle(self: &Rc<Self>, origin: CaptureOrigin) {
        if matches!(origin, CaptureOrigin::Continuation(_)) && self.phase().is_some() {
            return;
        }
        if self
            .live
            .borrow()
            .as_ref()
            .is_some_and(|live| live.finalizing())
        {
            self.page.set_status(
                "Finishing the live draft. Cancel it before starting another recording.",
            );
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
        if view.review_active || view.meeting_busy {
            return;
        }
        let manual = matches!(
            origin,
            CaptureOrigin::Manual | CaptureOrigin::Continuation(_)
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
                || launch.session.options.application_identifier.is_some()
                || launch.session.options.selected_text.is_some())
        {
            self.page
                .set_error("Manual recording cannot capture an external delivery target.");
            return;
        }
        let session = launch.session.clone();
        if let Some(live) = self.live.borrow().as_ref() {
            live.set_config(session.config().clone());
            if let Err(error) = live.begin(
                &session.identifier,
                &session.options.mode,
                session.options.incognito,
                continuation.as_deref(),
                false,
            ) {
                self.page.set_error(&error);
                return;
            }
            session.set_preview_enabled(live.owns(&session.identifier));
        }
        self.continuation.replace(continuation.clone());
        self.bind_preview(&session);
        self.page.set_pending_mode(&session.options.mode);
        self.page.workspace.set_private(session.options.incognito);
        self.page
            .workspace
            .set_screenshot_context(Some(session.identifier.clone()), continuation.clone());
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
                }
                Err(CaptureError::Cancelled(cancelled)) => {
                    controller.cancel_finished(&session.identifier, cancelled.status())
                }
                Err(CaptureError::Failed(message)) => {
                    controller.release(&session.identifier, false);
                    if let Some(live) = controller.live.borrow().as_ref() {
                        live.cancel();
                    }
                    if message.starts_with("Codex preparation failed before microphone capture:") {
                        controller.page.set_status(&message);
                    } else {
                        controller.page.set_error(&message);
                    }
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
        self.clear_preview();
        if let Some(live) = self.live.borrow().as_ref() {
            live.cancel();
        }
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
        self.clear_preview();
        self.consume_live_once();
        if let Some(live) = self.live.borrow().as_ref() {
            live.shutdown();
        }
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
        self.release(identifier, false);
        if let Some(live) = self.live.borrow().as_ref() {
            live.cancel();
        }
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
        let waiting = (self.callbacks.wait_for_images)(&session.identifier);
        let snapshot = self.callbacks.images.clone();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let visual_warning = Cell::new(false);
            let images = async {
                waiting.await?;
                Ok(snapshot(&session.identifier).unwrap_or_else(|_| {
                    visual_warning.set(true);
                    vec![]
                }))
            };
            let mut result = session.complete_with_images(target.as_deref(), images).await;
            let Some(controller) = weak
                .upgrade()
                .filter(|controller| controller.owns(&session.identifier))
            else {
                return;
            };
            (controller.callbacks.prepare_result)(&session, &mut result);
            let viewing_live = controller.page.workspace.is_viewing_live();
            let live = controller.live.borrow().clone();
            let continuation = controller.continuation.borrow().clone();
            if let Some(identifier) = continuation.as_deref() && let Ok(result) = &mut result && let Some(segment) = result.history_entry.as_ref().filter(|_|!result.incognito) {
                let appended = (|| {
                    if !controller.page.workspace.save_conversation_edits(identifier) {return Err(mluva_core::database::StoreError::Invalid("Could not save the existing document.".into()));}
                    let output = controller.page.workspace.store.append_recording(identifier, segment)?;
                    let entry = controller.page.workspace.store.history.find(identifier)?;
                    Ok::<_,mluva_core::database::StoreError>((output,entry))
                })();
                let receipt = |guidance: &str|DeliveryReceipt {copied:false,pasted:false,guidance:guidance.into(),paste_dispatched:false,paste_confirmed:None};
                match appended {
                    Ok((output,entry)) => {
                        result.delivery = if controller.page.config().auto_copy_dictation {
                            deliver_text(&output,false,DeliveryOptions::default()).unwrap_or_else(|_|receipt("Recording appended. Automatic copy failed; use Copy."))
                        } else {receipt("Recording appended.")};
                        result.output_text = output;
                        result.history_entry = Some(entry);
                    }
                    Err(_) => {
                        if let Some(live) = &live {live.cancel();}
                        result.delivery = receipt("Could not append. This recording remains separately in History.");
                        controller.continuation.borrow_mut().take();
                        controller.continuation_source.borrow_mut().clear();
                    }
                }
            }
            let final_source = result.as_ref().ok().map(|result| {
                if controller.continuation.borrow().is_some() && let Some(entry) = &result.history_entry {
                    controller.page.workspace.store.source_text(entry,false).unwrap_or_else(|_|result.transcription.text.clone())
                } else {result.transcription.text.clone()}
            });
            let finishing_live = result.as_ref().is_ok_and(|result| result.mode=="dictation" && !result.incognito && result.history_entry.is_some() && !text::trim(&result.transcription.text).is_empty()) && live.as_ref().is_some_and(|live|live.owns(&session.identifier));
            let preserved_draft = if result.as_ref().is_ok_and(|result|text::trim(&result.transcription.text).is_empty() && result.history_entry.is_none()) && viewing_live && controller.continuation.borrow().is_none() && live.as_ref().is_some_and(|live|live.owns(&session.identifier)) { controller.page.workspace.live_draft() } else { String::new() };
            let failed_draft = if result.is_err() && live.as_ref().is_some_and(|live|live.owns(&session.identifier)) {controller.page.workspace.live_draft()} else { String::new() };
            controller.release(&session.identifier, finishing_live);
            if !finishing_live && let Some(live) = &live { live.cancel(); }
            match result {
                Ok(mut result) => {
                    if visual_warning.get() {
                        result.delivery.guidance.push_str(" Screenshot was not ready. Save it in the editor, then rewrite with its context.");
                    }
                    controller.page.output_view.buffer().set_text(&result.output_text);
                    controller.page.refresh_output_visibility();
                    (controller.callbacks.refresh_history)(&mut result);
                    if result.mode == "dictation" && (!text::trim(&result.transcription.text).is_empty() || result.history_entry.is_some()) {
                        let displayed = if let Some(entry) = result.history_entry.clone() {
                            controller.page.workspace.refresh_history().and_then(|()| {
                                if viewing_live && text::trim(&controller.page.workspace.prompt_text()).is_empty() {
                                    let replies = controller.page.workspace.store.replies(&entry.identifier)?;
                                    controller.page.workspace.show_conversation(Some(entry), &replies, finishing_live)
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
                    if !text::trim(&preserved_draft).is_empty() {
                        let _ = controller.page.workspace.show_transient("", &preserved_draft);
                        controller.page.workspace.notice.set_label("No speech detected. Your live draft is still here.");
                    }
                    let title_entry = result.history_entry.as_ref().filter(|entry| {
                        result.mode == "dictation" && !result.incognito
                            && !controller.page.config().incognito_mode
                            && !text::trim(&entry.raw_text).is_empty()
                    }).cloned();
                    (controller.callbacks.phase_changed)(CapturePhase::Completed);
                    if finishing_live && let Some(live) = &live {
                        let entry = result.history_entry.as_ref().expect("final Live entry");
                        let source = final_source.as_deref().expect("final Live source");
                        controller.page.workspace.set_live("Finishing live draft…", source, false);
                        live.finish_capture(&entry.identifier, source);
                    }
                    (controller.callbacks.completed)(CaptureCompletion {
                        result,
                        session: session.clone(),
                        delivery_target: target,
                    });
                    // History reconciliation first refreshes the archive. Title updates
                    // then change its existing row without rebuilding raw recordings.
                    if let Some(entry) = title_entry {
                        (controller.callbacks.queue_title)(&entry);
                    }
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
                    if !text::trim(&failed_draft).is_empty() && !controller.page.config().incognito_mode {
                        let _ = controller.page.workspace.show_transient(&failed_draft, &failed_draft);
                        controller.page.workspace.notice.set_label("Capture failed. Your live draft is available to copy.");
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
        if session.realtime_healthy()
            && let Some(live) = self
                .live
                .borrow()
                .as_ref()
                .filter(|live| live.owns(&session.identifier))
            && let Some(snapshot) = session.preview()
        {
            live.offer(&snapshot.display_text(), false);
        }
    }
    fn bind_preview(self: &Rc<Self>, session: &Rc<CaptureSession>) {
        self.clear_preview();
        if self.live.borrow().is_none() || session.options.mode != "dictation" {
            return;
        }
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
        session.set_preview_callback(Box::new(move |preview| {
            let _ = sender.send(preview.display_text());
            Ok(())
        }));
        let identifier = session.identifier.clone();
        let weak = Rc::downgrade(self);
        let task = self.runtime.spawn(async move {
            let mut previous = String::new();
            while let Some(text) = receiver.recv().await {
                let Some(owner) = weak.upgrade().filter(|owner| owner.owns(&identifier)) else {
                    return;
                };
                let live = owner.live.borrow().clone();
                if !text.is_empty()
                    && text != previous
                    && let Some(live) = live.filter(|live| live.owns(&identifier))
                {
                    previous = text.clone();
                    if owner.phase() != Some(CapturePhase::Processing) && !live.finalizing() {
                        live.offer(&text, false);
                    }
                }
            }
        });
        self.preview_task.replace(Some(task));
    }
    fn clear_preview(&self) {
        if let Some(task) = self.preview_task.borrow_mut().take() {
            task.abort();
        }
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
    fn release(&self, identifier: &str, keep_live: bool) {
        if !self.owns(identifier) {
            return;
        }
        self.clear_clock();
        self.clear_preview();
        self.consume_live_once();
        self.active.borrow_mut().take();
        self.continuation.borrow_mut().take();
        self.continuation_source.borrow_mut().clear();
        if !keep_live {
            self.page.workspace.finish_live();
        }
        self.page.workspace.set_screenshot_context(None, None);
        self.page.set_view_state(CaptureViewState::default());
    }
}

impl Drop for CaptureController {
    fn drop(&mut self) {
        self.unbind_keys();
        self.clear_clock();
        self.clear_preview();
        self.consume_live_once();
        if let Some(live) = self.live.get_mut().take() {
            live.shutdown();
        }
        if let Some(active) = self.active.get_mut().take() {
            active.session.request_shutdown();
            self.runtime.spawn(async move {
                active.session.shutdown().await;
            });
        }
    }
}
