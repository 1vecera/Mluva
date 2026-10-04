//! The native application window, in-window navigation, shortcuts and resident lifecycle.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;

use crate::command_palette::{Command, CommandPalette, dispatch_shortcut};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationAction {
    Latest,
    Record,
    GlobalRecord,
    Screenshot,
    Cancel,
    Status,
    History,
    Settings,
    Commands,
    Meeting,
    Personalization,
    Quit,
    Review {
        operation: String,
        identifier: String,
        option: String,
    },
}

/// Register the same externally activatable action names and typed review arguments as 1.6.0.
pub fn register_actions(app: &adw::Application, invoke: Rc<dyn Fn(ApplicationAction)>) {
    for (name, action) in [
        ("latest", ApplicationAction::Latest),
        ("record", ApplicationAction::Record),
        ("global-record", ApplicationAction::GlobalRecord),
        ("screenshot", ApplicationAction::Screenshot),
        ("cancel", ApplicationAction::Cancel),
        ("status", ApplicationAction::Status),
        ("history", ApplicationAction::History),
        ("settings", ApplicationAction::Settings),
        ("commands", ApplicationAction::Commands),
        ("meeting", ApplicationAction::Meeting),
        ("personalization", ApplicationAction::Personalization),
        ("quit", ApplicationAction::Quit),
    ] {
        let handler = invoke.clone();
        let simple = gio::SimpleAction::new(name, None);
        simple.connect_activate(move |_, _| handler(action.clone()));
        app.add_action(&simple);
    }
    let review = gio::SimpleAction::new("review", Some(glib::VariantTy::new("(sss)").unwrap()));
    review.connect_activate(move |_, parameters| {
        if let Some((operation, identifier, option)) =
            parameters.and_then(glib::Variant::get::<(String, String, String)>)
        {
            invoke(ApplicationAction::Review {
                operation,
                identifier,
                option,
            });
        }
    });
    app.add_action(&review);
    app.set_accels_for_action("app.commands", &["<Control>p"]);
}

pub struct ShellPages {
    pub capture: gtk::Widget,
    pub settings: gtk::Widget,
    pub welcome: gtk::Widget,
    pub meeting: gtk::Widget,
    pub history: gtk::Widget,
    pub personalization: gtk::Widget,
}

pub struct ShellCallbacks {
    pub invoke: Rc<dyn Fn(ApplicationAction)>,
    pub toggle_sidebar: Rc<dyn Fn()>,
    pub commands: Rc<dyn Fn() -> Vec<Command>>,
    pub cancel_capture: Rc<dyn Fn() -> bool>,
    pub compact: Rc<dyn Fn(bool)>,
}

/// Adapt the real capture controls without duplicating their state in the window.
pub struct AdaptiveWorkspace {
    pub split: adw::OverlaySplitView,
    pub live_panes: gtk::Box,
    pub capture_actions: gtk::Box,
    pub capture_buttons: gtk::Box,
    pub record_button: gtk::Button,
    pub sidebar_visible: Rc<dyn Fn() -> bool>,
    pub compact_documents: Rc<dyn Fn(bool)>,
    pub compact_recording: Rc<dyn Fn(bool)>,
}

impl AdaptiveWorkspace {
    pub fn set_compact(&self, compact: bool) {
        self.split.set_collapsed(compact);
        self.live_panes.set_orientation(if compact {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        self.split
            .set_show_sidebar(!compact && (self.sidebar_visible)());
        (self.compact_documents)(compact);
        self.capture_actions.set_orientation(if compact {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        if compact {
            self.capture_actions.add_css_class("compact");
        } else {
            self.capture_actions.remove_css_class("compact");
        }
        self.capture_buttons.set_halign(if compact {
            gtk::Align::Fill
        } else {
            gtk::Align::End
        });
        self.record_button.set_hexpand(compact);
        (self.compact_recording)(compact);
    }
}

pub struct ApplicationShell {
    pub window: adw::ApplicationWindow,
    pub stack: adw::ViewStack,
    pub header: adw::HeaderBar,
    pub page_title: gtk::Label,
    pub toast_overlay: adw::ToastOverlay,
    callbacks: ShellCallbacks,
    palette: RefCell<Option<Rc<CommandPalette>>>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
}

impl ApplicationShell {
    pub fn new(app: &adw::Application, pages: ShellPages, callbacks: ShellCallbacks) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Mluva")
            .default_width(1060)
            .default_height(780)
            .width_request(420)
            .height_request(520)
            .build();
        let toolbar = adw::ToolbarView::new();
        let header = adw::HeaderBar::new();
        let page_title = gtk::Label::builder().xalign(0.0).build();
        page_title.add_css_class("vs-page-title");
        header.set_title_widget(Some(&page_title));
        toolbar.add_top_bar(&header);
        let stack = adw::ViewStack::builder()
            .hhomogeneous(false)
            .vhomogeneous(false)
            .build();
        stack.add_titled_with_icon(
            &pages.capture,
            Some("capture"),
            "Conversations",
            "audio-input-microphone-symbolic",
        );
        stack.add_titled(&pages.settings, Some("settings"), "Settings");
        stack.add_titled(&pages.welcome, Some("welcome"), "Welcome");
        stack.add_titled_with_icon(
            &pages.meeting,
            Some("meeting"),
            "Meeting",
            "system-users-symbolic",
        );
        stack.add_titled_with_icon(
            &pages.history,
            Some("history"),
            "History",
            "document-open-recent-symbolic",
        );
        stack.add_titled_with_icon(
            &pages.personalization,
            Some("personalization"),
            "Personalization",
            "document-edit-symbolic",
        );
        let settings = gtk::Button::builder()
            .icon_name("preferences-system-symbolic")
            .tooltip_text("Mluva settings")
            .build();
        settings.add_css_class("vs-utility");
        header.pack_end(&settings);
        let sidebar = gtk::Button::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("Show or hide history")
            .build();
        header.pack_start(&sidebar);
        let home = gtk::Button::builder()
            .icon_name("go-home-symbolic")
            .has_frame(false)
            .tooltip_text("Conversations")
            .build();
        header.pack_start(&home);
        let menu = gio::Menu::new();
        for (label, action) in [
            ("Commands", "commands"),
            ("Latest conversation", "latest"),
            ("Manage history", "history"),
            ("Meeting recording", "meeting"),
            ("Vocabulary and prompts", "personalization"),
            ("Quit Mluva", "quit"),
        ] {
            menu.append(Some(label), Some(&format!("app.{action}")));
        }
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .tooltip_text("Mluva menu")
            .build();
        header.pack_end(&menu_button);
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        content.append(&stack);
        let toast_overlay = adw::ToastOverlay::new();
        toast_overlay.set_child(Some(&content));
        let workspace = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        workspace.append(&toast_overlay);
        toolbar.set_content(Some(&workspace));
        window.set_content(Some(&toolbar));
        let shell = Rc::new(Self {
            window,
            stack,
            header,
            page_title,
            toast_overlay,
            callbacks,
            palette: RefCell::new(None),
            hold: RefCell::new(Some(app.hold())),
        });
        let weak = Rc::downgrade(&shell);
        settings.connect_clicked(move |_| {
            if let Some(shell) = weak.upgrade() {
                (shell.callbacks.invoke)(ApplicationAction::Settings);
            }
        });
        let weak = Rc::downgrade(&shell);
        sidebar.connect_clicked(move |_| {
            if let Some(shell) = weak.upgrade() {
                (shell.callbacks.toggle_sidebar)();
            }
        });
        let weak = Rc::downgrade(&shell);
        home.connect_clicked(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.navigate("capture");
            }
        });
        let weak = Rc::downgrade(&shell);
        shell.stack.connect_visible_child_notify(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.sync_page_title();
            }
        });
        shell.window.connect_close_request(|window| {
            window.set_visible(false);
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&shell);
        shell.window.connect_destroy(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.hold.borrow_mut().take();
            }
        });
        let keys = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(&shell);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(shell) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if shell.window.visible_dialog().is_some() || key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if matches!(
                shell.stack.visible_child_name().as_deref(),
                Some("settings" | "welcome")
            ) {
                shell.navigate("capture");
                return glib::Propagation::Stop;
            }
            (shell.callbacks.cancel_capture)().into()
        });
        shell.window.add_controller(keys);
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&shell);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(shell) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let filtered = modifiers & gtk::accelerator_get_default_mod_mask();
            if !filtered.contains(gdk::ModifierType::CONTROL_MASK)
                || shell.window.visible_dialog().is_some()
            {
                return glib::Propagation::Proceed;
            }
            if key.to_lower() == gdk::Key::p && filtered == gdk::ModifierType::CONTROL_MASK {
                shell.show_commands();
                return glib::Propagation::Stop;
            }
            dispatch_shortcut(&(shell.callbacks.commands)(), key, modifiers).into()
        });
        shell.window.add_controller(keys);
        let condition =
            adw::BreakpointCondition::parse("max-width: 736sp").expect("released breakpoint");
        let breakpoint = adw::Breakpoint::new(condition);
        let weak = Rc::downgrade(&shell);
        breakpoint.connect_apply(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.adapt(true);
            }
        });
        let weak = Rc::downgrade(&shell);
        breakpoint.connect_unapply(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.adapt(false);
            }
        });
        shell.window.add_breakpoint(breakpoint);
        shell.sync_page_title();
        shell
    }

    pub fn navigate(&self, name: &str) {
        self.stack.set_visible_child_name(name);
    }

    pub fn present(&self) {
        self.window.present();
    }

    pub fn show_message(&self, message: &str) {
        self.toast_overlay.add_toast(adw::Toast::new(message));
    }

    pub fn show_commands(self: &Rc<Self>) {
        self.present();
        let current = self.palette.borrow().clone();
        if let Some(palette) = current {
            palette.dialog.close();
            return;
        }
        if self.window.visible_dialog().is_some() {
            return;
        }
        let palette = CommandPalette::new((self.callbacks.commands)());
        let weak = Rc::downgrade(self);
        palette.dialog.connect_closed(move |_| {
            if let Some(shell) = weak.upgrade() {
                shell.palette.borrow_mut().take();
            }
        });
        palette.dialog.present(Some(&self.window));
        self.palette.replace(Some(palette));
    }

    fn adapt(&self, compact: bool) {
        (self.callbacks.compact)(compact);
        self.header.set_show_title(self.window.width() > 736);
        self.header.set_title_widget(Some(&self.page_title));
    }

    fn sync_page_title(&self) {
        let title = self
            .stack
            .visible_child()
            .filter(|_| self.stack.visible_child_name().as_deref() != Some("capture"))
            .map(|child| self.stack.page(&child).title().unwrap_or_default())
            .unwrap_or_default();
        self.page_title.set_label(&title);
    }
}
