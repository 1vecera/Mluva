"""Token-defined visual theme for the Mluva Linux shell.

Canonical brand hues come from :mod:`voice_scribe_linux.brand`; all remaining
colors live in the semantic token tables below. The GTK stylesheet is generated
from the tokens, remaps libadwaita's named palette onto them so native widgets
and dialogs stay coherent, and adds the editorial surface components used by
the application shell.
"""

from typing import Final

import gi

from voice_scribe_linux.brand import BRAND_ACTION, BRAND_INK, BRAND_SURFACE

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, Gtk  # noqa: E402

LightTokens: Final[dict[str, str]] = {
    "canvas": "#FAFAFC",
    "surface": "#FFFFFF",
    "surface_subtle": BRAND_SURFACE,
    "ink": BRAND_INK,
    "ink_secondary": "#666474",
    "ink_muted": "#737180",
    "outline": "#E3E2E9",
    "outline_subtle": "#E3E2E9",
    "shadow": BRAND_INK,
    "action": BRAND_ACTION,
    "action_hover": "#5946C4",
    "on_action": "#FFFFFF",
    "accent_strong": "#5B49BE",
    "accent_soft": "#EEEBFA",
    "danger": "#E2483F",
    "on_danger": "#FFFFFF",
    "danger_soft": "#FBDBD7",
    "success": "#1F8A5D",
    "success_soft": "#D7F1E5",
    "warning": "#8A5F10",
    "warning_soft": "#F8EAC8",
    "focus": "#5B49BE",
}

DarkTokens: Final[dict[str, str]] = {
    "canvas": "#202027",
    "surface": "#27272F",
    "surface_subtle": "#303039",
    "ink": "#F2EFFA",
    "ink_secondary": "#B9B2D4",
    "ink_muted": "#8E86AC",
    "outline": "#0D0B15",
    "outline_subtle": "#3E3855",
    "shadow": "#07060D",
    "action": "#A998EF",
    "action_hover": "#BCADF4",
    "on_action": "#15121F",
    "accent_strong": "#C3B5F5",
    "accent_soft": "#443C66",
    "danger": "#E2554E",
    "on_danger": "#FFFFFF",
    "danger_soft": "#4A2E2B",
    "success": "#4CC38A",
    "success_soft": "#2E4439",
    "warning": "#D8A03E",
    "warning_soft": "#48402B",
    "focus": "#C3B5F5",
}

BORDER_WIDTH: Final = 1
RADIUS_CARD: Final = 12
RADIUS_CONTROL: Final = 10
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
window.background { background: @vs_canvas; color: @vs_ink; font-family: "Inter", sans-serif; }
headerbar { background: @vs_surface; color: @vs_ink; border-bottom: 1px solid @vs_outline_subtle; box-shadow: none; }
.vs-page-title { font-size: 14px; font-weight: 600; color: @vs_ink_secondary; }
.caption { font-size: 12px; color: @vs_ink_secondary; }
.dim-label { color: @vs_ink_secondary; opacity: 1; }
.heading, .title-1, .title-2, .title-3, .title-4 { font-weight: 600; }
button { min-height: 32px; border-radius: 9px; font-weight: 500; }
button:focus-visible, textview:focus-visible, entry:focus-visible { outline: 2px solid @vs_focus; outline-offset: 2px; }
button.suggested-action { background: @vs_action; color: @vs_on_action; }
button.suggested-action:hover { background: @vs_action_hover; }
button.destructive-action { background: @vs_danger; color: @vs_on_danger; }
button:disabled { opacity: 0.48; }
button.flat { box-shadow: none; }
.card, .boxed-list { background: @vs_surface; border: 1px solid @vs_outline_subtle; border-radius: 12px;
  box-shadow: none; }
.boxed-list > row { border-bottom: 1px solid @vs_divider; }
.boxed-list > row:last-child { border-bottom: none; }
separator { background: @vs_outline_subtle; }
.ml-history-pane, .ml-history-sidebar { background: @vs_surface_subtle; }
.ml-history-sidebar list { background: transparent; }
.ml-history-sidebar row { border-radius: 9px; margin: 2px 0; }
.ml-history-sidebar row:selected { background: @vs_accent_soft; color: @vs_ink; }
.ml-history-sidebar row:hover { background: alpha(@vs_accent_soft, 0.65); }
.ml-history-sidebar button { min-height: 34px; }
.ml-wordmark { font-size: 21px; font-weight: 650; letter-spacing: -0.5px; }
.ml-conversation { background: @vs_surface; }
.ml-conversation-title { font-size: 24px; font-weight: 600; letter-spacing: -0.5px; margin-bottom: 8px; }
.ml-transcript, .ml-transcript text { background: transparent; color: @vs_ink; font-size: 16px; }
.ml-transcript { line-height: 1.3; }
.ml-source { padding: 0 0 20px; border-bottom: 1px solid @vs_outline_subtle; }
.ml-reply { padding: 0 0 12px; }
.ml-instruction { background: @vs_surface_subtle; border-radius: 12px; padding: 12px 16px; color: @vs_ink_secondary; }
.ml-composer { background: @vs_surface; border-top: 1px solid @vs_outline_subtle; padding-top: 12px; }
.ml-composer flowboxchild { padding: 0; }
.ml-composer flowbox { padding: 0; }
.ml-prompt, .ml-prompt text { background: @vs_surface_subtle; color: @vs_ink; font-size: 14px; }
.ml-prompt { padding: 10px 12px; border-radius: 12px; }
.ml-recording-dock { background: @vs_canvas; }
.ml-recording-dock button, button.ml-primary { min-height: 36px; padding: 4px 16px; }
.ml-live { background: @vs_surface_subtle; border-radius: 12px; padding: 12px; }
.ml-live .heading { color: @vs_accent_strong; }
.ml-empty { margin-top: 12px; }
.ml-empty image { -gtk-icon-size: 64px; }
.vs-callout { background: @vs_warning_soft; border-radius: 12px; padding: 10px; }
.vs-callout-title { font-weight: 600; }
.vs-callout-body { font-size: 13px; }
.vs-maturity-notice { padding: 4px 0; }
.vs-maturity-badge { border-radius: 6px; padding: 3px 7px; font-size: 11px; }
.vs-maturity-badge.vs-verified { background: @vs_success_soft; color: @vs_success; }
.vs-maturity-badge.vs-experimental { background: @vs_surface_subtle; color: @vs_ink_secondary; }
.vs-maturity-detail { font-size: 12px; color: @vs_ink_secondary; }
.vs-segment button:checked { background: @vs_accent_soft; }
.vs-key-cap { background: @vs_surface_subtle; border-radius: 5px; padding: 3px 6px; }
.vs-recording-bar { background: @vs_surface; border-radius: 14px; padding: 12px 16px;
  border: 1px solid @vs_outline_subtle; }
.vs-recording-time { font-feature-settings: "tnum"; font-weight: 600; }
.vs-live-chip { background: @vs_danger_soft; color: @vs_danger; padding: 3px 8px; border-radius: 6px; }
.vs-live-chip.vs-preparing { color: @vs_accent_strong; background: @vs_accent_soft; }
.vs-mode-chip, .vs-delivery-chip { font-size: 12px; color: @vs_ink_secondary; }
.vs-recording-phase, .vs-recording-preview.vs-quiet { font-size: 12px; color: @vs_ink_secondary; }
levelbar trough { background: @vs_surface_subtle; border-radius: 4px; }
levelbar block.filled { background: @vs_action; border-radius: 3px; }
label.warning { color: @vs_warning; }
label.error { color: @vs_danger; }
scrollbar slider { min-width: 5px; min-height: 5px; border-radius: 10px; background: alpha(@vs_ink_muted, 0.35); }
scrollbar slider:hover { background: alpha(@vs_ink_muted, 0.6); }
"""
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

    def __init__(self) -> None:
        """Prepare one provider that will follow the system color scheme."""
        self._provider = Gtk.CssProvider()
        self._installed = False

    def apply(self) -> None:
        """Load the stylesheet for the active scheme onto the default display."""
        style_manager = Adw.StyleManager.get_default()
        self._load(style_manager.get_dark())
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
        style_manager.connect("notify::dark", self._on_scheme_changed)

    def _load(self, dark: bool) -> None:
        """Swap the token table behind the installed stylesheet."""
        tokens = DarkTokens if dark else LightTokens
        self._provider.load_from_data(build_stylesheet(tokens).encode("utf-8"))

    def _on_scheme_changed(self, manager: Adw.StyleManager, _param: object) -> None:
        """Reload when the system scheme flips."""
        self._load(manager.get_dark())
