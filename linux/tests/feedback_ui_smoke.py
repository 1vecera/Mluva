"""Verify onboarding, full-window settings, pane controls and visible text revisions offscreen."""

import json
import os
import sys
import traceback
from dataclasses import replace
from pathlib import Path

from conversation_ui_smoke import IsolatedApplication
from gi.repository import Gdk, GLib, Gtk, Pango
from live_workspace_smoke import paint, settle

from mluva_linux.config import load_config


class FirstRunApplication(IsolatedApplication):
    """Keep real onboarding while replacing only external device and provider boundaries."""

    def _initialize_local_services(self) -> None:
        super()._initialize_local_services()
        self.config = replace(self.config, welcome_completed=False)


def main() -> None:
    """Exercise the production views at one declared viewport in a fresh isolated session."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    app = FirstRunApplication()
    errors = []
    checks = []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise() -> bool:
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            app.window.set_default_size(width, height)
            app.window.set_size_request(width, height)
            settle(lambda: app.welcome_view.get_mapped())
            assert app.page_stack.get_visible_child_name() == "welcome"
            assert app.window.get_visible_dialog() is None
            assert app.welcome_view.providers.speech.provider.id == "elevenlabs"
            assert app.welcome_view.providers.rewrite.provider.id == "codex"
            assert not app.config.welcome_completed
            paint(app.window, output / "welcome.png")
            app._finish_welcome()
            assert load_config(app.config_path).welcome_completed
            assert app.page_stack.get_visible_child_name() == "capture"
            checks.append("First-run provider setup and explicit completion persist")

            app._show_settings(None)
            app.settings_view.set_visible_page_name("providers")
            settle(app.settings_view.get_mapped)
            assert app.window.get_visible_dialog() is None
            success, bounds = app.settings_view.compute_bounds(app.window)
            assert success and bounds.get_width() >= app.window.get_width() - 4
            paint(app.window, output / "settings-providers.png")
            assert app._key_pressed(None, Gdk.KEY_Escape, 0, Gdk.ModifierType(0))
            assert app.page_stack.get_visible_child_name() == "capture"
            checks.append("Settings occupies the viewport and Escape returns to the workspace")

            workspace = app.conversation_workspace
            source = (
                "We need a calm dictation app that keeps every word and makes room before the next line arrives. " * 18
            )
            workspace.set_live("00:12", source)
            workspace.show_live_draft("## Task\nBuild a calm dictation app.\n\n## Next step\nReview the recorder.")
            settle(lambda: workspace.live_draft_text.get_mapped() and workspace.live_text.get_mapped())
            assert workspace.live_divider.get_visible()
            font = workspace.live_text.get_pango_context().load_font(
                Pango.FontDescription.from_string("JetBrains Mono 14")
            )
            assert font.describe().get_family() == "JetBrains Mono", font.describe().to_string()
            paint(app.window, output / "workspace-split.png")
            draft = workspace.live_draft()
            control = Gdk.ModifierType.CONTROL_MASK
            assert app._command_key_pressed(None, Gdk.KEY_1, 0, control)
            assert not workspace.live_source_box.get_visible() and workspace.live_draft_box.get_visible()
            assert app._command_key_pressed(None, Gdk.KEY_2, 0, control)
            assert workspace.live_source_box.get_visible() and not workspace.live_draft_box.get_visible()
            workspace.pane_buttons["draft"].emit("clicked")
            assert workspace.live_source_box.get_visible() and workspace.live_draft_box.get_visible()
            assert workspace.live_draft() == draft and workspace.live_text.get_text() == source
            checks.append("Either pane collapses by shortcut/button without losing text or hiding both")

            Gtk.Settings.get_default().set_property("gtk-enable-animations", True)
            original = "Move it to Tuesday. Keep this middle sentence. Thanks, Alex."
            changed = "Move it to Wednesday. Keep this middle sentence. Cheers, Alex."
            workspace.show_live_draft(original)
            settle(lambda: workspace.live_draft_text._revision_tick == 0)
            workspace.show_live_draft(changed)
            assert workspace.live_draft_text.get_text() == changed
            assert workspace.live_draft_text._revision_tick
            settle(lambda: workspace.live_draft_text._revision_tick == 0)
            assert not workspace.live_draft_text._removed_runs
            workspace.show_live_draft(original)
            buffer = workspace.live_draft_text.get_buffer()
            buffer.insert(buffer.get_end_iter(), " My edit.")
            assert workspace.live_draft_text._revision_tick == 0
            assert workspace.live_draft_text.get_text() == original + " My edit."
            checks.append("Diff animation keeps exact source immediately and cancels on manual edits")
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            workspace.set_live("00:40", source * 8)
            adjustment = workspace.live_scroll.get_vadjustment()
            settle(lambda: adjustment.get_value() > 100 and not workspace.live_follower.pending)
            position = adjustment.get_value()
            workspace.set_live("00:41", "A corrected sentence that is much shorter than the previous recognition.")
            settle(lambda: not workspace.live_follower.pending)
            end = workspace.live_text.get_iter_location(workspace.live_text.get_buffer().get_end_iter())
            visible = workspace.live_text.get_visible_rect()
            assert adjustment.get_value() >= position - 1, (position, adjustment.get_value())
            assert visible.y <= end.y < visible.y + visible.height, (end.y, visible.y, visible.height)
            checks.append("Large recognition contractions retain the tail in view without reverse scrolling")
            workspace.live_follower._reader_scrolled(None, 0, -1)
            settle(lambda: workspace.live_text.get_top_margin() == workspace.live_follower.base_top)
            assert workspace.live_follower.revision_inset == 0 and not workspace.live_follower.following
            assert adjustment.get_value() < adjustment.get_page_size(), adjustment.get_value()
            checks.append("Manual reading releases correction padding without exposing an empty historical extent")
            (output / "feedback-ui.json").write_text(
                json.dumps({"viewport": [app.window.get_width(), app.window.get_height()], "checks": checks}, indent=2)
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(300, exercise)

    activation = app.connect("activate", activated)
    app.run([])
    sys.excepthook = sys.__excepthook__
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
