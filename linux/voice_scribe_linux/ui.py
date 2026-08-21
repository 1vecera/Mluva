"""Shared native Libadwaita layout primitives for the Linux application."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import Final

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Pango", "1.0")
from gi.repository import Adw, Gtk, Pango  # noqa: E402

from voice_scribe_linux.brand import PRODUCT_NAME
from voice_scribe_linux.feature_maturity import FeatureMaturity, feature_capability
from voice_scribe_linux.theme import NAV_RAIL_WIDTH

SPACE_1: Final = 4
SPACE_2: Final = 8
SPACE_3: Final = 12
SPACE_4: Final = 16

PAGE_MARGIN: Final = SPACE_4
PAGE_SPACING: Final = SPACE_4
SECTION_SPACING: Final = SPACE_4
CARD_PADDING: Final = SPACE_4
CONTENT_MAX_WIDTH: Final = 680
CONTENT_TIGHTENING_THRESHOLD: Final = 640
PRIMARY_ACTION_HEIGHT: Final = 56
RESULT_EDITOR_MIN_HEIGHT: Final = 120


class SummaryRow(Gtk.Box):
    """Render one compact title/value pair inside an already-contained card."""

    def __init__(self, title: str = "", subtitle: str = "") -> None:
        """Create a lightweight row without adding another panel or focus target."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        self.set_margin_top(SPACE_2)
        self.set_margin_bottom(SPACE_2)
        self.title_label = Gtk.Label(label=title, xalign=0, wrap=True)
        self.append(self.title_label)
        self.subtitle_label = Gtk.Label(label=subtitle, xalign=0, wrap=True)
        self.subtitle_label.add_css_class("caption")
        self.subtitle_label.add_css_class("dim-label")
        self.append(self.subtitle_label)

    def set_title(self, title: str) -> None:
        """Update the prominent summary value."""
        self.title_label.set_label(title)

    def set_subtitle(self, subtitle: str) -> None:
        """Update the supporting summary consequence."""
        self.subtitle_label.set_label(subtitle)


def maturity_badge(maturity: FeatureMaturity) -> Gtk.Label:
    """Render one compact semantic maturity label without relying on color alone."""
    badge = Gtk.Label(label=maturity.label)
    badge.add_css_class("vs-maturity-badge")
    badge.add_css_class("vs-verified" if maturity is FeatureMaturity.VERIFIED else "vs-experimental")
    badge.set_valign(Gtk.Align.CENTER)
    return badge


class FeatureMaturityNotice(Gtk.Box):
    """Show one capability's current acceptance status beside its boundary."""

    def __init__(self, identifier: str) -> None:
        """Build a reusable status row from the canonical capability registry."""
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        self.add_css_class("vs-maturity-notice")
        self.badge = Gtk.Label()
        self.badge.add_css_class("vs-maturity-badge")
        self.badge.set_valign(Gtk.Align.CENTER)
        self.append(self.badge)
        self.detail = Gtk.Label(xalign=0, wrap=True, hexpand=True)
        self.detail.add_css_class("vs-maturity-detail")
        self.append(self.detail)
        self.present(identifier)

    def present(self, identifier: str) -> None:
        """Project a registered capability without permitting an unlabeled fallback."""
        capability = feature_capability(identifier)
        self.badge.set_label(capability.maturity.label)
        self.badge.remove_css_class("vs-verified")
        self.badge.remove_css_class("vs-experimental")
        self.badge.add_css_class(
            "vs-verified" if capability.maturity is FeatureMaturity.VERIFIED else "vs-experimental"
        )
        self.detail.set_label(capability.summary)


def summary_list(*rows: SummaryRow) -> Gtk.Box:
    """Stack compact rows with native separators inside one existing surface."""
    summary = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    for index, row in enumerate(rows):
        if index:
            summary.append(Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL))
        summary.append(row)
    return summary


def set_margins(widget: Gtk.Widget, margin: int) -> None:
    """Apply one semantic margin consistently on every side of a widget."""
    widget.set_margin_top(margin)
    widget.set_margin_bottom(margin)
    widget.set_margin_start(margin)
    widget.set_margin_end(margin)


def page_content(*, spacing: int = PAGE_SPACING) -> Gtk.Box:
    """Create the shared vertically spaced and inset page body."""
    content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=spacing)
    set_margins(content, PAGE_MARGIN)
    return content


def card_box(*, spacing: int = SECTION_SPACING) -> tuple[Gtk.Box, Gtk.Box]:
    """Create a native card and return its padded vertical content box."""
    card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
    card.add_css_class("card")
    body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=spacing)
    set_margins(body, CARD_PADDING)
    card.append(body)
    return card, body


def clamp(child: Gtk.Widget, *, maximum_size: int = CONTENT_MAX_WIDTH) -> Adw.Clamp:
    """Center a page body and prevent unreadably wide desktop rows."""
    container = Adw.Clamp(
        maximum_size=maximum_size,
        tightening_threshold=CONTENT_TIGHTENING_THRESHOLD,
    )
    container.set_child(child)
    return container


def empty_state(title: str, description: str, icon_name: str) -> Adw.StatusPage:
    """Create a compact semantic empty state instead of an oversized blank list."""
    state = Adw.StatusPage(title=title, description=description, icon_name=icon_name)
    state.add_css_class("compact")
    state.set_vexpand(True)
    state.set_size_request(-1, 160)
    return state


class NavigationRail(Gtk.Box):
    """Stable labeled navigation for wide layouts, backed by one list of rows."""

    def __init__(
        self,
        items: tuple[tuple[str, str, str], ...],
        selected_name: str,
        on_activate: Callable[[str], None],
    ) -> None:
        """Build the vertical shell navigation from (name, title, icon) triples."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL)
        self.add_css_class("vs-nav")
        self.set_size_request(NAV_RAIL_WIDTH, -1)
        self.on_activate = on_activate

        brand = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
        brand.set_margin_top(SPACE_4)
        brand.set_margin_bottom(SPACE_4)
        brand.set_margin_start(SPACE_3)
        brand.set_margin_end(SPACE_3)
        chip = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        chip.add_css_class("vs-brand-chip")
        chip_mark = Gtk.Label(label="M")
        chip_mark.add_css_class("vs-brand-mark")
        chip.append(chip_mark)
        set_margins(chip, SPACE_2)
        brand.append(chip)
        brand_name = Gtk.Label(label=PRODUCT_NAME, xalign=0)
        brand_name.add_css_class("vs-brand-name")
        brand.append(brand_name)
        self.append(brand)

        self.list_box = Gtk.ListBox(selection_mode=Gtk.SelectionMode.SINGLE)
        self.list_box.add_css_class("navigation-sidebar")
        self.row_names: dict[Gtk.ListBoxRow, str] = {}
        for name, title, icon_name in items:
            content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(18)
            content.append(icon)
            label = Gtk.Label(label=title, xalign=0, hexpand=True)
            content.append(label)
            row = Gtk.ListBoxRow(child=content)
            row.add_css_class("vs-nav-row")
            self.list_box.append(row)
            self.row_names[row] = name
        self.list_box.connect("row-activated", self._row_activated)
        self.append(self.list_box)

        self.append(Gtk.Box(vexpand=True))

        shortcut = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
        set_margins(shortcut, SPACE_3)
        key_cap = Gtk.Label(label="F9")
        key_cap.add_css_class("vs-key-cap")
        shortcut.append(key_cap)
        hint = Gtk.Label(label="Global key", xalign=0)
        hint.add_css_class("vs-nav-hint")
        shortcut.append(hint)
        self.append(shortcut)
        self.select_page(selected_name)

    def select_page(self, name: str) -> None:
        """Move the visual selection without emitting navigation."""
        for row, row_name in self.row_names.items():
            if row_name == name:
                self.list_box.select_row(row)
                return

    def _row_activated(self, _list: Gtk.ListBox, row: Gtk.ListBoxRow) -> None:
        """Forward one keyboard- or pointer-activated destination."""
        self.on_activate(self.row_names.get(row, ""))


def segmented_control(
    options: tuple[str, ...],
    selected_index: int,
    on_clicked: Callable[[int], None],
) -> tuple[Gtk.Box, list[Gtk.ToggleButton]]:
    """Create one joined segment group with clearly separated check states."""
    group = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_1)
    group.add_css_class("vs-segment")
    buttons: list[Gtk.ToggleButton] = []
    for index, option in enumerate(options):
        button = Gtk.ToggleButton(label=option, hexpand=True)
        button.set_active(index == selected_index)
        button.connect("clicked", _single_checked_handler(on_clicked, index, buttons))
        group.append(button)
        buttons.append(button)
    return group, buttons


def _single_checked_handler(
    on_clicked: Callable[[int], None],
    index: int,
    buttons: list[Gtk.ToggleButton],
) -> Callable[[Gtk.ToggleButton], None]:
    """Enforce single-checked segments for the lifetime of the group."""

    def clicked(_button: Gtk.ToggleButton) -> None:
        for other, other_button in enumerate(buttons):
            other_button.set_active(other == index)
        on_clicked(index)

    return clicked


def sync_segment_group(buttons: list[Gtk.ToggleButton], selected_index: int) -> None:
    """Project one authoritative selection onto an existing segment group."""
    for index, button in enumerate(buttons):
        button.set_active(index == selected_index)


@dataclass(frozen=True, slots=True)
class RecordingBarState:
    """One bounded snapshot of everything the transient recording bar shows.

    ``kind`` is one of ``"preparing"``, ``"recording"``, or ``"terminal"``.
    A terminal snapshot erases the bar; no other state is ever blanked early.
    """

    kind: str
    detail: str
    elapsed: str
    mode: str
    delivery: str
    level: float
    preview: str
    quiet: bool


RECORDING_KIND_PREPARING: Final = "preparing"
RECORDING_KIND_RECORDING: Final = "recording"
RECORDING_KIND_TERMINAL: Final = "terminal"

# Below this window width the bar keeps only phase, timer, level, and preview.
COMPACT_LAYOUT_MAX_WIDTH: Final = 736


def is_compact_layout(width: int) -> bool:
    """Return whether the shell uses the narrow bottom-navigation layout."""
    return width <= COMPACT_LAYOUT_MAX_WIDTH


class RecordingStatusBar(Gtk.Box):
    """Compact transient strip projecting one :class:`RecordingBarState`.

    The widget only renders the immutable state it is handed, so the same
    projection can later be serialized to a display-only GNOME Shell
    extension without touching this class or the capture lifecycle. The app
    reveals and hides the bar through an outer revealer; this class never
    decides visibility from data other than an explicit terminal snapshot.
    """

    def __init__(self) -> None:
        """Build the bar around display-only children."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=SPACE_1)
        self.add_css_class("vs-recording-bar")
        self.set_halign(Gtk.Align.CENTER)
        self._compact = False

        meters = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_3)
        self.phase_chip = Gtk.Label(label="REC")
        self.phase_chip.add_css_class("vs-live-chip")
        meters.append(self.phase_chip)
        self.time_label = Gtk.Label(label="00:00")
        self.time_label.add_css_class("vs-recording-time")
        self.time_label.set_accessible_role(Gtk.AccessibleRole.STATUS)
        meters.append(self.time_label)
        self.level_bar = Gtk.LevelBar.new_for_interval(0, 1)
        self.level_bar.set_value(0)
        self.level_bar.set_valign(Gtk.Align.CENTER)
        self.level_bar.set_size_request(96, 10)
        self.level_bar.set_tooltip_text("Live microphone level")
        meters.append(self.level_bar)
        self.mode_chip = Gtk.Label()
        self.mode_chip.add_css_class("vs-mode-chip")
        meters.append(self.mode_chip)
        self.delivery_chip = Gtk.Label()
        self.delivery_chip.add_css_class("vs-delivery-chip")
        meters.append(self.delivery_chip)
        self.append(meters)

        self.phase_label = Gtk.Label(xalign=0, ellipsize=Pango.EllipsizeMode.END)
        self.phase_label.add_css_class("vs-recording-phase")
        self.phase_label.set_hexpand(True)
        self.phase_label.set_max_width_chars(64)
        self.append(self.phase_label)

        self.preview_label = Gtk.Label(xalign=0.5, wrap=False, ellipsize=Pango.EllipsizeMode.END)
        self.preview_label.add_css_class("vs-recording-preview")
        self.preview_label.set_max_width_chars(48)
        self.append(self.preview_label)

    def set_compact(self, compact: bool) -> None:
        """Apply responsive disclosure for the narrow layout."""
        self._compact = compact
        self.mode_chip.set_visible(not compact)
        self.delivery_chip.set_visible(not compact)
        self.phase_label.set_visible(not compact)
        self.preview_label.set_max_width_chars(40 if compact else 48)

    def present(self, state: RecordingBarState) -> bool:
        """Project one immutable snapshot; return whether the bar stays visible."""
        if state.kind == RECORDING_KIND_TERMINAL:
            self.clear()
            return False
        preparing = state.kind == RECORDING_KIND_PREPARING
        self.phase_chip.set_label("PREPARE" if preparing else "REC")
        if preparing:
            self.phase_chip.add_css_class("vs-preparing")
        else:
            self.phase_chip.remove_css_class("vs-preparing")
        self.time_label.set_label(state.elapsed)
        self.level_bar.set_value(max(0.0, min(1.0, state.level)))
        self.mode_chip.set_label(state.mode)
        self.delivery_chip.set_label(state.delivery)
        self.phase_label.set_label(state.detail)
        self.preview_label.set_label(state.preview)
        if state.quiet:
            self.preview_label.add_css_class("vs-quiet")
        else:
            self.preview_label.remove_css_class("vs-quiet")
        return True

    def clear(self) -> None:
        """Erase every volatile projection immediately for any terminal state."""
        self.phase_chip.set_label("REC")
        self.phase_chip.remove_css_class("vs-preparing")
        self.time_label.set_label("00:00")
        self.level_bar.set_value(0)
        self.mode_chip.set_label("")
        self.delivery_chip.set_label("")
        self.phase_label.set_label("")
        self.preview_label.set_label("")
        self.preview_label.remove_css_class("vs-quiet")


def set_button_content(button: Gtk.Button, icon_name: str, text: str) -> None:
    """Give one action button a leading icon so state reads without color."""
    content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=SPACE_2)
    icon = Gtk.Image.new_from_icon_name(icon_name)
    content.append(icon)
    label = Gtk.Label(label=text)
    content.append(label)
    button.set_child(content)
