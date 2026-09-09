"""Persistent document behavior and Live rewrite controls."""

from collections.abc import Callable
from dataclasses import replace

import gi

from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.live_rewrite import TEMPLATE_CHOICES

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402


class WorkspaceSettings(Adw.PreferencesPage):
    """Apply a coherent group of validated settings from the UI to the same dotfile."""

    def __init__(self, config: AppConfig, save: Callable[[dict], bool]) -> None:
        """Keep document and Live settings independent from provider connection setup."""
        super().__init__(
            name="workspace",
            title="Workspace",
            icon_name="preferences-system-symbolic",
        )
        self.config = config
        self.save = save
        self.fields: dict[str, Callable] = {}
        self.setters: dict[str, Callable] = {}
        behavior = Adw.PreferencesGroup(title="Documents")
        for name, title in (
            ("auto_copy_dictation", "Copy completed dictation automatically"),
            ("auto_copy_rewrite", "Copy completed rewrites automatically"),
            ("show_copy_action", "Show Copy icon"),
            ("show_save_action", "Show Save icon"),
            ("smooth_scrolling", "Smoothly follow new text"),
        ):
            self.switch(behavior, name, title)
        self.spin(behavior, "review_timeout_seconds", "Widget dismissal delay (seconds)", 1, 60)
        self.spin(behavior, "scroll_duration_ms", "Scroll animation (milliseconds)", 0, 2000)
        self.spin(behavior, "scroll_lookahead_lines", "Space below new text (lines)", 0, 6)
        self.add(behavior)
        live = Adw.PreferencesGroup(
            title="Live rewrite",
            description=(
                "Live rewrite sends provisional recognition to the model as speech arrives. "
                "Stop reconciles the draft with the final transcript. Later updates are grouped; "
                "short tails update after a pause. Batch speech engines update by chunk, not every word. "
                "Extra provider requests may use credits."
            ),
        )
        self.switch(live, "live_rewrite_enabled", "Enable live rewrite for the next dictation")
        self.choice(live, "live_rewrite_template", "Template", TEMPLATE_CHOICES)
        self.spin(live, "live_rewrite_min_characters", "New characters to group after the first draft", 40, 4000)
        self.spin(live, "live_rewrite_interval_seconds", "Minimum time between updates (seconds)", 2, 60)
        custom = Adw.ExpanderRow(title="Custom template instructions")
        editor = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR, accepts_tab=False)
        editor.get_buffer().set_text(config.live_rewrite_custom_instructions)
        scroll = Gtk.ScrolledWindow(min_content_height=150, max_content_height=250)
        scroll.set_child(editor)
        custom.add_row(scroll)
        live.add(custom)
        self.fields["live_rewrite_custom_instructions"] = lambda: editor.get_buffer().get_text(
            editor.get_buffer().get_start_iter(), editor.get_buffer().get_end_iter(), False
        )
        self.setters["live_rewrite_custom_instructions"] = editor.get_buffer().set_text
        self.add(live)
        actions = Adw.PreferencesGroup()
        row = Adw.ActionRow(title="Save settings", subtitle="Changes apply to the next request or recording.")
        apply = Gtk.Button(label="Apply", valign=Gtk.Align.CENTER, css_classes=["suggested-action"])
        apply.connect("clicked", self.apply)
        row.add_suffix(apply)
        actions.add(row)
        self.status = Gtk.Label(xalign=0, wrap=True)
        actions.add(self.status)
        self.add(actions)

    def switch(self, group, name: str, title: str) -> None:
        """Expose a persisted boolean without saving half an edited form."""
        row = Adw.SwitchRow(title=title, active=getattr(self.config, name))
        group.add(row)
        self.fields[name] = row.get_active
        self.setters[name] = row.set_active

    def spin(self, group, name: str, title: str, lower: int, upper: int) -> None:
        """Constrain numeric choices to the config file's validated range."""
        row = Adw.SpinRow(
            title=title,
            adjustment=Gtk.Adjustment(
                value=getattr(self.config, name),
                lower=lower,
                upper=upper,
                step_increment=1,
                page_increment=10,
            ),
        )
        group.add(row)
        self.fields[name] = lambda: int(row.get_value())
        self.setters[name] = row.set_value

    def choice(self, group, name: str, title: str, choices) -> None:
        """Present stable config identifiers using concise visible labels."""
        values = [value for value, _label in choices]
        row = Adw.ComboRow(title=title, model=Gtk.StringList.new([label for _value, label in choices]))
        row.set_selected(values.index(getattr(self.config, name)))
        group.add(row)
        self.fields[name] = lambda: values[row.get_selected()]
        self.setters[name] = lambda value: row.set_selected(values.index(value))

    def refresh_config(self, config: AppConfig) -> None:
        """Reflect choices changed outside this page when the dialog is opened again."""
        self.config = config
        for name, write in self.setters.items():
            write(getattr(config, name))

    def apply(self, _button) -> None:
        """Validate and save the form atomically, preserving it on failure."""
        changes = {name: read() for name, read in self.fields.items()}
        try:
            replace(self.config, **changes)
            if self.save(changes):
                self.status.set_label("Settings saved")
            else:
                self.status.set_label("Could not apply settings. Stop active work, check the provider and try again.")
        except (ValueError, TypeError) as error:
            self.status.set_label(str(error))
