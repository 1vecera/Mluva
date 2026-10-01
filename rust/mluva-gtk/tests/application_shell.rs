//! Released command identities, stale-state refusal and real GTK preference/dialog behavior.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command as Process;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use adw::prelude::*;
use mluva_core::config::AppConfig;
use mluva_core::history::HistoryStore;
use mluva_core::personalization::PersonalizationStore;
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;
use mluva_gtk::application_shell::{
    AdaptiveWorkspace, ApplicationAction, ApplicationShell, ShellCallbacks, ShellPages,
    register_actions,
};
use mluva_gtk::command_palette::{Command, CommandPalette};
use mluva_gtk::commands::{
    CommandAction, CommandContext, CommandState, application_commands, settings_commands,
};
use mluva_gtk::editor_pages::EditorPages;
use mluva_gtk::settings_view::SettingsView;
use mluva_gtk::theme::ThemeController;
use serde_json::{Value, json};

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(200);
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn widgets(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    fn collect(widget: gtk::Widget, output: &mut Vec<gtk::Widget>) {
        output.push(widget.clone());
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(current.clone(), output);
            child = current.next_sibling();
        }
    }
    let mut output = Vec::new();
    collect(widget.as_ref().clone(), &mut output);
    output
}

fn key(chord: &str) {
    let executable = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/private_input");
    let output = Process::new(executable)
        .args(["key", chord])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private key helper failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    settle();
}

fn settings_fixture(back: Rc<dyn Fn()>) -> (Rc<SettingsView>, BTreeMap<String, gtk::Widget>) {
    let settings = SettingsView::new(back);
    let mut rows = BTreeMap::new();
    for (name, title) in [
        ("workspace", "Workspace"),
        ("capture", "Capture"),
        ("prompts", "Prompts"),
    ] {
        let page = adw::PreferencesPage::builder()
            .name(name)
            .title(title)
            .icon_name("preferences-system-symbolic")
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(format!("{title} choices"))
            .description("Synthetic preference boundaries.")
            .build();
        for (label, subtitle) in [
            (format!("Primary {title}"), "Résumé Straße 😀"),
            (format!("Secondary {title}"), "Next capture only"),
        ] {
            let row = adw::ActionRow::builder()
                .title(&label)
                .subtitle(subtitle)
                .build();
            group.add(&row);
            rows.insert(label, row.upcast());
        }
        if name == "workspace" {
            let advanced = adw::ExpanderRow::builder()
                .title("Advanced")
                .subtitle("Nested choices")
                .build();
            let nested = adw::SwitchRow::builder()
                .title("Smooth scrolling")
                .subtitle("Respect manual scrolling")
                .active(true)
                .build();
            advanced.add_row(&nested);
            group.add(&advanced);
            rows.insert("Advanced".into(), advanced.upcast());
            rows.insert("Smooth scrolling".into(), nested.upcast());
        }
        page.add(&group);
        settings.add(&page);
    }
    (settings, rows)
}

fn settings_observation(
    settings: &SettingsView,
    window: &adw::Window,
    rows: &BTreeMap<String, gtk::Widget>,
    messages: &[String],
) -> Value {
    let mut focus = gtk::prelude::GtkWindowExt::focus(window);
    let mut focused = None;
    while let Some(widget) = focus {
        if let Some(row) = widget.downcast_ref::<adw::PreferencesRow>() {
            focused = Some(row.title().to_string());
            break;
        }
        focus = widget.parent();
    }
    let tree = widgets(&settings.widget);
    let buttons: Vec<_> = tree.iter().filter_map(|widget| widget.downcast_ref::<gtk::ToggleButton>())
        .map(|button| json!({"label":button.label().map(|label|label.to_string()),"active":button.is_active()})).collect();
    let navigation = tree
        .iter()
        .find_map(|widget| widget.downcast_ref::<gtk::FlowBox>())
        .unwrap();
    json!({"page":settings.visible_page_name().map(|name|name.to_string()),"buttons":buttons,
        "expanded":rows["Advanced"].downcast_ref::<adw::ExpanderRow>().unwrap().is_expanded(),
        "focused_row":focused,"messages":messages,
        "navigation":{"min":navigation.min_children_per_line(),"max":navigation.max_children_per_line(),"homogeneous":navigation.is_homogeneous(),
            "row_spacing":navigation.row_spacing(),"column_spacing":navigation.column_spacing()}})
}

fn command_state(values: &Value, first: &gtk::TextView, second: &gtk::TextView) -> CommandState {
    let boolean = |key: &str| values[key].as_bool().unwrap();
    CommandState {
        page: values["page"].as_str().unwrap().into(),
        document: values["document"].as_str().map(str::to_owned),
        editor: values["editor"].as_str().map(|name| {
            if name == "first" {
                first.clone().upcast()
            } else {
                second.clone().upcast()
            }
        }),
        recording: boolean("recording"),
        preparing: boolean("preparing"),
        record_sensitive: boolean("record_sensitive"),
        live_enabled: boolean("live_enabled"),
        live_sensitive: boolean("live_sensitive"),
        polish_sensitive: boolean("polish_sensitive"),
        send_sensitive: boolean("send_sensitive"),
        live_visible: boolean("live_visible"),
        can_copy: boolean("can_copy"),
        busy: boolean("busy"),
        private: boolean("private"),
        edit_draft: boolean("edit_draft"),
        live_active: boolean("live_active"),
        viewing_live: boolean("viewing_live"),
        source_visible: boolean("source_visible"),
        draft_visible: boolean("draft_visible"),
        draft_available: boolean("draft_available"),
        processing: boolean("processing"),
        live_final_entry: boolean("live_final_entry"),
        rewriting: boolean("rewriting"),
        incognito: boolean("incognito"),
        pending_incognito: boolean("pending_incognito"),
        pending_mode: values["pending_mode"].as_str().unwrap().into(),
    }
}

fn metadata(commands: &[Command]) -> Value {
    Value::Array(commands.iter().map(|command| json!({"title":command.title,"icon":command.icon,"keywords":command.keywords,"shortcut":command.shortcut,"enabled":(command.enabled)()})).collect())
}

fn palette_observation(palette: &CommandPalette, window: &adw::Window, calls: &[String]) -> Value {
    let mut rows = Vec::new();
    let selected = palette.results.selected_row();
    let mut child = palette.results.first_child();
    while let Some(widget) = child {
        if let Some(row) = widget.downcast_ref::<adw::ActionRow>() {
            rows.push(json!({"title":row.title().as_str(),"sensitive":row.get_sensitive(),"selected":selected.as_ref().is_some_and(|selected| *selected == *row)}));
        }
        child = widget.next_sibling();
    }
    let tree = widgets(&palette.dialog);
    let empty = tree
        .iter()
        .find_map(|widget| {
            widget
                .downcast_ref::<gtk::Label>()
                .filter(|label| label.text() == "No matching actions")
        })
        .unwrap();
    let scroll = tree
        .iter()
        .find_map(|widget| widget.downcast_ref::<gtk::ScrolledWindow>())
        .unwrap();
    json!({"open":window.visible_dialog().is_some(),"title":palette.dialog.title().as_str(),"width":palette.dialog.content_width(),"height":palette.dialog.content_height(),
        "search":palette.search.text().as_str(),"rows":rows,"empty":empty.get_visible(),"calls":calls,
        "scroll":(scroll.vadjustment().value()*10_000.0).round()/10_000.0})
}

#[test]
#[ignore = "requires the disposable X11/session/accessibility runner and the private_input example"]
fn native_commands_and_settings_match_the_released_widgets() {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private desktop required"),
    )
    .canonicalize()
    .unwrap();
    let data = PathBuf::from(std::env::var_os("XDG_DATA_HOME").unwrap())
        .canonicalize()
        .unwrap();
    assert!(data.starts_with(&root));
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    adw::init().unwrap();
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/application-shell-cases.json")).unwrap();
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(
        reference["libadwaita"],
        json!([
            adw::major_version(),
            adw::minor_version(),
            adw::micro_version()
        ])
    );
    assert_eq!(reference["pango"], gtk::pango::version_string().as_str());
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
    )
    .unwrap();
    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let target = messages.clone();
    let (settings, rows) =
        settings_fixture(Rc::new(move || target.borrow_mut().push("back".into())));
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .content(&settings.widget)
        .build();
    window.present();
    settle();
    assert_eq!(
        settings_observation(&settings, &window, &rows, &messages.borrow()),
        reference["settings"]["observations"][0]
    );
    for (index, action) in reference["settings"]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        match action["op"].as_str().unwrap() {
            "page" => {
                assert!(settings.set_visible_page_name(action["name"].as_str().unwrap()));
            }
            "focus" => {
                messages.borrow_mut().push("settings".into());
                settings.focus_row(&settings.pages()[0], &rows[action["row"].as_str().unwrap()]);
            }
            "close" => settings.close(),
            operation => panic!("unknown operation {operation}"),
        }
        settle();
        assert_eq!(
            settings_observation(&settings, &window, &rows, &messages.borrow()),
            reference["settings"]["observations"][index + 1],
            "settings step {index}"
        );
    }
    let prompts = PromptStore::new(data.join("catalog-prompts"), "", &DEFAULTS.styles).unwrap();
    for case in reference["catalog_cases"].as_array().unwrap() {
        let mut initial = reference["catalog_defaults"].clone();
        for (key, value) in case["initial"].as_object().unwrap() {
            initial[key] = value.clone();
        }
        let values = Rc::new(RefCell::new(initial));
        let first = gtk::TextView::new();
        let second = gtk::TextView::new();
        let current = values.clone();
        let context = Rc::new(CommandContext {
            state: Rc::new(move || command_state(&current.borrow(), &first, &second)),
            invoke: Rc::new(|_| {}),
        });
        let commands = application_commands(&context, &settings, prompts.catalog());
        assert_eq!(
            metadata(&commands),
            case["observations"][0],
            "initial catalog {}",
            case["name"]
        );
        for (index, changes) in case["changes"].as_array().unwrap().iter().enumerate() {
            for (key, value) in changes.as_object().unwrap() {
                values.borrow_mut()[key] = value.clone();
            }
            assert_eq!(
                metadata(&commands),
                case["observations"][index + 1],
                "stale catalog {} step {index}",
                case["name"]
            );
        }
    }
    for flow in reference["palette_flows"].as_array().unwrap() {
        let available = Rc::new(RefCell::new(BTreeMap::<String, bool>::new()));
        let expire = Rc::new(RefCell::new(None::<String>));
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut commands = Vec::new();
        for spec in reference["palette_commands"].as_array().unwrap() {
            let title = spec["title"].as_str().unwrap().to_owned();
            available
                .borrow_mut()
                .insert(title.clone(), spec["enabled"].as_bool().unwrap());
            let state = available.clone();
            let dispatch = calls.clone();
            let expires = expire.clone();
            let name = title.clone();
            let action = title.clone();
            commands.push(Command {
                title,
                icon: spec["icon"].as_str().unwrap().into(),
                keywords: spec["keywords"].as_str().unwrap().into(),
                shortcut: spec["shortcut"].as_str().unwrap().into(),
                run: Rc::new(move || dispatch.borrow_mut().push(action.clone())),
                enabled: Rc::new(move || {
                    let result = state.borrow()[&name];
                    if expires.borrow().as_ref() == Some(&name) {
                        state.borrow_mut().insert(name.clone(), false);
                        expires.borrow_mut().take();
                    }
                    result
                }),
            });
        }
        let palette = CommandPalette::new(commands);
        palette.dialog.present(Some(&window));
        settle();
        assert_eq!(
            palette_observation(&palette, &window, &calls.borrow()),
            flow["observations"][0],
            "palette initial {}",
            flow["name"]
        );
        for (index, action) in flow["actions"].as_array().unwrap().iter().enumerate() {
            match action["op"].as_str().unwrap() {
                "search" => {
                    palette.search.set_text(action["text"].as_str().unwrap());
                    settle();
                }
                "key" => key(action["chord"].as_str().unwrap()),
                "availability" => {
                    available.borrow_mut().insert(
                        action["name"].as_str().unwrap().into(),
                        action["enabled"].as_bool().unwrap(),
                    );
                }
                "expire-after-next-check" => {
                    expire.replace(Some(action["name"].as_str().unwrap().into()));
                }
                "row" => {
                    let row = widgets(&palette.results)
                        .into_iter()
                        .find_map(|widget| {
                            widget
                                .downcast::<adw::ActionRow>()
                                .ok()
                                .filter(|row| row.title() == action["title"].as_str().unwrap())
                        })
                        .unwrap();
                    palette.results.emit_by_name::<()>("row-activated", &[&row]);
                    settle();
                }
                operation => panic!("unknown operation {operation}"),
            }
            assert_eq!(
                palette_observation(&palette, &window, &calls.borrow()),
                flow["observations"][index + 1],
                "palette {} step {index}",
                flow["name"]
            );
        }
        if window.visible_dialog().is_some() {
            palette.dialog.force_close();
            settle();
        }
    }
    window.destroy();
    settle();
    root_window_contract(&root, &data);
    settings_binding_contract();
    drop(theme);
}

fn settings_binding_contract() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/settings-command-binding.json")).unwrap();
    let settings = SettingsView::new(Rc::new(|| {}));
    let first = adw::PreferencesPage::builder()
        .name("first")
        .title("First")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Mutable choices")
        .build();
    let row = adw::ActionRow::builder()
        .title("Refreshable setting")
        .subtitle("Deferred navigation must retain its destination.")
        .build();
    group.add(&row);
    first.add(&group);
    settings.add(&first);
    let second = adw::PreferencesPage::builder()
        .name("second")
        .title("Second")
        .build();
    settings.add(&second);
    let window = adw::Window::builder()
        .title("Mluva")
        .content(&settings.widget)
        .default_width(1060)
        .default_height(780)
        .build();
    window.present();
    let messages = Rc::new(RefCell::new(Vec::<String>::new()));
    let target = messages.clone();
    let command = settings_commands(
        &settings,
        Rc::new(move |_| target.borrow_mut().push("settings".into())),
    )
    .into_iter()
    .find(|command| command.title.ends_with("Refreshable setting"))
    .unwrap();
    let original = row.downgrade();
    let observe = || {
        let row = original.upgrade();
        json!({"page":settings.visible_page_name().map(|name|name.to_string()),"original_alive":row.is_some(),
            "original_parented":row.as_ref().map(|row|row.parent().is_some()),"original_focusable":row.map(|row|row.is_focusable()),"messages":*messages.borrow()})
    };
    settle();
    assert_eq!(observe(), reference["observations"][0]);
    settings.set_visible_page_name("second");
    group.remove(&row);
    drop(row);
    settle();
    assert_eq!(
        observe(),
        reference["observations"][1],
        "a deferred command must retain the original settings binding"
    );
    (command.run)();
    settle();
    assert_eq!(
        observe(),
        reference["observations"][2],
        "refreshed rows must not lose their original destination"
    );
    window.destroy();
    settle();
}

fn root_window_contract(root: &std::path::Path, data: &std::path::Path) {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/root-window-cases.json")).unwrap();
    let app = adw::Application::builder()
        .application_id("com.mluva.Linux")
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    assert!(
        !app.is_remote(),
        "the application action name must belong to this private process"
    );
    let owner = Rc::new(RefCell::new(std::rc::Weak::<ApplicationShell>::new()));
    let editor_owner = Rc::new(RefCell::new(std::rc::Weak::<EditorPages>::new()));
    let config = Rc::new(RefCell::new(AppConfig {
        welcome_completed: true,
        automatic_titles: false,
        ..Default::default()
    }));
    let events = Rc::new(RefCell::new(Vec::<String>::new()));
    let split = adw::OverlaySplitView::builder()
        .vexpand(true)
        .min_sidebar_width(200.0)
        .max_sidebar_width(232.0)
        .sidebar_width_fraction(0.23)
        .show_sidebar(false)
        .build();
    split.set_sidebar(Some(&gtk::Label::new(Some("Synthetic history sidebar"))));
    let live = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .vexpand(true)
        .build();
    let first = gtk::TextView::builder().hexpand(true).vexpand(true).build();
    live.append(&first);
    live.append(&gtk::TextView::builder().hexpand(true).vexpand(true).build());
    split.set_content(Some(&live));
    let capture_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    capture_actions.append(
        &gtk::Label::builder()
            .label("Synthetic capture controls")
            .hexpand(true)
            .build(),
    );
    let capture_buttons = gtk::Box::builder()
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let record = gtk::Button::builder()
        .label("Start dictation")
        .sensitive(false)
        .build();
    capture_buttons.append(&record);
    capture_actions.append(&capture_buttons);
    let capture = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .vexpand(true)
        .build();
    capture.append(&split);
    capture.append(&capture_actions);
    let external_received = Rc::new(RefCell::new(Vec::<Value>::new()));
    let weak = owner.clone();
    let settings = SettingsView::new(Rc::new(move || {
        if let Some(shell) = weak.borrow().upgrade() {
            shell.navigate("capture");
        }
    }));
    for (name, title) in [("workspace", "Workspace"), ("capture", "Capture")] {
        let page = adw::PreferencesPage::builder()
            .name(name)
            .title(title)
            .icon_name("preferences-system-symbolic")
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(format!("{title} choices"))
            .description("Synthetic preference boundaries.")
            .build();
        group.add(
            &adw::ActionRow::builder()
                .title(format!("Primary {title}"))
                .subtitle("Résumé Straße 😀")
                .build(),
        );
        group.add(
            &adw::ActionRow::builder()
                .title(format!("Secondary {title}"))
                .subtitle("Next capture only")
                .build(),
        );
        page.add(&group);
        settings.add(&page);
    }
    let config_home = PathBuf::from(std::env::var_os("XDG_CONFIG_HOME").unwrap());
    assert!(config_home.canonicalize().unwrap().starts_with(root));
    let history = HistoryStore::new(data.join("root-mluva/history.sqlite3"));
    history.initialize().unwrap();
    let personalization = Rc::new(RefCell::new(PersonalizationStore::new(
        config_home.join("mluva/personalization.json"),
    )));
    let prompts = Rc::new(RefCell::new(
        PromptStore::new(config_home.join("mluva/prompts"), "", &DEFAULTS.styles).unwrap(),
    ));
    let weak = owner.clone();
    let editors = EditorPages::new(
        &settings.widget,
        history,
        personalization,
        prompts.clone(),
        Rc::new(|| true),
        Rc::new(|| {}),
        Rc::new(move |message| {
            if let Some(shell) = weak.borrow().upgrade() {
                shell.show_message(message);
            }
        }),
    )
    .unwrap();
    editor_owner.replace(Rc::downgrade(&editors));
    settings.add(&editors.prompts.widget);
    let weak = owner.clone();
    let editor_target = editor_owner.clone();
    let received = external_received.clone();
    let invoke: Rc<dyn Fn(ApplicationAction)> = Rc::new(move |action| {
        let Some(shell) = weak.borrow().upgrade() else {
            return;
        };
        match action {
            ApplicationAction::Commands => shell.show_commands(),
            ApplicationAction::Settings => {
                shell.present();
                if let Some(editors) = editor_target.borrow().upgrade() {
                    editors.prompts.refresh().unwrap();
                }
                shell.navigate("settings");
            }
            ApplicationAction::History => {
                shell.present();
                shell.navigate("history");
            }
            ApplicationAction::Meeting => shell.navigate("meeting"),
            ApplicationAction::Personalization => shell.navigate("personalization"),
            ApplicationAction::Status => received.borrow_mut().push(json!(["status"])),
            ApplicationAction::Review {
                operation,
                identifier,
                option,
            } => received
                .borrow_mut()
                .push(json!([operation, identifier, option])),
            _ => {} // Capture, provider and delivery services are synthetic boundaries in this window check.
        }
    });
    register_actions(&app, invoke.clone());
    let weak = owner.clone();
    let sidebar = split.clone();
    let current = config.clone();
    let toggle_sidebar: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(shell) = weak.borrow().upgrade() {
            shell.navigate("capture");
        }
        let visible = !sidebar.shows_sidebar();
        current.borrow_mut().history_sidebar_visible = visible;
        sidebar.set_show_sidebar(visible);
    });
    let current = config.clone();
    let adaptive = Rc::new(AdaptiveWorkspace {
        split: split.clone(),
        live_panes: live.clone(),
        capture_actions: capture_actions.clone(),
        capture_buttons: capture_buttons.clone(),
        record_button: record.clone(),
        sidebar_visible: Rc::new(move || current.borrow().history_sidebar_visible),
        compact_documents: Rc::new(|_| {}),
        compact_recording: Rc::new(|_| {}),
    });
    let weak = owner.clone();
    let current = config.clone();
    let control = record.clone();
    let panes = live.clone();
    let action_target = invoke.clone();
    let sidebar = toggle_sidebar.clone();
    let editor_target = editor_owner.clone();
    let context = Rc::new(CommandContext {
        state: Rc::new(move || CommandState {
            page: weak
                .borrow()
                .upgrade()
                .and_then(|shell| shell.stack.visible_child_name())
                .unwrap_or_default()
                .to_string(),
            record_sensitive: control.is_sensitive(),
            live_enabled: current.borrow().live_rewrite_enabled,
            live_visible: panes.get_visible(),
            source_visible: true,
            draft_visible: true,
            pending_mode: "dictation".into(),
            ..Default::default()
        }),
        invoke: Rc::new(move |action| match action {
            CommandAction::Settings => action_target(ApplicationAction::Settings),
            CommandAction::History => action_target(ApplicationAction::History),
            CommandAction::ToggleSidebar => sidebar(),
            CommandAction::EditPrompt(identifier) => {
                if let Some(editors) = editor_target.borrow().upgrade() {
                    editors.open_prompt(&identifier);
                }
            }
            _ => {}
        }),
    });
    let indexed = settings.clone();
    let boundary = |text: &str| {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .vexpand(true)
            .build();
        widget.append(&gtk::Label::new(Some(text)));
        widget.upcast()
    };
    let cancelled = events.clone();
    let shell = ApplicationShell::new(
        &app,
        ShellPages {
            capture: capture.upcast(),
            settings: settings.widget.clone().upcast(),
            welcome: boundary("Synthetic welcome boundary"),
            meeting: boundary("Synthetic meeting boundary"),
            history: boundary("Synthetic history boundary"),
            personalization: editors.personalization.widget.clone().upcast(),
        },
        ShellCallbacks {
            invoke,
            toggle_sidebar,
            commands: Rc::new(move || {
                application_commands(&context, &indexed, prompts.borrow().catalog())
            }),
            cancel_capture: Rc::new(move || {
                cancelled.borrow_mut().push("cancel-capture".into());
                false
            }),
            compact: Rc::new(move |compact| adaptive.set_compact(compact)),
        },
    );
    owner.replace(Rc::downgrade(&shell));
    let weak = owner.clone();
    app.connect_activate(move |_| {
        if let Some(shell) = weak.borrow().upgrade() {
            shell.present();
        }
    });
    gtk::prelude::GtkWindowExt::set_focus(&shell.window, Some(&first));
    app.activate();
    settle();
    let observe = || {
        let controls: Vec<_> = widgets(&shell.header).iter().filter_map(|widget| {
            let tooltip = widget.tooltip_text()?;
            if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                Some(json!({"tooltip":tooltip.as_str(),"icon":button.icon_name().map(|icon|icon.to_string())}))
            } else {
                widget.downcast_ref::<gtk::MenuButton>().map(|button|json!({"tooltip":tooltip.as_str(),"icon":button.icon_name().map(|icon|icon.to_string())}))
            }
        }).collect();
        json!({"page":shell.stack.visible_child_name().map(|name|name.to_string()),"title":shell.page_title.text().as_str(),"header_title":shell.header.shows_title(),
            "window":{"title":shell.window.title().map(|title|title.to_string()),"size":[shell.window.width(),shell.window.height()],"minimum":shell.window.size_request(),
                "visible":shell.window.get_visible(),"count":app.windows().len()},
            "stack":{"horizontal":shell.stack.is_hhomogeneous(),"vertical":shell.stack.is_vhomogeneous()},
            "capture":{"collapsed":split.is_collapsed(),"sidebar":split.shows_sidebar(),"live_orientation":enum_nick(&live.orientation()),
                "action_orientation":enum_nick(&capture_actions.orientation()),"compact":capture_actions.has_css_class("compact"),"buttons_alignment":enum_nick(&capture_buttons.halign()),
                "record_expand":record.hexpands(),"sidebar_setting":config.borrow().history_sidebar_visible},
            "dialog":shell.window.visible_dialog().map(|dialog|dialog.title().to_string()),"settings_page":settings.visible_page_name().map(|name|name.to_string()),"events":*events.borrow(),"controls":controls})
    };
    assert_eq!(
        observe(),
        reference["observations"][0],
        "initial root window"
    );
    paint(&shell.window, "root-initial");
    let mut protected = None;
    for (index, action) in reference["actions"].as_array().unwrap().iter().enumerate() {
        let operation = action["op"].as_str().unwrap();
        match operation {
            "button" => {
                let button = widgets(&shell.header)
                    .into_iter()
                    .find_map(|widget| {
                        widget.downcast::<gtk::Button>().ok().filter(|button| {
                            button.tooltip_text().as_deref() == action["tooltip"].as_str()
                        })
                    })
                    .unwrap();
                button.emit_clicked();
            }
            "key" => key(action["chord"].as_str().unwrap()),
            "action" => app.activate_action(action["name"].as_str().unwrap(), None),
            "size" => shell.window.set_default_size(
                action["width"].as_i64().unwrap() as i32,
                action["height"].as_i64().unwrap() as i32,
            ),
            "navigate" => shell.navigate(action["name"].as_str().unwrap()),
            "protected-dialog" => {
                let dialog = adw::Dialog::builder()
                    .title("Synthetic protected dialog")
                    .child(&gtk::Label::new(Some("Dialog boundary")))
                    .build();
                dialog.present(Some(&shell.window));
                protected = Some(dialog);
            }
            "hide" => shell.window.close(),
            "activate" => app.activate(),
            "settings-page" => {
                assert!(settings.set_visible_page_name(action["name"].as_str().unwrap()));
            }
            "edit-prompt" => editors.open_prompt(action["identifier"].as_str().unwrap()),
            operation => panic!("unknown root operation {operation}"),
        }
        settle();
        assert_eq!(
            observe(),
            reference["observations"][index + 1],
            "root step {index}: {action}"
        );
        if operation == "size"
            || operation == "settings-page"
            || (operation == "action" && action["name"] == "personalization")
        {
            paint(&shell.window, &format!("root-step-{index}"));
        }
    }
    let types: serde_json::Map<_, _> = app
        .list_actions()
        .into_iter()
        .map(|name| {
            let value = app
                .action_parameter_type(&name)
                .map(|value| Value::String(value.as_str().into()))
                .unwrap_or(Value::Null);
            (name.to_string(), value)
        })
        .collect();
    assert_eq!(Value::Object(types), reference["registered_actions"]);
    let accelerators: Vec<_> = app
        .accels_for_action("app.commands")
        .iter()
        .map(|value| value.to_string())
        .collect();
    assert_eq!(json!(accelerators), reference["command_accels"]);
    for call in reference["external_actions"]["calls"].as_array().unwrap() {
        let mut process = Process::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "com.mluva.Linux",
                "--object-path",
                "/com/mluva/Linux",
                "--method",
                "org.gtk.Actions.Activate",
                call["name"].as_str().unwrap(),
                call["parameters"].as_str().unwrap(),
                "{}",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                process.kill().unwrap();
                process.wait().unwrap();
                panic!("private action call timed out");
            }
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(5));
        }
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "private action failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        settle();
    }
    assert_eq!(
        json!(*external_received.borrow()),
        reference["external_actions"]["received"]
    );
    shell.window.destroy();
    settle();
    drop(protected);
    app.quit();
}

fn paint(window: &adw::ApplicationWindow, name: &str) {
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(window)).snapshot(
        &snapshot,
        f64::from(window.width()),
        f64::from(window.height()),
    );
    let viewport =
        gtk::graphene::Rect::new(0.0, 0.0, window.width() as f32, window.height() as f32);
    let texture = window
        .renderer()
        .unwrap()
        .render_texture(snapshot.to_node().unwrap(), Some(&viewport));
    texture
        .save_to_png(
            PathBuf::from(std::env::var_os("OFFSCREEN_ARTIFACT_DIR").unwrap())
                .join(format!("{name}.png")),
        )
        .unwrap();
}

fn enum_nick(value: &impl ToValue) -> String {
    glib::EnumValue::from_value(&value.to_value())
        .unwrap()
        .1
        .nick()
        .into()
}
