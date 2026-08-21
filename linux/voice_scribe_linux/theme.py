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
    "canvas": "#E9E4F3",
    "surface": "#FFFFFF",
    "surface_subtle": BRAND_SURFACE,
    "ink": BRAND_INK,
    "ink_secondary": "#5A5470",
    "ink_muted": "#827B9F",
    "outline": BRAND_INK,
    "outline_subtle": "#CFC8E4",
    "shadow": BRAND_INK,
    "action": BRAND_ACTION,
    "action_hover": "#C7BAF3",
    "on_action": BRAND_INK,
    "accent_strong": "#5B49BE",
    "accent_soft": "#DCD5F4",
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
    "canvas": "#191622",
    "surface": "#262234",
    "surface_subtle": "#2F2A40",
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
    """Derive the complete application stylesheet from one token table."""
    return _named_colors(tokens) + _COMPONENT_RULES


_COMPONENT_RULES = """
/* Focus visibility stays strong on both canvas and surfaces. */
*:focus-visible {
  outline-style: solid;
  outline-width: 2px;
  outline-color: @vs_focus;
  outline-offset: 2px;
}

window.background {
  background: @vs_canvas;
  color: @vs_ink;
}

/* Primary surfaces: white, crisp 1px outline, selective hard offset shadow. */
.card {
  background: @vs_surface;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 12px;
  box-shadow: 4px 4px 0 @vs_shadow;
  padding: 0;
}

/* Secondary lists sit flat: outlined once, never nested inside a shadow. */
.boxed-list {
  background: @vs_surface;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 10px;
  box-shadow: none;
}

.boxed-list > row {
  background: @vs_surface;
  color: @vs_ink;
  border-bottom: 1px solid @vs_divider;
}

.boxed-list > row:last-child {
  border-bottom: none;
}

.boxed-list > row:hover {
  background: @vs_surface_subtle;
}

.boxed-list > row:selected {
  background: @vs_accent_soft;
  color: @vs_ink;
}

/* Hairline inner separators never repeat the outer outline. */
separator {
  background: @vs_divider;
}

/* Simple, flat utility chrome above the workspace. */
headerbar {
  background: @vs_surface;
  color: @vs_ink;
  border-bottom: 1px solid @vs_outline;
  box-shadow: none;
}

headerbar windowtitle .title {
  font-weight: 800;
  color: @vs_ink;
}

.vs-page-title {
  font-weight: 800;
  font-size: 112.5%;
  color: @vs_ink;
}

/* Readable hierarchy: metadata and controls hold a real size floor. */
.caption {
  font-size: 13px;
}

.dim-label {
  color: @vs_ink_secondary;
  font-size: 14px;
  opacity: 1;
}

.title-1,
.title-2,
.title-3,
.title-4,
.heading {
  color: @vs_ink;
  font-weight: 800;
}

.title-2 {
  font-size: 127%;
}

.title-3 {
  font-size: 114%;
}

/* Stable left navigation rail on wide layouts. */
.vs-nav {
  background: @vs_surface;
  color: @vs_ink;
  border-right: 1px solid @vs_outline;
}

.vs-nav list {
  background: transparent;
}

.vs-nav-row {
  background: transparent;
  border: 1px solid transparent;
  border-radius: 10px;
  padding: 8px 10px;
  margin: 2px 0;
  font-weight: 650;
  font-size: 15px;
  color: @vs_ink_secondary;
  transition: none;
}

.vs-nav-row:hover {
  background: @vs_surface_subtle;
  color: @vs_ink;
}

.vs-nav-row:active {
  background: @vs_accent_soft;
}

.vs-nav-row:selected {
  background: @vs_action;
  color: @vs_on_action;
  border-color: @vs_outline;
  box-shadow: 2px 2px 0 @vs_shadow;
}

.vs-brand-chip {
  background: @vs_action;
  color: @vs_on_action;
  border: 1px solid @vs_outline;
  border-radius: 10px;
  box-shadow: 2px 2px 0 @vs_shadow;
  min-width: 32px;
  min-height: 32px;
}

.vs-brand-mark {
  font-weight: 900;
  font-size: 17px;
  color: @vs_on_action;
}

.vs-brand-name {
  font-weight: 800;
  font-size: 16px;
  color: @vs_ink;
}

.vs-nav-hint {
  color: @vs_ink_secondary;
  font-weight: 600;
  font-size: 13px;
}

.vs-key-cap {
  background: @vs_surface_subtle;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 8px;
  padding: 3px 8px;
  font-weight: 700;
  font-size: 13px;
}

/* Clear segment states for the near-action capture decisions. */
.vs-segment {
  background: @vs_surface_subtle;
  border: 1px solid @vs_outline;
  border-radius: 10px;
  padding: 3px;
}

.vs-segment button {
  background: transparent;
  border: none;
  border-radius: 7px;
  padding: 8px 12px;
  font-weight: 700;
  font-size: 14px;
  color: @vs_ink_secondary;
  box-shadow: none;
}

.vs-segment button:hover {
  background: @vs_surface;
  color: @vs_ink;
}

.vs-segment button:checked {
  background: @vs_action;
  color: @vs_on_action;
  box-shadow: 2px 2px 0 @vs_shadow;
}

.vs-segment button:disabled {
  opacity: 0.55;
}

/* One unmistakable recording action. */
button.vs-record {
  min-height: 52px;
  border-radius: 12px;
  border: 2px solid @vs_outline;
  box-shadow: 4px 4px 0 @vs_shadow;
  font-weight: 750;
  font-size: 15px;
  padding: 10px 18px;
  background: @vs_surface_subtle;
  color: @vs_ink;
}

button.vs-record.suggested-action {
  background: @vs_action;
  color: @vs_on_action;
}

button.vs-record.suggested-action:hover {
  background: @vs_action_hover;
}

button.vs-record.destructive-action {
  background: @vs_danger;
  color: @vs_on_danger;
}

button.vs-record:active {
  box-shadow: 2px 2px 0 @vs_shadow;
}

button.vs-record:disabled {
  opacity: 0.55;
  box-shadow: 2px 2px 0 @vs_shadow;
}

/* Secondary outlined actions stay tactile but calm. */
button.suggested-action:not(.vs-record),
button.destructive-action:not(.vs-record) {
  border: 1px solid @vs_outline;
  box-shadow: 2px 2px 0 @vs_shadow;
  font-weight: 700;
}

button.suggested-action:not(.vs-record):active,
button.destructive-action:not(.vs-record):active {
  box-shadow: 1px 1px 0 @vs_shadow;
}

.vs-utility {
  border: 1px solid @vs_outline;
  border-radius: 10px;
  background: @vs_surface;
  color: @vs_ink;
  box-shadow: 2px 2px 0 @vs_shadow;
}

.vs-utility:active {
  box-shadow: 1px 1px 0 @vs_shadow;
}

/* Compact dismissible setup callout instead of a full-width slab. */
.vs-callout {
  background: @vs_surface;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 10px;
  box-shadow: 2px 2px 0 @vs_shadow;
}

.vs-callout-title {
  font-weight: 700;
  font-size: 14px;
  color: @vs_ink;
}

.vs-callout-body {
  color: @vs_ink_secondary;
  font-size: 13px;
}

/* Feature maturity is text-first and uses only reserved semantic colors. */
.vs-maturity-notice {
  margin-top: 2px;
  margin-bottom: 2px;
}

.vs-maturity-badge {
  border: 1px solid @vs_outline;
  border-radius: 999px;
  padding: 2px 8px;
  font-weight: 750;
  font-size: 12px;
}

.vs-maturity-badge.vs-verified {
  background: @vs_success_soft;
  color: @vs_success;
}

.vs-maturity-badge.vs-experimental {
  background: @vs_warning_soft;
  color: @vs_warning;
}

.vs-maturity-detail {
  color: @vs_ink_secondary;
  font-size: 13px;
}

/* Live capture signal components. */
levelbar trough {
  border: 1px solid @vs_outline;
  border-radius: 7px;
  background: @vs_surface_subtle;
}

levelbar block.filled {
  background: @vs_accent_strong;
  border-radius: 4px;
}

levelbar block.empty {
  background: transparent;
}

label.warning {
  color: @vs_warning;
}

label.error {
  color: @vs_danger;
}

/* Scrolled workspaces keep the pale canvas visible. */
scrolledwindow undershoot.top,
scrolledwindow undershoot.bottom,
scrolledwindow undershoot.left,
scrolledwindow undershoot.right {
  background: none;
}

scrollbar slider {
  background: @vs_outline_subtle;
  border-radius: 4px;
  min-width: 8px;
  min-height: 32px;
}

scrollbar slider:hover {
  background: @vs_ink_muted;
}

/* Bottom navigation stays compact on narrow layouts. */
.viewswitcherbar actionbar > revealer > box {
  background: @vs_surface;
  border-top: 1px solid @vs_outline;
}

viewswitcher button {
  font-weight: 650;
}

viewswitcher button:checked {
  color: @vs_accent_strong;
}

/* The persistent capture dock sits on the canvas with a crisp top edge. */
.vs-dock {
  background: @vs_surface;
  border-top: 1px solid @vs_outline;
}

banner {
  border-bottom: 1px solid @vs_outline;
}

expander > title {
  color: @vs_ink;
  font-weight: 650;
}

/* Transient recording status bar: one compact bottom-centered strip that
   lives in its own toolbar row above the persistent dock, so it can never
   obscure Capture content and reserves its space only while revealed. */
.vs-recording-slot {
  background: transparent;
  padding: 0;
}

.vs-recording-bar {
  background: @vs_surface;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 12px;
  box-shadow: 4px 4px 0 @vs_shadow;
  padding: 8px 14px;
  margin-bottom: 6px;
  margin-top: 6px;
}

.vs-recording-bar levelbar trough {
  min-height: 10px;
}

.vs-recording-time {
  font-weight: 800;
  font-size: 15px;
  color: @vs_ink;
}

.vs-recording-phase {
  font-weight: 650;
  font-size: 13px;
  color: @vs_ink_secondary;
}

.vs-recording-preview {
  font-size: 13.5px;
  color: @vs_ink;
}

.vs-recording-preview.vs-quiet {
  color: @vs_ink_muted;
}

.vs-live-chip {
  background: @vs_danger;
  color: @vs_on_danger;
  border: 1px solid @vs_outline;
  border-radius: 8px;
  padding: 2px 8px;
  font-weight: 750;
  font-size: 12.5px;
}

.vs-live-chip.vs-preparing {
  background: @vs_warning_soft;
  color: @vs_warning;
}

.vs-mode-chip {
  background: @vs_accent_soft;
  color: @vs_ink;
  border: 1px solid @vs_outline;
  border-radius: 8px;
  padding: 2px 8px;
  font-weight: 700;
  font-size: 12.5px;
}

.vs-delivery-chip {
  background: @vs_surface_subtle;
  color: @vs_ink_secondary;
  border: 1px solid @vs_outline;
  border-radius: 8px;
  padding: 2px 8px;
  font-weight: 700;
  font-size: 12.5px;
}
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
