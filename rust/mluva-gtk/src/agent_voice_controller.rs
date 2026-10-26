//! Application-owned routing, one-shot delivery and optional spoken status.
use crate::{
    agent_voice_view::AgentVoiceView, async_runtime::DesktopRuntime, capture_view::CapturePage,
};
use adw::prelude::*;
use mluva_core::{
    agent_voice::{
        AgentStatus, Attempt, Delivery, DraftReason, RoutePhase, SendOutcome, Submission, Target,
        VoiceRoute, VoiceSettings,
    },
    diagnostics::{RouteEvent, RouteKind},
};
use mluva_providers::{
    agent_targets::{self, Sources},
    claude_sessions::ClaudeRegistry,
    herdr, spoken_reply,
};
use mluva_workflows::services::ApplicationServices;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

mod cli;
mod wheel;

pub struct AgentVoiceCallbacks {
    pub toggle: Rc<dyn Fn()>,
    pub transcript: Rc<dyn Fn()>,
    pub select: Rc<dyn Fn()>,
    pub release: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub interrupt: Rc<dyn Fn()>,
    pub settings_changed: Rc<dyn Fn()>,
    pub draft_saved: Rc<dyn Fn()>,
    pub copy: Rc<dyn Fn(&str)>,
    pub capture_busy: Rc<dyn Fn() -> bool>,
}
struct SpeechJob {
    cancel: CancellationToken,
    done: CancellationToken,
}
pub struct AgentVoiceController {
    pub view: AgentVoiceView,
    pub wheel: crate::agent_session_wheel::SessionWheel,
    pub route: RefCell<VoiceRoute>,
    settings: RefCell<VoiceSettings>,
    services: Rc<ApplicationServices>,
    page: Rc<CapturePage>,
    runtime: Rc<DesktopRuntime>,
    callbacks: AgentVoiceCallbacks,
    targets: RefCell<Vec<Target>>,
    /// Claude Code's own registry, when this account has a Claude Code configuration.
    registry: Option<Arc<Mutex<ClaudeRegistry>>>,
    editor: RefCell<Option<crate::markdown_view::MarkdownTextView>>,
    /// The screen annotation summary of the recording whose draft is in the editor.
    draft_appendix: RefCell<Option<String>>,
    voice_error: RefCell<Option<String>>,
    syncing: Cell<bool>,
    polling: Cell<bool>,
    generation: Cell<u64>,
    last_poll: Cell<u64>,
    last_tick: Cell<u64>,
    timer: RefCell<Option<glib::SourceId>>,
    subscriptions: RefCell<Vec<gio::SignalSubscription>>,
    connection: RefCell<String>,
    spoken_attempt: RefCell<Option<String>>,
    speech: RefCell<Option<SpeechJob>>,
    closed: CancellationToken,
    cli: RefCell<Option<(gio::DBusConnection, gio::RegistrationId)>>,
    settings_timer: RefCell<Option<glib::SourceId>>,
    pub(crate) approved_key: RefCell<Option<String>>,
    /// The attempt and step last written to diagnostics and History, so each is written once.
    reported: RefCell<(String, Vec<&'static str>)>,
    /// The open wheel's diagnostic session and when it opened.
    wheel_session: RefCell<Option<(String, Instant)>>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// What a new recording that stops a pending send tells the user: the text is in History only
/// when History holds it, which Incognito never does.
fn interrupted_send_message(saved_in_history: bool) -> &'static str {
    if saved_in_history {
        "A new recording stopped the pending send. Nothing was sent; the text is in History."
    } else {
        "A new recording stopped the pending send. Nothing was sent."
    }
}

impl AgentVoiceController {
    pub fn new(
        runtime: Rc<DesktopRuntime>,
        services: Rc<ApplicationServices>,
        page: Rc<CapturePage>,
        callbacks: AgentVoiceCallbacks,
    ) -> Rc<Self> {
        let loaded = VoiceSettings::load(&services.paths.config.join("agent-voice.json"));
        let settings = loaded.clone().unwrap_or_default();
        let invalid = settings
            .validate(&services.config().global_recording_key)
            .err();
        let settings = if invalid.is_some() {
            VoiceSettings::default()
        } else {
            settings
        };
        let view = AgentVoiceView::new(&settings);
        if let Some(error) = loaded.err().or(invalid) {
            view.settings_status.set_label(error);
        }
        let owner = Rc::new(Self {
            view,
            wheel: crate::agent_session_wheel::SessionWheel::new(),
            route: RefCell::new(VoiceRoute::default()),
            settings: RefCell::new(settings),
            services,
            page,
            runtime,
            callbacks,
            targets: RefCell::new(vec![]),
            registry: ClaudeRegistry::from_environment().map(|r| Arc::new(Mutex::new(r))),
            editor: RefCell::new(None),
            draft_appendix: RefCell::new(None),
            voice_error: RefCell::new(None),
            syncing: Cell::new(false),
            polling: Cell::new(false),
            generation: Cell::new(0),
            last_poll: Cell::new(0),
            last_tick: Cell::new(now()),
            timer: RefCell::new(None),
            subscriptions: RefCell::new(vec![]),
            connection: RefCell::new("Looking for local sessions…".into()),
            spoken_attempt: RefCell::new(None),
            speech: RefCell::new(None),
            closed: CancellationToken::new(),
            cli: RefCell::new(None),
            settings_timer: RefCell::new(None),
            approved_key: RefCell::new(None),
            reported: RefCell::new((String::new(), vec![])),
            wheel_session: RefCell::new(None),
        });
        owner.bind();
        owner.bind_wheel();
        owner.render();
        owner
    }
    fn bind(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.view
            .recipients
            .connect_selected_notify(move |dropdown| {
                let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) else {
                    return;
                };
                owner.disarm(DraftReason::RecipientChanged);
                owner.stop_voice();
                let selected = dropdown
                    .selected()
                    .checked_sub(1)
                    .and_then(|i| owner.targets.borrow().get(i as usize).cloned());
                owner.settings.borrow_mut().selected = selected;
                owner.generation.set(owner.generation.get() + 1);
                owner.save_choice();
                owner.view.reply.set_label("");
                owner.render();
            });
        let weak = Rc::downgrade(self);
        self.view.refresh.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.refresh();
            }
        });
        for control in [&self.view.enabled, &self.view.spoken] {
            let weak = Rc::downgrade(self);
            control.connect_toggled(move |_| {
                if let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) {
                    owner.save_settings();
                }
            });
        }
        for control in [&self.view.hold_key, &self.view.draft_key] {
            let weak = Rc::downgrade(self);
            control.connect_selected_notify(move |_| {
                if let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) {
                    owner.save_settings();
                }
            });
        }
        for entry in [&self.view.socket, &self.view.voice_id] {
            let weak = Rc::downgrade(self);
            entry.connect_changed(move |_| {
                if let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) {
                    owner.schedule_settings();
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.view.keep.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.keep_draft();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.cancel.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.cancel)();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.send_draft.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.send_draft();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.copy_draft.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.copy)(&owner.with_annotation(owner.draft_text()));
            }
        });
        let weak = Rc::downgrade(self);
        self.view.record.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.toggle)();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.read.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.read_status();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.replay.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.speak_visible();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.mute.connect_toggled(move |button| {
            let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) else {
                return;
            };
            if button.is_active() {
                owner.stop_voice();
            }
            owner.settings.borrow_mut().spoken_updates = !button.is_active();
            owner.view.spoken.set_active(!button.is_active());
            owner.save_choice();
            owner.render();
        });
        let weak = Rc::downgrade(self);
        self.timer.replace(Some(glib::timeout_add_local(
            Duration::from_millis(100),
            move || {
                let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled()) else {
                    return glib::ControlFlow::Break;
                };
                owner.tick();
                glib::ControlFlow::Continue
            },
        )));
        self.monitor_interruptions();
    }
    pub fn settings(&self) -> VoiceSettings {
        self.settings.borrow().clone()
    }
    fn save_choice(&self) {
        if !self.services.config().incognito_mode
            && let Err(error) = self.settings.borrow().save(
                &self.services.paths.config.join("agent-voice.json"),
                &self.services.config().global_recording_key,
            )
        {
            self.view.settings_status.set_label(error);
        }
    }
    fn schedule_settings(self: &Rc<Self>) {
        if let Some(timer) = self.settings_timer.borrow_mut().take() {
            timer.remove();
        }
        let weak = Rc::downgrade(self);
        self.settings_timer
            .replace(Some(glib::timeout_add_local_once(
                Duration::from_millis(350),
                move || {
                    if let Some(owner) = weak.upgrade() {
                        owner.settings_timer.borrow_mut().take();
                        owner.save_settings();
                    }
                },
            )));
    }
    pub(crate) fn save_settings(self: &Rc<Self>) {
        let proposed = VoiceSettings {
            enabled: self.view.enabled.is_active(),
            socket: (!self.view.socket.text().trim().is_empty())
                .then(|| self.view.socket.text().trim().into()),
            selected: self.settings.borrow().selected.clone(),
            hold_key: format!("F{}", self.view.hold_key.selected() + 1),
            draft_key: format!("F{}", self.view.draft_key.selected() + 1),
            spoken_updates: self.view.spoken.is_active(),
            voice_id: self.view.voice_id.text().trim().into(),
        };
        if proposed.spoken_updates && proposed.voice_id.is_empty() {
            self.view
                .settings_status
                .set_label("Choose an ElevenLabs voice ID before enabling speech.");
            return;
        }
        let validation = proposed.validate(&self.services.config().global_recording_key);
        let persisted = validation.and_then(|()| {
            if self.services.config().incognito_mode {
                Ok(())
            } else {
                proposed.save(
                    &self.services.paths.config.join("agent-voice.json"),
                    &self.services.config().global_recording_key,
                )
            }
        });
        if let Err(error) = persisted {
            self.view.settings_status.set_label(error);
            return;
        }
        let previous = self.settings();
        let shortcuts_changed = previous.enabled != proposed.enabled
            || previous.hold_key != proposed.hold_key
            || previous.draft_key != proposed.draft_key;
        if previous == proposed {
            return;
        }
        if previous.enabled != proposed.enabled || previous.socket != proposed.socket {
            self.interrupt();
        }
        self.settings.replace(proposed);
        self.generation.set(self.generation.get() + 1);
        self.syncing.set(true);
        self.view
            .mute
            .set_active(!self.settings.borrow().spoken_updates);
        self.syncing.set(false);
        self.view.settings_status.set_label("");
        if shortcuts_changed {
            (self.callbacks.settings_changed)();
        }
        self.refresh();
        self.render();
    }
    /// The remembered session is present but nothing can deliver to it: the next agent
    /// key press records an editable draft in Mluva instead.
    pub fn draft_only_selected(&self) -> bool {
        self.selected().is_some_and(|t| t.draft_only())
    }
    /// The delivery level of the attempt being recorded, for diagnostics.
    pub fn attempt_delivery(&self) -> Option<Delivery> {
        self.route
            .borrow()
            .attempt
            .as_ref()
            .map(|a| a.target.delivery())
    }
    fn selected(&self) -> Option<Target> {
        let settings = self.settings.borrow();
        let selected = settings.selected.as_ref()?;
        self.targets
            .borrow()
            .iter()
            .find(|t| t.same_instance(selected))
            .cloned()
    }
    /// Where discovery may look: local Herdr servers and Claude Code's registry.
    fn sources(&self, socket: Option<&str>) -> Sources {
        Sources {
            herdr: herdr::socket_candidates(
                self.services
                    .paths
                    .config
                    .parent()
                    .unwrap_or(Path::new("/")),
                socket,
            ),
            claude: self.registry.clone(),
            apps: true,
        }
    }
    fn draft_text(&self) -> String {
        self.editor
            .borrow()
            .as_ref()
            .map_or_else(String::new, |editor| editor.text())
    }
    /// A draft keeps the screen annotation its own recording delivered, never the summaries
    /// of earlier recordings on a continued or merged conversation.
    fn with_annotation(&self, text: String) -> String {
        mluva_workflows::dictation::with_appendix(&text, self.draft_appendix.borrow().as_deref())
    }
    pub fn save_draft(&self) -> bool {
        let history = self
            .route
            .borrow()
            .attempt
            .as_ref()
            .and_then(|a| a.history_id.clone());
        if let Some(id) = history
            && !self.services.config().incognito_mode
        {
            if !self.page.workspace.save_conversation_edits(&id) {
                self.view
                    .reply
                    .set_label("Edits could not be saved. Keep this draft open or copy it.");
                return false;
            }
            (self.callbacks.draft_saved)();
            self.view
                .reply
                .set_label("Edits saved. The original recognition is preserved in History.");
        }
        true
    }
    fn send_draft(self: &Rc<Self>) {
        if !self
            .editor
            .borrow()
            .as_ref()
            .is_some_and(|editor| editor.is_mapped())
        {
            return;
        }
        let history = {
            let route = self.route.borrow();
            let Some(attempt) = route
                .attempt
                .as_ref()
                .filter(|a| matches!(a.phase, RoutePhase::Draft(_)))
            else {
                return;
            };
            attempt.history_id.clone()
        };
        if !self.save_draft() {
            return;
        }
        let text = self.with_annotation(self.draft_text());
        if let Ok(id) = self.begin() {
            self.route.borrow_mut().release(now());
            self.route
                .borrow_mut()
                .recognized(&id, &text, history, now());
            self.attach_editor(&id);
            self.render();
        }
    }
    pub fn begin(&self) -> Result<String, &'static str> {
        if !self.settings.borrow().enabled {
            return Err("Enable agent dictation in Settings → Capture first.");
        }
        let target = self
            .selected()
            .ok_or("Choose an available agent session.")?;
        let id = self.route.borrow_mut().begin(target, now())?;
        self.wheel.window.set_visible(false);
        self.view.reply.set_label("");
        self.spoken_attempt.borrow_mut().take();
        self.render();
        Ok(id)
    }
    pub fn leave_for_capture(&self) {
        self.wheel.window.set_visible(false);
        // The attempt that was counting down, and whether its text reached History: Incognito
        // keeps no History row, so only a saved one may be promised.
        let countdown = self.route.borrow().attempt.as_ref().and_then(|a| {
            matches!(a.phase, RoutePhase::Grace { .. }).then(|| a.history_id.is_some())
        });
        self.disarm(DraftReason::Interrupted);
        if let Some(saved) = countdown {
            self.view.reply.set_label(interrupted_send_message(saved));
        }
        if self
            .route
            .borrow()
            .attempt
            .as_ref()
            .is_none_or(|a| a.phase != RoutePhase::Submitting)
        {
            self.route.borrow_mut().attempt = None;
        }
        self.render();
    }
    pub fn release(&self) -> bool {
        let released = self.route.borrow_mut().release(now());
        self.render();
        released
    }
    pub fn recording(&self, id: &str) -> bool {
        self.route
            .borrow()
            .attempt
            .as_ref()
            .is_some_and(|a| a.id == id && a.phase == RoutePhase::Recording)
    }
    /// Final recognition for an attempt; `appendix` is its recording's annotation summary,
    /// sent after the text and kept for the draft.
    pub fn completed(
        self: &Rc<Self>,
        id: &str,
        text: &str,
        history: Option<String>,
        appendix: Option<String>,
    ) {
        let message = mluva_workflows::dictation::with_appendix(text, appendix.as_deref());
        if !self
            .route
            .borrow_mut()
            .recognized(id, &message, history, now())
        {
            return;
        }
        self.draft_appendix.replace(appendix);
        self.attach_editor(id);
        self.render();
    }
    fn attach_editor(self: &Rc<Self>, id: &str) {
        let editor = self.page.workspace.documents().first().cloned();
        self.editor.replace(editor.clone());
        if let Some(editor) = editor {
            let weak = Rc::downgrade(self);
            let unmap_id = id.to_owned();
            let id = id.to_owned();
            editor.buffer().connect_changed(move |_| {
                if let Some(owner) = weak.upgrade() {
                    let current = owner
                        .route
                        .borrow()
                        .attempt
                        .as_ref()
                        .is_some_and(|a| a.id == id);
                    if current {
                        owner.disarm(DraftReason::Edited);
                    }
                }
            });
            let weak = Rc::downgrade(self);
            editor.connect_unmap(move |_| {
                if let Some(owner) = weak.upgrade()
                    && owner
                        .route
                        .borrow()
                        .attempt
                        .as_ref()
                        .is_some_and(|a| a.id == unmap_id)
                {
                    owner.disarm(DraftReason::Kept);
                }
            });
        } else {
            self.disarm(DraftReason::Interrupted);
        }
    }
    pub fn disarm(&self, reason: DraftReason) -> bool {
        let changed = self.route.borrow_mut().disarm(reason);
        if changed {
            self.render();
        }
        changed
    }
    pub fn abandoned(&self, reason: DraftReason) {
        self.route.borrow_mut().abandon_capture(reason);
        self.render();
    }
    pub fn keep_draft(&self) {
        self.disarm(DraftReason::Kept);
        (self.callbacks.release)();
    }
    pub fn interrupt(&self) {
        self.wheel.window.set_visible(false);
        self.disarm(DraftReason::Interrupted);
        self.stop_voice();
        (self.callbacks.interrupt)();
    }
    pub fn shortcut_binding(&self, hold: Option<&str>, draft: Option<&str>) {
        self.approved_key.replace(hold.map(str::to_owned));
        self.view.shortcut_status.set_label(&match (hold, draft) {
            (Some(hold), Some(draft)) => format!("{hold} chooses a session; press again to record · {draft} keeps a draft"),
            _ => "Global shortcuts are not approved. Use Dictate to agent while checking desktop approval.".into(),
        });
        // An open wheel shows the approved key at once, not on the next tick.
        self.render_wheel();
    }
    fn tick(self: &Rc<Self>) {
        let time = now();
        let previous = self.last_tick.replace(time);
        // A busy GTK frame is not a release/cancel gesture. Only an expired
        // sending countdown needs the strict timing guard; lock/sleep signals
        // and the recording safety limit still stop an active capture.
        let countdown = self
            .route
            .borrow()
            .attempt
            .as_ref()
            .is_some_and(|a| matches!(a.phase, RoutePhase::Grace { .. }));
        if (countdown && (time < previous || time.saturating_sub(previous) > 1_000))
            || self.route.borrow().recording_timed_out(time)
        {
            self.interrupt();
        }
        if self.settings.borrow().enabled && time.saturating_sub(self.last_poll.get()) > 2_000 {
            self.refresh();
        }
        let submission = self.route.borrow_mut().take_submission(time);
        if let Some(submission) = submission {
            self.submit(submission);
        }
        self.render();
    }
    /// Delivers a message whose automatic-send permission was already consumed, through the
    /// target's own transport.
    fn submit(self: &Rc<Self>, submission: Submission) {
        let weak = Rc::downgrade(self);
        let cancel = self.closed.clone();
        self.runtime.spawn(async move {
            let receipt = agent_targets::deliver(&submission, &cancel).await;
            let outcome = receipt.outcome;
            {
                let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled()) else {
                    return;
                };
                if owner
                    .route
                    .borrow_mut()
                    .delivered(&submission.id, outcome, now())
                    && matches!(outcome, SendOutcome::Accepted | SendOutcome::Taken)
                {
                    owner
                        .view
                        .reply
                        .set_label(&accepted_message(&submission.target));
                }
                owner.render();
            }
            // A queued message: History learns when the agent takes it, if that is seen.
            if let Some(taken) = receipt.taken
                && taken.await
                && let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled())
                && owner.route.borrow_mut().taken(&submission.id)
            {
                owner.render();
            }
        });
    }
    /// A second press while the cancel window runs ends it and delivers at once, to the same
    /// target. Returns whether a pending delivery was taken.
    pub fn send_now(self: &Rc<Self>) -> bool {
        let submission = self.route.borrow_mut().send_now(now());
        let Some(submission) = submission else {
            return false;
        };
        self.submit(submission);
        self.render();
        true
    }
    /// Writes each route step once to diagnostics, and the send outcome to the History row.
    /// Every path that ends a pending send passes through `render`, so none is unrecorded.
    fn report_phase(&self, attempt: Option<&Attempt>) {
        let Some(a) = attempt else {
            return;
        };
        // Every step the current phase implies, in order; each is written once.
        let steps: Vec<(&'static str, RouteEvent, Option<&'static str>)> = match a.phase {
            RoutePhase::Grace { .. } => vec![("grace", RouteEvent::CountdownStarted, None)],
            RoutePhase::Delivered => {
                let mut steps = vec![(
                    "delivered",
                    RouteEvent::SendAccepted,
                    Some(a.target.delivery().outcome()),
                )];
                if a.taken {
                    steps.push(("taken", RouteEvent::QueueTaken, Some("delivered")));
                }
                steps
            }
            RoutePhase::Held => vec![("held", RouteEvent::SendHeld, Some("held"))],
            RoutePhase::Unconfirmed => vec![(
                "unconfirmed",
                RouteEvent::SendUnconfirmed,
                Some("unconfirmed"),
            )],
            RoutePhase::Draft(reason) => vec![match reason {
                DraftReason::Kept => ("kept", RouteEvent::DraftKept, Some("draft-kept")),
                DraftReason::Edited => ("edited", RouteEvent::DraftEdited, Some("draft-kept")),
                DraftReason::RecipientChanged => (
                    "recipient-changed",
                    RouteEvent::DraftRecipientChanged,
                    Some("recipient-unavailable"),
                ),
                DraftReason::Unavailable => (
                    "unavailable",
                    RouteEvent::DraftUnavailable,
                    Some("recipient-unavailable"),
                ),
                DraftReason::Rejected => (
                    "rejected",
                    RouteEvent::SendRejected,
                    Some("recipient-unavailable"),
                ),
                DraftReason::Busy => ("busy", RouteEvent::DraftBusy, Some("busy")),
                DraftReason::Cancelled => {
                    ("cancelled", RouteEvent::DraftCancelled, Some("cancelled"))
                }
                DraftReason::Interrupted => (
                    "interrupted",
                    RouteEvent::DraftInterrupted,
                    Some("draft-kept"),
                ),
                // The row already says no speech was recognised; that stays.
                DraftReason::Empty => ("empty", RouteEvent::DraftEmpty, None),
                DraftReason::TooLong => ("too-long", RouteEvent::DraftTooLong, Some("draft-kept")),
            }],
            _ => return,
        };
        for (step, event, outcome) in steps {
            self.report_step(a, step, event, outcome);
        }
    }
    fn report_step(
        &self,
        a: &Attempt,
        step: &'static str,
        event: RouteEvent,
        outcome: Option<&'static str>,
    ) {
        {
            let mut reported = self.reported.borrow_mut();
            if reported.0 != a.id {
                *reported = (a.id.clone(), vec![]);
            }
            if reported.1.contains(&step) {
                return;
            }
            reported.1.push(step);
        }
        let milliseconds = match (a.submitted_ms, a.received_ms, a.final_ms, a.released_ms) {
            (Some(sent), Some(received), ..) => received.saturating_sub(sent),
            _ if step == "grace" => a
                .final_ms
                .zip(a.released_ms)
                .map_or(0, |(done, released)| done.saturating_sub(released)),
            _ => now().saturating_sub(a.final_ms.unwrap_or(a.started_ms)),
        };
        self.trace(
            &a.id,
            event,
            RouteKind::Agent(a.target.delivery()),
            milliseconds,
        );
        if let (Some(outcome), Some(id)) = (outcome, a.history_id.as_deref()) {
            self.record_outcome(id, outcome);
        }
    }
    /// A diagnostics event for the F12 route; skipped in Incognito. Never carries text.
    pub fn trace(&self, session: &str, event: RouteEvent, route: RouteKind, milliseconds: u64) {
        if self.services.config().incognito_mode {
            return;
        }
        let _ = self.services.diagnostics.record_route(
            session,
            event,
            route,
            milliseconds as f64 / 1000.0,
        );
    }
    /// Tells the History row what became of its text; nothing without a row or in Incognito.
    fn record_outcome(&self, history_id: &str, outcome: &str) {
        if self.services.config().incognito_mode {
            return;
        }
        let history = &self.services.history;
        let Ok(entry) = history.find(history_id) else {
            return;
        };
        if entry.delivery_outcome == outcome {
            return;
        }
        if history
            .mark_delivered(
                history_id,
                &entry.delivered_text,
                outcome,
                entry.retained_audio_path.is_some(),
                None,
            )
            .is_ok()
        {
            let refresh = self.callbacks.draft_saved.clone();
            glib::idle_add_local_once(move || refresh());
        }
    }
    pub fn refresh(self: &Rc<Self>) {
        if !self.settings.borrow().enabled || self.polling.replace(true) {
            return;
        }
        self.last_poll.set(now());
        let generation = self.generation.get();
        let sources = self.sources(self.settings.borrow().socket.as_deref());
        let weak = Rc::downgrade(self);
        let cancel = self.closed.clone();
        self.runtime.spawn(async move {
            let Some(found) = agent_targets::discover(sources, &cancel).await else {
                return;
            };
            if let Some(owner) = weak.upgrade() {
                owner.polling.set(false);
                if owner.generation.get() != generation || owner.closed.is_cancelled() {
                    return;
                }
                owner.connection.replace(found.summary());
                // A refresh in which no Herdr server answered is a failed read, not an empty
                // list. While the wheel is open it keeps what it shows, so a transient error
                // never collapses it or moves its page; sending stays guarded either way.
                if !found.herdr && !found.errors.is_empty() && owner.wheel.window.is_visible() {
                    owner.render();
                    return;
                }
                owner.update_targets(found.targets);
            }
        });
    }
    fn update_targets(self: &Rc<Self>, mut targets: Vec<Target>) {
        // Discovery already enumerates Herdr's workspace, tab and layout-pane order, then the
        // sessions outside it. Selection is the target's identity, never this position.
        let previous = self.selected();
        self.route.borrow_mut().observe(&targets);
        let selected = self.settings.borrow().selected.clone();
        if let Some(selected) = &selected
            && !targets.iter().any(|t| t.same_instance(selected))
        {
            let mut missing = selected.clone();
            missing.status = AgentStatus::Unknown;
            missing.missing = true;
            targets.push(missing);
        }
        let changed = *self.targets.borrow() != targets;
        let selected_index = selected
            .as_ref()
            .and_then(|s| targets.iter().position(|t| t.same_instance(s)))
            .map_or(0, |i| i + 1) as u32;
        self.targets.replace(targets);
        if changed || self.view.recipients.selected() != selected_index {
            let targets = self.targets.borrow();
            let mut labels = vec!["Choose a session".to_owned()];
            labels.extend(targets.iter().map(menu_label));
            let strings: Vec<_> = labels.iter().map(String::as_str).collect();
            let index = selected
                .as_ref()
                .and_then(|s| targets.iter().position(|t| t.same_instance(s)))
                .map_or(0, |i| i + 1);
            self.syncing.set(true);
            self.view
                .recipients
                .set_model(Some(&gtk::StringList::new(&strings)));
            self.view.recipients.set_selected(index as u32);
            self.syncing.set(false);
        }
        if let Some(current) = self.selected() {
            let transition = previous
                .as_ref()
                .is_some_and(|old| old.same_instance(&current) && old.status != current.status);
            let attempt = self
                .route
                .borrow()
                .attempt
                .as_ref()
                .filter(|a| a.phase == RoutePhase::Delivered && a.target.same_instance(&current))
                .map(|a| a.id.clone());
            if transition
                && matches!(current.status, AgentStatus::Blocked | AgentStatus::Done)
                && let Some(id) = attempt
                && self.spoken_attempt.borrow().as_ref() != Some(&id)
            {
                self.spoken_attempt.replace(Some(id));
                self.view.reply.set_label(&current.spoken_status());
                self.speak_visible();
            }
        }
        self.render();
    }
    pub fn stop_voice(&self) -> Option<CancellationToken> {
        let speech = self.speech.borrow();
        let job = speech.as_ref()?;
        job.cancel.cancel();
        (!job.done.is_cancelled()).then(|| job.done.clone())
    }
    pub fn read_status(self: &Rc<Self>) {
        if let Some(target) = self.selected() {
            self.view.reply.set_label(&target.spoken_status());
            self.speak_visible();
        }
    }
    fn speak_visible(self: &Rc<Self>) {
        let settings = self.settings();
        if (self.callbacks.capture_busy)()
            || !settings.spoken_updates
            || settings.voice_id.is_empty()
            || self.services.config().incognito_mode
            || self.route.borrow().attempt.as_ref().is_some_and(|a| {
                matches!(
                    a.phase,
                    RoutePhase::Recording
                        | RoutePhase::Finalizing
                        | RoutePhase::Grace { .. }
                        | RoutePhase::Submitting
                )
            })
        {
            return;
        }
        self.voice_error.borrow_mut().take();
        let prior = self.stop_voice();
        let cancel = self.closed.child_token();
        let done = CancellationToken::new();
        self.speech.replace(Some(SpeechJob {
            cancel: cancel.clone(),
            done: done.clone(),
        }));
        let sentence = self.view.reply.label().to_string();
        let credentials = self.services.credentials.clone();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            if let Some(prior) = prior { prior.cancelled().await; }
            let key = tokio::select! { _ = cancel.cancelled() => None, key = credentials.elevenlabs_api_key() => Some(key) };
            let result = match key {
                Some(Ok(key)) => spoken_reply::speak(&sentence, &settings.voice_id, key, &cancel).await.map(Some),
                Some(Err(error)) => Err(error), None => Ok(None),
            };
            done.cancel();
            if let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled())
                && let Err(error) = result && !cancel.is_cancelled() {
                owner.voice_error.replace(Some(error.to_string()));
                owner.render();
            }
        });
    }
    fn render(&self) {
        self.render_wheel();
        let enabled = self.settings.borrow().enabled;
        self.view.widget.set_visible(enabled);
        let attempt = self.route.borrow().attempt.clone();
        self.report_phase(attempt.as_ref());
        let pending = attempt.as_ref().is_some_and(|a| {
            matches!(
                a.phase,
                RoutePhase::Recording | RoutePhase::Finalizing | RoutePhase::Grace { .. }
            )
        });
        self.view.keep.set_visible(pending);
        self.view.cancel.set_visible(pending);
        let draft = attempt
            .as_ref()
            .is_some_and(|a| matches!(a.phase, RoutePhase::Draft(_)) && !a.text.is_empty());
        // Some input may be in the session, or it holds the message: never offered again.
        let unconfirmed = attempt
            .as_ref()
            .is_some_and(|a| matches!(a.phase, RoutePhase::Unconfirmed | RoutePhase::Held));
        self.view.draft_actions.set_visible(
            (draft || unconfirmed)
                && self
                    .editor
                    .borrow()
                    .as_ref()
                    .is_some_and(|editor| editor.is_mapped()),
        );
        self.view
            .send_draft
            .set_sensitive(draft && enabled && self.selected().is_some_and(|t| t.can_receive()));
        self.view.feedback.set_visible(enabled);
        self.view.record.set_sensitive(
            enabled
                && self.selected().is_some_and(|t| t.can_receive())
                && !attempt.as_ref().is_some_and(|a| {
                    matches!(a.phase, RoutePhase::Finalizing | RoutePhase::Submitting)
                }),
        );
        let recording = attempt
            .as_ref()
            .is_some_and(|a| a.phase == RoutePhase::Recording);
        self.view.record.set_label(if recording {
            "Stop and send"
        } else {
            "Dictate to agent"
        });
        self.view.recipients.set_sensitive(enabled && !recording);
        self.view.refresh.set_sensitive(enabled);
        self.view
            .read
            .set_sensitive(enabled && self.selected().is_some());
        self.view.replay.set_sensitive(
            !self.view.reply.label().is_empty() && self.settings.borrow().spoken_updates,
        );
        self.view
            .mute
            .set_sensitive(enabled && !self.settings.borrow().voice_id.is_empty());
        let status = match attempt {
            Some(a) => match a.phase {
                RoutePhase::Recording => format!("Recording for {} · {}", a.target.name, stop_hint(a.target.delivery())),
                RoutePhase::Finalizing => format!("Finishing your message to {}…", a.target.name),
                RoutePhase::Grace {deadline_ms} => countdown_message(&a.target, deadline_ms.saturating_sub(now())),
                RoutePhase::Submitting => format!("Delivering to {}… Cancel can no longer recall this message.", a.target.name),
                RoutePhase::Delivered => delivered_message(&a.target, a.taken),
                RoutePhase::Held => format!("{} holds your message until you approve it there. It is never sent again; your draft is here.", a.target.name),
                RoutePhase::Unconfirmed => format!("Delivery unconfirmed. Check {} before sending again; your draft is here.", a.target.place_to_check()),
                RoutePhase::Draft(reason) => match reason {
                    DraftReason::Edited | DraftReason::Kept => "Draft kept. Automatic sending is off; edit or copy your text.".into(),
                    DraftReason::Empty => "No words recognized. Nothing was sent.".into(),
                    DraftReason::TooLong => "Message is too long. Split it into smaller messages; nothing was sent.".into(),
                    DraftReason::Cancelled => "Cancelled. Nothing was sent.".into(),
                    DraftReason::RecipientChanged | DraftReason::Unavailable | DraftReason::Rejected => "Recipient changed or unavailable. Nothing was submitted; your text stays here.".into(),
                    DraftReason::Busy => format!("{} was busy or not open, so nothing was sent. Your draft is here; send it when the session is idle.", a.target.name),
                    DraftReason::Interrupted => "Automatic sending stopped after an interruption. Your text stays here.".into(),
                },
            },
            None if !enabled => "Enable agent dictation in Settings → Capture.".into(),
            None => match self.selected().filter(Target::draft_only) {
                Some(t) => format!("{} · {}.", t.display_name(), t.draft_hint()),
                None => self.connection.borrow().clone(),
            },
        };
        let status = if !pending {
            self.voice_error.borrow().clone().unwrap_or(status)
        } else {
            status
        };
        self.view.recipients.set_tooltip_text(
            self.selected()
                .map(|t| {
                    format!(
                        "Local · {} · {} · {} · {} · {}",
                        t.name,
                        t.kind.label(),
                        t.project,
                        t.place(),
                        t.delivery().verb()
                    )
                })
                .as_deref(),
        );
        if self.view.status.label() != status {
            self.view.status.set_label(&status);
        }
    }
    fn monitor_interruptions(self: &Rc<Self>) {
        for (bus_type, sender, path, interface, member) in [
            (
                gio::BusType::Session,
                "org.freedesktop.ScreenSaver",
                "/org/freedesktop/ScreenSaver",
                "org.freedesktop.ScreenSaver",
                "ActiveChanged",
            ),
            (
                gio::BusType::Session,
                "org.gnome.ScreenSaver",
                "/org/gnome/ScreenSaver",
                "org.gnome.ScreenSaver",
                "ActiveChanged",
            ),
            (
                gio::BusType::System,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
            ),
        ] {
            let weak = Rc::downgrade(self);
            self.runtime.spawn(async move {
                let Ok(bus) = gio::bus_get_future(bus_type).await else {
                    return;
                };
                let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled()) else {
                    return;
                };
                let weak = Rc::downgrade(&owner);
                owner
                    .subscriptions
                    .borrow_mut()
                    .push(bus.subscribe_to_signal(
                        Some(sender),
                        Some(interface),
                        Some(member),
                        Some(path),
                        None,
                        gio::DBusSignalFlags::NONE,
                        move |signal| {
                            if signal.parameters.get::<(bool,)>() == Some((true,))
                                && let Some(owner) = weak.upgrade()
                            {
                                owner.interrupt();
                            }
                        },
                    ));
            });
        }
    }
    pub fn close(&self) {
        self.wheel.window.set_visible(false);
        if let Some(timer) = self.settings_timer.borrow_mut().take() {
            timer.remove();
        }

        if let Some((connection, registration)) = self.cli.borrow_mut().take() {
            let _ = connection.unregister_object(registration);
        }
        self.closed.cancel();
        self.stop_voice();
        self.route
            .borrow_mut()
            .abandon_capture(DraftReason::Interrupted);
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.subscriptions.borrow_mut().clear();
    }
}

/// One dropdown row: name, product, project, place, state and what recording does.
fn menu_label(target: &Target) -> String {
    format!(
        "{} · {} · {} · {} ({:?}) · {}",
        target.display_name(),
        target.kind.label(),
        target.project_name(),
        target.place(),
        target.status,
        target.delivery().verb()
    )
}

/// What the second press does, said while recording.
fn stop_hint(delivery: Delivery) -> &'static str {
    match delivery {
        Delivery::Send => "press again to stop and send",
        Delivery::Queue => "press again to stop and queue",
        Delivery::Prefill => "press again to stop and place it in the composer",
        Delivery::DraftOnly => "press again to stop and keep a draft",
    }
}

/// The cancel window's line, by what will happen when it ends.
fn countdown_message(target: &Target, remaining_ms: u64) -> String {
    let seconds = remaining_ms as f64 / 1000.0;
    match target.delivery() {
        Delivery::Queue => format!("Queueing for {} in {seconds:.1} s", target.name),
        Delivery::Prefill => format!("Placing in {}'s composer in {seconds:.1} s", target.name),
        Delivery::Send | Delivery::DraftOnly => {
            format!("Sending to {} in {seconds:.1} s", target.name)
        }
    }
}

/// The status once the transport accepted the message.
fn delivered_message(target: &Target, taken: bool) -> String {
    if taken {
        return format!(
            "Delivered to {} · it took the message into a turn",
            target.name
        );
    }
    match target.delivery() {
        Delivery::Queue => format!(
            "Queued for {} · it takes the message when it is ready",
            target.name
        ),
        Delivery::Prefill => format!("In {}'s composer · press Enter there to send", target.name),
        Delivery::Send | Delivery::DraftOnly => {
            format!("Sent to {} · agent completion is separate", target.name)
        }
    }
}

/// The reply line after an accepted delivery; completion is never claimed.
fn accepted_message(target: &Target) -> String {
    match target.delivery() {
        Delivery::Queue => format!(
            "{} queued your message. When it runs is up to the agent.",
            target.name
        ),
        Delivery::Prefill => format!(
            "Your message is in {}'s composer. Nothing is sent until you press Enter.",
            target.name
        ),
        Delivery::Send | Delivery::DraftOnly => format!(
            "{} accepted your message. Completion is not confirmed.",
            target.name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::interrupted_send_message;

    #[test]
    fn an_interrupted_send_promises_history_only_when_a_row_exists() {
        assert!(interrupted_send_message(true).contains("the text is in History"));
        let incognito = interrupted_send_message(false);
        assert!(incognito.contains("Nothing was sent.") && !incognito.contains("History"));
    }
}
