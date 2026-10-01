//! Search ordinary application commands and recheck their availability after dismissal.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use mluva_core::text;

#[derive(Clone)]
pub struct Command {
    pub title: String,
    pub icon: String,
    pub run: Rc<dyn Fn()>,
    pub enabled: Rc<dyn Fn() -> bool>,
    pub keywords: String,
    pub shortcut: String,
}

pub struct CommandPalette {
    pub dialog: adw::Dialog,
    pub search: gtk::SearchEntry,
    pub results: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    empty: gtk::Label,
    commands: Vec<Command>,
    rows: RefCell<Vec<(gtk::ListBoxRow, Command)>>,
    pending: RefCell<Option<Command>>,
}

impl CommandPalette {
    pub fn new(commands: Vec<Command>) -> Rc<Self> {
        let dialog = adw::Dialog::builder()
            .title("Commands")
            .content_width(460)
            .content_height(500)
            .build();
        dialog.add_css_class("ml-command-palette");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search actions…")
            .hexpand(true)
            .search_delay(0)
            .build();
        search.update_property(&[gtk::accessible::Property::Label("Search actions")]);
        content.append(&search);
        let results = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .build();
        results.add_css_class("ml-command-results");
        let scroll = gtk::ScrolledWindow::builder()
            .child(&results)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .overlay_scrolling(false)
            .vexpand(true)
            .build();
        content.append(&scroll);
        let empty = gtk::Label::builder()
            .label("No matching actions")
            .vexpand(true)
            .build();
        empty.add_css_class("dim-label");
        content.append(&empty);
        let hint = gtk::Label::new(Some("↑ ↓ to choose · Enter to run · Esc to close"));
        hint.add_css_class("caption");
        hint.add_css_class("dim-label");
        content.append(&hint);
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));
        dialog.set_focus(Some(&search));
        let palette = Rc::new(Self {
            dialog,
            search,
            results,
            scroll,
            empty,
            commands,
            rows: RefCell::new(Vec::new()),
            pending: RefCell::new(None),
        });
        let weak = Rc::downgrade(&palette);
        palette.search.connect_search_changed(move |_| {
            if let Some(palette) = weak.upgrade() {
                palette.filter();
            }
        });
        let weak = Rc::downgrade(&palette);
        palette.results.connect_row_activated(move |_, row| {
            if let Some(palette) = weak.upgrade() {
                palette.activate(Some(row));
            }
        });
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&palette);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            weak.upgrade()
                .is_some_and(|palette| palette.key_pressed(key, modifiers))
                .into()
        });
        palette.dialog.add_controller(keys);
        let weak = Rc::downgrade(&palette);
        palette.dialog.connect_closed(move |_| {
            if let Some(palette) = weak.upgrade() {
                let pending = palette.pending.borrow_mut().take();
                if let Some(command) = pending
                    && (command.enabled)()
                {
                    (command.run)();
                }
            }
        });
        palette.filter();
        palette
    }

    fn filter(&self) {
        self.results.remove_all();
        self.rows.borrow_mut().clear();
        let search = caseless::default_case_fold_str(&self.search.text());
        let terms: Vec<_> = search
            .split(text::whitespace)
            .filter(|term| !term.is_empty())
            .collect();
        for command in &self.commands {
            let searchable =
                caseless::default_case_fold_str(&format!("{} {}", command.title, command.keywords));
            if !terms.iter().all(|term| searchable.contains(term)) {
                continue;
            }
            let row = adw::ActionRow::builder()
                .title(&command.title)
                .activatable(true)
                .build();
            row.add_prefix(&gtk::Image::from_icon_name(&command.icon));
            if !command.shortcut.is_empty() {
                row.add_suffix(&gtk::ShortcutLabel::new(&command.shortcut));
            }
            row.set_sensitive((command.enabled)());
            self.rows
                .borrow_mut()
                .push((row.clone().upcast(), command.clone()));
            self.results.append(&row);
        }
        self.empty.set_visible(self.rows.borrow().is_empty());
        if let Some(row) = self.available_rows().first() {
            self.results.select_row(Some(row));
        }
        self.scroll.vadjustment().set_value(0.0);
    }

    fn available_rows(&self) -> Vec<gtk::ListBoxRow> {
        self.rows
            .borrow()
            .iter()
            .filter_map(|(row, command)| {
                let enabled = (command.enabled)();
                row.set_sensitive(enabled);
                enabled.then(|| row.clone())
            })
            .collect()
    }

    fn activate(&self, row: Option<&gtk::ListBoxRow>) {
        let Some(command) = row.and_then(|row| {
            self.rows
                .borrow()
                .iter()
                .find(|(item, _)| item == row)
                .map(|(_, command)| command.clone())
        }) else {
            return;
        };
        if (command.enabled)() {
            self.pending.replace(Some(command));
            self.dialog.close();
        } else if let Some(row) = row {
            row.set_sensitive(false);
        }
    }

    fn key_pressed(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        if key == gdk::Key::Escape
            || (matches!(key, gdk::Key::p | gdk::Key::P)
                && state.contains(gdk::ModifierType::CONTROL_MASK))
        {
            self.dialog.close();
            return true;
        }
        let Some(focus) = self.dialog.focus() else {
            return false;
        };
        if focus != self.search && !focus.is_ancestor(&self.search) {
            return false;
        }
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
            self.activate(self.results.selected_row().as_ref());
            return true;
        }
        if !matches!(key, gdk::Key::Up | gdk::Key::Down) {
            return false;
        }
        let rows = self.available_rows();
        if !rows.is_empty() {
            let selected = self.results.selected_row();
            let index = rows
                .iter()
                .position(|row| Some(row) == selected.as_ref())
                .map(|index| index as isize)
                .unwrap_or(-1);
            let step = if key == gdk::Key::Down { 1 } else { -1 };
            let row = &rows[(index + step).rem_euclid(rows.len() as isize) as usize];
            self.results.select_row(Some(row));
            if let Some(bounds) = row.compute_bounds(&self.results) {
                self.scroll.vadjustment().clamp_page(
                    f64::from(bounds.y()),
                    f64::from(bounds.y() + bounds.height()),
                );
            }
        }
        true
    }
}

/// Match the application's displayed shortcuts with GTK's default modifier filtering.
pub fn dispatch_shortcut(commands: &[Command], key: gdk::Key, state: gdk::ModifierType) -> bool {
    let modifiers = state & gtk::accelerator_get_default_mod_mask();
    if !modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        return false;
    }
    for command in commands {
        if !command.shortcut.is_empty()
            && let Some((shortcut_key, shortcut_modifiers)) =
                gtk::accelerator_parse(&command.shortcut)
            && key.to_lower() == shortcut_key
            && modifiers == shortcut_modifiers
            && (command.enabled)()
        {
            (command.run)();
            return true;
        }
    }
    false
}
