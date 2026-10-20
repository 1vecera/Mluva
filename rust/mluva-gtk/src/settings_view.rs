//! Full-window preference navigation and deep links to the real settings rows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use mluva_core::config::{AppConfig, ConfigError};
use serde_json::{Map, Value};

pub type SaveSettings = Rc<dyn Fn(&Map<String, Value>) -> bool>;

pub(crate) fn proposed_config(
    config: &AppConfig,
    changes: &Map<String, Value>,
) -> Result<AppConfig, ConfigError> {
    let mut document = serde_json::to_value(config).expect("serializable settings");
    let fields = document.as_object_mut().unwrap();
    for (name, value) in changes {
        if !fields.contains_key(name) {
            return Err(ConfigError::Fields);
        }
        fields.insert(name.clone(), value.clone());
    }
    let config: AppConfig = serde_json::from_value(document).map_err(|_| ConfigError::Fields)?;
    config.validate()?;
    Ok(config)
}

pub struct SettingsView {
    pub widget: gtk::Box,
    pub scroll: gtk::ScrolledWindow,
    navigation: gtk::FlowBox,
    content: gtk::Box,
    pages: RefCell<Vec<adw::PreferencesPage>>,
    sections: RefCell<Vec<gtk::Box>>,
    buttons: RefCell<Vec<gtk::ToggleButton>>,
    current: Cell<usize>,
    jump_generation: Cell<u64>,
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
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.set_margin_bottom(24);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        widget.append(&scroll);
        let view = Rc::new(Self {
            widget,
            scroll,
            navigation,
            content,
            pages: RefCell::new(Vec::new()),
            sections: RefCell::new(Vec::new()),
            buttons: RefCell::new(Vec::new()),
            current: Cell::new(0),
            jump_generation: Cell::new(0),
            go_back,
        });
        let weak = Rc::downgrade(&view);
        back.connect_clicked(move |_| {
            if let Some(view) = weak.upgrade() {
                view.close();
            }
        });
        let weak = Rc::downgrade(&view);
        view.scroll.vadjustment().connect_value_changed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.track_section();
            }
        });
        view
    }

    /// Keep the real pages and rows, but let only the outer viewport scroll.
    pub fn add(self: &Rc<Self>, page: &adw::PreferencesPage) {
        self.insert(page, false);
    }

    pub fn prepend(self: &Rc<Self>, page: &adw::PreferencesPage) {
        self.insert(page, true);
    }

    fn insert(self: &Rc<Self>, page: &adw::PreferencesPage, first: bool) {
        // PreferencesPage owns a scroller around its groups. Give it its full
        // natural height instead of nesting independently scrolling pages.
        let scroller = page_scroller(page.upcast_ref()).expect("PreferencesPage scroll container");
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
        scroller.set_propagate_natural_height(true);
        page.set_vexpand(false);
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let heading = gtk::Label::builder()
            .label(page.title())
            .xalign(0.0)
            .wrap(true)
            .focusable(true)
            .accessible_role(gtk::AccessibleRole::Heading)
            .build();
        heading.add_css_class("title-2");
        let title = adw::Clamp::builder()
            .maximum_size(600)
            .margin_start(24)
            .margin_end(24)
            .margin_top(16)
            .child(&heading)
            .build();
        section.append(&title);
        section.append(page);
        let button = gtk::ToggleButton::builder()
            .label(page.title())
            .tooltip_text(format!("Jump to {}", page.title()))
            .hexpand(true)
            .build();
        if let Some(first) = self.buttons.borrow().first() {
            button.set_group(Some(first));
        }
        let weak = Rc::downgrade(self);
        let target = page.downgrade();
        button.connect_clicked(move |_| {
            if let (Some(view), Some(page)) = (weak.upgrade(), target.upgrade()) {
                view.set_visible_page(&page);
            }
        });
        if first {
            self.content.prepend(&section);
            self.pages.borrow_mut().insert(0, page.clone());
            self.sections.borrow_mut().insert(0, section);
            self.buttons.borrow_mut().insert(0, button.clone());
            self.navigation.insert(&button, 0);
        } else {
            self.content.append(&section);
            self.pages.borrow_mut().push(page.clone());
            self.sections.borrow_mut().push(section);
            self.buttons.borrow_mut().push(button.clone());
            self.navigation.insert(&button, -1);
        }
        if first || self.pages.borrow().len() == 1 {
            self.select(0);
        }
    }

    pub fn pages(&self) -> Vec<adw::PreferencesPage> {
        self.pages.borrow().clone()
    }

    pub fn set_visible_page(self: &Rc<Self>, page: &adw::PreferencesPage) -> bool {
        let index = self.pages.borrow().iter().position(|item| item == page);
        let Some(index) = index else {
            return false;
        };
        let section = self.sections.borrow()[index].clone();
        self.jump_to(&section, true);
        self.select(index);
        true
    }

    pub fn set_visible_page_name(self: &Rc<Self>, name: &str) -> bool {
        let page = self
            .pages
            .borrow()
            .iter()
            .find(|page| page.name().as_deref() == Some(name))
            .cloned();
        page.is_some_and(|page| self.set_visible_page(&page))
    }

    pub fn visible_page(&self) -> Option<adw::PreferencesPage> {
        self.pages.borrow().get(self.current.get()).cloned()
    }

    pub fn visible_page_name(&self) -> Option<glib::GString> {
        self.visible_page()?.name()
    }

    fn select(&self, index: usize) {
        self.current.set(index);
        self.buttons.borrow()[index].set_active(true);
    }

    fn track_section(&self) {
        let adjustment = self.scroll.vadjustment();
        let sections = self.sections.borrow();
        let index = if adjustment.value() > 0.0
            && adjustment.value() >= adjustment.upper() - adjustment.page_size() - 1.0
        {
            sections.len().checked_sub(1)
        } else {
            sections.iter().rposition(|section| {
                section
                    .compute_bounds(&self.content)
                    .is_some_and(|bounds| f64::from(bounds.y()) <= adjustment.value() + 24.0)
            })
        };
        if let Some(index) = index {
            self.select(index);
        }
    }

    /// Wait for layout, including expanded rows and settings opened from another page.
    fn jump_to(self: &Rc<Self>, target: &impl IsA<gtk::Widget>, heading: bool) {
        let generation = self.jump_generation.get().wrapping_add(1);
        self.jump_generation.set(generation);
        let target = target.as_ref().downgrade();
        let weak = Rc::downgrade(self);
        self.content.add_tick_callback(move |_, _| {
            let (Some(view), Some(target)) = (weak.upgrade(), target.upgrade()) else {
                return glib::ControlFlow::Break;
            };
            if view.jump_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            if !target.is_mapped() || target.height() == 0 {
                return glib::ControlFlow::Continue;
            }
            let Some(bounds) = target.compute_bounds(&view.content) else {
                return glib::ControlFlow::Break;
            };
            if heading {
                target.child_focus(gtk::DirectionType::TabForward);
            } else {
                target.set_focusable(true);
                target.grab_focus();
            }
            let adjustment = view.scroll.vadjustment();
            let maximum = (adjustment.upper() - adjustment.page_size()).max(0.0);
            adjustment.set_value((f64::from(bounds.y()) - 16.0).clamp(0.0, maximum));
            glib::ControlFlow::Break
        });
    }

    pub fn close(&self) {
        (self.go_back)();
    }

    /// Expand the original ancestor rows before GTK moves focus and scrolls the selected row.
    pub fn focus_row(self: &Rc<Self>, page: &adw::PreferencesPage, row: &impl IsA<gtk::Widget>) {
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
        self.jump_to(row, false);
    }
}

/// Find only the page's own container, before descending into preference controls.
fn page_scroller(widget: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
    if let Some(scroller) = widget.downcast_ref::<gtk::ScrolledWindow>() {
        return Some(scroller.clone());
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if !widget.is::<adw::PreferencesGroup>()
            && let Some(scroller) = page_scroller(&widget)
        {
            return Some(scroller);
        }
        child = widget.next_sibling();
    }
    None
}
