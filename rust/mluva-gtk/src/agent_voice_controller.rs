//! Application-owned routing, one-shot delivery and optional spoken status.
use crate::{
    agent_voice_view::AgentVoiceView, async_runtime::DesktopRuntime, capture_view::CapturePage,
};
use adw::prelude::*;
use mluva_core::agent_voice::{
    AgentStatus, DraftReason, Recipient, RoutePhase, SendOutcome, VoiceRoute, VoiceSettings,
};
use mluva_providers::{herdr, spoken_reply};
use mluva_workflows::services::ApplicationServices;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

pub struct AgentVoiceCallbacks {
    pub press: Rc<dyn Fn()>,
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
    pub route: RefCell<VoiceRoute>,
    settings: RefCell<VoiceSettings>,
    services: Rc<ApplicationServices>,
    page: Rc<CapturePage>,
    runtime: Rc<DesktopRuntime>,
    callbacks: AgentVoiceCallbacks,
    recipients: RefCell<Vec<Recipient>>,
    editor: RefCell<Option<crate::markdown_view::MarkdownTextView>>,
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
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
            route: RefCell::new(VoiceRoute::default()),
            settings: RefCell::new(settings),
            services,
            page,
            runtime,
            callbacks,
            recipients: RefCell::new(vec![]),
            editor: RefCell::new(None),
            voice_error: RefCell::new(None),
            syncing: Cell::new(false),
            polling: Cell::new(false),
            generation: Cell::new(0),
            last_poll: Cell::new(0),
            last_tick: Cell::new(now()),
            timer: RefCell::new(None),
            subscriptions: RefCell::new(vec![]),
            connection: RefCell::new("Looking for local Herdr sessions…".into()),
            spoken_attempt: RefCell::new(None),
            speech: RefCell::new(None),
            closed: CancellationToken::new(),
        });
        owner.bind();
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
                    .and_then(|i| owner.recipients.borrow().get(i as usize).cloned());
                owner.settings.borrow_mut().selected = selected;
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
        let weak = Rc::downgrade(self);
        self.view.save.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.save_settings();
            }
        });
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
        self.view.save_draft.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.save_draft();
            }
        });
        let weak = Rc::downgrade(self);
        self.view.copy_draft.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.copy)(&owner.draft_text());
            }
        });
        let press = gtk::GestureClick::new();
        press.set_button(1);
        press.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        press.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(owner) = weak.upgrade() {
                owner.view.hold.add_css_class("agent-ui-held");
                (owner.callbacks.press)();
            }
        });
        let weak = Rc::downgrade(self);
        press.connect_released(move |_, _, _, _| {
            if let Some(owner) = weak.upgrade() {
                owner.view.hold.remove_css_class("agent-ui-held");
                (owner.callbacks.release)();
            }
        });
        let weak = Rc::downgrade(self);
        press.connect_cancel(move |_, _| {
            if let Some(owner) = weak.upgrade() {
                owner.view.hold.remove_css_class("agent-ui-held");
                owner.interrupt();
            }
        });
        self.view.hold.add_controller(press);
        let keyboard = gtk::EventControllerKey::new();
        let held = Rc::new(Cell::new(false));
        let pressed = held.clone();
        let focused_hold = held.clone();
        let weak = Rc::downgrade(self);
        keyboard.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::space {
                if !pressed.replace(true)
                    && let Some(owner) = weak.upgrade()
                {
                    owner.view.hold.add_css_class("agent-ui-held");
                    (owner.callbacks.press)();
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = Rc::downgrade(self);
        keyboard.connect_key_released(move |_, key, _, _| {
            if key == gtk::gdk::Key::space
                && held.replace(false)
                && let Some(owner) = weak.upgrade()
            {
                owner.view.hold.remove_css_class("agent-ui-held");
                (owner.callbacks.release)();
            }
        });
        self.view.hold.add_controller(keyboard);
        let weak = Rc::downgrade(self);
        self.view.hold.connect_has_focus_notify(move |button| {
            if !button.has_focus()
                && let Some(owner) = weak.upgrade()
            {
                focused_hold.set(false);
                // Only the on-screen gesture loses focus; global holds do not require this widget.
                if owner.view.hold.has_css_class("agent-ui-held") {
                    owner.view.hold.remove_css_class("agent-ui-held");
                    owner.interrupt();
                }
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
    fn save_settings(self: &Rc<Self>) {
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
        if let Err(error) = proposed.save(
            &self.services.paths.config.join("agent-voice.json"),
            &self.services.config().global_recording_key,
        ) {
            self.view.settings_status.set_label(error);
            return;
        }
        self.interrupt();
        self.settings.replace(proposed);
        self.generation.set(self.generation.get() + 1);
        self.syncing.set(true);
        self.view
            .mute
            .set_active(!self.settings.borrow().spoken_updates);
        self.syncing.set(false);
        self.view.settings_status.set_label("");
        self.view.options.popdown();
        (self.callbacks.settings_changed)();
        self.refresh();
        self.render();
    }
    fn selected(&self) -> Option<Recipient> {
        let settings = self.settings.borrow();
        let selected = settings.selected.as_ref()?;
        self.recipients
            .borrow()
            .iter()
            .find(|r| r.same_instance(selected))
            .cloned()
    }
    fn draft_text(&self) -> String {
        self.editor
            .borrow()
            .as_ref()
            .map_or_else(String::new, |editor| editor.text())
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
        let text = self.draft_text();
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
            return Err("Enable agent routing in Herdr voice settings first.");
        }
        let recipient = self.selected().ok_or("Choose an available Herdr agent.")?;
        let id = self.route.borrow_mut().begin(recipient, now())?;
        self.view.reply.set_label("");
        self.spoken_attempt.borrow_mut().take();
        self.render();
        Ok(id)
    }
    pub fn leave_for_capture(&self) {
        self.disarm(DraftReason::Interrupted);
        if self
            .route
            .borrow()
            .attempt
            .as_ref()
            .is_none_or(|a| a.phase != RoutePhase::Submitting)
        {
            self.route.borrow_mut().attempt = None;
        }
        self.page.refresh_output_visibility();
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
    pub fn completed(self: &Rc<Self>, id: &str, text: &str, history: Option<String>) {
        if !self.route.borrow_mut().recognized(id, text, history, now()) {
            return;
        }
        self.attach_editor(id);
        self.page.output_section.set_visible(false);
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
        self.disarm(DraftReason::Interrupted);
        self.stop_voice();
        (self.callbacks.interrupt)();
    }
    pub fn shortcut_binding(&self, hold: Option<&str>, draft: Option<&str>) {
        self.view.shortcut_status.set_label(&match (hold, draft) {
            (Some(hold), Some(draft)) => format!("Hold {hold} to talk · {draft} keeps a draft"),
            _ => "Global shortcuts are not approved. Use the on-screen Hold to talk button.".into(),
        });
        if hold.is_none() {
            self.interrupt();
        }
    }
    fn tick(self: &Rc<Self>) {
        let time = now();
        let previous = self.last_tick.replace(time);
        if time < previous
            || time.saturating_sub(previous) > 1_000
            || self.route.borrow().lost_release(time)
        {
            self.interrupt();
        }
        if self.settings.borrow().enabled && time.saturating_sub(self.last_poll.get()) > 2_000 {
            self.refresh();
        }
        let submission = self.route.borrow_mut().take_submission(time);
        if let Some(submission) = submission {
            let weak = Rc::downgrade(self);
            let cancel = self.closed.clone();
            self.runtime.spawn(async move {
                let outcome = herdr::submit(&submission, &cancel).await;
                if let Some(owner) = weak.upgrade().filter(|o| !o.closed.is_cancelled()) {
                    if owner
                        .route
                        .borrow_mut()
                        .delivered(&submission.id, outcome, now())
                        && outcome == SendOutcome::Accepted
                    {
                        owner.view.reply.set_label(&format!(
                            "{} accepted your message. Completion is not confirmed.",
                            submission.recipient.name
                        ));
                    }
                    owner.render();
                }
            });
        }
        self.render();
    }
    pub fn refresh(self: &Rc<Self>) {
        if !self.settings.borrow().enabled || self.polling.replace(true) {
            return;
        }
        self.last_poll.set(now());
        let generation = self.generation.get();
        let paths = herdr::socket_candidates(
            self.services
                .paths
                .config
                .parent()
                .unwrap_or(Path::new("/")),
            self.settings.borrow().socket.as_deref(),
        );
        let weak = Rc::downgrade(self);
        let cancel = self.closed.clone();
        self.runtime.spawn(async move {
            let mut recipients = Vec::new(); let mut compatible = false; let mut error = None;
            for path in paths.into_iter().take(16) {
                let result = tokio::select! { _ = cancel.cancelled() => return, result = herdr::discover(&path) => result };
                match result { Ok(found) => { compatible |= found.guarded; recipients.extend(found.recipients); }, Err(e) => error = Some(e.to_string()) }
            }
            if let Some(owner) = weak.upgrade() {
                owner.polling.set(false);
                if owner.generation.get() != generation || owner.closed.is_cancelled() { return; }
                let message = if !compatible && !recipients.is_empty() { "Herdr needs the guarded prompt integration. Sending is unavailable.".into() }
                    else if recipients.is_empty() { error.unwrap_or_else(|| "No available agents. Start a named Claude Code or Codex session in local Herdr, then refresh.".into()) }
                    else { "Local Herdr · release to finish, then two seconds to cancel".into() };
                owner.connection.replace(message); owner.update_recipients(recipients);
            }
        });
    }
    fn update_recipients(self: &Rc<Self>, mut recipients: Vec<Recipient>) {
        recipients.sort_by(|a, b| {
            (&a.name, &a.socket, &a.pane_id).cmp(&(&b.name, &b.socket, &b.pane_id))
        });
        let previous = self.selected();
        self.route.borrow_mut().observe(&recipients);
        let selected = self.settings.borrow().selected.clone();
        if let Some(selected) = &selected
            && !recipients.iter().any(|r| r.same_instance(selected))
        {
            let mut missing = selected.clone();
            missing.status = AgentStatus::Unknown;
            recipients.push(missing);
        }
        let changed = *self.recipients.borrow() != recipients;
        self.recipients.replace(recipients);
        if changed {
            let recipients = self.recipients.borrow();
            let mut labels = vec!["Choose a Herdr agent".to_owned()];
            labels.extend(recipients.iter().map(|r| {
                format!(
                    "{} · {} · {} · {} ({:?}) · Local {}",
                    r.name,
                    r.kind,
                    Path::new(&r.project)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Local"),
                    r.pane_id,
                    r.status,
                    r.socket
                )
            }));
            let strings: Vec<_> = labels.iter().map(String::as_str).collect();
            let index = selected
                .as_ref()
                .and_then(|s| recipients.iter().position(|r| r.same_instance(s)))
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
                .filter(|a| a.phase == RoutePhase::Sent && a.recipient.same_instance(&current))
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
        if let Some(recipient) = self.selected() {
            self.view.reply.set_label(&recipient.spoken_status());
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
        let enabled = self.settings.borrow().enabled;
        self.view.widget.set_visible(enabled);
        let attempt = self.route.borrow().attempt.clone();
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
        let unconfirmed = attempt
            .as_ref()
            .is_some_and(|a| a.phase == RoutePhase::Unconfirmed);
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
            .set_sensitive(draft && enabled && self.selected().is_some_and(|r| r.can_receive()));
        self.view.save_draft.set_visible(
            attempt.as_ref().is_some_and(|a| a.history_id.is_some())
                && !self.services.config().incognito_mode,
        );
        self.view.feedback.set_visible(enabled);
        self.view.hold.set_sensitive(
            enabled
                && self.selected().is_some_and(|r| r.can_receive())
                && !attempt.as_ref().is_some_and(|a| {
                    matches!(a.phase, RoutePhase::Finalizing | RoutePhase::Submitting)
                }),
        );
        self.view.recipients.set_sensitive(enabled);
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
                RoutePhase::Recording => format!("Talking to {} · release to finish", a.recipient.name),
                RoutePhase::Finalizing => format!("Finishing your message to {}…", a.recipient.name),
                RoutePhase::Grace {deadline_ms} => format!("Sending to {} in {:.1} s", a.recipient.name, deadline_ms.saturating_sub(now()) as f64/1000.0),
                RoutePhase::Submitting => format!("Submitting to {}… Cancel can no longer recall this message.", a.recipient.name),
                RoutePhase::Sent => format!("Sent to {} · agent completion is separate", a.recipient.name),
                RoutePhase::Unconfirmed => "Delivery unconfirmed. Check Herdr before sending again; your draft is here.".into(),
                RoutePhase::Draft(reason) => match reason {
                    DraftReason::Edited | DraftReason::Kept => "Draft kept. Automatic sending is off; edit or copy your text.".into(),
                    DraftReason::Empty => "No words recognized. Nothing was sent.".into(),
                    DraftReason::TooLong => "Message is too long. Split it into smaller messages; nothing was sent.".into(),
                    DraftReason::Cancelled => "Cancelled. Nothing was sent.".into(),
                    DraftReason::RecipientChanged | DraftReason::Unavailable | DraftReason::Rejected => "Recipient changed or unavailable. Nothing was submitted; your text stays here.".into(),
                    DraftReason::Interrupted => "Automatic sending stopped after an interruption. Your text stays here.".into(),
                },
            },
            None if !enabled => "Enable routing in Herdr voice settings.".into(),
            None => self.connection.borrow().clone(),
        };
        let status = if !pending {
            self.voice_error.borrow().clone().unwrap_or(status)
        } else {
            status
        };
        self.view.recipients.set_tooltip_text(
            self.selected()
                .map(|r| {
                    format!(
                        "Local · {} · {} · {} · {}",
                        r.name, r.kind, r.project, r.socket
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
