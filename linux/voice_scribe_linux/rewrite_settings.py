"""Catalog-backed Codex controls beside the conversation's rewrite actions."""

from collections.abc import Callable
from dataclasses import replace

import gi

from voice_scribe_linux.codex_client import CodexAppServerError, CodexModel, select_model
from voice_scribe_linux.config import AppConfig

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gtk, Pango  # noqa: E402


class RewriteSettings(Gtk.MenuButton):
    """Edit persisted rewrite choices without making provider calls on the GTK thread."""

    def __init__(
        self,
        config: AppConfig,
        load_models: Callable[[], None],
        save_settings: Callable[[str | None, bool], None],
    ) -> None:
        """Keep a usable saved choice even when Codex cannot supply its catalog."""
        super().__init__()
        self.config = config
        self.load_models = load_models
        self.save_settings = save_settings
        self.models: list[CodexModel] = []
        self.choices: list[str | None] = []
        self.choice_labels: list[str] = []
        self.updating = False
        self.caption = Gtk.Label(ellipsize=Pango.EllipsizeMode.END, max_width_chars=18)
        caption_box = Gtk.Box(spacing=6)
        caption_box.append(self.caption)
        caption_box.append(Gtk.Image(icon_name="pan-down-symbolic"))
        self.set_child(caption_box)
        self.set_tooltip_text("Choose the Codex model and speed for rewrites")
        self.popover = Gtk.Popover()
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8, width_request=300)
        for edge in ("top", "bottom", "start", "end"):
            getattr(content, f"set_margin_{edge}")(12)
        group = Adw.PreferencesGroup(title="Rewriting")
        self.model_row = Adw.ComboRow(title="Codex model")
        self.model_row.connect("notify::selected", self._changed)
        group.add(self.model_row)
        self.fast_row = Adw.SwitchRow(title="Fast mode")
        self.fast_row.connect("notify::active", self._changed)
        group.add(self.fast_row)
        content.append(group)
        self.status = Gtk.Label(xalign=0, wrap=True, max_width_chars=34)
        self.status.add_css_class("caption")
        content.append(self.status)
        self.refresh = Gtk.Button(label="Refresh models", halign=Gtk.Align.START)
        self.refresh.connect("clicked", lambda _button: self.load_models())
        content.append(self.refresh)
        self.popover.set_child(content)
        self.popover.connect("show", lambda _popover: self.load_models())
        self.set_popover(self.popover)
        self.set_config(config)

    def set_config(self, config: AppConfig) -> None:
        """Reflect saved settings, guarding GTK notifications during a catalog refresh or rollback."""
        remote = config.rewrite_provider == "litellm"
        self.config = config
        self.model_row.set_title("LiteLLM model" if remote else "Codex model")
        self.fast_row.set_visible(not remote)
        self.set_tooltip_text("Choose the rewrite model and speed")
        if remote:
            config = replace(
                config, rewrite_model=config.litellm_model, codex_model=config.litellm_model, rewrite_fast_mode=False
            )
        self.updating = True
        selected = config.rewrite_model
        choices = [None]
        labels = ["Default"]
        for model in self.models:
            if not model.hidden or selected in {model.id, model.identifier}:
                choices.append(model.identifier)
                labels.append(model.name)
                if selected == model.id:
                    selected = model.identifier
        if selected not in choices:
            choices.append(selected)
            labels.append(f"{selected} (unavailable)" if self.models else selected)
        if choices != self.choices or labels != self.choice_labels:
            self.choices = choices
            self.choice_labels = labels
            self.model_row.set_model(Gtk.StringList.new(labels))
        self.model_row.set_selected(self.choices.index(selected))
        self.model_row.set_sensitive(bool(self.models))
        try:
            effective = select_model(self.models, config.rewrite_model or config.codex_model)
        except CodexAppServerError:
            effective = None
        supports_fast = effective is not None and effective.fast_tier is not None
        # A stale saved choice must still be switchable off when a model loses Fast support.
        self.fast_row.set_sensitive(supports_fast or config.rewrite_fast_mode)
        self.fast_row.set_active(config.rewrite_fast_mode)
        self.fast_row.set_subtitle(
            "Uses more Codex credits"
            if supports_fast
            else "Unavailable for this model"
            if self.models
            else "Load models to check availability"
        )
        self.model_row.set_subtitle(
            f"Default: {effective.name}" if selected is None and effective is not None else "Applies to rewrites only"
        )
        label = effective.name if effective is not None else config.rewrite_model or "Codex model"
        self.caption.set_label(f"{label} · Fast" if config.rewrite_fast_mode else label)
        self.updating = False

    def set_loading(self) -> None:
        """Expose bounded background discovery while keeping the saved configuration intact."""
        self.refresh.set_sensitive(False)
        self.status.set_label("Loading models…")

    def set_models(self, models: list[CodexModel] | None) -> None:
        """Publish catalog results without changing persisted selections on discovery failure."""
        self.refresh.set_sensitive(True)
        if not models:
            self.status.set_label("Could not load models. Check your provider, then refresh.")
            return
        self.models = models
        self.set_config(self.config)
        self.status.set_label(
            "Select an available deployment. You can also enter its alias in Settings → Providers."
            if self.config.rewrite_provider == "litellm"
            else "Rewrites use low reasoning when supported. Default follows your Codex model setting."
        )

    def _changed(self, *_args: object) -> None:
        """Save an explicit choice, clearing Fast when the newly chosen model cannot serve it."""
        if self.updating:
            return
        selected = self.choices[self.model_row.get_selected()]
        try:
            model = select_model(self.models, selected or self.config.codex_model)
        except CodexAppServerError:
            fast = False
        else:
            fast = self.fast_row.get_active() and model.fast_tier is not None
        if self.config.rewrite_provider == "litellm":
            if selected != self.config.litellm_model:
                self.save_settings(selected, False)
            return
        if (selected, fast) == (self.config.rewrite_model, self.config.rewrite_fast_mode):
            return
        self.save_settings(selected, fast)
