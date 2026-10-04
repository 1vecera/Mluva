//! Independently observed sketch decoding, actual WebKit/native textures and scroll forecasting.

use adw::prelude::*;
use mluva_gtk::conversation_view::split_grilling_draft;
use mluva_gtk::document_layout::{DocumentResources, SpeechScrollForecast};
use mluva_gtk::markdown_view::MarkdownTextView;
use mluva_gtk::mermaid::{MermaidPreview, mermaid_blocks, native_svg};
use mluva_gtk::theme::ThemeController;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
#[path = "support/diagram_transitions.rs"]
mod diagram_transitions;

fn reference() -> Value {
    serde_json::from_str(include_str!("fixtures/released-document-surfaces.json")).unwrap()
}

#[test]
fn sketch_bounds_xml_policy_and_speech_forecast_match_released_observations() {
    let reference = reference();
    assert_eq!(
        reference["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let questions: Value =
        serde_json::from_str(include_str!("fixtures/released-conversation-page.json")).unwrap();
    for case in questions["pure"].as_array().unwrap() {
        let (head, body) = split_grilling_draft(case["text"].as_str().unwrap());
        assert_eq!(json!([head, body]), case["split"]);
    }
    for (index, case) in reference["blocks"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            serde_json::to_value(mermaid_blocks(case["source"].as_str().unwrap())).unwrap(),
            case["expected"],
            "fence case {index}"
        );
    }
    let mut differences = Vec::new();
    for (index, case) in reference["xml"].as_array().unwrap().iter().enumerate() {
        let result = native_svg(case["svg"].as_str().unwrap());
        let actual = match result {
            Ok(_) => json!({"accepted":true,"category":null}),
            Err(error) => {
                json!({"accepted":false,"category":if error == "Invalid sketch XML" { "xml" } else { &error }})
            }
        };
        if actual != case["expected"] {
            differences.push(json!({"case":index,"actual":actual,"expected":case["expected"]}));
        }
    }
    assert!(differences.is_empty(), "SVG differences: {differences:?}");
    let mut forecast = SpeechScrollForecast::default();
    let mut previous = None;
    for case in reference["forecast"].as_array().unwrap() {
        let sample = (
            case["characters"].as_u64().unwrap() as usize,
            case["now"].as_f64().unwrap(),
        );
        if previous != Some(sample) {
            forecast.observe(sample.0, sample.1);
            previous = Some(sample);
        }
        let actual = forecast.reserve(
            case["columns"].as_f64().unwrap(),
            case["fill"].as_f64().unwrap(),
            case["horizon"].as_f64().unwrap(),
            case["limit"].as_i64().unwrap(),
        );
        assert!(
            (actual - case["expected"].as_f64().unwrap()).abs() < 1e-12,
            "forecast mismatch at {sample:?}"
        );
    }
}

fn settle() {
    let until = Instant::now() + Duration::from_millis(50);
    let context = glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(4));
    }
}

#[test]
#[ignore = "requires real WebKitGTK on the private display, network/PID namespace and device-free runner"]
fn offline_mermaid_textures_and_lossless_documents_match_released_rendering() {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_string_lossy(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for p in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!std::path::Path::new(p).exists());
    }
    assert!(
        PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
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
    window.present();
    for case in reference()["renders"].as_array().unwrap() {
        let editor = MarkdownTextView::document(case["source"].as_str().unwrap(), true, true);
        let preview = MermaidPreview::new(&editor, &resources);
        body.append(&editor);
        body.append(&preview.widget);
        let pictures = || {
            let mut result = Vec::new();
            let mut child = preview.widget.first_child();
            while let Some(c) = child {
                if let Some(p) = c.downcast_ref::<gtk::Picture>() {
                    result.push(p.clone());
                }
                child = c.next_sibling();
            }
            result
        };
        let until = Instant::now() + Duration::from_secs(15);
        while Instant::now() < until && pictures().is_empty() && preview.notice.label().is_empty() {
            settle();
        }
        assert!(
            Instant::now() < until,
            "native sketch did not finish: {}",
            case["name"]
        );
        let textures=pictures().into_iter().enumerate().map(|(index,picture)| {let texture=picture.paintable().and_downcast::<gtk::gdk::Texture>().unwrap();let path=root.join(format!("native-{}-{index}.png",case["name"].as_str().unwrap()));texture.save_to_png(&path).unwrap();let output=Command::new("sha256sum").arg(&path).output().unwrap();assert!(output.status.success());json!({"width":texture.width(),"height":texture.height(),"sha256":String::from_utf8(output.stdout).unwrap().split_whitespace().next().unwrap()})}).collect::<Vec<_>>();
        let buffer = editor.buffer();
        let actual = json!({"textures":textures,"notice":preview.notice.label().to_string(),"notice_visible":preview.notice.get_visible(),"source":editor.text(),"visible":buffer.text(&buffer.start_iter(),&buffer.end_iter(),false).to_string(),"preview_visible":preview.widget.get_visible()});
        assert_eq!(actual, case["expected"], "native sketch {}", case["name"]);
        eprintln!("native_sketch={} PASS", case["name"]);
        body.remove(&preview.widget);
        body.remove(&editor);
        drop(preview);
        settle();
    }
    diagram_transitions::exercise(&body, &resources, &root);
    window.close();
    settle();
}
