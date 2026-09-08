"""Persistent workspace behavior and independent rewrite and speech provider controls."""

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

    def __init__(self, config: AppConfig, save: Callable[[dict], bool], *, providers: bool = False) -> None:
        """Expose credentials by environment variable name; secret values never enter the UI."""
        super().__init__(
            name="providers" if providers else "workspace",
            title="Providers" if providers else "Workspace",
            icon_name="preferences-system-symbolic",
        )
        self.config = config
        self.save = save
        self.fields: dict[str, Callable] = {}
        self.setters: dict[str, Callable] = {}
        if providers:
            rewrite = Adw.PreferencesGroup(
                title="Rewriting", description="Codex app-server, or any configured LiteLLM deployment."
            )
            self.choice(
                rewrite,
                "rewrite_provider",
                "Provider",
                (("codex", "Codex app-server"), ("litellm", "LiteLLM / compatible API")),
            )
            self.entry(rewrite, "litellm_base_url", "API base URL")
            self.entry(rewrite, "litellm_model", "Rewrite model / deployment alias", optional=True)
            self.entry(rewrite, "litellm_api_key_env", "API key environment variable")
            self.add(rewrite)
            speech = Adw.PreferencesGroup(
                title="Dictation",
                description="Voxtype uses local Whisper. LiteLLM routes audio to the service you configure.",
            )
            self.choice(
                speech,
                "transcription_provider",
                "Speech provider",
                (
                    ("elevenlabs", "ElevenLabs Scribe"),
                    ("voxtype", "Voxtype · local Whisper"),
                    ("litellm", "LiteLLM / compatible API"),
                ),
            )
            self.entry(speech, "transcription_base_url", "Speech API base URL")
            self.entry(speech, "transcription_remote_model", "Speech model / deployment alias")
            self.entry(speech, "transcription_api_key_env", "Speech key environment variable")
            self.entry(speech, "voxtype_model", "Local Whisper model (empty uses Voxtype config)", optional=True)
            self.spin(speech, "transcription_chunk_seconds", "Preview chunk length (seconds)", 3, 30)
            self.add(speech)
            info = Adw.PreferencesGroup(
                description=(
                    "Use an existing LiteLLM proxy or OpenAI-compatible server, including a local model server. "
                    "Choose its deployment alias here; service credentials stay in the proxy or environment. "
                    "Batch providers send preview chunks in Live rewrite mode, then the whole recording at Stop. "
                    "Meeting continues to use ElevenLabs diarization."
                )
            )
            self.add(info)
        else:
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
                description="See a structured draft while speaking. Extra provider requests may use credits.",
            )
            self.switch(live, "live_rewrite_enabled", "Enable live rewrite for the next dictation")
            self.choice(live, "live_rewrite_template", "Template", TEMPLATE_CHOICES)
            self.spin(live, "live_rewrite_min_characters", "New characters before updating", 40, 4000)
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

    def entry(self, group, name: str, title: str, *, optional: bool = False) -> None:
        """Edit a nonsecret field, supporting an unset optional model."""
        row = Adw.EntryRow(title=title)
        row.set_text(getattr(self.config, name) or "")
        group.add(row)
        self.fields[name] = lambda: (row.get_text().strip() or None) if optional else row.get_text().strip()
        self.setters[name] = lambda value: row.set_text(value or "")

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
