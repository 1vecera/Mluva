//! Real settings remain reachable in one viewport, with shared command and save bindings.
use adw::prelude::*;
use mluva_core::config::{AppConfig, AppPaths};
use mluva_gtk::{
    application::{ApplicationDesktop, ApplicationPlatform},
    async_runtime::DesktopRuntime,
    commands::{CommandContext, CommandState, application_commands},
    document_layout::DocumentResources,
    theme::ThemeController,
};
use mluva_workflows::services::{ApplicationServices, NativeBinaries};
use std::{path::PathBuf, process::Command, rc::Rc};

#[path = "support/dictation_ui.rs"]
mod ui;

#[test]
#[ignore = "requires the private display, network, device and clipboard runner"]
fn all_settings_scroll_jump_focus_and_save_without_hiding_other_sections() {
    let root = ui::isolated();
    let size = std::env::var("MLUVA_SETTINGS_VIEWPORT").unwrap_or_else(|_| "1060x780".into());
    let (width, height) = size.split_once('x').unwrap();
    let (width, height) = (
        width.parse::<i32>().unwrap(),
        height.parse::<i32>().unwrap(),
    );
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let paths = AppPaths {
        config: root.join("settings-config"),
        data: root.join("settings-data"),
        runtime: root.join("settings-runtime"),
    };
    AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: "http://127.0.0.1:9/v1".into(),
        transcription_api_key_env: "MLUVA_UNUSED_TEST_KEY".into(),
        transcription_remote_model: Some("fixture".into()),
        welcome_completed: true,
        ..Default::default()
    }
    .save(&paths.config.join("config.json"))
    .unwrap();
    let services = ApplicationServices::open(paths.clone()).unwrap();
    let app = adw::Application::builder()
        .application_id("com.mluva.SettingsAcceptance")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(gio::Cancellable::NONE).unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug");
    let owner = ApplicationDesktop::new(
        &app,
        services.clone(),
        DesktopRuntime::new().unwrap(),
        resources,
        NativeBinaries {
            asr_worker: target.join("mluva-asr-worker"),
            audio_cleanup: target.join("mluva-audio-cleanup"),
            screenshot_editor: target.join("mluva-screenshot-editor"),
        },
        ApplicationPlatform {
            compact_recording: Rc::new(|_| {}),
            close: Rc::new(|| {}),
        },
    )
    .unwrap();
    let window = &owner.shell.window;
    window.set_default_size(width, height);
    app.activate_action("settings", None);
    let view = &owner.settings.view;
    ui::until(|| {
        (width - 24..=width).contains(&window.width())
            && (height - 24..=height).contains(&window.height())
            && view.scroll.height() > 0
            && view.pages().iter().all(|page| page.height() > 0)
    });
    let pages = view.pages();
    assert_eq!(
        pages
            .iter()
            .map(|page| page.name().unwrap().to_string())
            .collect::<Vec<_>>(),
        [
            "account",
            "workspace",
            "providers",
            "capture",
            "audio",
            "privacy",
            "advanced"
        ]
    );
    assert_eq!(view.visible_page_name().as_deref(), Some("account"));
    assert!(!view.set_visible_page_name("feature-maturity"));
    assert!(
        ui::widgets(&view.widget)
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Label>())
            .all(|label| label.label() != "Feature maturity")
    );
    let adjustment = view.scroll.vadjustment();
    assert!(adjustment.upper() > adjustment.page_size() * 4.0);
    ui::paint(window, &format!("settings-{size}-account"));

    for page in &pages {
        let button = ui::button(&view.widget, page.title().as_str());
        button.emit_clicked();
        ui::until(|| section_in_view(page, &view.scroll));
        assert!(pages.iter().all(|page| page.is_mapped()));
        // Every page fits its content; none retains an independent scroll range.
        for scroller in ui::widgets(page)
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
            .filter(|scroller| scroller.vscrollbar_policy() == gtk::PolicyType::Never)
        {
            let inner = scroller.vadjustment();
            assert!(inner.upper() - inner.page_size() <= 1.0);
        }
    }
    ui::paint(window, &format!("settings-{size}-bottom"));
    adjustment.set_value(0.0);
    ui::until(|| view.visible_page_name().as_deref() == Some("account"));
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    ui::until(|| view.visible_page_name().as_deref() == Some("advanced"));

    // A real keyboard activation uses only this runner's private X11 display.
    let capture = ui::button(&view.widget, "Capture");
    assert!(capture.grab_focus());
    assert!(
        Command::new("xdotool")
            .args(["key", "--clearmodifiers", "space"])
            .status()
            .unwrap()
            .success()
    );
    ui::until(|| section_in_view(&owner.settings.capture.capture, &view.scroll));
    let focus = gtk::prelude::RootExt::focus(window).unwrap();
    assert!(focus.is_ancestor(&owner.settings.capture.capture.parent().unwrap()));
    assert!(window.child_focus(gtk::DirectionType::TabForward));

    // Search still expands and focuses the original row in the same continuous page.
    let commands = application_commands(
        &Rc::new(CommandContext {
            state: Rc::new(|| CommandState {
                page: "settings".into(),
                ..Default::default()
            }),
            invoke: Rc::new(|_| {}),
        }),
        view,
        &pages,
    );
    let endpoint = &owner.settings.providers.speech.endpoint_entry;
    owner.settings.providers.speech.advanced.set_expanded(false);
    let command = commands
        .iter()
        .find(|command| command.title.ends_with(endpoint.title().as_str()))
        .unwrap();
    (command.run)();
    ui::until(|| {
        owner.settings.providers.speech.advanced.is_expanded()
            && endpoint.compute_bounds(&view.scroll).is_some_and(|bounds| {
                bounds.y() >= 0.0 && bounds.y() + bounds.height() <= view.scroll.height() as f32
            })
    });
    ui::paint(window, &format!("settings-{size}-providers"));
    owner.settings.capture.language.set_selected(2);
    ui::until(|| services.config().language_code == "ces");
    assert_eq!(
        AppConfig::load(&paths.config.join("config.json"))
            .unwrap()
            .language_code,
        "ces"
    );
    ui::button(&view.widget, "Back to workspace · Esc").emit_clicked();
    ui::until(|| owner.shell.stack.visible_child_name().as_deref() == Some("capture"));
    ui::until(|| !owner.settings.providers.speech.widget.is_mapped());
    app.activate_action("account", None);
    ui::until(|| section_in_view(&owner.account.page, &view.scroll));
    assert_eq!(owner.settings.capture.language.selected(), 2);
    glib::MainContext::default()
        .block_on(owner.shutdown())
        .unwrap();
    window.close();
    app.quit();
}

fn section_in_view(page: &adw::PreferencesPage, scroll: &gtk::ScrolledWindow) -> bool {
    page.parent()
        .unwrap()
        .compute_bounds(scroll)
        .is_some_and(|bounds| bounds.y() >= -1.0 && bounds.y() < scroll.height() as f32)
}
