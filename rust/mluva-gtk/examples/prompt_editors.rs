//! A disposable desktop surface around the production shared prompt/style editors.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use mluva_core::history::HistoryStore;
use mluva_core::personalization::PersonalizationStore;
use mluva_core::prompt_catalog::DEFAULTS;
use mluva_core::prompts::PromptStore;
use mluva_gtk::editor_pages::EditorPages;
use mluva_gtk::theme::ThemeController;

fn main() -> glib::ExitCode {
    let root = std::env::var_os("OFFSCREEN_SESSION_ROOT")
        .map(PathBuf::from)
        .expect("private off-screen desktop required")
        .canonicalize()
        .unwrap();
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .expect("private data directory")
        .canonicalize()
        .unwrap();
    assert!(data.starts_with(&root));
    let writable = !std::env::args().any(|argument| argument == "--view-only");
    let app = adw::Application::builder()
        .application_id("cz.mluva.NativePromptComparison")
        .build();
    app.connect_activate(move |app| {
        // Avoid cursor-phase differences in screenshots; the production widgets keep GTK defaults.
        gtk::Settings::default()
            .unwrap()
            .set_gtk_cursor_blink(false);
        gtk::Settings::default()
            .unwrap()
            .set_gtk_enable_animations(false);
        let theme = ThemeController::apply(
            root.join("state/omarchy/current/theme"),
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
        )
        .unwrap();
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
        let stack = adw::ViewStack::new();
        let switcher = adw::ViewSwitcher::builder()
            .stack(&stack)
            .policy(adw::ViewSwitcherPolicy::Narrow)
            .build();
        header.set_title_widget(Some(&switcher));
        toolbar.add_top_bar(&header);
        let overlay = adw::ToastOverlay::new();
        let history = HistoryStore::new(data.join("mluva/history.sqlite3"));
        history.initialize().unwrap();
        let personalization = Rc::new(RefCell::new(PersonalizationStore::new(
            data.join("mluva/personalization.json"),
        )));
        let styles = DEFAULTS
            .styles
            .iter()
            .chain(&personalization.borrow().state().custom_styles)
            .cloned()
            .collect::<Vec<_>>();
        let prompts = Rc::new(RefCell::new(
            PromptStore::new(
                data.join("mluva/prompts"),
                "Legacy custom instructions.",
                &styles,
            )
            .unwrap(),
        ));
        let weak = overlay.downgrade();
        let pages = EditorPages::new(
            &window,
            history,
            personalization,
            prompts,
            Rc::new(move || writable),
            Rc::new(|| {}),
            Rc::new(move |message| {
                if let Some(overlay) = weak.upgrade() {
                    overlay.add_toast(adw::Toast::new(message));
                }
            }),
        )
        .unwrap();
        stack.add_titled_with_icon(
            &pages.prompts.widget,
            Some("prompts"),
            "Prompts",
            "document-edit-symbolic",
        );
        stack.add_titled_with_icon(
            &pages.personalization.widget,
            Some("personalization"),
            "Personalization",
            "preferences-other-symbolic",
        );
        overlay.set_child(Some(&stack));
        toolbar.set_content(Some(&overlay));
        window.set_content(Some(&toolbar));
        window.connect_destroy(move |_| {
            let _ = (&pages, &theme);
        });
        window.present();
    });
    app.run_with_args(&["mluva-native-prompt-comparison"])
}
