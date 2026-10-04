//! Searchable native recognition-language modal; persisted ISO forms stay intact.
use adw::prelude::*;
use mluva_providers::languages;
use std::{cell::RefCell, rc::Rc};

pub struct LanguagePicker {
    pub widget: gtk::Button,
    language: RefCell<String>,
    model: RefCell<String>,
    changed: Rc<dyn Fn(&str)>,
    dialog: RefCell<Option<adw::Dialog>>,
}
impl LanguagePicker {
    pub fn new(language: &str, model: &str, changed: Rc<dyn Fn(&str)>) -> Rc<Self> {
        let widget = gtk::Button::builder()
            .label(languages::label(language))
            .halign(gtk::Align::Start)
            .tooltip_text("Choose recognition language")
            .build();
        let picker = Rc::new(Self {
            widget,
            language: RefCell::new(language.into()),
            model: RefCell::new(model.into()),
            changed,
            dialog: RefCell::new(None),
        });
        let weak = Rc::downgrade(&picker);
        picker.widget.connect_clicked(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.open();
            }
        });
        picker
    }
    pub fn language(&self) -> String {
        self.language.borrow().clone()
    }
    pub fn refresh(&self, language: &str, model: &str) {
        self.language.replace(language.into());
        self.model.replace(model.into());
        self.widget.set_label(&languages::label(language));
    }
    pub fn open(self: &Rc<Self>) {
        let dialog = adw::Dialog::builder()
            .title("Recognition language")
            .content_width(440)
            .content_height(430)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.append(&adw::HeaderBar::new());
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Find a language")
            .margin_start(16)
            .margin_end(16)
            .build();
        content.append(&search);
        let auto = gtk::Button::builder()
            .label("◎ Detect automatically")
            .margin_start(16)
            .margin_end(16)
            .build();
        let weak = Rc::downgrade(self);
        auto.connect_clicked(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.choose("auto");
            }
        });
        content.append(&auto);
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(2)
            .max_children_per_line(2)
            .row_spacing(6)
            .column_spacing(6)
            .margin_start(16)
            .margin_end(16)
            .margin_bottom(16)
            .build();
        let allowed = languages::supported(&self.model.borrow());
        let mut names = Vec::new();
        for language in allowed {
            let button = gtk::Button::builder()
                .label(format!("{} {}", language.flag, language.name))
                .tooltip_text(&language.name)
                .hexpand(true)
                .build();
            let code = language.code.clone();
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(picker) = weak.upgrade() {
                    picker.choose(&code);
                }
            });
            flow.insert(&button, -1);
            names.push(caseless::default_case_fold_str(&format!(
                "{} {} {}",
                language.code, language.iso, language.name
            )));
        }
        let entry = search.clone();
        flow.set_filter_func(move |child| {
            names
                .get(child.index() as usize)
                .is_some_and(|name| name.contains(&caseless::default_case_fold_str(&entry.text())))
        });
        let weak = flow.downgrade();
        search.connect_search_changed(move |_| {
            if let Some(flow) = weak.upgrade() {
                flow.invalidate_filter();
            }
        });
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&flow)
            .build();
        content.append(&scroll);
        dialog.set_child(Some(&content));
        self.dialog.replace(Some(dialog.clone()));
        dialog.present(Some(&self.widget));
    }
    pub fn choose(&self, code: &str) {
        self.language.replace(code.into());
        self.widget.set_label(&languages::label(code));
        (self.changed)(code);
        if let Some(dialog) = self.dialog.borrow().as_ref() {
            dialog.close();
        }
    }
}
