"""Model-aware thinking controls shared by both rewrite settings surfaces."""

import gi

from mluva_linux.codex_client import CodexAppServerError, select_model

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk  # noqa: E402

COMPATIBLE_EFFORTS = ("none", "minimal", "low", "medium", "high", "xhigh", "max")
EFFORT_LABELS = {"none": "None", "xhigh": "Extra high"}


class ThinkingRow(Adw.ComboRow):
    """Offer advertised native levels and explicit compatible-server overrides."""

    def __init__(self, changed) -> None:
        """Keep model discovery and saving outside this shared control."""
        super().__init__(title="Thinking level")
        self.changed = changed
        self.updating = False
        self.choices = [None]
        self.choice_labels = []
        self.connect("notify::selected", self._changed)

    def configure(self, config, models) -> None:
        """Keep stale selections recoverable without silently changing saved preferences."""
        remote = config.rewrite_provider == "litellm"
        effort = config.litellm_reasoning_effort if remote else config.rewrite_reasoning_effort
        try:
            model = select_model(models, config.litellm_model if remote else config.rewrite_model or config.codex_model)
        except CodexAppServerError:
            model = None
        levels = model.reasoning_efforts if model else ()
        if remote and not levels:
            levels = COMPATIBLE_EFFORTS
        self.updating = True
        self.choices = [None, *levels]
        labels = ["Provider default" if remote else "Auto · low when supported"]
        labels.extend(EFFORT_LABELS.get(level, level.replace("_", " ").capitalize()) for level in levels)
        if effort not in self.choices:
            self.choices.append(effort)
            labels.append(f"{effort} · unavailable")
        if labels != self.choice_labels:
            self.choice_labels = labels
            self.set_model(Gtk.StringList.new(labels))
        self.set_selected(self.choices.index(effort))
        self.set_visible(config.rewrite_provider != "none")
        self.set_sensitive(bool(levels) or effort is not None)
        self.set_subtitle(
            "Server support varies by model; default sends no override."
            if remote and (model is None or not model.reasoning_efforts)
            else "Applies to rewrites and Live rewrite."
            if levels
            else "Refresh models to check supported levels."
            if not models
            else "This model advertises no thinking levels."
        )
        self.updating = False

    def _changed(self, *_args) -> None:
        """Save only an intentional selection, never catalog-driven notifications."""
        if not self.updating:
            self.changed(self.choices[self.get_selected()])
