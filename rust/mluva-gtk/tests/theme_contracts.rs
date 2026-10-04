use adw::prelude::*;
use mluva_gtk::theme::{Palette, ThemeController, build_stylesheet, read_omarchy_palette};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Reference {
    css: Vec<Stylesheet>,
    palettes: Vec<PaletteCase>,
}
#[derive(Deserialize)]
struct Stylesheet {
    tokens: Palette,
    css: String,
}
#[derive(Deserialize)]
struct PaletteCase {
    source: String,
    expected: Option<(Palette, bool)>,
}

#[test]
fn native_semantic_colors_and_validated_omarchy_palettes_match_released_stylesheets() {
    let reference: Reference =
        serde_json::from_str(include_str!("fixtures/theme-cases.json")).unwrap();
    for case in reference.css {
        assert_eq!(build_stylesheet(&case.tokens), case.css);
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("colors.toml");
    for case in reference.palettes {
        fs::write(&path, &case.source).unwrap();
        assert_eq!(
            read_omarchy_palette(&path),
            case.expected,
            "{}",
            case.source
        );
    }
    fs::remove_file(&path).unwrap();
    assert!(read_omarchy_palette(&path).is_none());
}

fn pump(duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn scheme() -> Value {
    let manager = adw::StyleManager::default();
    json!({
        "dark": manager.is_dark(),
        "scheme": glib::EnumValue::from_value(&manager.color_scheme().to_value()).unwrap().1.nick(),
    })
}

// GTK's public named-color lookup observes the installed CSS, not generated source.
#[allow(deprecated)]
fn observe(window: &adw::Window, name: &str) -> Value {
    let mut value = scheme();
    value["name"] = json!(name);
    value["width"] = json!(window.width());
    value["height"] = json!(window.height());
    value["colors"] = [
        "vs_canvas",
        "vs_ink",
        "vs_action",
        "window_bg_color",
        "accent_bg_color",
    ]
    .into_iter()
    .map(|key| {
        (
            key.to_owned(),
            json!(
                window
                    .style_context()
                    .lookup_color(key)
                    .map(|color| color.to_string())
            ),
        )
    })
    .collect::<serde_json::Map<_, _>>()
    .into();
    value
}

fn paint(window: &adw::Window, path: &Path) {
    assert!(window.is_mapped());
    // Match the reference's actual X11 surface capture, including its border and alpha.
    let result = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Mluva theme comparison$",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let identifiers = String::from_utf8(result.stdout).unwrap();
    let identifiers = identifiers.lines().collect::<Vec<_>>();
    assert_eq!(identifiers.len(), 1);
    assert!(
        Command::new("import")
            .args(["-window", identifiers[0]])
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
#[ignore = "requires private X11/buses/HOME/XDG/network/devices via the isolated browser runner"]
fn open_window_follows_theme_replacement_and_recovers_like_release() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XAUTHORITY",
    ] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&private)
        );
    }
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for path in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists());
    }
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/released-theme-lifecycle.json")).unwrap();
    assert_eq!(
        reference["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    adw::init().unwrap();
    let manager = adw::StyleManager::default();
    manager.set_color_scheme(adw::ColorScheme::ForceLight);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    let root = private.join("native-theme");
    fs::create_dir(&root).unwrap();
    let palettes = &reference["palettes"];
    for (name, source) in palettes.as_object().unwrap() {
        fs::create_dir(root.join(name)).unwrap();
        fs::write(
            root.join(name).join("colors.toml"),
            source.as_str().unwrap(),
        )
        .unwrap();
    }
    let link = root.join("theme");
    let fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
    let theme = ThemeController::apply(&link, &fonts).unwrap();
    let window = adw::Window::builder()
        .title("Mluva theme comparison")
        .default_width(520)
        .default_height(160)
        .build();
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    let label = gtk::Label::new(Some("Mluva follows your Omarchy theme"));
    label.add_css_class("title-2");
    body.append(&label);
    body.append(
        &gtk::Entry::builder()
            .text("Žluťoučký kůň · a readable note")
            .build(),
    );
    let button = gtk::Button::with_label("Polish");
    button.add_css_class("suggested-action");
    body.append(&button);
    window.set_content(Some(&body));
    window.present();
    let (closed_reference, open_states) = reference["states"]
        .as_array()
        .unwrap()
        .split_last()
        .unwrap();
    assert_eq!(closed_reference["name"], "closed-no-reload");
    let mut states = vec![];
    for expected in open_states {
        let name = expected["name"].as_str().unwrap();
        match name {
            "initial" => {}
            "tokyo-night" | "rose-pine" => {
                if link.is_symlink() {
                    fs::remove_file(&link).unwrap();
                }
                symlink(root.join(name), &link).unwrap();
            }
            "malformed" => fs::write(link.join("colors.toml"), "invalid = [").unwrap(),
            "rewritten" => fs::write(
                link.join("colors.toml"),
                palettes["tokyo-night"].as_str().unwrap(),
            )
            .unwrap(),
            "removed" => fs::remove_file(link.join("colors.toml")).unwrap(),
            "system-dark" => manager.set_color_scheme(adw::ColorScheme::ForceDark),
            "system-light" => manager.set_color_scheme(adw::ColorScheme::ForceLight),
            "recovered" => fs::write(
                link.join("colors.toml"),
                palettes["rose-pine"].as_str().unwrap(),
            )
            .unwrap(),
            _ => panic!("unknown theme transition {name}"),
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            pump(Duration::from_millis(50));
            let actual = observe(&window, name);
            if &actual == expected && window.is_mapped() {
                // A second settled observation must retain the complete state.
                pump(Duration::from_millis(50));
                if observe(&window, name) == actual {
                    states.push(actual);
                    paint(&window, &root.join(format!("{name}.png")));
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "theme {name}: {actual} != {expected}"
            );
        }
    }
    drop(theme);
    fs::write(
        link.join("colors.toml"),
        palettes["tokyo-night"].as_str().unwrap(),
    )
    .unwrap();
    // The release's close retains CSS but stops reloading. Native Drop additionally
    // removes the provider; compare the shared scheme behavior and test cleanup separately.
    pump(Duration::from_secs(3));
    let closed = scheme();
    for key in ["scheme", "dark"] {
        assert_eq!(closed[key], closed_reference[key]);
    }
    #[allow(deprecated)]
    {
        assert!(window.style_context().lookup_color("vs_canvas").is_none());
    }
    // Preserve the former document test's restoration contract when dropped while dark.
    let dark = ThemeController::apply(&link, &fonts).unwrap();
    assert_eq!(manager.color_scheme(), adw::ColorScheme::ForceDark);
    drop(dark);
    assert_eq!(manager.color_scheme(), adw::ColorScheme::ForceLight);
    fs::write(root.join("receipt.json"), serde_json::to_vec_pretty(&json!({"states":states,"closed":closed,"provider_removed":true,"previous_scheme_restored":true})).unwrap()).unwrap();
    window.destroy();
    eprintln!(
        "Native open-window theme matched nine released states, close/no-reload and native cleanup; evidence {}",
        root.display()
    );
}
