use super::*;
use crate::{
    capture_controller::{CaptureCompletion, CaptureFailure, CaptureLaunch},
    overlay_state::OverlayState,
};
use mluva_core::{config::AudioRetentionPolicy, screenshots::ImageInput};
use mluva_workflows::{
    capture::CapturePhase,
    dictation::WorkflowResult,
    meeting_session::MeetingPhase,
    services::{SettingsActivity, SettingsKind, SettingsUpdate},
};
use std::{collections::BTreeSet, time::Duration};

impl ApplicationDesktop {
    pub(super) fn activity(&self) -> PreferenceActivity {
        let capture = self.capture.phase();
        let meeting = self.meeting.phase();
        PreferenceActivity {
            preparing: capture == Some(CapturePhase::Preparing),
            recording: capture == Some(CapturePhase::Recording),
            processing: matches!(
                capture,
                Some(CapturePhase::Processing | CapturePhase::Cancelling)
            ),
            retrying: self.history.retry_identifier().is_some(),
            meeting_recording: matches!(
                meeting,
                Some(MeetingPhase::Recording | MeetingPhase::Preparing)
            ),
            meeting_processing: matches!(meeting, Some(MeetingPhase::Processing)),
            meeting_retrying: self.meeting.retry_identifier().is_some(),
            excluded_history: self.excluded_history(),
        }
    }
    pub(super) fn excluded_history(&self) -> BTreeSet<String> {
        let mut excluded = BTreeSet::new();
        excluded.extend(self.history.retry_identifier());
        if let Some(current) = self.current.borrow().as_ref() {
            excluded.extend(current.continuation.clone());
        }
        excluded
    }
    pub(super) fn settings_activity(&self) -> SettingsActivity {
        let activity = self.activity();
        SettingsActivity {
            preparing: activity.preparing,
            processing: activity.processing
                || activity.retrying
                || activity.meeting_processing
                || activity.meeting_retrying,
            recording: activity.recording || activity.meeting_recording,
        }
    }
    pub(super) fn idle(&self) {
        if self.closed.get() {
            return;
        }
        let activity = self.activity();
        let available = !self.meeting.busy() && self.capture.phase().is_none();
        self.settings.capture.set_activity(activity.clone());
        self.settings.capture.set_controls_available(available);
        self.meeting.page.record_button.set_sensitive(available);
        let mut state = self.page().view_state();
        state.meeting_busy = self.meeting.busy();
        state.initialization_failed = self.initialization_failed.get();
        self.page().set_view_state(state);
        // A provider repair enables capture; the released startup callout stays
        // until its explicit Retry action acknowledges initialization.
        self.page()
            .record_button
            .set_sensitive(available && self.ready.borrow().is_some());
        self.install_history_factory();
    }

    pub(super) fn initialize_services(self: &Rc<Self>, initialization: bool) {
        if self.closed.get() || self.capture.phase().is_some() {
            return;
        }
        let generation = self.readiness_generation.get() + 1;
        // Startup/Retry belongs to the latest readiness generation even when
        // settings supersede the first asynchronous credential lookup.
        if initialization {
            self.initialization_pending.set(true);
        }
        self.readiness_generation.set(generation);
        self.ready.borrow_mut().take();
        self.history.set_workflow_factory(None);
        self.page().record_button.set_sensitive(false);
        let shortcuts = if initialization {
            self.close_shortcuts()
        } else {
            None
        };
        let services = self.services.clone();
        let binaries = self.binaries.clone();
        let expected = services.config();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            if let Some(shortcuts) = shortcuts {
                shortcuts.await;
            }
            let result = services.capture_services(binaries.clone()).await;
            let Some(owner) = weak.upgrade().filter(|owner| {
                !owner.closed.get()
                    && owner.readiness_generation.get() == generation
                    && owner.services.config() == expected
            }) else {
                return;
            };
            let initializing = owner.initialization_pending.replace(false);
            let recovering = initializing && owner.initialization_failed.get();
            match result {
                Ok(ready) => {
                    owner.ready.replace(Some(Rc::new(ready)));
                    owner.install_history_factory();
                    if initializing {
                        owner.initialization_failed.set(false);
                        owner.page().setup_callout.set_reveal_child(false);
                    }
                    owner.idle();
                    if recovering || (initializing && !initialization) {
                        // A setting changed before readiness can leave a stale
                        // unavailable-service message behind the approved key.
                        owner.page().set_status("Ready");
                    }
                    if recovering {
                        owner.shell.show_message("Capture services are ready.");
                    }
                    if initializing {
                        owner.start_shortcuts();
                    }
                    if owner.startup_record_requested.replace(false) {
                        owner.toggle_recording(CaptureOrigin::Manual);
                    }
                }
                Err(error) => {
                    owner.startup_record_requested.set(false);
                    if initializing {
                        owner.initialization_failed.set(true);
                        owner.page().set_initialization_error(&error.to_string());
                        owner.meeting.page.record_button.set_sensitive(false);
                    } else {
                        owner.page().set_status(
                            "Speech provider needs credentials. Saved dictation remains available.",
                        );
                    }
                }
            }
            drop(owner);
            let meeting = services.meeting_services(&binaries).await.ok();
            if let Some(owner) = weak.upgrade().filter(|owner| {
                !owner.closed.get() && owner.readiness_generation.get() == generation
            }) {
                owner.meeting.set_services(meeting);
            }
        });
    }
    fn install_history_factory(&self) {
        if self.history.retry_identifier().is_some() {
            return;
        }
        let factory = self.ready.borrow().clone().map(|ready| {
            Rc::new(move || ready.workflow()) as crate::history_controller::HistoryWorkflowFactory
        });
        self.history.set_workflow_factory(factory);
    }
    fn synchronize_capture(&self) {
        let ready = self.ready.borrow().clone();
        if let Some(ready) = ready {
            let mut updated = (*ready).clone();
            updated.config = self.services.config();
            self.ready.replace(Some(Rc::new(updated)));
            self.install_history_factory();
        }
    }
    pub(super) fn settings_committed(self: &Rc<Self>, update: SettingsUpdate) {
        if self.closed.get() {
            return;
        }
        let config = update.config;
        self.account.privacy(config.incognito_mode);
        if let Err(error) = self.page().set_config(config.clone()) {
            self.shell.show_message(&error.to_string());
        }
        if update.changed.contains("time_format") {
            self.history
                .page
                .set_time_format(config.time_format.clone());
            let _ = self.history.page.refresh();
            self.meeting
                .page
                .set_time_format(config.time_format.clone());
            let _ = self.meeting.page.refresh();
        }
        match update.kind {
            SettingsKind::Providers => {
                self.meeting.sync_config(true);
                self.initialize_services(false);
            }
            SettingsKind::Presentation => {
                self.synchronize_capture();
                if self.ready.borrow().is_none() {
                    self.initialize_services(false);
                }
            }
            SettingsKind::Unchanged => {}
        }
    }
    pub(super) fn inline_changed(self: &Rc<Self>, effect: InlineEffect) {
        if self.closed.get() {
            return;
        }
        let config = self.services.config();
        if let Err(error) = self.page().set_config(config.clone()) {
            self.shell.show_message(&error.to_string());
        }
        self.synchronize_capture();
        match effect {
            InlineEffect::Privacy { persisted } => {
                self.agent_voice.interrupt();
                self.account.privacy(config.incognito_mode);
                if config.incognito_mode {
                    self.cancel_screenshot_picker();
                }
                self.workspace().set_private(config.incognito_mode);
                self.meeting.sync_config(persisted);
            }
            InlineEffect::Shortcut => self.rebind_shortcut(),
            InlineEffect::Routing => self.meeting.sync_config(true),
            _ => {}
        }
        if self.ready.borrow().is_none() {
            self.initialize_services(false);
        }
    }
    pub(super) fn routes_changed(&self) {
        if let Ok(catalog) = PipeWireDeviceCatalog::from_system(None) {
            self.devices.replace(catalog.clone());
            self.meeting.set_catalog(catalog);
        }
    }

    /// A cold public Record action also opens the application. Preserve that
    /// first toggle while native credential/readiness work yields to GTK.
    pub fn record_after_startup(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        if self.initialization_pending.get() {
            self.startup_record_requested.set(true);
        } else {
            self.toggle_recording(CaptureOrigin::Manual);
        }
    }
    pub(super) fn toggle_recording(self: &Rc<Self>, origin: CaptureOrigin) {
        if self.closed.get() {
            return;
        }
        if self.startup_record_requested.replace(false) {
            return;
        }
        if self.capture_audio_waiting.replace(false) {
            return;
        }
        if self.capture.phase().is_none() {
            if self.ready.borrow().is_none() {
                return;
            }
            let message = if self.meeting.busy() {
                Some("Finish the explicit Meeting capture or retry before starting dictation.")
            } else if self.history.retry_identifier().is_some() {
                Some("Finish the retained-audio retry before starting another recording.")
            } else {
                None
            };
            if let Some(message) = message {
                self.page().set_status(message);
                return;
            }
        }
        if self.capture.phase().is_none() {
            if let CaptureOrigin::AgentVoice(id) = &origin {
                if !self.agent_voice.recording(id) {
                    return;
                }
            } else {
                self.agent_voice.leave_for_capture();
            }
            if let Some(done) = self.agent_voice.stop_voice() {
                self.capture_audio_waiting.set(true);
                let weak = Rc::downgrade(self);
                self.runtime.spawn(async move {
                    done.cancelled().await;
                    if let Some(owner) = weak.upgrade().filter(|o| !o.closed.get())
                        && owner.capture_audio_waiting.replace(false)
                    {
                        owner.toggle_recording(origin);
                    }
                });
                return;
            }
        }
        if self.capture.phase() == Some(CapturePhase::Recording)
            && self
                .current
                .borrow()
                .as_ref()
                .is_some_and(|context| context.agent_attempt.is_some())
        {
            self.agent_voice.release();
        }
        self.capture.toggle(origin);
    }
    pub(super) fn agent_press(self: &Rc<Self>) {
        if self.closed.get()
            || self.capture.phase().is_some()
            || self.ready.borrow().is_none()
            || self.meeting.busy()
            || self.history.retry_identifier().is_some()
        {
            return;
        }
        match self.agent_voice.begin() {
            Ok(id) => {
                self.toggle_recording(CaptureOrigin::AgentVoice(id));
                if self.capture.phase().is_none() && !self.capture_audio_waiting.get() {
                    self.agent_voice
                        .abandoned(mluva_core::agent_voice::DraftReason::Interrupted);
                }
            }
            Err(message) => self.page().set_status(message),
        }
    }
    pub(super) fn agent_release(self: &Rc<Self>) {
        if !self.agent_voice.release() {
            return;
        }
        self.capture_audio_waiting.set(false);
        match self.capture.phase() {
            Some(CapturePhase::Recording) => self.capture.stop(),
            Some(CapturePhase::Preparing) => {
                self.capture.cancel();
            }
            _ => self
                .agent_voice
                .abandoned(mluva_core::agent_voice::DraftReason::Interrupted),
        }
    }
    pub(super) fn agent_interrupted(self: &Rc<Self>) {
        self.capture_audio_waiting.set(false);
        if self
            .current
            .borrow()
            .as_ref()
            .is_some_and(|context| context.agent_attempt.is_some())
        {
            self.agent_release();
        } else if self.capture.phase().is_none() {
            self.agent_voice
                .abandoned(mluva_core::agent_voice::DraftReason::Interrupted);
        }
    }
    pub(super) fn continue_recording(self: &Rc<Self>, identifier: &str) {
        if self.closed.get()
            || self.services.config().incognito_mode
            || self.capture.phase().is_some()
        {
            return;
        }
        self.toggle_recording(CaptureOrigin::Continuation(identifier.into()));
    }
    pub(super) fn prepare_capture(
        self: &Rc<Self>,
        origin: CaptureOrigin,
    ) -> WorkflowOutcome<CaptureLaunch> {
        if self.closed.get() {
            return Err(WorkflowError::Invalid("The application is closed.".into()));
        }
        let phone = match &origin {
            CaptureOrigin::Phone(id) => Some(id.clone()),
            _ => None,
        };
        let automatic = matches!(origin, CaptureOrigin::ApprovedShortcut);
        let agent_attempt = match &origin {
            CaptureOrigin::AgentVoice(id) => Some(id.clone()),
            _ => None,
        };
        let agent_voice = agent_attempt.is_some();
        let continuation = match origin {
            CaptureOrigin::Continuation(id) => Some(id),
            _ => None,
        };
        let target = if automatic {
            self.tracker
                .borrow()
                .as_ref()
                .and_then(FocusedTextTargetTracker::capture_delivery_target)
        } else {
            None
        };
        let mut application = target
            .as_ref()
            .and_then(|target| target.application_identifier().map(str::to_owned));
        if automatic && application.is_none() {
            application = self
                .tracker
                .borrow()
                .as_ref()
                .and_then(FocusedTextTargetTracker::capture_application_identifier);
        }
        let prefs = &self.settings.capture;
        let incognito = prefs.incognito.is_active();
        let policy = match prefs.audio_retention.selected() {
            0 => AudioRetentionPolicy::Never,
            2 => AudioRetentionPolicy::Always,
            _ => AudioRetentionPolicy::Failures,
        };
        let options = CaptureOptions {
            allow_auto_paste: automatic,
            incognito,
            audio_retention: policy,
            application_identifier: phone
                .as_ref()
                .map(|id| format!("mluva-web:{id}"))
                .or(application),
            defer_delivery: continuation.is_some() || agent_voice,
            preview_enabled: true,
        };
        let ready = self
            .ready
            .borrow()
            .clone()
            .ok_or_else(|| WorkflowError::Invalid("Capture services are not ready.".into()))?;
        let session = if phone.is_some() {
            ready.launch_phone(options.clone())?
        } else {
            ready.launch(options.clone())?
        };
        self.current.replace(Some(CaptureContext {
            options,
            target: target.clone(),
            continuation,
            agent_attempt,
            session: session.clone(),
        }));
        Ok(CaptureLaunch {
            session,
            delivery_target: target.map(|target| {
                Rc::new(target) as Rc<dyn mluva_workflows::dictation::DeliveryTarget>
            }),
        })
    }
    pub(super) fn capture_images(
        &self,
        capture: Option<&str>,
        conversation: Option<&str>,
    ) -> WorkflowOutcome<Vec<ImageInput>> {
        if self.services.config().incognito_mode
            || self
                .current
                .borrow()
                .as_ref()
                .is_some_and(|current| current.options.incognito)
        {
            return Ok(vec![]);
        }
        let mut images = if let Some(id) = conversation {
            self.services.screenshots.snapshot(id, false)?
        } else {
            vec![]
        };
        if let Some(id) = capture {
            images.extend(self.services.screenshots.snapshot(id, true)?);
        }
        Ok(images)
    }
    pub(super) fn capture_phase(self: &Rc<Self>, phase: CapturePhase) {
        if self.closed.get() {
            return;
        }
        match phase {
            CapturePhase::Preparing
            | CapturePhase::Recording
            | CapturePhase::Processing
            | CapturePhase::Cancelling => {
                let activity = self.activity();
                self.settings.capture.set_activity(activity);
                self.settings.capture.set_controls_available(false);
                self.meeting.page.record_button.set_sensitive(false);
                if self.initialization_failed.get() {
                    let mut state = self.page().view_state();
                    state.initialization_failed = true;
                    self.page().set_view_state(state);
                    self.page().record_button.set_sensitive(matches!(
                        phase,
                        CapturePhase::Preparing | CapturePhase::Recording
                    ));
                }
                if phase == CapturePhase::Processing {
                    self.publish_terminal("processing", "Finishing your dictation…");
                } else {
                    self.publish_capture();
                }
            }
            _ => self.idle(),
        }
    }
    pub(super) fn capture_history_changed(&self, result: &mut WorkflowResult) {
        if self.closed.get()
            || (result.history_entry.is_none()
                && mluva_core::text::trim(&result.transcription.text).is_empty())
        {
            return;
        }
        let excluded = self.excluded_history();
        if let Err(error) = self.services.prune_history(&excluded) {
            result
                .delivery
                .guidance
                .push_str(&format!(" History retention failed: {error}"));
        }
        let _ = self.history.page.refresh();
        self.history_changed();
    }
    pub(super) fn capture_completed(self: &Rc<Self>, completion: CaptureCompletion) {
        if self.closed.get() {
            return;
        }
        let context = self.current.borrow_mut().take();
        self.clear_overlay();
        let result = completion.result;
        self.phone_completed(&completion.session.identifier, &result);
        if !result.incognito {
            self.account.live(
                &completion.session.identifier,
                "",
                Some(&result.transcription.text),
            );
            self.account.sync();
        }
        let id = result
            .history_entry
            .as_ref()
            .map(|entry| entry.identifier.clone());
        self.finish_result_screenshots(
            &completion.session.identifier,
            id.as_deref(),
            result.incognito,
        );
        if let Some(context) = context.as_ref()
            && let Some(entry) = result.history_entry.as_ref()
            && let Some(target) = context.target.as_ref()
        {
            self.history.remember_target(&entry.identifier, target);
        }
        if let Some(attempt) = context.as_ref().and_then(|c| c.agent_attempt.as_deref()) {
            self.idle();
            let shown = if let Some(entry) = result.history_entry.clone() {
                self.workspace().show_conversation(Some(entry), &[], false)
            } else {
                self.workspace()
                    .show_transient(&result.transcription.text, &result.transcription.text)
            };
            if shown.is_err() {
                self.agent_voice
                    .disarm(mluva_core::agent_voice::DraftReason::Interrupted);
            }
            self.agent_voice
                .completed(attempt, &result.transcription.text, id);
            return;
        }
        self.idle();
        {
            let message = if result.delivery.pasted {
                "Inserted. Text also stays on the clipboard."
            } else if result.delivery.paste_dispatched {
                "Paste unconfirmed. Check the target before pasting again."
            } else if result.delivery.copied {
                "Copied. Ready to paste."
            } else {
                "Dictation ready."
            };
            self.publish_terminal("copied", message);
        }
    }
    pub(super) fn capture_failed(self: &Rc<Self>, failure: CaptureFailure) {
        if self.closed.get() {
            return;
        }
        if self
            .current
            .borrow()
            .as_ref()
            .is_some_and(|c| c.agent_attempt.is_some())
        {
            self.agent_voice
                .abandoned(mluva_core::agent_voice::DraftReason::Interrupted);
        }
        self.account.cancel_live(&failure.session_identifier);
        if failure.phase == CapturePhase::Preparing {
            self.preserve_interrupted_screenshots(&failure.session_identifier);
        }
        let context = self.current.borrow_mut().take();
        let entry = match &failure.error {
            WorkflowError::Failure(failure) => failure.history_entry.as_ref(),
            _ => None,
        };
        if failure.phase != CapturePhase::Preparing {
            self.finish_result_screenshots(
                &failure.session_identifier,
                entry.map(|entry| entry.identifier.as_str()),
                context
                    .as_ref()
                    .is_some_and(|context| context.options.incognito),
            );
        }
        if let Some(entry) = entry
            && let Some(target) = context.as_ref().and_then(|context| context.target.as_ref())
        {
            self.history.remember_target(&entry.identifier, target);
        }
        let _ = self.history.page.refresh();
        self.history_changed();
        self.idle();
        self.publish_terminal("error", "Dictation needs attention. Open Mluva.");
    }
    pub(super) fn capture_cancelled(&self, id: &str) {
        if self.closed.get() {
            return;
        }
        if self
            .current
            .borrow()
            .as_ref()
            .is_some_and(|c| c.agent_attempt.is_some())
        {
            self.agent_voice
                .abandoned(mluva_core::agent_voice::DraftReason::Cancelled);
        }
        self.current.borrow_mut().take();
        self.discard_screenshots(id);
        self.account.cancel_live(id);
        self.idle();
        self.clear_overlay();
    }

    pub(super) fn cancel(self: &Rc<Self>) -> bool {
        if self.closed.get() {
            return false;
        }
        if self.startup_record_requested.replace(false) {
            return true;
        }
        let agent_disarmed = self
            .agent_voice
            .disarm(mluva_core::agent_voice::DraftReason::Cancelled);
        self.capture_audio_waiting.set(false);
        if self.capture.cancel() {
            return true;
        }
        if agent_disarmed {
            return true;
        }
        false
    }
    pub(super) fn publish(&self, state: OverlayState) {
        if self.closed.get() {
            return;
        }
        self.clear_overlay_timer();
        let sent = self
            .overlay
            .borrow_mut()
            .as_mut()
            .map(|publisher| publisher.publish(&state));
        if sent == Some(false) {
            self.overlay.borrow_mut().take();
        }
    }
    fn publish_capture(self: &Rc<Self>) {
        let capture = self.current.borrow();
        let Some(current) = capture.as_ref() else {
            return;
        };
        let phase = current.session.phase();
        // Cancellation drains audio/providers asynchronously. The released app
        // keeps its recording feedback until that cleanup clears the overlay.
        if phase == CapturePhase::Cancelling {
            return;
        }
        let config = self.services.config();
        let mut state = OverlayState {
            phase: match phase {
                CapturePhase::Preparing => "preparing",
                CapturePhase::Recording => "recording",
                _ => "processing",
            }
            .into(),
            detail: if phase == CapturePhase::Preparing {
                "Preparing recognition readiness…"
            } else if phase == CapturePhase::Recording {
                "Recording"
            } else {
                "Finishing your dictation…"
            }
            .into(),
            mode: "Dictate".into(),
            level: current.session.audio_level(),
            elapsed_seconds: current.session.elapsed_seconds().unwrap_or_default() as i64,
            preview: self.workspace().live_text.text(),
            delivery: if current.options.allow_auto_paste
                && self.tracker.borrow().is_some()
                && (current.target.as_ref().is_some_and(|target| {
                    matches!(target, DeliveryTargetSnapshot::Text(text) if text.editable_text_available())
                        || mluva_core::delivery::keyboard_paste_available(None, target.application_identifier())
                }))
            {
                "Paste armed"
            } else {
                "Copy only"
            }
            .into(),

            reduce_motion: gtk::Settings::default().is_some_and(|settings| !settings.is_gtk_enable_animations()),
            widget_position: config.widget_position,
            widget_lines: config.widget_lines,
            widget_opacity: config.widget_opacity,
            ..Default::default()
        };
        if phase == CapturePhase::Recording {
            let provider = if !current.session.realtime_healthy() {
                format!(
                    "{} · final transcription at Stop",
                    config.transcription_provider
                )
            } else if config.transcription_provider == "elevenlabs" {
                "Scribe Realtime".into()
            } else {
                format!("{} · chunk preview", config.transcription_provider)
            };
            let microphone = if current
                .session
                .options
                .application_identifier
                .as_deref()
                .is_some_and(|id| id.starts_with("mluva-web:"))
            {
                "Phone microphone".into()
            } else {
                self.devices.borrow().display_name(
                    mluva_audio::catalog::PipeWireDeviceKind::Microphone,
                    config.microphone_target.as_deref(),
                )
            };
            state.route = format!("{provider} · {microphone}");
        }
        if !current.options.incognito && phase == CapturePhase::Recording {
            self.account
                .live(&current.session.identifier, &state.preview, None);
        }
        drop(capture);
        self.publish(state);
        if phase == CapturePhase::Recording {
            let weak = Rc::downgrade(self);
            self.overlay_timer
                .replace(Some(glib::timeout_add_local_once(
                    Duration::from_millis(250),
                    move || {
                        if let Some(owner) = weak.upgrade() {
                            owner.overlay_timer.borrow_mut().take();
                            owner.publish_capture();
                        }
                    },
                )));
        }
    }
    fn clear_overlay_timer(&self) {
        if let Some(timer) = self.overlay_timer.borrow_mut().take() {
            timer.remove();
        }
    }
    pub(super) fn clear_overlay(&self) {
        self.clear_overlay_timer();
        if let Some(publisher) = self.overlay.borrow_mut().as_mut() {
            publisher.clear();
        }
    }
    fn publish_terminal(self: &Rc<Self>, phase: &str, detail: &str) {
        let config = self.services.config();
        self.publish(OverlayState {
            phase: phase.into(),
            detail: detail.into(),
            preview: if phase == "processing" {
                self.workspace().live_text.text()
            } else {
                String::new()
            },
            reduce_motion: gtk::Settings::default()
                .is_some_and(|settings| !settings.is_gtk_enable_animations()),
            widget_position: config.widget_position,
            widget_lines: config.widget_lines,
            widget_opacity: config.widget_opacity,
            ..Default::default()
        });
        if matches!(phase, "copied" | "error") && self.overlay.borrow().is_some() {
            let weak = Rc::downgrade(self);
            self.overlay_timer
                .replace(Some(glib::timeout_add_seconds_local_once(
                    if phase == "copied" { 5 } else { 10 },
                    move || {
                        if let Some(owner) = weak.upgrade() {
                            owner.overlay_timer.borrow_mut().take();
                            owner.clear_overlay();
                        }
                    },
                )));
        }
    }
    pub fn shutdown(self: &Rc<Self>) -> glib::JoinHandle<()> {
        let complete = self.shutdown_complete.clone();
        if self.closed.replace(true) {
            return glib::MainContext::default().spawn_local(async move {
                complete.cancelled().await;
            });
        }
        let hold = self.application.hold();
        self.account.shutdown();
        self.agent_voice.close();
        self.capture_audio_waiting.set(false);
        self.close_phone_bridge();
        self.close_screenshot_monitoring();
        let capture_identifier = self.capture.session_identifier();
        if let Some(identifier) = capture_identifier {
            self.preserve_interrupted_screenshots(&identifier);
        }
        self.initialization_pending.set(false);
        self.startup_record_requested.set(false);
        self.readiness_generation
            .set(self.readiness_generation.get() + 1);
        self.clear_overlay();
        let shortcuts = self.close_shortcuts();
        (self.platform.close)();
        if let Some(mut tracker) = self.tracker.borrow_mut().take() {
            tracker.close();
        }
        self.current.borrow_mut().take();
        let capture = self.capture.shutdown();
        let history = self.history.shutdown();
        let meeting = self.meeting.shutdown();
        self.shell.window.destroy();
        let closed = self.runtime.shutdown(async move {
            if let Some(shortcuts) = shortcuts {
                shortcuts.await;
            }
            let _ = capture.await;
            let _ = history.await;
            let _ = meeting.await;
        });
        glib::MainContext::default().spawn_local(async move {
            let _ = closed.await;
            drop(hold);
            complete.cancel();
        })
    }
}
