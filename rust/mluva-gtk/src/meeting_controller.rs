//! Application-owned explicit Meeting capture/retry, independently of dictation.

use crate::{
    async_runtime::DesktopRuntime,
    capture_preferences::{CapturePreferences, PreferenceActivity},
    capture_view::CapturePage,
    meeting_view::{MeetingCallbacks, MeetingPage},
    prompt_editor::Message,
};
use gtk::prelude::*;
use mluva_audio::catalog::{PipeWireDeviceCatalog, PipeWireDeviceKind};
use mluva_core::{
    database::StoreResult,
    diagnostics::{DiagnosticOutcome, DiagnosticProvider, DiagnosticStage},
    meeting::MeetingRecord,
};
use mluva_workflows::{
    meeting::{MeetingCompletion, MeetingError, MeetingWorkflowResult},
    meeting_services::{MeetingRetry, MeetingServices},
    meeting_session::{MeetingPhase, MeetingSession, MeetingSessionError},
    services::ApplicationServices,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

pub struct MeetingControllerCallbacks {
    /// The parent supplies dictation/retry/review state from their actual owners.
    pub activity: Rc<dyn Fn() -> PreferenceActivity>,
    /// Restore the parent's remaining controls, overlays and shortcuts at idle.
    pub idle: Rc<dyn Fn()>,
    pub copy: Message,
}

pub struct MeetingController {
    pub page: Rc<MeetingPage>,
    services: Rc<ApplicationServices>,
    runtime: Rc<DesktopRuntime>,
    capture: Rc<CapturePage>,
    preferences: Rc<CapturePreferences>,
    ready: RefCell<Option<Arc<MeetingServices>>>,
    active: RefCell<Option<Arc<MeetingSession>>>,
    retry: RefCell<Option<Arc<MeetingRetry>>>,
    catalog: RefCell<PipeWireDeviceCatalog>,
    timer: RefCell<Option<glib::SourceId>>,
    closed: Cell<bool>,
    processing: Cell<bool>,
    cancelling: Cell<bool>,
    callbacks: MeetingControllerCallbacks,
}
impl MeetingController {
    pub fn new(
        services: Rc<ApplicationServices>,
        runtime: Rc<DesktopRuntime>,
        capture: Rc<CapturePage>,
        preferences: Rc<CapturePreferences>,
        catalog: PipeWireDeviceCatalog,
        callbacks: MeetingControllerCallbacks,
    ) -> StoreResult<Rc<Self>> {
        let current = Rc::new(RefCell::new(Weak::<Self>::new()));
        let weak = current.clone();
        let toggle_capture = Rc::new(move || {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.toggle();
            }
        });
        let weak = current.clone();
        let retry_recognition = Rc::new(move |record: &MeetingRecord| {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.retry_record(record);
            }
        });
        let weak = current.clone();
        let delete_meeting = Rc::new(move |record: &MeetingRecord| {
            weak.borrow()
                .upgrade()
                .is_some_and(|owner| owner.delete(record))
        });
        let config = services.config();
        let weak = current.clone();
        let show_message = Rc::new(move |message: &str| {
            if let Some(owner) = weak.borrow().upgrade() {
                owner.status(message);
            }
        });
        let page = MeetingPage::new(
            services.meetings.clone(),
            services.paths.data.join("exports/meetings"),
            MeetingCallbacks {
                toggle_capture,
                copy_text: callbacks.copy.clone(),
                retry_recognition,
                delete_meeting,
                show_message,
            },
            config.time_format,
        )?;
        let owner = Rc::new(Self {
            page,
            services,
            runtime,
            capture,
            preferences,
            catalog: RefCell::new(catalog),
            ready: RefCell::new(None),
            active: RefCell::new(None),
            retry: RefCell::new(None),
            timer: RefCell::new(None),
            closed: Cell::new(false),
            processing: Cell::new(false),
            cancelling: Cell::new(false),
            callbacks,
        });
        *current.borrow_mut() = Rc::downgrade(&owner);
        owner.sync_config(true);
        Ok(owner)
    }
    pub fn set_services(&self, services: Option<Arc<MeetingServices>>) -> bool {
        if self.closed.get() || self.busy() {
            return false;
        }
        self.ready.replace(services);
        true
    }
    pub fn phase(&self) -> Option<MeetingPhase> {
        if self.processing.get() {
            return Some(MeetingPhase::Processing);
        }
        self.active.borrow().as_ref().map(|session| session.phase())
    }
    pub fn retry_identifier(&self) -> Option<String> {
        self.retry
            .borrow()
            .as_ref()
            .map(|retry| retry.identifier.clone())
    }
    pub fn busy(&self) -> bool {
        self.active.borrow().is_some() || self.retry.borrow().is_some()
    }
    pub fn set_catalog(&self, catalog: PipeWireDeviceCatalog) {
        self.catalog.replace(catalog);
        self.sync_config(true);
    }
    /// Failed inline privacy writes apply to the session, but retain the
    /// released Meeting summary until a persisted edit or completed capture.
    pub fn sync_config(&self, persisted: bool) {
        let config = self.services.config();
        let catalog = self.catalog.borrow();
        self.page.set_audio_routes(
            &catalog.display_name(
                PipeWireDeviceKind::Microphone,
                config.microphone_target.as_deref(),
            ),
            &catalog.display_name(
                PipeWireDeviceKind::SystemOutput,
                config.system_audio_target.as_deref(),
            ),
        );
        if persisted && self.active.borrow().is_none() {
            self.page.set_privacy(config.incognito_mode);
        }
        self.page.set_time_format(config.time_format);
    }
    fn status(&self, message: &str) {
        self.page.set_status(message);
    }
    fn activity(&self) -> PreferenceActivity {
        let mut state = (self.callbacks.activity)();
        state.meeting_recording = matches!(
            self.phase(),
            Some(MeetingPhase::Preparing | MeetingPhase::Recording)
        );
        state.meeting_processing = self.phase() == Some(MeetingPhase::Processing);
        state.meeting_retrying = self.retry.borrow().is_some();
        state
    }
    fn block_capture(&self, retry: bool) {
        let mut state = self.capture.view_state();
        state.meeting_busy = true;
        self.capture.set_view_state(state);
        for row in [
            &self.preferences.recording_key,
            &self.preferences.language,
            &self.preferences.microphone,
            &self.preferences.system_audio,
        ] {
            row.set_sensitive(false);
        }
        if !retry {
            self.preferences.incognito.set_sensitive(false);
        }
        self.preferences.refresh_audio.set_sensitive(false);
        self.preferences.set_activity(self.activity());
    }
    fn clear_clock(&self) {
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
    }
    fn reset(&self) {
        self.clear_clock();
        self.processing.set(false);
        self.cancelling.set(false);
        self.active.borrow_mut().take();
        self.retry.borrow_mut().take();
        self.page.set_capture_state(false, false);
        self.page.set_privacy(self.services.config().incognito_mode);
        let activity = self.activity();
        let available = !activity.pending_review
            && !activity.preparing
            && !activity.recording
            && !activity.processing
            && !activity.retrying;
        self.preferences.set_activity(activity);
        self.preferences.set_controls_available(available);
        let mut state = self.capture.view_state();
        state.meeting_busy = false;
        self.capture.set_view_state(state);
        self.page.record_button.set_sensitive(available);
        (self.callbacks.idle)();
    }
    fn owns(&self, session: &Arc<MeetingSession>) -> bool {
        !self.closed.get()
            && self
                .active
                .borrow()
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, session))
    }
    fn owns_retry(&self, retry: &Arc<MeetingRetry>) -> bool {
        !self.closed.get()
            && self
                .retry
                .borrow()
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, retry))
    }
    pub fn toggle(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        if self.ready.borrow().is_none() {
            self.status(
                "Meeting capture is unavailable until PipeWire and ElevenLabs are configured.",
            );
        } else if self.phase() == Some(MeetingPhase::Processing) {
            self.status("The stopped Meeting is still being transcribed.");
        } else if self.retry.borrow().is_some() {
            self.status("Finish the retained Meeting retry before starting another capture.");
        } else if self.phase() == Some(MeetingPhase::Recording) {
            self.stop();
        } else if self.active.borrow().is_none() {
            self.start();
        }
    }
    fn start(self: &Rc<Self>) {
        let config = self.services.config();
        if config.transcription_provider != "elevenlabs" {
            self.status("Meeting diarization requires ElevenLabs. Select it in Providers to enable uploads.");
            return;
        }
        let activity = self.activity();
        if activity.preparing || activity.processing || activity.recording {
            self.status("Finish the active dictation capture before starting Meeting.");
            return;
        }
        if activity.retrying {
            self.status("Finish the retained dictation retry before starting Meeting.");
            return;
        }
        if activity.pending_review {
            self.status("Resolve the active Command or Scratchpad review before starting Meeting.");
            return;
        }
        if !config.incognito_mode
            && self
                .services
                .meetings
                .lock()
                .unwrap()
                .persistence_error
                .is_some()
        {
            self.status("The private Meeting archive is malformed. Repair it before a retained Meeting, or enable Incognito for a non-persistent session.");
            return;
        }
        let completion = MeetingCompletion {
            incognito: config.incognito_mode,
            ..Default::default()
        };
        let session = match self
            .ready
            .borrow()
            .as_ref()
            .unwrap()
            .launch(config, completion)
        {
            Ok(session) => session,
            Err(error) => {
                self.status(&format!("Meeting capture could not start: {error}"));
                return;
            }
        };
        self.active.replace(Some(session.clone()));
        self.block_capture(false);
        self.page.record_button.set_sensitive(false);
        let owner = session.clone();
        let task = self
            .runtime
            .spawn_background(async move { owner.start().await });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = task.await;
            let Some(controller) = weak
                .upgrade()
                .filter(|owner| owner.owns(&session) && !owner.cancelling.get())
            else {
                return;
            };
            match result {
                Ok(Ok(_)) => {
                    controller.page.set_capture_state(true, false);
                    controller.preferences.set_activity(controller.activity());
                    controller.update_clock();
                    let weak = Rc::downgrade(&controller);
                    controller.timer.replace(Some(glib::timeout_add_local(
                        Duration::from_millis(250),
                        move || {
                            if let Some(owner) = weak.upgrade().filter(|owner| {
                                !owner.closed.get()
                                    && owner.phase() == Some(MeetingPhase::Recording)
                            }) {
                                owner.update_clock();
                                glib::ControlFlow::Continue
                            } else {
                                glib::ControlFlow::Break
                            }
                        },
                    )));
                }
                Ok(Err(error)) => {
                    controller.reset();
                    controller.status(&format!("Meeting capture could not start: {error}"));
                }
                Err(_) => {
                    controller.reset();
                    controller.status(
                        "Meeting capture could not start: The Meeting recorder owner is closed.",
                    );
                }
            }
        });
    }
    fn update_clock(&self) {
        let active = self.active.borrow();
        let Some(session) = active.as_ref() else {
            return;
        };
        let elapsed = session.elapsed_seconds().unwrap_or(0.0) as u64;
        let config = self.services.config();
        let catalog = self.catalog.borrow();
        let microphone = catalog.display_name(
            PipeWireDeviceKind::Microphone,
            config.microphone_target.as_deref(),
        );
        let system = catalog.display_name(
            PipeWireDeviceKind::SystemOutput,
            config.system_audio_target.as_deref(),
        );
        let retention = if session.incognito {
            "Incognito · erase after transcription"
        } else {
            "private archive"
        };
        self.status(&format!(
            "Meeting {:02}:{:02} · {microphone} + {system} · {retention}",
            elapsed / 60,
            elapsed % 60
        ));
    }
    fn diagnostic(
        &self,
        session: &MeetingSession,
        stage: DiagnosticStage,
        provider: DiagnosticProvider,
        outcome: DiagnosticOutcome,
        seconds: f64,
    ) {
        if !session.incognito {
            let _ = self.services.diagnostics.record(
                &session.identifier,
                "meeting",
                stage,
                provider,
                outcome,
                seconds,
            );
        }
    }
    fn stop(self: &Rc<Self>) {
        let Some(session) = self.active.borrow().clone() else {
            return;
        };
        if self.processing.replace(true) {
            return;
        }
        self.clear_clock();
        self.page.set_capture_state(false, true);
        self.status("Finalizing microphone and system audio, then transcribing with Scribe v2…");
        self.preferences.set_activity(self.activity());
        let owner = session.clone();
        let task = self
            .runtime
            .spawn_background(async move { owner.finish().await });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = task.await;
            let Some(controller) = weak.upgrade().filter(|owner| owner.owns(&session)) else { return; };
            controller.reset();
            let _ = controller.page.refresh();
            match result {
                Ok(Ok(result)) => controller.completed(result),
                Ok(Err(MeetingSessionError::Workflow(error))) => controller.failed(error, false),
                Ok(Err(error)) => controller.status(&format!("Meeting could not complete: {error} No Meeting audio was retained.")),
                Err(_) => controller.status("Meeting could not complete: The Meeting recorder owner is closed. No Meeting audio was retained."),
            }
        });
    }
    fn completed(&self, result: MeetingWorkflowResult) {
        let status = if result.incognito {
            "Meeting transcribed in memory. Incognito audio and metadata were erased."
        } else {
            "Meeting transcribed and saved to the private Meeting archive."
        };
        let warnings = result.meeting.warnings.join(" ");
        self.status(&if warnings.is_empty() {
            status.into()
        } else {
            format!("{status} {warnings}")
        });
    }
    fn failed(&self, error: MeetingError, retry: bool) {
        let (message, meeting, retained) = match error {
            MeetingError::Failed(failure) => (
                failure.message.to_owned(),
                failure.meeting,
                failure.retained_audio_path,
            ),
            MeetingError::Invalid(error) => (error.to_string(), None, None),
        };
        let transcript = meeting
            .as_ref()
            .map(|meeting| meeting.transcript.as_str())
            .filter(|text| !text.is_empty());
        if let Some(transcript) = transcript {
            self.capture.output_view.buffer().set_text(transcript);
        }
        let status = if retry {
            format!(
                "Meeting retry failed: {message} The private recording remains available.{}",
                if transcript.is_some() {
                    " The completed transcript is available in the Capture editor for manual review."
                } else {
                    ""
                }
            )
        } else {
            let recovery = retained.map_or_else(
                || "No Meeting audio was retained.".into(),
                |path| {
                    format!(
                        "The private recording remains available at {}.",
                        path.display()
                    )
                },
            );
            format!(
                "Meeting could not complete: {message} {}{recovery}",
                if transcript.is_some() {
                    "The completed transcript is available in the Capture editor for manual review. "
                } else {
                    ""
                }
            )
        };
        self.status(&status);
    }
    pub fn retry_record(self: &Rc<Self>, record: &MeetingRecord) {
        if self.closed.get() {
            return;
        }
        let config = self.services.config();
        if config.transcription_provider != "elevenlabs" {
            self.status("Meeting retry uploads audio to ElevenLabs. Select it in Providers first.");
            return;
        }
        if self.ready.borrow().is_none() {
            self.status("Meeting retry is unavailable until capture services are configured.");
            return;
        }
        if self.active.borrow().is_some() {
            self.status("Finish the active Meeting capture before retrying retained audio.");
            return;
        }
        let activity = self.activity();
        if activity.preparing || activity.processing || activity.recording {
            self.status("Finish the active dictation capture before retrying this Meeting.");
            return;
        }
        if activity.pending_review {
            self.status(
                "Resolve the active Command or Scratchpad review before retrying this Meeting.",
            );
            return;
        }
        if activity.retrying || self.retry.borrow().is_some() {
            self.status("A transcription retry is already in progress.");
            return;
        }
        let retry = match self
            .ready
            .borrow()
            .as_ref()
            .unwrap()
            .retry(config, record.identifier.clone())
        {
            Ok(retry) => retry,
            Err(error) => {
                self.failed(error, true);
                return;
            }
        };
        self.retry.replace(Some(retry.clone()));
        self.page.record_button.set_sensitive(false);
        self.block_capture(true);
        self.status("Retrying retained Meeting audio with ElevenLabs Scribe v2…");
        let worker = retry.clone();
        let started = Instant::now();
        let task = self
            .runtime
            .spawn_background(async move { worker.run().await });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = task.await;
            let Some(controller) = weak.upgrade().filter(|owner| owner.owns_retry(&retry)) else {
                return;
            };
            if !controller.services.config().incognito_mode {
                let _ = controller.services.diagnostics.record(
                    &retry.identifier,
                    "meeting",
                    DiagnosticStage::Recognition,
                    DiagnosticProvider::ElevenlabsScribeV2,
                    if matches!(result, Ok(Some(Ok(_)))) {
                        DiagnosticOutcome::Completed
                    } else {
                        DiagnosticOutcome::Failed
                    },
                    started.elapsed().as_secs_f64(),
                );
            }
            controller.reset();
            let _ = controller.page.refresh();
            match result {
                Ok(Some(Ok(_))) => controller.status(
                    "Meeting transcription recovered. Review or copy it explicitly from Meeting.",
                ),
                Ok(Some(Err(error))) => controller.failed(error, true),
                _ => {}
            }
            controller.runtime.spawn_background(async move {
                retry.close().await;
            });
        });
    }
    pub fn delete(&self, record: &MeetingRecord) -> bool {
        if self.closed.get() {
            return false;
        }
        if self.retry_identifier().as_deref() == Some(&record.identifier) {
            self.status("Wait for the active Meeting retry before deleting this record.");
            return false;
        }
        let result = self
            .services
            .meetings
            .lock()
            .unwrap()
            .delete(&record.identifier);
        if let Err(error) = result {
            self.status(&format!("Meeting deletion failed: {error}"));
            return false;
        }
        true
    }
    /// Escape cancels only a still-recording Meeting, matching the released rule.
    pub fn cancel(self: &Rc<Self>) -> bool {
        if self.closed.get()
            || self.cancelling.get()
            || !matches!(
                self.phase(),
                Some(MeetingPhase::Preparing | MeetingPhase::Recording)
            )
        {
            return false;
        }
        self.clear_clock();
        self.cancelling.set(true);
        let session = self.active.borrow().clone().unwrap();
        session.cancel();
        self.diagnostic(
            &session,
            DiagnosticStage::Capture,
            DiagnosticProvider::Pipewire,
            DiagnosticOutcome::Cancelled,
            session.elapsed_seconds().unwrap_or(0.0),
        );
        let worker = session.clone();
        let task = self
            .runtime
            .spawn_background(async move { worker.close().await });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let _ = task.await;
            if let Some(controller) = weak.upgrade().filter(|owner| owner.owns(&session)) {
                controller.reset();
                controller.status("Meeting cancelled. Microphone and system-audio files were erased without upload.");
            }
        });
        true
    }
    /// The root waits for this task before quitting the resident application.
    pub fn shutdown(&self) -> tokio::task::JoinHandle<()> {
        self.closed.set(true);
        self.clear_clock();
        let session = self.active.borrow_mut().take();
        let retry = self.retry.borrow_mut().take();
        if let Some(session) = &session {
            session.cancel();
        }
        self.runtime.spawn_background(async move {
            if let Some(session) = session {
                let _ = session.close().await;
            }
            if let Some(retry) = retry {
                retry.close().await;
            }
        })
    }
}
impl Drop for MeetingController {
    fn drop(&mut self) {
        drop(self.shutdown());
    }
}
