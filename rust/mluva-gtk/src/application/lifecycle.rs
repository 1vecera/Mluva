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
    services::{InlineSavePolicy, SettingsActivity, SettingsKind, SettingsUpdate},
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
            pending_review: self.pending.busy(),
            excluded_history: self.excluded_history(),
        }
    }
    pub(super) fn excluded_history(&self) -> BTreeSet<String> {
        let mut excluded = BTreeSet::new();
        excluded.extend(self.history.retry_identifier());
        excluded.extend(self.pending.command_identifier());
        excluded.extend(self.live.final_entry());
        if let Some(current) = self.current.borrow().as_ref() {
            excluded.extend(current.continuation.clone());
        }
        excluded
    }
    pub(super) fn settings_activity(&self) -> SettingsActivity {
        let activity = self.activity();
        let current = self.current.borrow();
        SettingsActivity {
            preparing: activity.preparing,
            processing: activity.processing,
            recording: activity.recording,
            pending_incognito: current
                .as_ref()
                .is_some_and(|current| current.options.incognito),
            pending_mode: current.as_ref().map_or_else(
                || "dictation".into(),
                |current| current.options.mode.clone(),
            ),
            final_live: self.live.finalizing(),
            rewriting: self.review.rewriting(),
            live_schedule: self.live.active(),
        }
    }
    pub(super) fn idle(&self) {
        if self.closed.get() {
            return;
        }
        let activity = self.activity();
        let available =
            !activity.pending_review && !self.meeting.busy() && self.capture.phase().is_none();
        self.settings.capture.set_activity(activity.clone());
        self.settings.capture.set_controls_available(available);
        self.meeting.page.record_button.set_sensitive(available);
        let mut state = self.page().view_state();
        state.review_active = activity.pending_review;
        state.meeting_busy = self.meeting.busy();
        state.initialization_failed = self.initialization_failed.get();
        self.page().set_view_state(state);
        // A provider repair enables capture; the released startup callout stays
        // until its explicit Retry action acknowledges initialization.
        self.page()
            .record_button
            .set_sensitive(available && self.ready.borrow().is_some());
        self.install_history_factory();
        self.refresh_status();
    }
    pub(super) fn refresh_status(&self) {
        self.page().refresh_output_visibility();
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
            if let Some(shortcuts) = shortcuts { shortcuts.await; }
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
                        owner.review.dismiss();
                        owner.shell.show_message("Capture services are ready.");
                    }
                    if initializing { owner.start_shortcuts(); }
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
                        owner.page().set_status("Speech provider needs credentials. Editing and rewriting remain available.");
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
        if let Err(error) = self.page().set_config(config.clone()) {
            self.shell.show_message(&error.to_string());
        }
        self.review.set_config(config.clone());
        self.live.set_config(config.clone());
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
                self.titles.cancel();
                self.cancel_catalog();
                self.page().rewrite_settings.reset_catalog();
                self.meeting.sync_config(true);
                self.initialize_services(false);
            }
            SettingsKind::Presentation { live_changed } => {
                if live_changed && let Err(error) = self.capture.apply_live_config(config) {
                    self.page().set_status(&error);
                }
                self.synchronize_capture();
                if self.ready.borrow().is_none() {
                    self.initialize_services(false);
                }
            }
            SettingsKind::Unchanged => {}
        }
        self.refresh_review();
    }
    pub(super) fn inline_changed(self: &Rc<Self>, effect: InlineEffect) {
        if self.closed.get() {
            return;
        }
        let config = self.services.config();
        if let Err(error) = self.page().set_config(config.clone()) {
            self.shell.show_message(&error.to_string());
        }
        self.review.set_config(config.clone());
        self.live.set_config(config.clone());
        self.synchronize_capture();
        match effect {
            InlineEffect::Privacy { persisted } => {
                if config.incognito_mode {
                    self.titles.cancel();
                    self.review.cancel();
                    self.review.dismiss();
                    self.live.cancel();
                }
                if config.incognito_mode {
                    self.cancel_screenshot_picker();
                }
                self.workspace().set_private(config.incognito_mode);
                self.meeting.sync_config(persisted);
            }
            InlineEffect::Titles if !config.automatic_titles => self.titles.cancel(),
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
    pub(super) fn save_rewrite_settings(
        &self,
        model: Option<String>,
        fast: bool,
        effort: Option<String>,
    ) {
        if self.closed.get() {
            return;
        }
        let mut proposed = self.services.config();
        if proposed.rewrite_provider == "litellm" {
            proposed.litellm_model = model;
            proposed.litellm_reasoning_effort = effort;
        } else {
            proposed.rewrite_model = model;
            proposed.rewrite_fast_mode = fast;
            proposed.rewrite_reasoning_effort = effort;
        }
        if self
            .services
            .save_inline_config(proposed.clone(), InlineSavePolicy::Persistent)
            .is_err()
        {
            self.page()
                .rewrite_settings
                .set_config(self.services.config());
            self.page()
                .rewrite_settings
                .status
                .set_label("Could not save rewrite settings. Your previous choices remain.");
            return;
        }
        self.synchronize_capture();
        self.review.set_config(proposed.clone());
        self.page().rewrite_settings.set_config(proposed);
    }
    pub(super) fn load_models(self: &Rc<Self>) {
        let config = self.services.config();
        if self.closed.get() || config.rewrite_provider == "none" || self.catalog.borrow().is_some()
        {
            return;
        }
        let Ok(client) = RewriteClient::new(&config, Some(Duration::from_secs(10)), None) else {
            self.page().rewrite_settings.set_models(None);
            return;
        };
        let client = Rc::new(client);
        self.catalog.replace(Some(client.clone()));
        self.page().rewrite_settings.set_loading();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let models = client.list_models().await.ok();
            client.close().await;
            let Some(owner) = weak.upgrade().filter(|owner| {
                !owner.closed.get()
                    && owner
                        .catalog
                        .borrow()
                        .as_ref()
                        .is_some_and(|active| Rc::ptr_eq(active, &client))
            }) else {
                return;
            };
            owner.catalog.borrow_mut().take();
            owner.page().rewrite_settings.set_models(models);
        });
    }
    fn cancel_catalog(&self) {
        if let Some(client) = self.catalog.borrow_mut().take() {
            client.cancel();
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
        if self.capture.phase().is_none() {
            if self.ready.borrow().is_none() {
                return;
            }
            let message = if self.meeting.busy() {
                Some("Finish the explicit Meeting capture or retry before starting dictation.")
            } else if self.history.retry_identifier().is_some() {
                Some("Finish the retained-audio retry before starting another recording.")
            } else if self.pending.has_command() {
                Some("Apply or discard the current Command preview before recording again.")
            } else if self.services.scratchpad.borrow().draft.is_some() {
                Some("Resolve or delete the current Scratchpad draft before recording again.")
            } else {
                None
            };
            if let Some(message) = message {
                self.page().set_status(message);
                return;
            }
        }
        self.capture.toggle(origin);
    }
    pub(super) fn continue_recording(self: &Rc<Self>, identifier: &str) {
        if self.closed.get()
            || self.services.config().incognito_mode
            || self.capture.phase().is_some()
            || self.review.rewriting()
            || self.live.finalizing()
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
        let continuation = match origin {
            CaptureOrigin::Continuation(id) => Some(id),
            _ => None,
        };
        let mut target = if automatic {
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
        let config = self.services.config();
        let prefs = &self.settings.capture;
        let fallback = if config.remember_per_application && application.is_some() {
            config.default_mode.as_str()
        } else {
            ["dictation", "command", "scratchpad"]
                .get(prefs.mode.selected() as usize)
                .copied()
                .unwrap_or("dictation")
        };
        let mode = if continuation.is_some() || phone.is_some() {
            "dictation".into()
        } else {
            self.services.personalization.borrow().selected_mode(
                application.as_deref(),
                config.remember_per_application,
                fallback,
            )?
        };
        prefs.set_profile(application.clone());
        prefs.mode.set_selected(
            ["dictation", "command", "scratchpad"]
                .iter()
                .position(|value| *value == mode)
                .unwrap() as u32,
        );
        let incognito = prefs.incognito.is_active();
        if mode == "command" && incognito {
            return Err(WorkflowError::Invalid("Command mode is unavailable in Incognito because Codex durability cannot be guaranteed.".into()));
        }
        if mode == "command" && config.rewrite_provider == "none" {
            return Err(WorkflowError::Invalid(
                "Choose a rewriting provider to use Command mode.".into(),
            ));
        }
        let command_target = if mode == "command" && automatic {
            target = None;
            self.tracker
                .borrow()
                .as_ref()
                .map(|tracker| {
                    tracker.capture_text_target(crate::text_target::MAX_SELECTED_TEXT_CHARACTERS)
                })
                .transpose()
                .map_err(|error| WorkflowError::Invalid(error.to_string()))?
                .flatten()
        } else {
            None
        };
        if let Some(command) = &command_target {
            application = command.application_identifier().map(str::to_owned);
        }
        if mode != "dictation" {
            target = None;
        }
        let policy = match prefs.audio_retention.selected() {
            0 => AudioRetentionPolicy::Never,
            2 => AudioRetentionPolicy::Always,
            _ => AudioRetentionPolicy::Failures,
        };
        let options = CaptureOptions {
            mode: mode.clone(),
            use_cleanup: prefs.cleanup_enabled() && !incognito && config.rewrite_provider != "none",
            allow_auto_paste: automatic,
            incognito,
            audio_retention: policy,
            selected_text: command_target
                .as_ref()
                .and_then(|target| target.selected_text().map(str::to_owned)),
            application_identifier: phone
                .as_ref()
                .map(|id| format!("mluva-web:{id}"))
                .or(application),
            style_identifier: prefs.selected_style(),
            use_saved_style: config.rewrite_provider != "none" && !incognito,
            defer_delivery: continuation.is_some(),
            preview_enabled: config.live_rewrite_enabled,
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
            command_target,
            continuation,
            session: session.clone(),
        }));
        self.page().set_pending_mode(&mode);
        self.review.clear_for_capture();
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
            || result.requires_acceptance
            || (result.history_entry.is_none()
                && mluva_core::text::trim(&result.transcription.text).is_empty())
        {
            return;
        }
        let mut excluded = self.excluded_history();
        if self.live.active() {
            excluded.extend(
                result
                    .history_entry
                    .as_ref()
                    .map(|entry| entry.identifier.clone()),
            );
        }
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
        if result.requires_acceptance && result.mode == "command" {
            self.pending.show_command(
                result,
                context.and_then(|context| context.command_target),
                completion.session.options.audio_retention,
            );
            return;
        }
        if result.requires_acceptance && result.mode == "scratchpad" {
            self.pending
                .show_notes(result, completion.session.options.audio_retention);
            return;
        }
        self.idle();
        if result.mode == "dictation"
            && !result.incognito
            && let Some(entry) = result.history_entry.as_ref()
        {
            self.review.publish(&entry.identifier, "ready", "");
        } else {
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
        self.current.borrow_mut().take();
        self.discard_screenshots(id);
        self.idle();
        self.clear_overlay();
    }
    pub(super) fn save_live_once(&self, config: &AppConfig) -> bool {
        let mut proposed = self.services.config();
        proposed.live_rewrite_enabled = config.live_rewrite_enabled;
        let saved = self
            .services
            .save_inline_config(proposed.clone(), InlineSavePolicy::SessionLiveOnce)
            .is_ok();
        self.synchronize_capture();
        self.live.set_config(proposed);
        saved
    }
    pub(super) fn cancel(self: &Rc<Self>) -> bool {
        if self.closed.get() {
            return false;
        }
        if self.startup_record_requested.replace(false) {
            return true;
        }
        if self.capture.cancel() {
            return true;
        }
        if self.live.finalizing() || self.review.rewriting() {
            self.review.cancel();
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
            mode: match current.options.mode.as_str() {
                "command" => "Command",
                "scratchpad" => "Notes",
                _ => "Dictate",
            }
            .into(),
            level: current.session.audio_level(),
            elapsed_seconds: current.session.elapsed_seconds().unwrap_or_default() as i64,
            preview: self.workspace().live_text.text(),
            delivery: if current.options.allow_auto_paste
                && config.auto_paste
                && self.tracker.borrow().is_some()
                && (current.target.as_ref().is_some_and(|target| {
                    matches!(target, DeliveryTargetSnapshot::Text(text) if text.editable_text_available())
                        || mluva_core::delivery::keyboard_paste_available(None, target.application_identifier())
                }) || current.command_target.as_ref().is_some_and(|target| {
                    target.editable_text_available()
                        || mluva_core::delivery::keyboard_paste_available(None, target.application_identifier())
                }))
            {
                "Paste armed"
            } else {
                "Copy only"
            }
            .into(),
            smooth_scrolling: config.smooth_scrolling
                && gtk::Settings::default().is_none_or(|settings| settings.is_gtk_enable_animations()),
            scroll_duration_ms: config.scroll_duration_ms,
            scroll_lookahead_lines: config.scroll_lookahead_lines,
            widget_position: config.widget_position,
            widget_lines: config.widget_lines,
            widget_opacity: config.widget_opacity,
            rewrite_enabled: config.rewrite_provider != "none",
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
            widget_position: config.widget_position,
            widget_lines: config.widget_lines,
            widget_opacity: config.widget_opacity,
            rewrite_enabled: config.rewrite_provider != "none",
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
        self.cancel_catalog();
        self.titles.close();
        self.review.shutdown();
        self.live.shutdown();
        self.pending.close();
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
