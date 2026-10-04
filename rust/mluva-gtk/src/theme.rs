//! Follow the system scheme or a valid Omarchy palette without editing desktop configuration.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use adw::prelude::*;
use serde::Deserialize;

pub type Palette = BTreeMap<String, String>;

#[derive(Deserialize)]
struct DefaultPalettes {
    light: Palette,
    dark: Palette,
}
static DEFAULTS: LazyLock<DefaultPalettes> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/default-palettes.json"))
        .expect("reviewed semantic palettes")
});

pub fn default_palette(dark: bool) -> Palette {
    if dark {
        DEFAULTS.dark.clone()
    } else {
        DEFAULTS.light.clone()
    }
}

fn color(value: &str) -> Option<[u8; 3]> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some([
        u8::from_str_radix(&value[1..3], 16).ok()?,
        u8::from_str_radix(&value[3..5], 16).ok()?,
        u8::from_str_radix(&value[5..7], 16).ok()?,
    ])
}

fn blend(first: &str, second: &str, weight: f64) -> String {
    let first = color(first).expect("validated color");
    let second = color(second).expect("validated color");
    let value: Vec<_> = first
        .into_iter()
        .zip(second)
        .map(|(a, b)| {
            (f64::from(a) * weight + f64::from(b) * (1.0 - weight)).round_ties_even() as u8
        })
        .collect();
    format!("#{:02X}{:02X}{:02X}", value[0], value[1], value[2])
}

pub fn read_omarchy_palette(path: &Path) -> Option<(Palette, bool)> {
    let source = fs::read_to_string(path).ok()?;
    let values: toml::Table = toml::from_str(&source).ok()?;
    let required = |name: &str| {
        values
            .get(name)?
            .as_str()
            .filter(|value| color(value).is_some())
    };
    let background = required("background")?;
    let foreground = required("foreground")?;
    let accent = required("accent")?;
    let dark = values
        .get("mode")
        .map(|value| value.as_str() == Some("dark"))
        .unwrap_or(true);
    let optional = |name: &str, fallback: String| {
        values
            .get(name)
            .and_then(toml::Value::as_str)
            .filter(|value| color(value).is_some())
            .map(str::to_owned)
            .unwrap_or(fallback)
    };
    let danger = optional("red", accent.into());
    let success = optional("green", accent.into());
    let warning = optional("yellow", accent.into());
    let values = [
        ("canvas", background.into()),
        ("surface", background.into()),
        (
            "surface_subtle",
            optional("selection", blend(foreground, background, 0.06)),
        ),
        ("ink", foreground.into()),
        ("ink_secondary", blend(foreground, background, 0.85)),
        ("ink_muted", blend(foreground, background, 0.65)),
        (
            "outline",
            optional("muted", blend(foreground, background, 0.3)),
        ),
        ("outline_subtle", blend(foreground, background, 0.18)),
        ("shadow", background.into()),
        ("action", accent.into()),
        ("action_hover", blend(accent, foreground, 0.85)),
        ("on_action", background.into()),
        ("accent_strong", accent.into()),
        (
            "accent_soft",
            optional("selection", blend(accent, background, 0.15)),
        ),
        ("danger", danger.clone()),
        ("on_danger", background.into()),
        ("danger_soft", blend(&danger, background, 0.12)),
        ("success", success.clone()),
        ("success_soft", blend(&success, background, 0.12)),
        ("warning", warning.clone()),
        ("warning_soft", blend(&warning, background, 0.12)),
        ("focus", accent.into()),
    ];
    Some((
        values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        dark,
    ))
}

pub fn build_stylesheet(tokens: &Palette) -> String {
    const NAMES: [&str; 22] = [
        "canvas",
        "surface",
        "surface_subtle",
        "ink",
        "ink_secondary",
        "ink_muted",
        "outline",
        "outline_subtle",
        "shadow",
        "action",
        "action_hover",
        "on_action",
        "accent_strong",
        "accent_soft",
        "danger",
        "on_danger",
        "danger_soft",
        "success",
        "success_soft",
        "warning",
        "warning_soft",
        "focus",
    ];
    let mut css = String::from("\n");
    for name in NAMES {
        css.push_str(&format!("@define-color vs_{name} {};\n", tokens[name]));
    }
    css.push_str(&format!(
        "@define-color vs_divider {};\n",
        blend(&tokens["outline_subtle"], &tokens["canvas"], 0.35)
    ));
    css.push_str(include_str!("../resources/color-aliases.css"));
    css.push_str(include_str!("../resources/desktop.css"));
    css
}

#[link(name = "fontconfig")]
unsafe extern "C" {
    fn FcConfigAppFontAddFile(config: *mut libc::c_void, filename: *const u8) -> libc::c_int;
}

/// Register bundled fonts in this process; leave the user's installed font set unchanged.
pub fn load_bundled_fonts(directory: &Path) -> std::io::Result<()> {
    for face in ["Regular", "Medium", "Bold", "Italic"] {
        let path = directory.join(format!("JetBrainsMono-{face}.ttf"));
        let filename = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::other("Invalid font path"))?;
        // SAFETY: null uses the process's current Fontconfig configuration; filename is terminated and valid for the call.
        if unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), filename.as_ptr().cast()) } == 0 {
            return Err(std::io::Error::other(format!(
                "Could not load the bundled JetBrains Mono {face} face"
            )));
        }
    }
    Ok(())
}

pub struct ThemeController {
    directory: PathBuf,
    provider: gtk::CssProvider,
    loading: Cell<bool>,
    stamp: Cell<Option<(u64, i64, i64, u64)>>,
    previous: Cell<Option<adw::ColorScheme>>,
    timer: RefCell<Option<glib::SourceId>>,
    signal: RefCell<Option<glib::SignalHandlerId>>,
}

impl ThemeController {
    pub fn apply(directory: impl AsRef<Path>, fonts: &Path) -> std::io::Result<Rc<Self>> {
        load_bundled_fonts(fonts)?;
        let controller = Rc::new(Self {
            directory: directory.as_ref().to_owned(),
            provider: gtk::CssProvider::new(),
            loading: Cell::new(false),
            stamp: Cell::new(None),
            previous: Cell::new(None),
            timer: RefCell::new(None),
            signal: RefCell::new(None),
        });
        controller.load();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &controller.provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        let weak = Rc::downgrade(&controller);
        controller
            .signal
            .replace(Some(adw::StyleManager::default().connect_dark_notify(
                move |_| {
                    if let Some(controller) = weak.upgrade() {
                        controller.load();
                    }
                },
            )));
        let weak = Rc::downgrade(&controller);
        controller
            .timer
            .replace(Some(glib::timeout_add_seconds_local(1, move || {
                let Some(controller) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let stamp = fs::metadata(controller.directory.join("colors.toml"))
                    .ok()
                    .map(|stat| (stat.ino(), stat.mtime(), stat.mtime_nsec(), stat.size()));
                if controller.stamp.replace(stamp) != stamp {
                    controller.load();
                }
                glib::ControlFlow::Continue
            })));
        Ok(controller)
    }

    fn load(&self) {
        if self.loading.replace(true) {
            return;
        }
        let manager = adw::StyleManager::default();
        let palette = read_omarchy_palette(&self.directory.join("colors.toml"));
        let css = if let Some((palette, dark)) = palette {
            if self.previous.get().is_none() {
                self.previous.set(Some(manager.color_scheme()));
            }
            manager.set_color_scheme(if dark {
                adw::ColorScheme::ForceDark
            } else {
                adw::ColorScheme::ForceLight
            });
            build_stylesheet(&palette)
        } else {
            if let Some(previous) = self.previous.take() {
                manager.set_color_scheme(previous);
            }
            build_stylesheet(if manager.is_dark() {
                &DEFAULTS.dark
            } else {
                &DEFAULTS.light
            })
        };
        self.provider.load_from_string(&css);
        self.loading.set(false);
    }
}

impl Drop for ThemeController {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.get_mut().take() {
            timer.remove();
        }
        if let Some(signal) = self.signal.get_mut().take() {
            adw::StyleManager::default().disconnect(signal);
        }
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.provider);
        }
        if let Some(previous) = self.previous.take() {
            adw::StyleManager::default().set_color_scheme(previous);
        }
    }
}
