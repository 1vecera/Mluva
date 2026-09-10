"""Verify the compact Live rewrite control and persisted settings in the actual private GTK app."""

import json
import os
import traceback
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication, render_widget
from gi.repository import GLib
from live_workspace_smoke import paint

from mluva_linux.config import load_config
from mluva_linux.live_rewrite import TEMPLATE_CHOICES


def main():
    """Check bidirectional settings, active-capture rejection and the requested viewport."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("DISPLAY") != ":192":
        raise RuntimeError("Use the isolated X11 runner on :192")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    app = IsolatedApplication()
    errors = []

    def exercise():
        """Use the production control, form and config file with provider construction disabled."""
        try:
            app.window.set_default_size(width, height)
            app.window.set_size_request(width, height)
            with (
                patch("mluva_linux.app.transcription_client", return_value=None),
                patch.object(app, "_show_toast"),
            ):
                app.live_mode_switch.set_active(True)
                for template, _label in TEMPLATE_CHOICES:
                    app.live_template_buttons[template].set_active(True)
                    assert app.config.live_rewrite_template == template
                    assert load_config(app.config_path).live_rewrite_template == template
                    assert sum(button.get_active() for button in app.live_template_buttons.values()) == 1
                app._show_settings(None)
                page = app.workspace_settings_pages[0]
                assert page.fields["live_rewrite_template"]() == "custom"
                assert page.fields["live_rewrite_enabled"]()
                page.setters["live_rewrite_template"]("structured-note")
                page.apply(None)
                assert app.live_template_buttons["structured-note"].get_active()
                assert app.config.live_rewrite_template == "structured-note"
                app.settings_dialog.close()
                app.capture_preparing = True
                app.live_template_buttons["polish"].set_active(True)
                app.live_mode_switch.set_active(False)
                assert app.live_template_buttons["structured-note"].get_active()
                assert app.live_mode_switch.get_active()
                app.capture_preparing = False
                with patch("mluva_linux.app.save_config", side_effect=OSError("Synthetic write failure")):
                    app.live_template_buttons["polish"].set_active(True)
                    assert app.live_template_buttons["structured-note"].get_active()
                assert load_config(app.config_path).live_rewrite_template == "structured-note"
            app._set_status("Ready to dictate")
            paint(app.window, output / "live-control.png")
            surface = app.window.get_surface()
            assert surface.get_width() == width and surface.get_height() == height
            control = app.live_mode_switch.get_parent()
            assert control.get_first_child() is app.live_mode_switch
            assert app.live_mode_switch.get_next_sibling() is app.live_mode_menu
            assert app.live_mode_menu.get_next_sibling() is None
            assert control.get_height() <= 40
            for widget in (app.live_mode_switch, app.live_mode_menu):
                success, bounds = widget.compute_bounds(app.window)
                assert success and bounds.get_x() >= 0 and bounds.get_x() + bounds.get_width() <= width
            app.live_mode_menu.popup()
            paint(app.window, output / "live-control-menu-open.png")
            render_widget(app.live_mode_menu.get_popover()).save_to_png(str(output / "template-menu.png"))
            (output / "live-mode.json").write_text(
                json.dumps(
                    {
                        "display": os.environ["DISPLAY"],
                        "viewport": [width, height],
                        "four_modes_persisted": True,
                        "settings_roundtrip": True,
                        "active_capture_rejected": True,
                        "save_failure_rolled_back": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    def activate_once(_app):
        """Allow Settings to reactivate the window without repeating the fixture."""
        app.disconnect(activation)
        GLib.timeout_add(500, exercise)

    activation = app.connect("activate", activate_once)
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
