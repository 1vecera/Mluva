//! Attached screenshot previews retain their exact owner and offset in narration.

use gtk::prelude::*;
use mluva_core::screenshots::Screenshot;
use std::rc::Rc;

pub struct ScreenshotShelf {
    pub widget: gtk::ScrolledWindow,
    pub images: gtk::Box,
    edit: Rc<dyn Fn(&str)>,
    remove: Rc<dyn Fn(&str)>,
}

impl ScreenshotShelf {
    pub fn new(edit: Rc<dyn Fn(&str)>, remove: Rc<dyn Fn(&str)>) -> Self {
        let images = gtk::Box::builder()
            .spacing(12)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .build();
        let widget = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&images)
            .visible(false)
            .margin_start(16)
            .margin_end(16)
            .margin_bottom(8)
            .build();
        Self {
            widget,
            images,
            edit,
            remove,
        }
    }

    pub fn show_images(&self, screenshots: &[Screenshot]) {
        while let Some(child) = self.images.first_child() {
            self.images.remove(&child);
        }
        for (index, screenshot) in screenshots.iter().enumerate() {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let preview = gtk::Picture::for_filename(&screenshot.path);
            preview.set_content_fit(gtk::ContentFit::Contain);
            preview.set_size_request(180, 100);
            let image_button = gtk::Button::builder()
                .child(&preview)
                .tooltip_text("Edit screenshot")
                .build();
            let edit = self.edit.clone();
            let identifier = screenshot.identifier.clone();
            image_button.connect_clicked(move |_| edit(&identifier));
            card.append(&image_button);
            let mut caption = format!("Screenshot {}", index + 1);
            if let Some(offset) = screenshot.captured_after_seconds {
                let seconds = offset.round_ties_even() as i64;
                caption += &format!(
                    " · {}:{:02}",
                    seconds.div_euclid(60),
                    seconds.rem_euclid(60)
                );
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            row.append(
                &gtk::Label::builder()
                    .label(&caption)
                    .xalign(0.0)
                    .hexpand(true)
                    .css_classes(["caption"])
                    .build(),
            );
            let remove = gtk::Button::builder()
                .icon_name("edit-delete-symbolic")
                .tooltip_text("Remove screenshot")
                .has_frame(false)
                .build();
            let callback = self.remove.clone();
            let identifier = screenshot.identifier.clone();
            remove.connect_clicked(move |_| callback(&identifier));
            row.append(&remove);
            card.append(&row);
            self.images.append(&card);
        }
        self.widget.set_visible(!screenshots.is_empty());
    }
}
