//! Always-visible short choices, with GTK's grouped keyboard navigation.
use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

pub struct DirectChoices {
    pub widget: gtk::Box,
    pub buttons: Vec<gtk::ToggleButton>,
    selected: Cell<usize>,
    changed: Rc<dyn Fn(usize)>,
}
impl DirectChoices {
    pub fn new(labels: &[&str], changed: Rc<dyn Fn(usize)>) -> Rc<Self> {
        Self::with_badges(labels, &[], changed)
    }

    /// `badges` names the choices that carry a small caption under their label, by index, such
    /// as the suggested provider.
    pub fn with_badges(
        labels: &[&str],
        badges: &[(usize, &str)],
        changed: Rc<dyn Fn(usize)>,
    ) -> Rc<Self> {
        Self::build(labels, badges, None, changed)
    }

    /// Choices that wrap onto further lines of at most `per_line` buttons, for sets too wide
    /// for the narrowest window in one row.
    pub fn wrapping(
        labels: &[&str],
        badges: &[(usize, &str)],
        per_line: u32,
        changed: Rc<dyn Fn(usize)>,
    ) -> Rc<Self> {
        Self::build(labels, badges, Some(per_line), changed)
    }

    fn build(
        labels: &[&str],
        badges: &[(usize, &str)],
        per_line: Option<u32>,
        changed: Rc<dyn Fn(usize)>,
    ) -> Rc<Self> {
        let widget = gtk::Box::builder()
            .spacing(0)
            .homogeneous(per_line.is_none())
            .margin_top(8)
            .margin_bottom(8)
            .build();
        let lines = per_line.map(|per_line| {
            let lines = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .homogeneous(true)
                .min_children_per_line(1)
                .max_children_per_line(per_line)
                .column_spacing(6)
                .row_spacing(6)
                .hexpand(true)
                .build();
            widget.append(&lines);
            lines
        });
        if lines.is_none() {
            widget.add_css_class("linked");
        }
        let buttons: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let button = gtk::ToggleButton::builder().hexpand(true).build();
                match badges.iter().find(|(badged, _)| *badged == index) {
                    Some((_, badge)) => {
                        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        content.append(&gtk::Label::new(Some(label)));
                        let caption = gtk::Label::new(Some(badge));
                        caption.add_css_class("caption");
                        caption.add_css_class("accent");
                        content.append(&caption);
                        button.set_child(Some(&content));
                        button.update_property(&[gtk::accessible::Property::Label(&format!(
                            "{label}, {badge}"
                        ))]);
                    }
                    None => button.set_label(label),
                }
                button
            })
            .collect();
        for button in &buttons {
            if button != &buttons[0] {
                button.set_group(Some(&buttons[0]));
            }
            match &lines {
                Some(lines) => lines.append(button),
                None => widget.append(button),
            }
        }
        let choices = Rc::new(Self {
            widget,
            buttons,
            selected: Cell::new(0),
            changed,
        });
        for (index, button) in choices.buttons.iter().enumerate() {
            let weak = Rc::downgrade(&choices);
            button.connect_toggled(move |button| {
                if button.is_active()
                    && let Some(choices) = weak.upgrade()
                    && choices.selected.replace(index) != index
                {
                    (choices.changed)(index);
                }
            });
        }
        choices.set_selected(0);
        choices
    }
    pub fn selected(&self) -> usize {
        self.selected.get()
    }
    pub fn set_selected(&self, index: usize) {
        if let Some(button) = self.buttons.get(index) {
            button.set_active(true);
        }
    }
}
