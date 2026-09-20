"""A compact searchable modal for language selection with flags and native names."""

import gi

from mluva_linux.speech_languages import LANGUAGES, language_label, supported_languages

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402


class LanguagePicker(Gtk.Button):
    """Expose languages without a large permanent menu in onboarding."""

    def __init__(self, language, model, changed):
        """Keep the selected language, model capabilities and save callback together."""
        super().__init__(label=language_label(language), halign=Gtk.Align.START)
        self.language, self.model, self.changed = language, model, changed
        self.set_tooltip_text("Choose recognition language")
        self.connect("clicked", self._open)
        self.dialog = None

    def refresh(self, language, model):
        """Refresh labels without changing a user's persisted language silently."""
        self.language, self.model = language, model
        self.set_label(language_label(language))

    def _open(self, *_args):
        dialog = self.dialog = Adw.Dialog(title="Recognition language", content_width=440, content_height=430)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        box.append(Adw.HeaderBar())
        search = Gtk.SearchEntry(placeholder_text="Find a language", margin_start=16, margin_end=16)
        box.append(search)
        auto = Gtk.Button(label="◎ Detect automatically", margin_start=16, margin_end=16)
        auto.connect("clicked", lambda *_args: self._choose("auto"))
        box.append(auto)
        flow = Gtk.FlowBox(
            selection_mode=Gtk.SelectionMode.NONE,
            homogeneous=True,
            min_children_per_line=2,
            max_children_per_line=2,
            row_spacing=6,
            column_spacing=6,
            margin_start=16,
            margin_end=16,
            margin_bottom=16,
        )
        allowed = supported_languages(self.model)
        for code, iso, name, flag in LANGUAGES:
            if code not in allowed:
                continue
            button = Gtk.Button(label=f"{flag} {name}", tooltip_text=name, hexpand=True)
            button.connect("clicked", lambda _button, value=code: self._choose(value))
            flow.append(button)
            button.get_parent().search_text = f"{code} {iso} {name}".casefold()
        flow.set_filter_func(lambda child: search.get_text().casefold() in child.search_text)
        search.connect("search-changed", lambda *_args: flow.invalidate_filter())
        scroll = Gtk.ScrolledWindow(vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER)
        scroll.set_child(flow)
        box.append(scroll)
        dialog.set_child(box)
        dialog.present(self)

    def _choose(self, code):
        self.language = code
        self.set_label(language_label(code))
        self.changed(code)
        if self.dialog:
            self.dialog.close()
