//! Screen annotation ownership: the opt-in, one session per recording, and the recording's bundle
//! until it belongs to a conversation.
use super::*;
use crate::screen_annotation::{
    AnnotationSession, hyprland,
    session::{FRAMES_PREFIX, remove_stale_frames},
};
use mluva_annotate::{bundle, model::Word, timing};
use mluva_core::annotations::{AnnotationModifier, AnnotationSettings, AnnotationStore};
use mluva_providers::TranscriptionResult;
use mluva_workflows::{capture::CapturePhase, dictation::DeliveryAppendix};

pub(super) struct AnnotationOwners {
    pub settings: RefCell<AnnotationSettings>,
    pub session: RefCell<Option<Rc<AnnotationSession>>>,
    /// This run's GlobalShortcuts session approved the ink and pin actions.
    pub portal: Cell<bool>,
    pub row: adw::SwitchRow,
    pub modifier: adw::ComboRow,
    pub group: adw::PreferencesGroup,
}
impl AnnotationOwners {
    pub fn new(services: &ApplicationServices) -> (Self, Option<&'static str>) {
        let (settings, error) =
            match AnnotationSettings::load(&services.paths.config.join(AnnotationSettings::FILE)) {
                Ok(settings) => (settings, None),
                Err(error) => (AnnotationSettings::default(), Some(error)),
            };
        let row = adw::SwitchRow::builder()
            .title("Annotate the screen while dictating")
            .subtitle("Experimental. On Hyprland, hold the selected key and move to draw; click while holding it to place a pin. Off in Incognito.")
            .active(settings.enabled)
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Screen annotation")
            .description("Changes save automatically and apply to the next recording.")
            .build();
        group.add(&row);
        let modifier = adw::ComboRow::builder()
            .title("Drawing key")
            .subtitle("Hold this key to draw or place a pin")
            .model(&gtk::StringList::new(&["Shift", "Control", "Alt", "Super"]))
            .selected(modifier_index(settings.modifier))
            .build();
        group.add(&modifier);
        (
            Self {
                settings: RefCell::new(settings),
                session: RefCell::new(None),
                portal: Cell::new(false),
                row,
                modifier,
                group,
            },
            error,
        )
    }
}

const MODIFIERS: [AnnotationModifier; 4] = [
    AnnotationModifier::Shift,
    AnnotationModifier::Control,
    AnnotationModifier::Alt,
    AnnotationModifier::Super,
];

fn modifier_index(modifier: AnnotationModifier) -> u32 {
    MODIFIERS
        .iter()
        .position(|value| *value == modifier)
        .unwrap_or(0) as u32
}

fn words(transcription: &TranscriptionResult) -> Vec<Word> {
    transcription
        .words
        .iter()
        .filter(|word| word.start_seconds.is_finite() && word.end_seconds.is_finite())
        .map(|word| Word {
            t0: (word.start_seconds.max(0.0) * 1000.0).round() as u64,
            t1: (word.end_seconds.max(word.start_seconds).max(0.0) * 1000.0).round() as u64,
            text: word.text.clone(),
        })
        .collect()
}

/// Removes a bundle whose writing did not finish, for example when Mluva quits mid-write, so
/// startup never recovers a summary that was not delivered.
struct Unfinished {
    store: AnnotationStore,
    identifier: Option<String>,
}
impl Drop for Unfinished {
    fn drop(&mut self) {
        if let Some(identifier) = self.identifier.take() {
            let _ = self.store.delete(&identifier);
        }
    }
}

/// Whether Hyprland may receive the temporary bindings: only once this run's portal session
/// has registered the actions they dispatch to. Other desktops get no bindings at all.
pub(super) fn may_bind(hyprland: bool, portal_registered: bool) -> bool {
    !hyprland || portal_registered
}

impl ApplicationDesktop {
    pub(super) fn annotation_enabled(&self) -> bool {
        self.annotations.settings.borrow().enabled
    }

    /// The portal registers the ink and pin actions only where they can be bound.
    pub(super) fn annotation_shortcuts_wanted(&self) -> bool {
        self.annotation_enabled() && hyprland::available()
    }

    /// Remove what a killed Mluva and its watchdog left behind: this desktop's annotation
    /// bindings and raw-frame directories. The current recording's frames stay.
    pub(super) fn clean_annotation_leftovers(&self) {
        let runtime = &self.services.paths.runtime;
        let current = self
            .annotations
            .session
            .borrow()
            .as_ref()
            .map(|session| session.frames_directory().to_owned());
        let stale = std::fs::read_dir(runtime).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(FRAMES_PREFIX)
                    && Some(entry.path()) != current
            })
        });
        remove_stale_frames(runtime, current.as_deref());
        // Hyprland is only touched where annotation is on or an earlier session left frames.
        if (self.annotation_enabled() || stale) && current.is_none() && hyprland::available() {
            self.runtime.spawn(async {
                hyprland::unbind_all().await;
            });
        }
    }

    pub(super) fn set_annotation_enabled(self: &Rc<Self>, enabled: bool) {
        if self.annotation_enabled() == enabled {
            return;
        }
        let mut updated = self.annotations.settings.borrow().clone();
        updated.enabled = enabled;
        if let Err(message) =
            updated.save(&self.services.paths.config.join(AnnotationSettings::FILE))
        {
            self.shell.show_message(message);
            self.annotations.row.set_active(!enabled);
            return;
        }
        self.annotations.settings.replace(updated);
        if !enabled {
            if let Some(session) = self.annotations.session.borrow().as_ref() {
                // Marks already drawn stay with this recording; drawing stops now.
                session.stop();
            }
            remove_stale_frames(
                &self.services.paths.runtime,
                self.annotations
                    .session
                    .borrow()
                    .as_ref()
                    .map(|session| session.frames_directory()),
            );
            if hyprland::available() {
                self.runtime.spawn(async {
                    hyprland::unbind_all().await;
                });
            }
        }
        self.restart_shortcuts();
    }

    pub(super) fn set_annotation_modifier(&self, selected: u32) {
        let Some(modifier) = MODIFIERS.get(selected as usize).copied() else {
            return;
        };
        let mut updated = self.annotations.settings.borrow().clone();
        if updated.modifier == modifier {
            return;
        }
        let previous = updated.modifier;
        updated.modifier = modifier;
        if let Err(message) =
            updated.save(&self.services.paths.config.join(AnnotationSettings::FILE))
        {
            self.shell.show_message(message);
            self.annotations
                .modifier
                .set_selected(modifier_index(previous));
            return;
        }
        self.annotations.settings.replace(updated);
    }

    /// Start annotating the current recording when it began capturing audio.
    pub(super) fn start_annotation(self: &Rc<Self>) {
        if !self.annotation_enabled() || self.annotations.session.borrow().is_some() {
            return;
        }
        let current = self.current.borrow();
        let Some(current) = current.as_ref() else {
            return;
        };
        // The phone recorder is used away from this screen.
        if current
            .options
            .application_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("mluva-web:"))
        {
            return;
        }
        if current.options.incognito || self.services.config().incognito_mode {
            self.page()
                .set_status("Screen annotation is off in Incognito. Recording continues.");
            return;
        }
        if !may_bind(hyprland::available(), self.annotations.portal.get()) {
            self.page().set_status(
                "Screen annotation needs Mluva's desktop shortcuts to be approved. Recording continues without it.",
            );
            return;
        }
        let capture = Rc::downgrade(&current.session);
        let owner = current.session.identifier.clone();
        let weak = Rc::downgrade(self);
        let session = AnnotationSession::new(
            self.runtime.clone(),
            &owner,
            &self.services.paths.runtime,
            &self.binaries.asr_worker.with_file_name("mluva"),
            self.annotations.settings.borrow().modifier,
            Rc::new(move || {
                capture
                    .upgrade()
                    .map(|session| (session.recorded_seconds() * 1000.0).round() as u64)
            }),
            Rc::new(move |message| {
                if let Some(app) = weak.upgrade().filter(|app| !app.closed.get()) {
                    app.page().set_status(message);
                }
            }),
        );
        match session {
            Ok(session) => {
                self.annotations.session.replace(Some(session.clone()));
                self.runtime.spawn(session.activate());
            }
            Err(message) => self.page().set_status(&message),
        }
    }

    pub(super) fn annotation_ink(&self, down: bool) {
        if let Some(session) = self.annotations.session.borrow().clone() {
            session.ink(down);
        }
    }

    pub(super) fn annotation_pin(&self) {
        if let Some(session) = self.annotations.session.borrow().clone() {
            session.pin();
        }
    }

    pub(super) fn annotation_preview(&self) {
        let Some(session) = self.annotations.session.borrow().clone() else {
            return;
        };
        let words = self
            .current
            .borrow()
            .as_ref()
            .filter(|current| current.session.phase() == CapturePhase::Recording)
            .and_then(|current| current.session.preview())
            .map(|preview| preview.display_text().split_whitespace().count());
        if let Some(words) = words {
            session.preview(words);
        }
    }

    /// At Stop: end drawing at once. A recording without marks adds nothing and waits for
    /// nothing. Otherwise, once recognition is final, write the bundle and return its summary.
    pub(super) fn annotation_appendix(self: &Rc<Self>, capture: &str) -> Option<DeliveryAppendix> {
        let session = {
            let mut owned = self.annotations.session.borrow_mut();
            if owned
                .as_ref()
                .is_none_or(|session| session.owner != capture)
            {
                return None;
            }
            owned.take()?
        };
        session.stop();
        if !session.has_marks() {
            return None;
        }
        let database = self.services.history.database.path.clone();
        let owner = capture.to_owned();
        let weak = Rc::downgrade(self);
        Some(Box::new(move |transcription: &TranscriptionResult| {
            // Quotes describe what was said, so they come from raw recognition.
            let text = transcription.text.clone();
            let words = words(transcription);
            Box::pin(async move {
                let notice = |message: &str| {
                    if let Some(app) = weak.upgrade().filter(|app| !app.closed.get()) {
                        app.shell.show_message(message);
                    }
                };
                let capture = session.collect().await;
                if !capture.has_marks() {
                    return None;
                }
                let transcript =
                    timing::transcript(&text, Some(words), &capture.preview, capture.duration_ms);
                let store = AnnotationStore::new(&database);
                let Ok(annotation) = store.create(&owner) else {
                    notice(
                        "Could not save the screen annotation. The dictation was delivered without it.",
                    );
                    return None;
                };
                let mut unfinished = Unfinished {
                    store: store.clone(),
                    identifier: Some(annotation.identifier.clone()),
                };
                let directory = annotation.directory.clone();
                let written = tokio::task::spawn_blocking(move || {
                    bundle::write(&directory, &capture, &transcript)
                })
                .await;
                // Releases the captured frames and lets the watchdog finish.
                drop(session);
                match written {
                    Ok(Ok(written)) => {
                        unfinished.identifier.take();
                        let _ = store.set_keyframes(&annotation.identifier, written.frames.len());
                        Some(written.summary)
                    }
                    Ok(Err(error)) => {
                        notice(&format!(
                            "{error} The dictation was delivered without screen annotation."
                        ));
                        None
                    }
                    Err(_) => {
                        notice(
                            "Could not draw the screen annotation. The dictation was delivered without it.",
                        );
                        None
                    }
                }
            })
        }))
    }

    /// Give a finished recording's bundle to its conversation. Without one, because History
    /// could not be saved, the bundle stays with its recording for recovery at the next start:
    /// its summary may already have been delivered.
    pub(super) fn finish_annotations(&self, capture: &str, conversation: Option<&str>) {
        let Some(conversation) = conversation else {
            return;
        };
        let store = AnnotationStore::new(&self.services.history.database.path);
        if store.bind_capture(capture, conversation).is_err() {
            self.shell.show_message(
                "Could not save screen annotation ownership. It will be recovered at the next start.",
            );
        }
    }

    /// Cancel or failure before recording: end the session and drop any bundle it owns.
    pub(super) fn discard_annotation(&self, capture: &str) {
        self.stop_annotation(capture);
        let _ =
            AnnotationStore::new(&self.services.history.database.path).delete_owner(capture, true);
    }

    /// End the recording's session if it is still drawing.
    pub(super) fn stop_annotation(&self, capture: &str) {
        let session = {
            let mut owned = self.annotations.session.borrow_mut();
            owned
                .as_ref()
                .filter(|session| session.owner == capture)
                .is_some()
                .then(|| owned.take())
                .flatten()
        };
        if let Some(session) = session {
            session.stop();
        }
    }

    pub(super) fn close_annotation(&self) {
        if let Some(session) = self.annotations.session.borrow_mut().take() {
            session.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::may_bind;

    #[test]
    fn hyprland_bindings_wait_for_the_portal_to_register_their_actions() {
        // Disabled shortcuts, a missing or failed portal, or a pending approval: no bindings.
        assert!(!may_bind(true, false));
        assert!(may_bind(true, true));
        // Elsewhere nothing is bound, so nothing can be swallowed.
        assert!(may_bind(false, false));
    }
}
