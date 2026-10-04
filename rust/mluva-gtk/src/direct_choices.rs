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
        let widget = gtk::Box::builder()
            .spacing(0)
            .homogeneous(true)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        widget.add_css_class("linked");
        let buttons: Vec<_> = labels
            .iter()
            .map(|label| {
                gtk::ToggleButton::builder()
                    .label(*label)
                    .hexpand(true)
                    .build()
            })
            .collect();
        for button in &buttons {
            if button != &buttons[0] {
                button.set_group(Some(&buttons[0]));
            }
            widget.append(button);
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
