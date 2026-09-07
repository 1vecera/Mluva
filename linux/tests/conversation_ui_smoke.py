"""Render and exercise the production workspace only inside the isolated X11 runner."""

import json
import os
import subprocess
import traceback
from pathlib import Path
from unittest.mock import patch

import gi
from conversation_lifecycle import exercise, exercise_widget_review

from voice_scribe_linux.app import MluvaApplication
from voice_scribe_linux.conversation import STRUCTURED_NOTE
from voice_scribe_linux.pipewire import PipeWireDeviceCatalog
from voice_scribe_linux.ui import set_button_content

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("GdkX11", "4.0")
from gi.repository import Adw, GLib, Gtk  # noqa: E402


class IsolatedApplication(MluvaApplication):
    """Replace device, provider and desktop-target boundaries while retaining production UI construction."""

    def _initialize_capture_services(self) -> None:
        """Leave real microphone, portal and network transports unstarted for this visual fixture."""
        self.approved_recording_trigger = "F9"
        self._set_status("Copied—ready to paste.")


def main() -> int:
    """Exercise real history, GTK text views, navigation, privacy and background-window behavior."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use the isolated X11 verification runner.")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    scenario = os.environ.get("MLUVA_UI_SCENARIO", "conversation")
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    theme = os.environ.get("MLUVA_UI_THEME")
    if theme in {"tokyo-night", "rose-pine"}:
        theme_path = Path(os.environ["XDG_STATE_HOME"]) / "omarchy/current/theme"
        theme_path.mkdir(parents=True)
        theme_path.joinpath("colors.toml").write_text(
            Path(f"/usr/share/omarchy/themes/{theme}/colors.toml").read_text()
        )
    application = IsolatedApplication()
    errors: list[str] = []
    source = (
        "Let's keep the first release focused on dictation, rewriting and history. "
        "When I press F9, I want to see that the microphone is listening, then know when the text is copied.\n\n"
        "Quick Polish should keep my voice and remove the filler words. Structured Note should put the main point "
        "first, then organize the details into bullets. The original needs to stay here so I can always go back."
    )
    if scenario in {"long-note", "long-live"}:
        source = (source + "\n\n") * 40 + "LATEST WORDS: Žluťoučký kůň — this ending must be visible."

    def prepare() -> bool:
        """Seed synthetic content through the production stores and select the requested state."""
        try:
            application.window.set_default_size(width, height)
            application.window.set_size_request(width, height)
            workspace = application.conversation_workspace
            assert workspace is not None
            if scenario == "lifecycle":
                exercise(application)
                exercise_widget_review(application)
            for text in (
                "A few ideas for Friday's meeting",
                "Notes from the morning walk",
                "Follow up with the design team",
            ):
                application.history_store.add(text, text, "dictation", "eng", None, "copied")
            entry = application.history_store.add(source, source, "dictation", "eng", None, "copied")
            application.history_store.update_title(entry.identifier, "A simpler dictation workflow")
            entry = application.history_store.find(entry.identifier)
            application.conversation_store.append(
                entry.identifier,
                STRUCTURED_NOTE,
                "Keep the first release focused on dictation, rewriting and history.\n\n"
                "• Show recording status when F9 starts the microphone.\n"
                "• Confirm when the finished text is copied.\n"
                "• Offer Quick Polish for faithful cleanup.\n"
                "• Offer Structured Note for a summary and organized details.\n"
                "• Preserve the original so it is always recoverable.",
                "fixture-model",
            )
            workspace.refresh_history()
            workspace.show_conversation(entry, application.conversation_store.replies(entry.identifier))
            if scenario == "long-note":
                workspace.set_live("Recording", source)
                workspace.finish_live()
                workspace.show_conversation(entry, [])
            if scenario == "empty":
                workspace.show_conversation(None, [])
            elif scenario in {"recording", "long-live"}:
                workspace.set_live("Recording  01:13", source if scenario == "long-live" else source * 3)
                application.capture_status_title.set_label("Recording")
                application.status_label.set_label("Listening. Press F9 when you're done.")
                set_button_content(application.record_button, "media-playback-stop-symbolic", "Stop")
            elif scenario == "processing":
                workspace.set_live("Processing…", source)
                application.capture_status_title.set_label("Processing…")
                application.status_label.set_label("Finishing your dictation.")
                application.record_button.set_sensitive(False)
            elif scenario == "rewriting":
                workspace.set_busy(True, "Rewriting…")
                workspace.set_rewrite_preview(
                    entry.identifier,
                    "Focus the first release on dictation, rewriting and history.\n\n"
                    "• Keep the latest words visible.\n• Offer direct rewrites",
                )
            elif scenario == "error":
                workspace.set_busy(False, "Rewrite failed. Check Codex, then try again. Your original is safe.")
            elif scenario == "incognito":
                workspace.set_private(True)
                workspace.show_transient(source, source)
            elif scenario == "dark":
                Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
            if scenario not in {"empty", "incognito"}:
                assert workspace.result_widgets[0].get_text() == source
            GLib.timeout_add(900, capture)
        except Exception:
            errors.append(traceback.format_exc())
            application.quit()
        return GLib.SOURCE_REMOVE

    def capture() -> bool:
        """Retain settled pixels, then assert navigation and close-to-background behavior."""
        try:
            window = application.window

            assert window.get_content().measure(Gtk.Orientation.HORIZONTAL, -1)[0] <= width - 10
            assert window.get_content().measure(Gtk.Orientation.VERTICAL, width - 10)[0] <= height - 10
            assert window.get_surface().get_width() == width, (window.get_surface().get_width(), width)
            assert window.get_surface().get_height() == height, (window.get_surface().get_height(), height)
            subprocess.run(
                ["import", "-window", str(window.get_surface().get_xid()), str(output / "workspace.png")], check=True
            )
            workspace = application.conversation_workspace
            if scenario in {"long-note", "long-live"}:
                scroll = workspace.live_scroll if scenario == "long-live" else workspace.scroll
                adjustment = scroll.get_vadjustment()
                assert adjustment.get_upper() > adjustment.get_page_size()
                assert abs(adjustment.get_value() + adjustment.get_page_size() - adjustment.get_upper()) <= 1, (
                    adjustment.get_value(),
                    adjustment.get_page_size(),
                    adjustment.get_upper(),
                )
                assert workspace.composer.get_visible() == (scenario == "long-note")
                assert workspace.quick_polish.is_sensitive()
                if scenario == "long-note":
                    label = workspace.result_widgets[-1]
                    success, bounds = label.compute_bounds(scroll)
                    assert success and bounds.get_y() < 0, (
                        success,
                        bounds.get_y(),
                        bounds.get_height(),
                        scroll.get_height(),
                        adjustment.get_value(),
                        adjustment.get_upper(),
                        label.get_text()[-70:],
                    )
                    assert bounds.get_y() + bounds.get_height() <= scroll.get_height()
                else:
                    view = workspace.live_text
                    ending = view.get_iter_location(view.get_buffer().get_end_iter())
                    visible = view.get_visible_rect()
                    assert visible.y <= ending.y < visible.y + visible.height
            assert workspace.quick_polish.get_label() == "Polish"
            assert workspace.structured_note.get_label() == "Structure"
            application._hide_window(window)
            assert not window.get_visible()
            application._open_latest_conversation()
            assert window.get_visible()
            assert workspace.entry.raw_text == source
            assert len(application.conversation_store.replies(workspace.entry.identifier)) == 1
            (output / "receipt.json").write_text(
                json.dumps(
                    {
                        "scenario": scenario,
                        "width": width,
                        "height": height,
                        "full_source": True,
                        "reopened_latest": True,
                        "close_keeps_window": True,
                    }
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        application.quit()
        return GLib.SOURCE_REMOVE

    with (
        patch("voice_scribe_linux.app.FocusedTextTargetTracker", return_value=None),
        patch.object(
            PipeWireDeviceCatalog,
            "from_system",
            return_value=PipeWireDeviceCatalog(),
        ),
    ):
        GLib.idle_add(prepare)
        application.run(None)
    if errors:
        raise RuntimeError("; ".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
