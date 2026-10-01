//! Full-window preference navigation and deep links to the real settings rows.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

pub struct SettingsView {
    pub widget: gtk::Box,
    pub stack: gtk::Stack,
    navigation: gtk::FlowBox,
    pages: RefCell<Vec<adw::PreferencesPage>>,
    buttons: RefCell<Vec<gtk::ToggleButton>>,
    go_back: Rc<dyn Fn()>,
}

impl SettingsView {
    pub fn new(go_back: Rc<dyn Fn()>) -> Rc<Self> {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .build();
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_top(16);
        header.set_margin_bottom(16);
        header.set_margin_start(16);
        header.set_margin_end(16);
        let back = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to workspace · Esc")
            .has_frame(false)
            .build();
        header.append(&back);
        let title = gtk::Label::builder().label("Settings").xalign(0.0).build();
        title.add_css_class("title-2");
        header.append(&title);
        widget.append(&header);
        let navigation = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(2)
            .max_children_per_line(4)
            .row_spacing(6)
            .column_spacing(6)
            .margin_start(16)
            .margin_end(16)
            .margin_bottom(8)
            .build();
        widget.append(&navigation);
        let stack = gtk::Stack::builder().vexpand(true).hexpand(true).build();
        widget.append(&stack);
        let view = Rc::new(Self {
            widget,
            stack,
            navigation,
            pages: RefCell::new(Vec::new()),
            buttons: RefCell::new(Vec::new()),
            go_back,
        });
        let weak = Rc::downgrade(&view);
        back.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.close();
            }
        });
        view
    }

    /// Retain one page instance so command search and visible preferences share their state.
    pub fn add(self: &Rc<Self>, page: &adw::PreferencesPage) {
        self.stack.add_named(page, page.name().as_deref());
        let button = gtk::ToggleButton::builder()
            .label(page.title())
            .hexpand(true)
            .build();
        if let Some(first) = self.buttons.borrow().first() {
            button.set_group(Some(first));
        }
        let weak = Rc::downgrade(self);
        let target = page.downgrade();
        button.connect_toggled(move |button| {
            if button.is_active()
                && let (Some(view), Some(page)) = (weak.upgrade(), target.upgrade())
            {
                view.stack.set_visible_child(&page);
            }
        });
        self.pages.borrow_mut().push(page.clone());
        self.buttons.borrow_mut().push(button.clone());
        self.navigation.insert(&button, -1);
        if self.pages.borrow().len() == 1 {
            button.set_active(true);
        }
    }

    pub fn pages(&self) -> Vec<adw::PreferencesPage> {
        self.pages.borrow().clone()
    }

    pub fn set_visible_page(&self, page: &adw::PreferencesPage) -> bool {
        let index = self.pages.borrow().iter().position(|item| item == page);
        let Some(index) = index else {
            return false;
        };
        self.buttons.borrow()[index].set_active(true);
        self.stack.set_visible_child(page);
        true
    }

    pub fn set_visible_page_name(&self, name: &str) -> bool {
        let page = self
            .pages
            .borrow()
            .iter()
            .find(|page| page.name().as_deref() == Some(name))
            .cloned();
        page.is_some_and(|page| self.set_visible_page(&page))
    }

    pub fn visible_page(&self) -> Option<adw::PreferencesPage> {
        self.stack.visible_child()?.downcast().ok()
    }

    pub fn visible_page_name(&self) -> Option<glib::GString> {
        self.stack.visible_child_name()
    }

    pub fn close(&self) {
        (self.go_back)();
    }

    /// Expand the original ancestor rows before GTK moves focus and scrolls the selected row.
    pub fn focus_row(&self, page: &adw::PreferencesPage, row: &impl IsA<gtk::Widget>) {
        if !self.set_visible_page(page) {
            return;
        }
        let row = row.as_ref();
        let mut parent = row.parent();
        while let Some(widget) = parent {
            if widget == *page {
                break;
            }
            if let Some(expander) = widget.downcast_ref::<adw::ExpanderRow>() {
                expander.set_expanded(true);
            }
            parent = widget.parent();
        }
        if let Some(expander) = row.downcast_ref::<adw::ExpanderRow>() {
            expander.set_expanded(true);
        }
        let target = row.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(row) = target.upgrade() {
                row.set_focusable(true);
                row.grab_focus();
            }
        });
    }
}
