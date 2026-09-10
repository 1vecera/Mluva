"""Provider-first speech and rewrite settings with optional, asynchronous model discovery."""

import os
import threading
from collections.abc import Callable
from dataclasses import replace

import gi

from mluva_linux.codex_client import CodexAppServerError, CodexModel, select_model
from mluva_linux.config import AppConfig
from mluva_linux.provider_catalog import (
    REWRITE_PROVIDERS,
    SPEECH_PROVIDERS,
    CatalogRequest,
    catalog_message,
    connection_hint,
    read_catalog,
)
from mluva_linux.providers import valid_model_id

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402

_MANUAL = object()


class ProviderSection(Adw.PreferencesGroup):
    """Keep one task's provider, models, connection and unsaved edits together."""

    def __init__(self, config: AppConfig, scope: str) -> None:
        """Build only the controls for supported routes, with connection details collapsed."""
        super().__init__(title="Speech recognition" if scope == "speech" else "Rewriting")
        self.scope = scope
        self.providers = SPEECH_PROVIDERS if scope == "speech" else REWRITE_PROVIDERS
        self.provider_field = "transcription_provider" if scope == "speech" else "rewrite_provider"
        self.base_field = "transcription_base_url" if scope == "speech" else "litellm_base_url"
        self.key_field = "transcription_api_key_env" if scope == "speech" else "litellm_api_key_env"
        self.config = config
        self.updating = True
        self.models: list[CodexModel] = []
        self.catalog_loaded = False
        self.catalog_client = None
        self.choices = []
        self.provider_row = Adw.ComboRow(
            title="Provider",
            model=Gtk.StringList.new([provider.label for provider in self.providers]),
        )
        self.provider_row.connect("notify::selected", self._provider_changed)
        self.add(self.provider_row)
        self.fixed_model_row = Adw.ActionRow(title="Model", subtitle="Scribe v2 · live recognition")
        self.add(self.fixed_model_row)
        self.model_row = Adw.ComboRow(title="Model", enable_search=True, use_subtitle=True)
        self.model_row.set_subtitle_lines(0)
        self.model_row.connect("notify::selected", self._model_changed)
        self.add(self.model_row)
        self.model_entry = Adw.EntryRow(title="Model ID / deployment alias")
        self.model_entry.connect("changed", self._manual_changed)
        self.add(self.model_entry)
        self.fast_row = Adw.SwitchRow(title="Fast mode")
        self.fast_row.connect("notify::active", self._fast_changed)
        self.add(self.fast_row)
        self.connection = Adw.ActionRow(title="Connection")
        self.connection.set_subtitle_lines(0)
        self.refresh_button = Gtk.Button(label="Refresh", valign=Gtk.Align.CENTER)
        self.refresh_button.set_tooltip_text("Refresh model availability without sending audio or text")
        self.refresh_button.connect("clicked", self._load_models)
        self.connection.add_suffix(self.refresh_button)
        self.add(self.connection)
        self.advanced = Adw.ExpanderRow(title="Connection details", subtitle="Endpoint and key variable name")
        self.endpoint_entry = Adw.EntryRow(title="API base URL (include the API prefix)")
        self.key_entry = Adw.EntryRow(title="API key environment variable · name only")
        for entry, field in ((self.endpoint_entry, self.base_field), (self.key_entry, self.key_field)):
            entry.connect("changed", self._connection_changed, field)
            self.advanced.add_row(entry)
        self.advanced.add_row(
            Adw.ActionRow(
                title="Keep keys in your secret manager",
                subtitle="Use HTTPS, or HTTP for a local server. Never put a key in the URL or these fields.",
            )
        )
        self.add(self.advanced)
        self.preview = Adw.ExpanderRow(title="Live speech previews")
        self.chunk_row = Adw.SpinRow(
            title="Seconds per audio chunk",
            adjustment=Gtk.Adjustment(lower=3, upper=30, step_increment=1, page_increment=5),
        )
        self.chunk_row.connect("notify::value", self._chunk_changed)
        self.preview.add_row(self.chunk_row)
        self.preview.add_row(
            Adw.ActionRow(
                title="Batch providers recognize again at Stop",
                subtitle="Shorter chunks update sooner but make more requests. Cloud previews can increase usage.",
            )
        )
        self.add(self.preview)
        self.status = Gtk.Label(xalign=0, wrap=True, margin_top=8, margin_bottom=8)
        self.status.add_css_class("caption")
        self.add(self.status)
        self.connect("unmap", self._stop_lookup)
        self.refresh_config(config)

    @property
    def provider(self):
        """Resolve the selected display row to its stable configuration identity."""
        return self.providers[self.provider_row.get_selected()]

    def refresh_config(self, config: AppConfig) -> None:
        """Start a new edit from persisted settings when the dialog is reopened."""
        self._stop_lookup()
        self.config = config
        names = [self.provider_field, self.base_field, self.key_field, *(p.model_field for p in self.providers)]
        names.append("transcription_chunk_seconds" if self.scope == "speech" else "rewrite_fast_mode")
        self.values = {name: getattr(config, name) for name in names}
        self.updating = True
        self.provider_row.set_selected([p.id for p in self.providers].index(self.values[self.provider_field]))
        self.endpoint_entry.set_text(self.values[self.base_field])
        self.key_entry.set_text(self.values[self.key_field])
        self.chunk_row.set_value(config.transcription_chunk_seconds)
        self.updating = False
        self._reset_catalog()

    def _provider_changed(self, *_args) -> None:
        """Keep each provider's model draft while exposing only the selected route."""
        if not self.updating:
            self.values[self.provider_field] = self.provider.id
            self._reset_catalog()

    def _reset_catalog(self) -> None:
        """Invalidate old endpoint results and show local setup evidence without making a request."""
        self._stop_lookup()
        self.models = []
        self.catalog_loaded = False
        provider = self.provider
        self.set_description(provider.description)
        self.advanced.set_visible(provider.id == "litellm")
        self.preview.set_visible(self.scope == "speech" and provider.id != "elevenlabs")
        self.fast_row.set_visible(provider.id == "codex")
        self.refresh_button.set_visible(provider.id != "elevenlabs")
        self.fixed_model_row.set_visible(provider.id == "elevenlabs")
        self.model_row.set_visible(provider.id != "elevenlabs")
        self.connection.set_subtitle(connection_hint(provider.id, self.values[self.key_field], os.environ))
        self.status.set_label("")
        self.status.set_visible(False)
        if provider.id == "elevenlabs":
            self.models = [CodexModel("scribe_v2", "scribe_v2", "Scribe v2", True)]
        self._show_models()

    def _show_models(self) -> None:
        """Retain a saved or typed model even when a server omits it from its catalog."""
        self.updating = True
        selected = self.values[self.provider.model_field]
        choices, labels = [], []
        if self.provider.default_label:
            choices.append(None)
            label = self.provider.default_label
            if self.provider.id == "codex" and self.config.codex_model:
                label += f" ({self.config.codex_model})"
            labels.append(label)
        visible = [m for m in self.models if not m.hidden or selected in {m.id, m.identifier}]
        current = next((m for m in visible if selected in {m.id, m.identifier}), None)
        if selected:
            choices.append(selected)
            labels.append(current.name if current else f"{selected} · not listed" if self.catalog_loaded else selected)
        for model in visible:
            if model is not current and model.identifier not in choices:
                choices.append(model.identifier)
                labels.append(model.name)
        if self.provider.id != "elevenlabs":
            choices.append(_MANUAL)
            labels.append("Enter model ID…")
        self.choices = choices
        self.model_row.set_model(Gtk.StringList.new(labels))
        self.model_row.set_selected(choices.index(selected) if selected in choices else choices.index(_MANUAL))
        self.model_row.set_sensitive(self.provider.id != "elevenlabs")
        self.model_entry.set_text(selected or "")
        self.model_entry.set_visible(self.choices[self.model_row.get_selected()] is _MANUAL)
        self.updating = False
        self._show_fast()

    def _model_changed(self, *_args) -> None:
        """Select a model or reveal manual entry without selecting an arbitrary catalog default."""
        if self.updating:
            return
        selected = self.choices[self.model_row.get_selected()]
        self.model_entry.set_visible(selected is _MANUAL)
        if selected is not _MANUAL:
            self.values[self.provider.model_field] = selected
            self._show_fast(reset=True)
        else:
            self.updating = True
            self.model_entry.set_text(self.values[self.provider.model_field] or "")
            self.updating = False
            self.model_entry.grab_focus()

    def _manual_changed(self, *_args) -> None:
        """Keep manual IDs editable offline, with final validation at Apply."""
        if not self.updating:
            self.values[self.provider.model_field] = self.model_entry.get_text().strip() or None
            self._show_fast(reset=True)

    def _show_fast(self, *, reset: bool = False) -> None:
        """Offer only advertised Codex speed tiers, while allowing a stale tier to be disabled."""
        if self.provider.id != "codex":
            return
        try:
            selected = select_model(self.models, self.values["rewrite_model"] or self.config.codex_model)
            supported = selected.fast_tier is not None
        except CodexAppServerError:
            supported = False
        if reset and not supported:
            self.values["rewrite_fast_mode"] = False
        self.updating = True
        active = self.values["rewrite_fast_mode"]
        self.fast_row.set_active(active)
        self.fast_row.set_sensitive(supported or active)
        self.fast_row.set_subtitle(
            "Uses more Codex credits"
            if supported
            else "Unavailable for this model"
            if self.catalog_loaded
            else "Refresh models to check Fast availability"
        )
        self.updating = False

    def _fast_changed(self, *_args) -> None:
        """Preserve native Codex speed choice independently from a compatible server."""
        if not self.updating:
            self.values["rewrite_fast_mode"] = self.fast_row.get_active()

    def _chunk_changed(self, *_args) -> None:
        """Retain the bounded preview setting without changing capture scheduling."""
        if not self.updating and self.scope == "speech":
            self.values["transcription_chunk_seconds"] = int(self.chunk_row.get_value())

    def _connection_changed(self, entry, field: str) -> None:
        """Forget old credentials' catalog as soon as an endpoint or key reference is edited."""
        if not self.updating:
            self.values[field] = entry.get_text().strip()
            self._reset_catalog()

    def _load_models(self, _button) -> None:
        """Freeze the edited route and run one content-free check away from GTK."""
        if self.catalog_client is not None:
            return
        try:
            replace(self.config, **self.values)
            request = CatalogRequest(
                self.scope,
                self.provider.id,
                self.values[self.base_field],
                self.values[self.key_field],
            )
            client = request.client()
        except (ValueError, TypeError):
            self.status.set_label("Check the model ID, endpoint and key variable name in Connection details.")
            self.status.set_visible(True)
            return
        self.catalog_client = client
        self.refresh_button.set_sensitive(False)
        self.status.set_label("Loading models… Your selection will stay unchanged.")
        self.status.set_visible(True)

        def load():
            """Close every child/response and return only catalog data or a controlled failure."""
            try:
                models = read_catalog(client, request)
            except Exception:
                models = None
            finally:
                try:
                    client.close()
                except Exception:
                    models = None
            GLib.idle_add(self._models_loaded, client, request, models)

        threading.Thread(target=load, name=f"{self.scope}-models", daemon=True).start()

    def _models_loaded(self, client, request: CatalogRequest, models: list[CodexModel] | None) -> bool:
        """Reject late catalogs after a provider switch, endpoint edit, dialog close or reopening."""
        if client is not self.catalog_client:
            return GLib.SOURCE_REMOVE
        self.catalog_client = None
        self.refresh_button.set_sensitive(True)
        if models is not None:
            self.models = models
            self.catalog_loaded = True
            self._show_models()
        self.status.set_label(catalog_message(request, models))
        self.status.set_visible(True)
        return GLib.SOURCE_REMOVE

    def _stop_lookup(self, *_args) -> None:
        """Invalidate immediately, keeping process/HTTP cleanup off the UI thread."""
        client = self.catalog_client
        self.catalog_client = None
        self.refresh_button.set_sensitive(True)
        if client is not None:
            self.status.set_label("Model refresh stopped. Your choice is kept.")

            def cancel():
                """Keep transport cleanup failures out of the UI and logs after invalidation."""
                try:
                    client.cancel()
                except Exception:
                    pass

            threading.Thread(target=cancel, name="cancel-model-list", daemon=True).start()


class ProviderSettings(Adw.PreferencesPage):
    """Save speech and rewrite choices atomically, without depending on successful discovery."""

    def __init__(self, config: AppConfig, save: Callable[[dict], bool]) -> None:
        """Use the existing config and save gate; no provider form changes the primary workspace."""
        super().__init__(name="providers", title="Providers", icon_name="network-server-symbolic")
        self.config = config
        self.save = save
        self.speech = ProviderSection(config, "speech")
        self.rewrite = ProviderSection(config, "rewrite")
        self.add(self.speech)
        self.add(self.rewrite)
        actions = Adw.PreferencesGroup()
        row = Adw.ActionRow(title="Save provider choices", subtitle="Applies to the next recording or request.")
        self.apply_button = Gtk.Button(label="Apply", valign=Gtk.Align.CENTER, css_classes=["suggested-action"])
        self.apply_button.connect("clicked", self.apply)
        row.add_suffix(self.apply_button)
        actions.add(row)
        self.status = Gtk.Label(xalign=0, wrap=True, margin_top=8)
        actions.add(self.status)
        self.add(actions)

    def refresh_config(self, config: AppConfig) -> None:
        """Reflect choices saved from the compact rewrite picker or another settings page."""
        self.config = config
        self.speech.refresh_config(config)
        self.rewrite.refresh_config(config)
        self.status.set_label("")

    def apply(self, _button) -> None:
        """Keep drafts on validation/write failure and persist no credentials or discovery output."""
        for section in (self.speech, self.rewrite):
            model = section.values[section.provider.model_field]
            if (model is not None and not valid_model_id(model)) or (section.provider.id == "litellm" and not model):
                self.status.set_label(f"Choose a {section.scope} model or enter its deployment ID.")
                return
        changes = self.speech.values | self.rewrite.values
        try:
            config = replace(self.config, **changes)
        except (ValueError, TypeError):
            self.status.set_label("Check Connection details. Use a plain base URL and an environment variable name.")
            return
        if self.save(changes):
            self.config = self.speech.config = self.rewrite.config = config
            self.status.set_label("Provider choices saved. They apply to the next recording or request.")
        else:
            self.status.set_label("Could not save. Stop active work and try again; your edits are kept.")
