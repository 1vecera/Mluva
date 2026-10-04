//! A private desktop comparison surface using the production native widgets and stores.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use mluva_core::history::HistoryStore;
use mluva_core::personalization::PersonalizationStore;
use mluva_gtk::personalization::PersonalizationPage;
use mluva_gtk::theme::ThemeController;

fn main() -> glib::ExitCode {
    let Some(root) = std::env::var_os("OFFSCREEN_SESSION_ROOT").map(PathBuf::from) else {
        eprintln!("Run this comparison inside the private off-screen desktop helper.");
        return glib::ExitCode::FAILURE;
    };
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .expect("private data directory");
    assert!(
        data.canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    let app = adw::Application::builder()
        .application_id("cz.mluva.NativeComparison")
        .build();
    app.connect_activate(move |app| {
        let theme = ThemeController::apply(
            root.join("state/omarchy/current/theme"),
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"),
        )
        .expect("bundled fonts");
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
        toolbar.add_top_bar(&header);
        let overlay = adw::ToastOverlay::new();
        let history = HistoryStore::new(data.join("mluva/history.sqlite3"));
        history.initialize().unwrap();
        let store = Rc::new(RefCell::new(PersonalizationStore::new(
            data.join("mluva/personalization.json"),
        )));
        let weak = overlay.downgrade();
        let page = PersonalizationPage::new(
            store,
            history,
            Rc::new(move |message| {
                if let Some(overlay) = weak.upgrade() {
                    overlay.add_toast(adw::Toast::new(message));
                }
            }),
            Rc::new(|| {}),
            None,
        );
        overlay.set_child(Some(&page.widget));
        toolbar.set_content(Some(&overlay));
        window.set_content(Some(&toolbar));
        window.connect_destroy(move |_| {
            let _ = (&page, &theme);
        });
        window.present();
    });
    app.run()
}
