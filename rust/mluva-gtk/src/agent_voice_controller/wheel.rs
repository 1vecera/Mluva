use super::*;
use crate::agent_session_wheel::KEYS;

impl AgentVoiceController {
    pub fn shortcut(&self) {
        (self.callbacks.select)();
    }
    pub fn open_wheel(self: &Rc<Self>, parent: &adw::ApplicationWindow) {
        if !self.settings.borrow().enabled || (self.callbacks.capture_busy)() {
            return;
        }
        self.disarm(DraftReason::Kept);
        self.stop_voice();
        // A missing remembered target stays selected and unavailable; never retarget it.
        let initial = self
            .settings
            .borrow()
            .selected
            .is_none()
            .then(|| {
                self.recipients
                    .borrow()
                    .iter()
                    .position(Recipient::can_receive)
            })
            .flatten();
        if let Some(index) = initial {
            self.view.recipients.set_selected(index as u32 + 1);
        }
        self.wheel.page.set(
            self.settings
                .borrow()
                .selected
                .as_ref()
                .and_then(|s| {
                    self.recipients
                        .borrow()
                        .iter()
                        .position(|r| r.same_instance(s))
                })
                .unwrap_or(0)
                / KEYS.len(),
        );
        // An overlay belongs to no window; an ordinary wheel stays attached to Mluva.
        if !self.wheel.overlay {
            self.wheel.window.set_transient_for(Some(parent));
        }
        // Fill the wheel before it maps, so a window manager places it at its real size.
        self.fill_wheel();
        self.wheel.window.present();
        self.wheel.record.grab_focus();
        self.refresh();
    }
    pub fn take_transcript(&self) -> bool {
        if !self.wheel.window.is_visible() {
            return false;
        }
        self.wheel.window.set_visible(false);
        true
    }
    fn choose_wheel(&self, slot: usize) {
        let target = self
            .wheel
            .entries
            .borrow()
            .get(self.wheel.page.get() * KEYS.len() + slot)
            .cloned();
        if let Some(target) = target {
            let index = self
                .recipients
                .borrow()
                .iter()
                .position(|r| r.same_instance(&target));
            if let Some(index) = index {
                self.view.recipients.set_selected(index as u32 + 1);
            }
        }
    }
    fn page_wheel(&self, forward: bool) {
        let page = self.wheel.page.get();
        let pages = self
            .wheel
            .entries
            .borrow()
            .len()
            .div_ceil(KEYS.len())
            .max(1);
        self.wheel.page.set(if forward {
            (page + 1).min(pages - 1)
        } else {
            page.saturating_sub(1)
        });
        self.render_wheel();
    }
    pub(super) fn render_wheel(&self) {
        if self.wheel.window.is_visible() {
            self.fill_wheel();
        }
    }
    fn fill_wheel(&self) {
        let agent_key = self
            .approved_key
            .borrow()
            .clone()
            .unwrap_or_else(|| self.settings.borrow().hold_key.clone());
        let transcript_key = self
            .page
            .shortcut_state()
            .recording_trigger
            .unwrap_or_else(|| self.services.config().global_recording_key);
        self.wheel.update(
            &self.recipients.borrow(),
            self.selected(),
            &agent_key,
            &transcript_key,
        );
    }
    pub(super) fn bind_wheel(self: &Rc<Self>) {
        for (slot, button) in self.wheel.choices.iter().enumerate() {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(owner) = weak.upgrade() {
                    owner.choose_wheel(slot);
                }
            });
        }
        for (button, forward) in [(&self.wheel.previous, false), (&self.wheel.next, true)] {
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(owner) = weak.upgrade() {
                    owner.page_wheel(forward);
                }
            });
        }
        let weak = Rc::downgrade(self);
        self.wheel.record.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.select)();
            }
        });
        let weak = Rc::downgrade(self);
        self.wheel.transcript.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                (owner.callbacks.transcript)();
            }
        });
        let weak = Rc::downgrade(self);
        self.wheel.cancel.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.wheel.window.set_visible(false);
            }
        });
        let weak = Rc::downgrade(self);
        self.wheel.connect_resized(move || {
            if let Some(owner) = weak.upgrade() {
                owner.render_wheel();
            }
        });
        if self.wheel.overlay {
            // Clicking the scrim, anywhere outside a control, dismisses the overlay.
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(self);
            click.connect_released(move |_, _, x, y| {
                let Some(owner) = weak.upgrade() else {
                    return;
                };
                let window = &owner.wheel.window;
                let on_control = window
                    .pick(x, y, gtk::PickFlags::DEFAULT)
                    .is_some_and(|widget| {
                        widget.is::<gtk::Button>()
                            || widget.ancestor(gtk::Button::static_type()).is_some()
                    });
                if !on_control {
                    window.set_visible(false);
                }
            });
            self.wheel.window.add_controller(click);
        }
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let held = Rc::new(Cell::new(false));
        let hidden = held.clone();
        self.wheel.window.connect_hide(move |_| hidden.set(false));
        let released = held.clone();
        keys.connect_key_released(move |_, _, _, _| released.set(false));
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(owner) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if modifiers.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            if key == gtk::gdk::Key::Escape {
                owner.wheel.window.set_visible(false);
            } else if key == gtk::gdk::Key::Left {
                owner.page_wheel(false);
            } else if key == gtk::gdk::Key::Right {
                owner.page_wheel(true);
            } else if let Some(slot) = key
                .to_unicode()
                .and_then(|c| KEYS.iter().position(|k| *k == c.to_ascii_lowercase()))
            {
                owner.choose_wheel(slot);
            } else if key.name().as_deref() == Some(owner.settings.borrow().hold_key.as_str()) {
                // The approved global action owns this key. Local fallback is only
                // for an unapproved portal, avoiding two handlers for one press.
                if owner.approved_key.borrow().is_none() && !held.replace(true) {
                    (owner.callbacks.select)();
                }
            } else if key.name().as_deref()
                == Some(owner.services.config().global_recording_key.as_str())
            {
                if owner.page.shortcut_state().recording_trigger.is_none() && !held.replace(true) {
                    (owner.callbacks.transcript)();
                }
            } else {
                return glib::Propagation::Proceed;
            }
            glib::Propagation::Stop
        });
        self.wheel.window.add_controller(keys);
    }
}
