//! Screen annotation ownership: the opt-in, one session per recording, and the recording's bundle
//! until it belongs to a conversation.
use super::*;
use crate::screen_annotation::{AnnotationSession, hyprland};
use mluva_annotate::{bundle, model::Word, timing};
use mluva_core::annotations::{AnnotationSettings, AnnotationStore};
use mluva_providers::TranscriptionResult;
use mluva_workflows::{capture::CapturePhase, dictation::DeliveryAppendix};

pub(super) struct AnnotationOwners {
    pub settings: RefCell<AnnotationSettings>,
    pub session: RefCell<Option<Rc<AnnotationSession>>>,
    pub row: adw::SwitchRow,
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
            .subtitle("Experimental. On Hyprland, while recording, hold Shift and move to draw; Shift-click drops a numbered pin. Marked recordings deliver keyframes and a short summary with the transcript. Off in Incognito.")
            .active(settings.enabled)
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Screen annotation")
            .description("Changes save automatically and apply to the next recording.")
            .build();
        group.add(&row);
        (
            Self {
                settings: RefCell::new(settings),
                session: RefCell::new(None),
                row,
                group,
            },
            error,
        )
    }
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

impl ApplicationDesktop {
    pub(super) fn annotation_enabled(&self) -> bool {
        self.annotations.settings.borrow().enabled
    }

    /// The portal registers the ink and pin actions only where they can be bound.
    pub(super) fn annotation_shortcuts_wanted(&self) -> bool {
        self.annotation_enabled() && hyprland::available()
    }

    pub(super) fn set_annotation_enabled(self: &Rc<Self>, enabled: bool) {
        if self.annotation_enabled() == enabled {
            return;
        }
        let updated = AnnotationSettings { enabled };
        if let Err(message) =
            updated.save(&self.services.paths.config.join(AnnotationSettings::FILE))
        {
            self.shell.show_message(message);
            self.annotations.row.set_active(!enabled);
            return;
        }
        self.annotations.settings.replace(updated);
        if !enabled && let Some(session) = self.annotations.session.borrow().as_ref() {
            // Marks already drawn stay with this recording; drawing stops now.
            session.stop();
        }
        self.restart_shortcuts();
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
        let capture = Rc::downgrade(&current.session);
        let owner = current.session.identifier.clone();
        let weak = Rc::downgrade(self);
        let session = AnnotationSession::new(
            self.runtime.clone(),
            &owner,
            &self.services.paths.runtime,
            &self.binaries.asr_worker.with_file_name("mluva"),
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

    /// At Stop: end drawing at once, and once recognition is final, write the bundle and
    /// return its summary for delivery. A recording without marks adds nothing.
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
        let database = self.services.history.database.path.clone();
        let owner = capture.to_owned();
        let weak = Rc::downgrade(self);
        Some(Box::new(move |transcription: &TranscriptionResult| {
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
                let directory = annotation.directory.clone();
                let written = tokio::task::spawn_blocking(move || {
                    bundle::write(&directory, &capture, &transcript)
                })
                .await;
                // Releases the captured frames and lets the watchdog finish.
                drop(session);
                match written {
                    Ok(Ok(written)) => {
                        let _ = store.set_keyframes(&annotation.identifier, written.frames.len());
                        Some(written.summary)
                    }
                    Ok(Err(error)) => {
                        let _ = store.delete(&annotation.identifier);
                        notice(&format!(
                            "{error} The dictation was delivered without screen annotation."
                        ));
                        None
                    }
                    Err(_) => {
                        let _ = store.delete(&annotation.identifier);
                        notice(
                            "Could not draw the screen annotation. The dictation was delivered without it.",
                        );
                        None
                    }
                }
            })
        }))
    }

    /// Give a completed recording's bundle to its conversation, or remove it with the recording.
    pub(super) fn finish_annotations(&self, capture: &str, conversation: Option<&str>) {
        let store = AnnotationStore::new(&self.services.history.database.path);
        let result = match conversation {
            Some(conversation) => store.bind_capture(capture, conversation),
            None => store.delete_owner(capture, true),
        };
        if result.is_err() {
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
