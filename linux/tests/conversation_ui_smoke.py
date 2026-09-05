"""Render and exercise the production workspace only inside the isolated X11 runner."""

import json
import os
import subprocess
import traceback
from pathlib import Path
from unittest.mock import patch

import gi
from conversation_lifecycle import exercise

from voice_scribe_linux.app import MluvaApplication
from voice_scribe_linux.conversation import STRUCTURED_NOTE
from voice_scribe_linux.pipewire import PipeWireDeviceCatalog

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
    application = IsolatedApplication()
    errors: list[str] = []
    source = (
        "Let's keep the first release focused on dictation, rewriting and history. "
        "When I press F9, I want to see that the microphone is listening, then know when the text is copied.\n\n"
        "Quick Polish should keep my voice and remove the filler words. Structured Note should put the main point "
        "first, then organize the details into bullets. The original needs to stay here so I can always go back."
    )

    def prepare() -> bool:
        """Seed synthetic content through the production stores and select the requested state."""
        try:
            application.window.set_default_size(width, height)
            workspace = application.conversation_workspace
            assert workspace is not None
            if scenario == "lifecycle":
                exercise(application)
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
            if scenario == "empty":
                workspace.show_conversation(None, [])
            elif scenario == "recording":
                workspace.set_live("Recording  01:13", source * 3)
            elif scenario == "processing":
                workspace.set_live("Processing…", source)
            elif scenario == "rewriting":
                workspace.set_busy(True, "Rewriting… You can keep browsing history.")
            elif scenario == "error":
                workspace.set_busy(False, "Rewrite failed. Check Codex, then try again. Your original is safe.")
            elif scenario == "incognito":
                workspace.set_private(True)
                workspace.show_transient(source, source)
            elif scenario == "dark":
                Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
            if scenario not in {"empty", "incognito"}:
                buffer = workspace.result_widgets[0].get_buffer()
                assert buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False) == source
            GLib.timeout_add(900, capture)
        except Exception:
            errors.append(traceback.format_exc())
            application.quit()
        return GLib.SOURCE_REMOVE

    def capture() -> bool:
        """Retain settled pixels, then assert navigation and close-to-background behavior."""
        try:
            window = application.window

            def inspect_size(widget: Gtk.Widget) -> None:
                """Reject clipping that GTK's application-window allocation otherwise allows."""
                if widget.get_visible() and widget.measure(Gtk.Orientation.HORIZONTAL, -1)[0] > width - 10:
                    print(f"MINIMUM_WIDTH {type(widget).__name__} {widget.measure(Gtk.Orientation.HORIZONTAL, -1)[0]}")
                child = widget.get_first_child()
                while child is not None:
                    inspect_size(child)
                    child = child.get_next_sibling()

            inspect_size(window.get_content())
            assert window.get_surface().get_width() == width, (window.get_surface().get_width(), width)
            assert window.get_surface().get_height() == height, (window.get_surface().get_height(), height)
            subprocess.run(
                ["import", "-window", str(window.get_surface().get_xid()), str(output / "workspace.png")], check=True
            )
            workspace = application.conversation_workspace
            assert workspace.quick_polish.get_label() == "Quick Polish"
            assert workspace.structured_note.get_label() == "Structured Note"
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
