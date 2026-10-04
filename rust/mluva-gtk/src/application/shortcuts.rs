//! Couple real portal ownership to readiness, settings and capture origins.
use super::*;
use crate::{capture_view::CaptureShortcutState, global_shortcuts::ShortcutCallbacks};
use std::future::Future;

fn current(
    weak: &Weak<ApplicationDesktop>,
    generation: u64,
    call: impl FnOnce(&Rc<ApplicationDesktop>),
) {
    if let Some(owner) = weak
        .upgrade()
        .filter(|owner| !owner.closed.get() && owner.shortcut_generation.get() == generation)
    {
        call(&owner);
    }
}

impl ApplicationDesktop {
    pub(super) fn start_shortcuts(self: &Rc<Self>) {
        if std::env::var_os("MLUVA_DISABLE_GLOBAL_SHORTCUT").is_some() {
            self.settings
                .capture
                .shortcut_status
                .set_subtitle("Disabled for this Mluva process");
            self.refresh_shortcuts();
            return;
        }
        let generation = self.shortcut_generation.get();
        let weak = Rc::downgrade(self);
        let dispatch = |action: ApplicationAction| {
            let weak = weak.clone();
            Rc::new(move || current(&weak, generation, |owner| owner.dispatch(action.clone())))
                as Rc<dyn Fn()>
        };
        let recording = weak.clone();
        let rewrite = weak.clone();
        let errors = weak.clone();
        let service = GlobalShortcutService::new(
            self.runtime.clone(),
            &self.services.config().global_recording_key,
            ShortcutCallbacks {
                toggle_recording: dispatch(ApplicationAction::GlobalRecord),
                cancel: dispatch(ApplicationAction::Cancel),
                open_rewrite: dispatch(ApplicationAction::Latest),
                binding_changed: Rc::new(move |key, trigger| {
                    current(&recording, generation, |owner| {
                        owner.recording_binding(key, trigger)
                    })
                }),
                rewrite_binding_changed: Rc::new(move |trigger| {
                    current(&rewrite, generation, |owner| owner.rewrite_binding(trigger))
                }),
                error: Rc::new(move |message| {
                    current(&errors, generation, |owner| {
                        owner
                            .page()
                            .set_status(&format!("Global shortcut unavailable: {message}"))
                    })
                }),
            },
        );
        match service {
            Ok(service) => {
                self.shortcuts.replace(Some(service.clone()));
                self.refresh_shortcuts();
                service.start();
            }
            Err(error) => self
                .page()
                .set_status(&format!("Global shortcut unavailable: {error}")),
        }
    }

    fn recording_binding(&self, key: &str, trigger: Option<&str>) {
        let preferred = self.services.config().global_recording_key;
        if key != preferred {
            return;
        }
        self.approved_recording.replace(trigger.map(str::to_owned));
        let message = match trigger {
            None => format!(
                "{preferred} is not currently approved; the on-screen copy-only button remains available"
            ),
            Some(trigger) if trigger.eq_ignore_ascii_case(&preferred) => {
                format!("Ready — press {trigger} once to start and again to stop")
            }
            Some(trigger) => format!(
                "Ready — the desktop assigned {trigger}; the saved preference is {preferred}"
            ),
        };
        self.settings.capture.shortcut_status.set_subtitle(&message);
        self.refresh_shortcuts();
    }

    fn rewrite_binding(&self, trigger: Option<&str>) {
        self.approved_rewrite.replace(trigger.map(str::to_owned));
        self.settings
            .capture
            .latest_shortcut_status
            .set_subtitle(&trigger.map_or_else(
                || {
                    "Shift+F9 is not approved. Open the latest conversation from the shell menu."
                        .into()
                },
                |trigger| format!("{trigger} opens the latest conversation"),
            ));
        self.refresh_shortcuts();
    }

    fn refresh_shortcuts(&self) {
        let available = self.shortcuts.borrow().is_some();
        self.settings.capture.set_shortcuts_available(available);
        self.page().set_shortcuts(CaptureShortcutState {
            recording_trigger: self.approved_recording.borrow().clone(),
            rewrite_trigger: self.approved_rewrite.borrow().clone(),
            shortcut_service_available: available,
            target_tracking_available: self.tracker.borrow().is_some(),
        });
    }

    pub(super) fn rebind_shortcut(&self) {
        self.approved_recording.borrow_mut().take();
        self.refresh_shortcuts();
        if let Some(service) = self.shortcuts.borrow().as_ref()
            && let Err(error) =
                service.set_recording_key(&self.services.config().global_recording_key)
        {
            self.page()
                .set_status(&format!("Global shortcut unavailable: {error}"));
        }
    }

    /// Invalidate callbacks synchronously; the root keeps GLib alive until this
    /// cleanup future and its queued approvals have acknowledged cancellation.
    pub(super) fn close_shortcuts(&self) -> Option<impl Future<Output = ()> + use<>> {
        self.shortcut_generation
            .set(self.shortcut_generation.get() + 1);
        self.approved_recording.borrow_mut().take();
        let service = self.shortcuts.borrow_mut().take();
        self.refresh_shortcuts();
        service.map(|service| service.shutdown())
    }
}
