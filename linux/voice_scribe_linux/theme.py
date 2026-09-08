"""Token-defined visual theme for the Mluva Linux shell.

Canonical brand hues come from :mod:`voice_scribe_linux.brand`; all remaining
colors live in the semantic token tables below. The GTK stylesheet is generated
from the tokens, remaps libadwaita's named palette onto them so native widgets
and dialogs stay coherent, and adds the editorial surface components used by
the application shell.
"""

import os
import re
import tomllib
from pathlib import Path
from typing import Final

import gi

from voice_scribe_linux.brand import BRAND_ACTION, BRAND_INK, BRAND_SIGNAL, BRAND_SURFACE

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, GLib, Gtk  # noqa: E402

LightTokens: Final[dict[str, str]] = {
    "canvas": "#F7FAF8",
    "surface": "#FFFFFF",
    "surface_subtle": BRAND_SURFACE,
    "ink": BRAND_INK,
    "ink_secondary": "#536B62",
    "ink_muted": "#62786F",
    "outline": "#DFE8E2",
    "outline_subtle": "#DFE8E2",
    "shadow": BRAND_INK,
    "action": BRAND_ACTION,
    "action_hover": "#206755",
    "on_action": "#FFFFFF",
    "accent_strong": BRAND_ACTION,
    "accent_soft": "#DCEFE6",
    "danger": "#E2483F",
    "on_danger": "#FFFFFF",
    "danger_soft": "#FBDBD7",
    "success": "#1F8A5D",
    "success_soft": "#D7F1E5",
    "warning": "#8A5F10",
    "warning_soft": "#F8EAC8",
    "focus": BRAND_ACTION,
}

DarkTokens: Final[dict[str, str]] = {
    "canvas": "#151C1A",
    "surface": "#1B2420",
    "surface_subtle": "#232F29",
    "ink": "#EDF5EF",
    "ink_secondary": "#B8CDC0",
    "ink_muted": "#8EA899",
    "outline": "#0C1510",
    "outline_subtle": "#354B3E",
    "shadow": "#070D09",
    "action": BRAND_SIGNAL,
    "action_hover": "#ABEDD7",
    "on_action": "#15382C",
    "accent_strong": BRAND_SIGNAL,
    "accent_soft": "#294C3C",
    "danger": "#E2554E",
    "on_danger": "#FFFFFF",
    "danger_soft": "#4A2E2B",
    "success": "#4CC38A",
    "success_soft": "#2E4439",
    "warning": "#D8A03E",
    "warning_soft": "#48402B",
    "focus": BRAND_SIGNAL,
}

BORDER_WIDTH: Final = 1
RADIUS_CARD: Final = 4
RADIUS_CONTROL: Final = 2
WINDOW_OPACITY: Final = 0.82
POPOVER_OPACITY: Final = 0.94
SHADOW_OFFSET: Final = 4
SHADOW_OFFSET_SMALL: Final = 2
NAV_RAIL_WIDTH: Final = 224


def _hex(value: str) -> tuple[int, int, int]:
    """Parse one #RRGGBB token value."""
    return (int(value[1:3], 16), int(value[3:5], 16), int(value[5:7], 16))


def _blend(first: str, second: str, weight: float) -> str:
    """Return one hex color mixed from two token values."""
    first_rgb = _hex(first)
    second_rgb = _hex(second)
    mixed = tuple(round(a * weight + b * (1 - weight)) for a, b in zip(first_rgb, second_rgb, strict=True))
    return "#{:02X}{:02X}{:02X}".format(*mixed)


def _named_colors(tokens: dict[str, str]) -> str:
    """Define the semantic tokens and remap libadwaita's palette onto them."""
    divider = _blend(tokens["outline_subtle"], tokens["canvas"], 0.35)
    return f"""
@define-color vs_canvas {tokens["canvas"]};
@define-color vs_surface {tokens["surface"]};
@define-color vs_surface_subtle {tokens["surface_subtle"]};
@define-color vs_ink {tokens["ink"]};
@define-color vs_ink_secondary {tokens["ink_secondary"]};
@define-color vs_ink_muted {tokens["ink_muted"]};
@define-color vs_outline {tokens["outline"]};
@define-color vs_outline_subtle {tokens["outline_subtle"]};
@define-color vs_shadow {tokens["shadow"]};
@define-color vs_action {tokens["action"]};
@define-color vs_action_hover {tokens["action_hover"]};
@define-color vs_on_action {tokens["on_action"]};
@define-color vs_accent_strong {tokens["accent_strong"]};
@define-color vs_accent_soft {tokens["accent_soft"]};
@define-color vs_danger {tokens["danger"]};
@define-color vs_on_danger {tokens["on_danger"]};
@define-color vs_danger_soft {tokens["danger_soft"]};
@define-color vs_success {tokens["success"]};
@define-color vs_success_soft {tokens["success_soft"]};
@define-color vs_warning {tokens["warning"]};
@define-color vs_warning_soft {tokens["warning_soft"]};
@define-color vs_focus {tokens["focus"]};
@define-color vs_divider {divider};

@define-color window_bg_color @vs_canvas;
@define-color window_fg_color @vs_ink;
@define-color view_bg_color @vs_surface;
@define-color view_fg_color @vs_ink;
@define-color headerbar_bg_color @vs_surface;
@define-color headerbar_fg_color @vs_ink;
@define-color headerbar_border_color @vs_outline;
@define-color card_bg_color @vs_surface;
@define-color card_fg_color @vs_ink;
@define-color dialog_bg_color @vs_canvas;
@define-color dialog_fg_color @vs_ink;
@define-color popover_bg_color @vs_surface;
@define-color popover_fg_color @vs_ink;
@define-color sidebar_bg_color @vs_surface;
@define-color sidebar_fg_color @vs_ink;
@define-color secondary_sidebar_bg_color @vs_surface;
@define-color secondary_sidebar_fg_color @vs_ink;
@define-color accent_bg_color @vs_action;
@define-color accent_fg_color @vs_on_action;
@define-color accent_color @vs_accent_strong;
@define-color destructive_bg_color @vs_danger;
@define-color destructive_fg_color @vs_on_danger;
@define-color destructive_color @vs_danger;
@define-color success_bg_color @vs_success;
@define-color success_fg_color @vs_on_danger;
@define-color success_color @vs_success;
@define-color warning_bg_color @vs_warning;
@define-color warning_fg_color @vs_on_danger;
@define-color warning_color @vs_warning;
@define-color error_bg_color @vs_danger;
@define-color error_fg_color @vs_on_danger;
@define-color error_color @vs_danger;
@define-color borders @vs_outline_subtle;
@define-color thick_borders @vs_outline;
"""


def build_stylesheet(tokens: dict[str, str]) -> str:
    """Generate coherent reading surfaces, focus states and controls from semantic tokens."""
    return (
        _named_colors(tokens)
        + """
window.background { background: alpha(@vs_canvas, $WINDOW_OPACITY); color: @vs_ink;
  font-family: "Inter", "Adwaita Sans", "Noto Sans", sans-serif; font-size: 0.92em; }
toolbarview, toolbarview > top-bar, toolbarview > bottom-bar { background: transparent; }
headerbar { min-height: 38px; padding: 0 8px; background: transparent; color: @vs_ink; border: none; box-shadow: none; }
headerbar button { min-height: 24px; min-width: 24px; padding: 2px 4px; }
windowcontrols button { background: transparent; border-radius: $CONTROL_RADIUSpx; box-shadow: none; }
windowcontrols button > image { background: transparent; border-radius: $CONTROL_RADIUSpx; box-shadow: none; }
button image { opacity: 0.68; }
button:hover image, button:focus-visible image, button:checked image { opacity: 1; }
.vs-page-title { font-size: 1em; font-weight: 600; color: @vs_ink_secondary; }
.caption { font-size: 0.9em; color: @vs_ink_secondary; }
.dim-label { color: @vs_ink_secondary; opacity: 1; }
.heading, .title-1, .title-2, .title-3, .title-4 { font-weight: 600; }
button { min-height: 28px; border-radius: $CONTROL_RADIUSpx; font-weight: 500; padding: 3px 10px;
  background: transparent; border: 1px solid transparent; box-shadow: none; color: @vs_ink_secondary; }
button:hover { background: alpha(@vs_ink, 0.065); color: @vs_ink; }
button:active, button:checked { background: alpha(@vs_action, 0.14); color: @vs_accent_strong; }
button:focus-visible, textview:focus-visible, entry:focus-visible { outline: 2px solid @vs_focus; outline-offset: 2px; }
button.suggested-action { background: alpha(@vs_action, 0.10); border-color: alpha(@vs_action, 0.26);
  color: @vs_accent_strong; }
button.suggested-action:hover { background: alpha(@vs_action, 0.20); border-color: alpha(@vs_action, 0.45); }
button.destructive-action { background: alpha(@vs_danger, 0.12); color: @vs_danger; }
button:disabled { opacity: 0.48; }
button.flat { box-shadow: none; }
.card, .boxed-list { background: @vs_surface; border: 1px solid alpha(@vs_outline_subtle, 0.45);
  border-radius: $CARD_RADIUSpx;
  box-shadow: none; }
.boxed-list > row { border-bottom: 1px solid @vs_divider; }
.boxed-list > row:last-child { border-bottom: none; }
separator { background: @vs_outline_subtle; }
.ml-history-pane { background: alpha(@vs_ink, 0.025); border-right: 1px solid alpha(@vs_ink, 0.075); }
overlay-split-view > .sidebar-pane, overlay-split-view > .content-pane { background: transparent; }
.ml-history-sidebar { background: transparent; }
.ml-history-sidebar entry.search { min-height: 28px; padding: 2px 8px; background: transparent;
  border: 1px solid alpha(@vs_ink, 0.10); box-shadow: none; border-radius: $CONTROL_RADIUSpx; }
.ml-history-sidebar list { background: transparent; }
.ml-history-sidebar row { border-radius: $CONTROL_RADIUSpx; margin: 2px 0; border-left: 2px solid transparent; }
.ml-history-sidebar row:selected { background: alpha(@vs_action, 0.08); border-left-color: alpha(@vs_action, 0.7);
  color: @vs_ink; }
.ml-history-sidebar row:hover { background: alpha(@vs_ink, 0.055); }
.ml-history-sidebar button { min-height: 28px; }
.ml-wordmark, .ml-conversation-title { font-size: 1.12em; font-weight: 600; letter-spacing: -0.2px; }
.ml-brand-mark { color: @vs_ink_secondary; opacity: 0.65; }
.ml-conversation { background: transparent; }
.ml-transcript, .ml-transcript text { background: transparent; color: @vs_ink; }
.ml-transcript { font-size: 1.04em; line-height: 1.3; }
.ml-source { padding: 0 0 8px; }
.ml-source .heading, .ml-reply .heading { font-size: 0.9em; font-weight: 500; color: @vs_ink_secondary; }
.ml-reply { padding: 0 0 4px; }
.ml-instruction { background: alpha(@vs_ink, 0.045); border-radius: $CONTROL_RADIUSpx;
  padding: 8px 12px; color: @vs_ink_secondary; }
.ml-composer { background: transparent; border: none; padding-top: 0; }
.ml-composer flowbox button { background: transparent; box-shadow: none; }
.ml-composer flowbox button:hover { background: alpha(@vs_ink, 0.065); }
.ml-composer flowboxchild { padding: 0; }
.ml-composer flowbox { padding: 0; }
.ml-prompt, .ml-prompt text { background: transparent; color: @vs_ink; font-size: 1em; }
.ml-prompt text { background: transparent; }
.ml-prompt { padding: 8px 10px; border: 1px solid alpha(@vs_ink, 0.16); border-radius: $CONTROL_RADIUSpx; }
.ml-recording-dock { background: transparent; }
.ml-recording-dock button, button.ml-primary { min-height: 30px; padding: 3px 12px; }
.ml-live { background: transparent; border-radius: 0; padding: 0; }
.ml-live .heading { color: @vs_accent_strong; }
.ml-empty { margin-top: 24px; }
popover > contents { background: alpha(@vs_surface, $POPOVER_OPACITY); border-radius: $CARD_RADIUSpx;
  border: 1px solid alpha(@vs_ink, 0.16); box-shadow: 0 4px 16px alpha(@vs_shadow, 0.12); }
popover modelbutton { min-height: 28px; padding: 4px 10px; border-radius: $CONTROL_RADIUSpx; }
popover modelbutton:hover { background: alpha(@vs_ink, 0.065); }
.vs-callout { background: @vs_warning_soft; border-radius: 12px; padding: 10px; }
.vs-callout-title { font-weight: 600; }
.vs-callout-body { font-size: 13px; }
.vs-maturity-notice { padding: 4px 0; }
.vs-maturity-badge { border-radius: 6px; padding: 3px 7px; font-size: 11px; }
.vs-maturity-badge.vs-verified { background: @vs_success_soft; color: @vs_success; }
.vs-maturity-badge.vs-experimental { background: @vs_surface_subtle; color: @vs_ink_secondary; }
.vs-maturity-detail { font-size: 0.9em; color: @vs_ink_secondary; }
.vs-segment button:checked { background: @vs_accent_soft; }
.vs-key-cap { background: @vs_surface_subtle; border-radius: 5px; padding: 3px 6px; }
.vs-recording-bar { background: @vs_surface; border-radius: 14px; padding: 12px 16px;
  border: 1px solid @vs_outline_subtle; }
.vs-recording-time { font-feature-settings: "tnum"; font-weight: 600; }
.vs-live-chip { background: @vs_danger_soft; color: @vs_danger; padding: 3px 8px; border-radius: 6px; }
.vs-live-chip.vs-preparing { color: @vs_accent_strong; background: @vs_accent_soft; }
.vs-mode-chip, .vs-delivery-chip { font-size: 0.9em; color: @vs_ink_secondary; }
.vs-recording-phase, .vs-recording-preview.vs-quiet { font-size: 0.9em; color: @vs_ink_secondary; }
levelbar trough { background: @vs_surface_subtle; border-radius: 4px; }
levelbar block.filled { background: @vs_action; border-radius: 3px; }
label.warning { color: @vs_warning; }
label.error { color: @vs_danger; }
scrollbar, scrollbar trough { background: transparent; border: none; box-shadow: none; }
scrollbar { padding: 0; }
scrollbar.vertical { min-width: 10px; }
scrollbar.horizontal { min-height: 10px; }
scrollbar slider { border: none; outline: none; box-shadow: none; padding: 0;
  border-radius: 6px; background: alpha(@vs_ink_muted, 0.4); }
scrollbar.vertical slider { min-width: 6px; min-height: 32px; margin: 2px; }
scrollbar.horizontal slider { min-height: 6px; min-width: 32px; margin: 2px; }
scrollbar slider:hover { background: alpha(@vs_ink_muted, 0.65); }
scrollbar slider:active { background: alpha(@vs_ink_secondary, 0.8); }
""".replace("$CONTROL_RADIUS", str(RADIUS_CONTROL))
        .replace("$CARD_RADIUS", str(RADIUS_CARD))
        .replace("$WINDOW_OPACITY", str(WINDOW_OPACITY))
        .replace("$POPOVER_OPACITY", str(POPOVER_OPACITY))
    )


def build_shell_stylesheet(tokens: dict[str, str] = DarkTokens) -> str:
    """Use the same semantic palette for the panel icon and noninteractive bottom bar."""
    return f"""/* Generated from voice_scribe_linux.theme; do not edit. */
.mluva-recording-bar {{
  min-width: 360px; max-width: 600px; spacing: 7px; padding: 12px 18px;
  color: {tokens["ink"]}; background-color: {tokens["surface"]};
  border: 1px solid {tokens["outline_subtle"]}; border-radius: 18px;
  box-shadow: 0 5px 22px 2px rgba(0, 0, 0, 0.22);
}}
.mluva-primary-row {{ spacing: 10px; }}
.mluva-phase-icon {{ icon-size: 14px; color: {tokens["danger"]}; }}
.mluva-phase-icon.mluva-preparing, .mluva-phase-icon.mluva-processing {{ color: {tokens["action"]}; }}
.mluva-phase-icon.mluva-copied {{ color: {tokens["success"]}; }}
.mluva-phase-label {{ font-size: 13px; font-weight: 600; }}
.mluva-time {{ min-width: 42px; font-feature-settings: "tnum"; font-size: 13px; }}
.mluva-waveform {{ min-width: 66px; min-height: 22px; spacing: 4px; }}
.mluva-wave-bar {{ width: 4px; min-height: 4px; background-color: {tokens["action"]}; border-radius: 4px; }}
.mluva-chip {{ padding: 3px 7px; border-radius: 6px; font-size: 11px; }}
.mluva-mode {{ color: {tokens["ink_secondary"]}; background-color: {tokens["surface_subtle"]}; }}
.mluva-delivery {{ color: {tokens["ink_secondary"]}; }}
.mluva-detail {{ max-width: 540px; color: {tokens["ink_secondary"]}; font-size: 12px; }}
.mluva-preview {{ max-width: 540px; font-size: 13px; }}
.mluva-panel-recording, .mluva-panel-error {{ color: {tokens["danger"]}; }}
.mluva-panel-processing, .mluva-panel-preparing {{ color: {tokens["action"]}; }}
.mluva-panel-copied {{ color: {tokens["success"]}; }}
"""


class ThemeController:
    """Install and re-load the token-derived stylesheet for the whole app."""

    def __init__(self, theme_directory: Path | None = None) -> None:
        """Follow Omarchy's active palette when available, otherwise the system scheme."""
        self._provider = Gtk.CssProvider()
        self._installed = False
        state_home = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
        self.theme_directory = theme_directory or state_home / "omarchy/current/theme"
        self._timer = 0
        self._scheme_handler = 0
        self._loading = False
        self._stamp: tuple[int, int, int] | None = None
        self._previous_scheme: Adw.ColorScheme | None = None

    def apply(self) -> None:
        """Load the stylesheet for the active scheme onto the default display."""
        style_manager = Adw.StyleManager.get_default()
        self._load()
        display = Gdk.Display.get_default()
        if display is None:
            return
        if not self._installed:
            Gtk.StyleContext.add_provider_for_display(
                display,
                self._provider,
                Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
            )
            self._installed = True
        if not self._scheme_handler:
            self._scheme_handler = style_manager.connect("notify::dark", self._on_scheme_changed)
        if not self._timer:
            self._timer = GLib.timeout_add_seconds(1, self._check_theme)

    def _load(self) -> None:
        """Derive all GTK colors from the current theme without writing desktop configuration."""
        if self._loading:
            return
        self._loading = True
        try:
            manager = Adw.StyleManager.get_default()
            palette = read_omarchy_palette(self.theme_directory / "colors.toml")
            if palette is None:
                if self._previous_scheme is not None:
                    manager.set_color_scheme(self._previous_scheme)
                    self._previous_scheme = None
                css = build_stylesheet(DarkTokens if manager.get_dark() else LightTokens)
            else:
                colors, dark = palette
                if self._previous_scheme is None:
                    self._previous_scheme = manager.get_color_scheme()
                manager.set_color_scheme(Adw.ColorScheme.FORCE_DARK if dark else Adw.ColorScheme.FORCE_LIGHT)
                css = build_stylesheet(colors)
            self._provider.load_from_data(css.encode("utf-8"))
        finally:
            self._loading = False

    def _check_theme(self) -> bool:
        """Notice file replacement and theme-symlink swaps with one inexpensive stat call."""
        try:
            stat = (self.theme_directory / "colors.toml").stat()
            stamp = (stat.st_ino, stat.st_mtime_ns, stat.st_size)
        except OSError:
            stamp = None
        if stamp != self._stamp:
            self._stamp = stamp
            self._load()
        return GLib.SOURCE_CONTINUE

    def _on_scheme_changed(self, manager: Adw.StyleManager, _param: object) -> None:
        """Reload when the system scheme flips."""
        self._load()

    def close(self) -> None:
        """Release the stylesheet's lifetime hooks when the application exits."""
        if self._timer:
            GLib.source_remove(self._timer)
            self._timer = 0
        if self._scheme_handler:
            Adw.StyleManager.get_default().disconnect(self._scheme_handler)
            self._scheme_handler = 0


def read_omarchy_palette(path: Path) -> tuple[dict[str, str], bool] | None:
    """Map a valid Omarchy palette onto the existing semantic tokens, with no CSS interpolation from prose."""
    try:
        palette = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not all(
        isinstance(palette.get(key), str) and re.fullmatch(r"#[0-9a-fA-F]{6}", palette[key])
        for key in ("background", "foreground", "accent")
    ):
        return None
    background, foreground, accent = (palette[key] for key in ("background", "foreground", "accent"))
    dark = palette.get("mode", "dark") == "dark"

    def color(key: str, fallback: str) -> str:
        """Use only complete color literals from optional palette roles."""
        value = palette.get(key)
        return value if isinstance(value, str) and re.fullmatch(r"#[0-9a-fA-F]{6}", value) else fallback

    danger = color("red", accent)
    success = color("green", accent)
    warning = color("yellow", accent)
    return {
        "canvas": background,
        "surface": background,
        "surface_subtle": color("selection", _blend(foreground, background, 0.06)),
        "ink": foreground,
        "ink_secondary": _blend(foreground, background, 0.85),
        "ink_muted": _blend(foreground, background, 0.65),
        "outline": color("muted", _blend(foreground, background, 0.3)),
        "outline_subtle": _blend(foreground, background, 0.18),
        "shadow": background,
        "action": accent,
        "action_hover": _blend(accent, foreground, 0.85),
        "on_action": background,
        "accent_strong": accent,
        "accent_soft": color("selection", _blend(accent, background, 0.15)),
        "danger": danger,
        "on_danger": background,
        "danger_soft": _blend(danger, background, 0.12),
        "success": success,
        "success_soft": _blend(success, background, 0.12),
        "warning": warning,
        "warning_soft": _blend(warning, background, 0.12),
        "focus": accent,
    }, dark
