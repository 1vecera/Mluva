//! Native, keyboard-first session wheel; routing remains in the application owner.
use adw::prelude::*;
use mluva_core::agent_voice::Recipient;
use std::cell::{Cell, RefCell};

pub const KEYS: [char; 6] = ['a', 's', 'd', 'j', 'k', 'l'];

pub struct SessionWheel {
    pub window: adw::Window,
    pub choices: Vec<gtk::Button>,
    layout: gtk::Fixed,
    pub previous: gtk::Button,
    pub next: gtk::Button,
    pub record: gtk::Button,
    pub transcript: gtk::Button,
    pub cancel: gtk::Button,
    pub heading: gtk::Label,
    pub hint: gtk::Label,
    pub center: gtk::Label,
    pub page_label: gtk::Label,
    pub page: Cell<usize>,
    rendered_page: Cell<usize>,
    rendered_keys: RefCell<(String, String)>,
    pub entries: RefCell<Vec<Recipient>>,
    pub selected: RefCell<Option<Recipient>>,
}
impl SessionWheel {
    pub fn new() -> Self {
        let window = adw::Window::builder()
            .title("Choose a Herdr session")
            .default_width(420)
            .default_height(520)
            .resizable(false)
            .modal(true)
            .hide_on_close(true)
            .build();
        window.add_css_class("ml-session-wheel");
        // This new surface owns its foreground override; the released shared
        // stylesheet and other windows keep their existing native contract.
        let style = gtk::CssProvider::new();
        style.load_from_string(
            ".ml-session-wheel button, .ml-session-wheel button label { color: @vs_ink; }",
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &style,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        crate::document_layout::margins(&content, 12);
        let heading = gtk::Label::new(Some("Choose a session"));
        heading.add_css_class("title-2");
        content.append(&heading);
        let hint = gtk::Label::new(Some("Select with A S D J K L, then press the agent key"));
        hint.set_wrap(true);
        content.append(&hint);
        let wheel = gtk::Fixed::new();
        wheel.set_size_request(396, 326);
        let coordinates = [
            (130., 0.),
            (252., 64.),
            (252., 198.),
            (130., 264.),
            (8., 198.),
            (8., 64.),
        ];
        let mut choices = Vec::new();
        for (x, y) in coordinates {
            let button = gtk::Button::builder()
                .width_request(136)
                .height_request(62)
                .build();
            wheel.put(&button, x, y);
            choices.push(button);
        }
        let center = gtk::Label::builder()
            .wrap(true)
            .justify(gtk::Justification::Center)
            .width_request(100)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(3)
            .height_request(62)
            .max_width_chars(12)
            .build();
        wheel.put(&center, 148., 132.);
        content.append(&wheel);
        let pages = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        pages.set_halign(gtk::Align::Center);
        let previous = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Previous sessions (Left)")
            .build();
        let next = gtk::Button::builder()
            .icon_name("go-next-symbolic")
            .tooltip_text("Next sessions (Right)")
            .build();
        let page_label = gtk::Label::new(None);
        pages.append(&previous);
        pages.append(&page_label);
        pages.append(&next);
        content.append(&pages);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::Center);
        let record = gtk::Button::with_label("Record");
        record.add_css_class("suggested-action");
        let transcript = gtk::Button::with_label("F9 · Transcript");
        transcript.set_tooltip_text(Some(
            "Keep an editable transcript in Mluva without copying or sending",
        ));
        let cancel = gtk::Button::with_label("Cancel");
        actions.append(&record);
        actions.append(&transcript);
        actions.append(&cancel);
        content.append(&actions);
        window.set_content(Some(&content));
        Self {
            window,
            choices,
            layout: wheel,
            previous,
            next,
            record,
            transcript,
            cancel,
            heading,
            hint,
            center,
            page_label,
            page: Cell::new(0),
            rendered_page: Cell::new(usize::MAX),
            rendered_keys: RefCell::new((String::new(), String::new())),
            entries: RefCell::new(vec![]),
            selected: RefCell::new(None),
        }
    }
    pub fn update(
        &self,
        entries: &[Recipient],
        selected: Option<Recipient>,
        agent_key: &str,
        transcript_key: &str,
    ) {
        if self.entries.borrow().as_slice() == entries
            && *self.selected.borrow() == selected
            && self.rendered_page.get() == self.page.get()
            && self.rendered_keys.borrow().0 == agent_key
            && self.rendered_keys.borrow().1 == transcript_key
        {
            return;
        }
        self.rendered_page.set(self.page.get());
        self.rendered_keys
            .replace((agent_key.to_owned(), transcript_key.to_owned()));
        self.entries.replace(entries.to_vec());
        self.selected.replace(selected.clone());
        let pages = entries.len().div_ceil(KEYS.len()).max(1);
        self.page.set(self.page.get().min(pages - 1));
        self.page_label
            .set_label(&format!("{} / {pages}", self.page.get() + 1));
        self.previous.set_sensitive(self.page.get() > 0);
        self.next.set_sensitive(self.page.get() + 1 < pages);
        self.record.set_label(&format!("{agent_key} · Record"));
        self.transcript
            .set_label(&format!("{transcript_key} · Transcript"));
        self.hint
            .set_label("A S D J K L select · Left/Right change page · Esc cancels");
        self.center.set_label(&selected.as_ref().map_or_else(
            || "No session\nUse Transcript".into(),
            |r| {
                format!(
                    "{}\n{}",
                    r.pane_id,
                    if r.can_receive() {
                        "Ready to dictate"
                    } else {
                        "Unavailable"
                    }
                )
            },
        ));
        self.record
            .set_sensitive(selected.as_ref().is_some_and(Recipient::can_receive));
        self.center.set_tooltip_text(
            selected
                .as_ref()
                .map(|r| format!("{} · {}", r.name, r.pane_id))
                .as_deref(),
        );
        let count = entries
            .len()
            .saturating_sub(self.page.get() * KEYS.len())
            .min(KEYS.len())
            .max(1);
        for (slot, button) in self.choices.iter().enumerate() {
            let angle = slot as f64 * std::f64::consts::TAU / count as f64;
            self.layout.move_(
                button,
                130.0 + 130.0 * angle.sin(),
                132.0 - 132.0 * angle.cos(),
            );
            let entry = entries.get(self.page.get() * KEYS.len() + slot);
            button.set_visible(entry.is_some());
            if let Some(entry) = entry {
                let text = format!(
                    "{} · {}\n{}",
                    KEYS[slot].to_ascii_uppercase(),
                    entry.name,
                    entry.pane_id
                );
                let label = gtk::Label::builder()
                    .label(format!(
                        "{} · {}",
                        KEYS[slot].to_ascii_uppercase(),
                        entry.name
                    ))
                    .wrap(true)
                    .wrap_mode(gtk::pango::WrapMode::WordChar)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .lines(2)
                    .max_width_chars(12)
                    .build();
                // A long session name must not push its exact pane off the button.
                let pane = gtk::Label::new(Some(&entry.pane_id));
                pane.add_css_class("caption");
                let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                content.append(&label);
                content.append(&pane);
                button.set_child(Some(&content));
                button.set_tooltip_text(Some(&format!(
                    "{} · {} · {}",
                    entry.name, entry.pane_id, entry.project
                )));
                button.update_property(&[gtk::accessible::Property::Label(&text)]);
                if selected.as_ref().is_some_and(|s| s.same_instance(entry)) {
                    button.add_css_class("suggested-action");
                } else {
                    button.remove_css_class("suggested-action");
                }
            }
        }
    }
}
