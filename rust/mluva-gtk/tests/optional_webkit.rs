//! Launch and actual editable document behavior when the optional system
//! renderer cannot load. Run only through support/without-webkit.sh.
use adw::prelude::*;
use mluva_gtk::{
    document_layout::DocumentResources, markdown_view::MarkdownTextView, mermaid::MermaidPreview,
    theme::ThemeController,
};
use serde_json::{Value, json};
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn settle() {
    let until = Instant::now() + Duration::from_millis(750);
    let context = glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(4));
    }
}

#[test]
#[ignore = "requires the guarded desktop runner and support/without-webkit.sh library mask"]
fn missing_optional_renderer_preserves_launch_and_editable_source() {
    let root = PathBuf::from(env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert_eq!(env::var("MLUVA_TEST_WITHOUT_WEBKIT").unwrap(), "1");
    assert_eq!(
        fs::metadata("/usr/lib/libwebkitgtk-6.0.so.4")
            .unwrap()
            .len(),
        0
    );
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    assert!(
        PathBuf::from(env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert!(env::var_os("WAYLAND_DISPLAY").is_none());
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!std::path::Path::new(path).exists());
    }

    // A main executable linked to WebKit cannot reach --help in this mount.
    let help = Command::new(env!("CARGO_BIN_EXE_mluva"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(help.stderr.is_empty());
    let bootstrap: Value =
        serde_json::from_str(include_str!("fixtures/released-bootstrap.json")).unwrap();
    assert_eq!(
        String::from_utf8(help.stdout).unwrap(),
        bootstrap["cli"][0]["stdout"]
    );

    adw::init().unwrap();
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
    let resources = DocumentResources::from_directory(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    );
    let _theme = ThemeController::apply(
        root.join("state/omarchy/current/theme"),
        resources.font.parent().unwrap(),
    )
    .unwrap();
    let window = adw::Window::builder()
        .title("Mluva")
        .default_width(1060)
        .default_height(780)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_content(Some(&body));
    let editor = MarkdownTextView::document("", true, true);
    let preview = MermaidPreview::new(&editor, &resources);
    body.append(&editor);
    body.append(&preview.widget);
    window.present();
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-optional-webkit.json")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        editor.buffer().set_text(case["source"].as_str().unwrap());
        settle();
        let mut child = preview.widget.first_child();
        let (mut pictures, mut web) = (0, false);
        while let Some(widget) = child {
            pictures += usize::from(widget.is::<gtk::Picture>());
            web |= widget.type_().name() == "WebKitWebView";
            child = widget.next_sibling();
        }
        let buffer = editor.buffer();
        let observed = json!({"source":editor.text(), "visible":buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string(),
            "editable":editor.is_editable(), "notice":preview.notice.label().to_string(), "notice_visible":preview.notice.get_visible(),
            "preview_visible":preview.widget.get_visible(), "pictures":pictures, "web":web});
        assert_eq!(observed, case["expected"], "{}", case["name"]);
        eprintln!("Matched missing WebKit {}", case["name"]);
    }
    assert!(
        !fs::read_to_string("/proc/self/maps")
            .unwrap()
            .contains("libwebkitgtk-6.0")
    );
    window.close();
    settle();
}
