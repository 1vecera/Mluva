"""A size-ordered local model slider with cancellable verified downloads."""

import os
import threading

import gi

from mluva_linux import local_gpu
from mluva_linux.language_picker import LanguagePicker
from mluva_linux.local_models import MODELS, download, ready, runtime_ready
from mluva_linux.speech_languages import supports_language

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import GLib, Gtk  # noqa: E402


class LocalModelSettings(Gtk.Box):
    """Keep Continue disabled until the exact selected model is ready."""

    def __init__(self, selected, changed, device="cpu", language="eng", language_changed=lambda code: None):
        """Display catalog metadata without loading any model into memory."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8, margin_top=12, margin_bottom=12)
        self.updating = False
        self.changed = changed
        self.cancelled = None
        self.generation = 0
        self.pending = 0
        self.label = Gtk.Label(xalign=0, wrap=True)
        self.append(self.label)
        self.scale = Gtk.Scale.new_with_range(Gtk.Orientation.HORIZONTAL, 0, len(MODELS) - 1, 1)
        self.scale.set_draw_value(False)
        self.scale.set_round_digits(0)
        self.scale.set_value(next(i for i, model in enumerate(MODELS) if model["id"] == selected))
        self.scale.update_property([Gtk.AccessibleProperty.LABEL], ["Local model size, smallest to largest"])
        for i in range(len(MODELS)):
            self.scale.add_mark(i, Gtk.PositionType.BOTTOM, None)
        self.append(self.scale)
        ends = Gtk.Box()
        ends.append(Gtk.Label(label="Smaller / lighter", xalign=0, hexpand=True))
        ends.append(Gtk.Label(label="Larger / heavier", xalign=1))
        self.append(ends)
        self.gpu = Gtk.CheckButton(label="Use NVIDIA GPU", active=device == "cuda")
        self.gpu.set_visible(bool(local_gpu.gpu_name()) or device == "cuda")
        self.gpu.set_tooltip_text(local_gpu.gpu_name() or "NVIDIA GPU unavailable on this computer")
        self.gpu.connect("toggled", self._selected)
        self.append(self.gpu)
        self.language_changed = language_changed
        self.languages = LanguagePicker(language, selected, self._language_changed)
        self.append(self.languages)
        self.requirements = Gtk.Label(xalign=0, wrap=True, css_classes=["caption"])
        self.append(self.requirements)
        self.progress = Gtk.ProgressBar(show_text=True)
        self.append(self.progress)
        self.button = Gtk.Button(label="Download model", halign=Gtk.Align.START)
        self.button.connect("clicked", lambda _button: self.start())
        self.append(self.button)
        self.status = Gtk.Label(xalign=0, wrap=True)
        self.append(self.status)
        ram = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE") / 1024**3
        self.append(
            Gtk.Label(
                label=f"This computer: {ram:.0f} GiB RAM · no GPU required\n"
                "Local previews use short audio chunks. Qwen also streams text as it decodes. No paid fallback.",
                xalign=0,
                wrap=True,
                css_classes=["caption"],
            )
        )
        self.scale.connect("value-changed", self._selected)
        self.refresh()

    @property
    def selected(self):
        """Resolve a discrete slider stop to a stable model identifier."""
        return MODELS[round(self.scale.get_value())]["id"]

    @property
    def device(self):
        """CPU stays the portable default; GPU is an explicit optional download."""
        return "cuda" if self.gpu.get_active() else "cpu"

    def is_ready(self):
        """Gate both selected weights and their optional execution runtime."""
        return self.downloaded() and supports_language(self.selected, self.languages.language)

    def downloaded(self):
        """Track files separately from whether the selected language is supported."""
        return ready(self.selected) and runtime_ready(self.selected, self.device)

    def _language_changed(self, language):
        self.language_changed(language)
        self.refresh()
        self.changed(self.selected)

    def refresh(self):
        """Reflect verified on-disk readiness, never mere download progress."""
        model = MODELS[round(self.scale.get_value())]
        self.languages.refresh(self.languages.language, self.selected)
        size = sum(item["size"] for item in model["files"]) / 1_000_000
        self.label.set_label(f"{model['label']} · {model['ram_mb'] / 1000:g} GB RAM · {size:.0f} MB storage")
        extra = (
            (
                "Compact GPU runtime included. "
                if self.selected == "qwen3-1.7b"
                else "GPU support adds up to 3.5 GB storage and additional working RAM. "
            )
            if self.device == "cuda"
            else ""
        )
        self.requirements.set_label(extra + "Estimated working memory on CPU.")
        available = self.downloaded()
        self.status.set_label(
            "Ready to use. Model memory is released after recording."
            if available
            else "Download required before continuing."
        )
        if not supports_language(self.selected, self.languages.language):
            self.status.set_label("Choose a supported language above before continuing.")
        self.progress.set_fraction(1 if available else 0)
        self.progress.set_text("Ready" if available else "Not downloaded")
        self.button.set_sensitive(not available)
        self.button.set_visible(not available)
        self.progress.set_visible(False)

    def set_selection(self, identifier, device="cpu"):
        """Refresh a saved selection without starting an unrequested download."""
        self.updating = True
        self.scale.set_value(next(i for i, model in enumerate(MODELS) if model["id"] == identifier))
        self.gpu.set_active(device == "cuda")
        self.updating = False
        self.refresh()

    def _selected(self, *_args):
        if self.updating:
            return
        self.stop()
        self.refresh()
        self.changed(self.selected)

    def start(self):
        """Start only a selected download, keeping network and hashing off GTK."""
        self.pending = 0
        if self.downloaded() or self.cancelled is not None:
            return GLib.SOURCE_REMOVE
        self.generation += 1
        generation = self.generation
        identifier = self.selected
        gpu = self.device == "cuda"
        cancelled = self.cancelled = threading.Event()
        self.button.set_sensitive(False)
        self.progress.set_visible(True)
        self.status.set_label(
            "Installing GPU support and verifying model…" if gpu else "Downloading and verifying model…"
        )
        self.changed(identifier)

        def work():
            previous = -1

            def progress(value):
                nonlocal previous
                percent = int(value * 100)
                if percent != previous:
                    previous = percent
                    GLib.idle_add(self._progress, generation, value)

            message = ""
            try:
                download(identifier, progress, cancelled, gpu=gpu)
            except RuntimeError as error:
                message = str(error)
            except Exception:
                message = "Download did not finish. Check your connection and disk space, then retry."
            GLib.idle_add(self._finished, generation, message)

        threading.Thread(target=work, daemon=True, name="local-model-download").start()
        return GLib.SOURCE_REMOVE

    def _progress(self, generation, value):
        if generation == self.generation:
            self.progress.set_fraction(value)
            self.progress.set_text(f"{value:.0%} · verifying before use")
        return GLib.SOURCE_REMOVE

    def _finished(self, generation, message):
        if generation == self.generation:
            self.cancelled = None
            self.refresh()
            if message:
                self.status.set_label(message)
            self.changed(self.selected)
        return GLib.SOURCE_REMOVE

    def stop(self):
        """Cancel stale selection work; a late completion cannot unlock a different model."""
        self.generation += 1
        if self.pending:
            GLib.source_remove(self.pending)
            self.pending = 0
        if self.cancelled:
            self.cancelled.set()
            self.cancelled = None
