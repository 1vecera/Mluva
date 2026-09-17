"""Full-window settings and provider onboarding built from the real preference pages."""

import threading
from collections.abc import Callable
from dataclasses import replace

import gi

from mluva_linux.appearance_settings import AppearanceSettings
from mluva_linux.config import AppConfig, elevenlabs_api_key
from mluva_linux.credentials import store_speech_key
from mluva_linux.polish_preview import PolishPreview
from mluva_linux.provider_settings import ProviderSection
from mluva_linux.ui import SPACE_2, SPACE_4, set_margins

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402


class SettingsView(Gtk.Box):
    """Use the application viewport without a nested window or modal boundary."""

    def __init__(self, go_back: Callable[[], None]) -> None:
        """Keep all preference pages reachable at narrow and wide window sizes."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, vexpand=True)
        self.go_back = go_back
        header = Gtk.Box(spacing=SPACE_2)
        set_margins(header, SPACE_4)
        back = Gtk.Button(icon_name="go-previous-symbolic", tooltip_text="Back to workspace · Esc", has_frame=False)
        back.connect("clicked", lambda _button: self.close())
        header.append(back)
        header.append(Gtk.Label(label="Settings", xalign=0, css_classes=["title-2"]))
        self.append(header)
        self.navigation = Gtk.FlowBox(
            selection_mode=Gtk.SelectionMode.NONE,
            homogeneous=True,
            min_children_per_line=2,
            max_children_per_line=4,
            row_spacing=6,
            column_spacing=6,
        )
        self.navigation.set_margin_start(16)
        self.navigation.set_margin_end(16)
        self.navigation.set_margin_bottom(8)
        self.buttons = []
        self.append(self.navigation)
        self.stack = Gtk.Stack(vexpand=True, hexpand=True)
        self.pages: list[Adw.PreferencesPage] = []
        self.append(self.stack)

    def add(self, page: Adw.PreferencesPage) -> None:
        """Index the actual page, retaining row deep links from command search."""
        self.pages.append(page)
        self.stack.add_named(page, page.get_name())
        button = Gtk.ToggleButton(label=page.get_title(), hexpand=True)
        if self.buttons:
            button.set_group(self.buttons[0])
        button.connect(
            "toggled", lambda selected: self.stack.set_visible_child(page) if selected.get_active() else None
        )
        self.buttons.append(button)
        self.navigation.append(button)
        if len(self.pages) == 1:
            button.set_active(True)

    def set_visible_page(self, page: Adw.PreferencesPage) -> None:
        """Select a page without introducing a second copy of its settings state."""
        self.buttons[self.pages.index(page)].set_active(True)
        self.stack.set_visible_child(page)

    def set_visible_page_name(self, name: str) -> None:
        """Open a stable destination used by commands and external app actions."""
        self.set_visible_page(next(page for page in self.pages if page.get_name() == name))

    def get_visible_page(self) -> Adw.PreferencesPage:
        """Return the real page for deep-link and focus verification."""
        return self.stack.get_visible_child()

    def get_visible_page_name(self) -> str:
        """Return the stable route for the currently visible settings page."""
        return self.stack.get_visible_child_name()

    def close(self) -> None:
        """Return to the workspace without closing or resizing the app."""
        self.go_back()


class WelcomeView(Gtk.Box):
    """Three clear steps: speech, optional rewriting, and a live recorder preview."""

    def __init__(self, config: AppConfig, save: Callable[[dict], bool], finish: Callable[[], None]) -> None:
        """Gate navigation on local model readiness and retain edits on failed saves."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, vexpand=True)
        self.config, self.save, self.finish = config, save, finish
        self.step = 0
        self.saving_key = False
        self.advance_after_key = False
        self.steps = Gtk.Stack(vexpand=True, vhomogeneous=False, hhomogeneous=False)
        self.speech = ProviderSection(config, "speech")
        self.rewrite = ProviderSection(config, "rewrite")
        self.speech.set_title("")
        self.rewrite.set_title("")
        self.providers = self
        self.appearance = AppearanceSettings(config)
        self.appearance.set_title("")
        self.appearance.set_description("")
        self.title = Gtk.Label(label="Choose speech recognition", xalign=0, wrap=True, css_classes=["title-1"])
        header = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        set_margins(header, 16)
        header.append(Gtk.Label(label="Set up Mluva in three steps", xalign=0, css_classes=["dim-label"]))
        header.append(self.title)
        self.step_label = Gtk.Label(xalign=0)
        header.append(self.step_label)
        self.progress = Gtk.ProgressBar()
        header.append(self.progress)
        self.append(header)
        speech = Adw.PreferencesPage()
        speech.add(self.speech)
        self.key_entry = self.speech.api_key_entry
        rewriting = Adw.PreferencesPage()
        rewriting.add(self.rewrite)
        example = Adw.PreferencesGroup()
        self.polish_preview = PolishPreview()
        example.add(self.polish_preview)
        rewriting.add(example)
        appearance = Gtk.ScrolledWindow(vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER)
        appearance.set_margin_start(20)
        appearance.set_margin_end(20)
        appearance.set_child(self.appearance)
        for name, page in (("speech", speech), ("rewrite", rewriting), ("appearance", appearance)):
            self.steps.add_named(page, name)
        self.append(self.steps)
        footer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        set_margins(footer, 20)
        self.status = Gtk.Label(wrap=True, xalign=0)
        footer.append(self.status)
        buttons = Gtk.Box(spacing=12)
        self.back = Gtk.Button(label="Back")
        self.back.connect("clicked", self._back)
        buttons.append(self.back)
        buttons.append(Gtk.Label(hexpand=True))
        self.next = Gtk.Button(label="Continue", css_classes=["suggested-action"])
        self.next.connect("clicked", self._next)
        buttons.append(self.next)
        footer.append(buttons)
        self.append(footer)
        self.connect("map", self._mapped)
        self.connect("unmap", self._unmapped)
        self.timer = 0
        self._refresh()

    def refresh_config(self, config):
        """Reopen setup with current choices instead of stale first-launch drafts."""
        self.config = config
        self.speech.refresh_config(config)
        self.rewrite.refresh_config(config)
        self.appearance.refresh_config(config)
        self.step = 0
        self._show_step()

    def _mapped(self, *_args):
        if not self.timer:
            self.timer = GLib.timeout_add(200, self._refresh)

    def _unmapped(self, *_args):
        if self.timer:
            GLib.source_remove(self.timer)
            self.timer = 0
        self.speech.local.stop()

    def _refresh(self):
        self.step_label.set_label(f"Step {self.step + 1} of 3 · " + ("Speech", "Polishing", "Recorder")[self.step])
        self.progress.set_fraction((self.step + 1) / 3)
        self.polish_preview.set_visible(self.rewrite.provider.id == "codex")
        self.back.set_visible(self.step > 0)
        self.next.set_label("Open Mluva" if self.step == 2 else "Continue")
        self.next.set_sensitive(not self.saving_key and (self.step != 0 or self.speech.is_ready()))
        return GLib.SOURCE_CONTINUE

    def _save_key(self, *_args):
        key = self.key_entry.get_text().strip()
        self.key_entry.set_text("")
        self.saving_key = True
        self.status.set_label("Saving key to your desktop keyring…")
        self._refresh()

        def work():
            success = False
            try:
                store_speech_key(key)
                success = True
                message = "Key saved. You can continue."
            except (ValueError, RuntimeError) as error:
                message = str(error)
            GLib.idle_add(self._key_saved, message, success)

        threading.Thread(target=work, daemon=True, name="save-speech-key").start()

    def _key_saved(self, message, success):
        self.saving_key = False
        self.status.set_label(message)
        if success and self.advance_after_key:
            self.step = 1
            self._show_step()
        self.advance_after_key = False
        self._refresh()
        return GLib.SOURCE_REMOVE

    def _back(self, *_args):
        self.step = max(0, self.step - 1)
        self._show_step()

    def _show_step(self):
        self.steps.set_visible_child_name(("speech", "rewrite", "appearance")[self.step])
        self.title.set_label(
            ("Choose speech recognition", "Optional: polish your words", "Make Mluva yours")[self.step]
        )
        self.status.set_label("")
        self._refresh()

    def _next(self, *_args):
        if self.saving_key or not self.speech.is_ready():
            return
        if self.step == 0 and self.speech.provider.id == "elevenlabs":
            if self.key_entry.get_text().strip():
                self.advance_after_key = True
                self._save_key()
                return
            try:
                elevenlabs_api_key()
            except RuntimeError:
                self.status.set_label("Paste your ElevenLabs key above, or choose a local model.")
                return
        changes = self.speech.values | self.rewrite.values | self.appearance.values()
        if changes["rewrite_provider"] == "none":
            changes.update(live_rewrite_enabled=False, automatic_titles=False)
        try:
            replace(self.config, **changes)
            section = self.speech if self.step == 0 else self.rewrite
            if section.provider.id == "litellm" and not section.values[section.provider.model_field]:
                raise ValueError("Enter the compatible endpoint's model ID before continuing.")
        except (ValueError, TypeError) as error:
            self.status.set_label(str(error))
            return
        if self.step < 2:
            self.step += 1
            self._show_step()
        elif self.save(changes):
            self.finish()
        else:
            self.status.set_label("Could not save settings. Your choices are kept; please try again.")
