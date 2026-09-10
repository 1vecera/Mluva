"""Render and exercise the production workspace only inside the isolated X11 runner."""

import json
import os
import sqlite3
import struct
import subprocess
import time
import traceback
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from pathlib import Path
from unittest.mock import patch

import gi
from conversation_lifecycle import exercise, exercise_widget_review
from rewrite_settings_lifecycle import exercise_rewrite_settings, seed_rewrite_models
from scroll_lifecycle import exercise_scrolling
from title_lifecycle import exercise_titles

from mluva_linux.app import MluvaApplication
from mluva_linux.conversation import STRUCTURED_NOTE
from mluva_linux.pipewire import PipeWireDeviceCatalog
from mluva_linux.ui import set_button_content

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("GdkX11", "4.0")
gi.require_version("Gsk", "4.0")
gi.require_version("Graphene", "1.0")
from gi.repository import Adw, Gdk, GLib, Graphene, Gsk, Gtk  # noqa: E402, F401


def render_widget(widget: Gtk.Widget) -> Gdk.Texture:
    """Keep the native render bounds and alpha, including for a separate popover surface."""
    snapshot = Gtk.Snapshot()
    width, height = widget.get_width(), widget.get_height()
    Gtk.WidgetPaintable.new(widget).snapshot(snapshot, width, height)
    viewport = Graphene.Rect().init(0, 0, width, height)
    return widget.get_native().get_renderer().render_texture(snapshot.to_node(), viewport)


def capture_alpha(window: Gtk.Window, workspace, output: Path) -> None:
    """Inspect the production render's alpha without needing a live desktop compositor."""
    texture = render_widget(window)
    texture.save_to_png(str(output / "workspace-alpha.png"))
    downloader = Gdk.TextureDownloader.new(texture)
    downloader.set_format(Gdk.MemoryFormat.R8G8B8A8)
    data, stride = downloader.download_bytes()
    pixels = data.get_data()
    samples = {}
    for name, widget in (("conversation", workspace.content), ("history", workspace.split.get_sidebar())):
        if name == "history" and workspace.split.get_collapsed():
            continue
        success, bounds = widget.compute_bounds(window)
        assert success
        x = round(bounds.get_x() + 4)
        y = round(bounds.get_y() + bounds.get_height() / 2)
        samples[name] = pixels[y * stride + x * 4 + 3]
    (output / "transparency.json").write_text(json.dumps(samples))
    assert all(180 <= alpha <= 230 for alpha in samples.values()), samples


class IsolatedApplication(MluvaApplication):
    """Replace device, provider and desktop-target boundaries while retaining production UI construction."""

    def _initialize_local_services(self) -> None:
        """Keep optional automatic title requests off the real authenticated provider."""
        super()._initialize_local_services()
        self.config = replace(self.config, automatic_titles=False, auto_copy_rewrite=False)

    def _initialize_capture_services(self) -> None:
        """Leave real microphone, portal and network transports unstarted for this visual fixture."""
        self.approved_recording_trigger = "F9"
        self._set_status("Copied—ready to paste.")


def main() -> int:
    """Exercise real history, GTK text views, navigation, privacy and background-window behavior."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use the isolated X11 verification runner.")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    os.environ["TZ"] = "UTC"
    time.tzset()
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
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            application.window.set_default_size(width, height)
            application.window.set_size_request(width, height)
            workspace = application.conversation_workspace
            assert workspace is not None
            if scenario == "lifecycle":
                exercise(application)
                exercise_widget_review(application)
                exercise_titles(application)
            elif scenario == "rewrite-settings-lifecycle":
                exercise_rewrite_settings(application)
            elif scenario == "titles":
                exercise_titles(application)
            elif scenario == "scroll-motion":
                exercise_scrolling(application, output)
            application._navigate_to_page("capture")
            for text in (
                "A few ideas for Friday's meeting",
                "Notes from the morning walk",
                "Follow up with the design team",
            ):
                application.history_store.add(text, text, "dictation", "eng", None, "copied")
            if scenario.startswith("scrollbars"):
                for index in range(40):
                    text = f"Synthetic conversation {index + 1}"
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
            with sqlite3.connect(application.history_store.path) as connection:
                for index, item in enumerate(application.history_store.recent()):
                    stamp = datetime(2026, 9, 7, 9, 30, tzinfo=UTC) - timedelta(hours=index)
                    connection.execute(
                        "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
                        (stamp.isoformat(), item.identifier),
                    )
            entry = application.history_store.find(entry.identifier)
            workspace.refresh_history()
            workspace.show_conversation(entry, application.conversation_store.replies(entry.identifier))
            if scenario == "long-note":
                workspace.set_live("Recording", source)
                workspace.finish_live()
                workspace.show_conversation(entry, [])
            if scenario == "empty":
                workspace.show_conversation(None, [])
            elif scenario in {"recording", "long-live"}:
                workspace.set_live("01:13", source if scenario == "long-live" else source * 3)
                application.capture_status_title.set_label("Recording")
                application.status_label.set_label("Listening. Press F9 when you're done.")
                set_button_content(application.record_button, "media-playback-stop-symbolic", "Stop")
            elif scenario == "processing":
                workspace.set_live("Processing…", source, recording=False)
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
            elif scenario == "settings":
                application._show_settings(application.settings_button)
            elif scenario == "menu":
                application.main_menu_button.popup()
            elif scenario == "prompts":
                workspace.saved_prompts.popup()
            elif scenario == "rewrite-models":
                seed_rewrite_models(application)
            elif scenario == "rewrite-models-error":
                application.rewrite_settings.set_models(None)
            if scenario.startswith("scrollbars"):
                # Force the production non-overlay track as seen on desktops with always-visible scrollbars.
                sidebar_scroll = workspace.history_list.get_parent()
                while not isinstance(sidebar_scroll, Gtk.ScrolledWindow):
                    sidebar_scroll = sidebar_scroll.get_parent()
                sidebar_scroll.set_overlay_scrolling(False)
                workspace.scroll.set_overlay_scrolling(False)
                if scenario == "scrollbars-dark":
                    Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
            if scenario not in {"empty", "incognito"}:
                assert workspace.result_widgets[0].get_text() == source
            frames = 0

            def settled_frame(_widget: Gtk.Widget, _clock: object) -> bool:
                """Capture after GTK has allocated and painted the requested adaptive layout."""
                nonlocal frames
                frames += 1
                if frames < 4:
                    return GLib.SOURCE_CONTINUE
                if scenario == "long-note" and frames < 60:
                    # The adjustment can reach the tail one frame before the viewport translates its child.
                    visible, bounds = workspace.result_widgets[-1].compute_bounds(workspace.scroll)
                    if not visible or bounds.get_y() + bounds.get_height() > workspace.scroll.get_height():
                        return GLib.SOURCE_CONTINUE
                if scenario.startswith("rewrite-models"):
                    if frames == 4:
                        # Open after allocation; startup resizing can dismiss an earlier popover.
                        with patch.object(application.rewrite_settings, "load_models"):
                            application.rewrite_settings.popup()
                    if frames < 8:
                        return GLib.SOURCE_CONTINUE
                GLib.idle_add(capture)
                return GLib.SOURCE_REMOVE

            application.window.add_tick_callback(settled_frame)
        except Exception:
            errors.append(traceback.format_exc())
            application.quit()
        return GLib.SOURCE_REMOVE

    def capture() -> bool:
        """Retain settled pixels, then assert navigation and close-to-background behavior."""
        try:
            window = application.window

            def oversized(widget: Gtk.Widget) -> list[dict[str, object]]:
                """Retain geometry diagnostics for any production widget forcing a wider window."""
                items = []
                minimum = widget.measure(Gtk.Orientation.HORIZONTAL, -1)[0]
                if minimum > width - 10:
                    items.append(
                        {
                            "type": type(widget).__name__,
                            "minimum": minimum,
                            "classes": widget.get_css_classes(),
                            "visible": widget.get_visible(),
                            "label": widget.get_label() if isinstance(widget, Gtk.Label) else "",
                        }
                    )
                child = widget.get_first_child()
                while child is not None:
                    items.extend(oversized(child))
                    child = child.get_next_sibling()
                return items

            (output / "layout.json").write_text(json.dumps(oversized(window.get_content()), indent=2))
            (output / "navigation.json").write_text(
                json.dumps(
                    {
                        "window_width": window.get_width(),
                        "content_width": window.get_content().get_width(),
                        "collapsed": application.conversation_workspace.split.get_collapsed(),
                        "sidebar": application.conversation_workspace.split.get_show_sidebar(),
                        "breakpoint": window.get_current_breakpoint() is not None,
                    },
                    indent=2,
                )
            )

            assert window.get_content().measure(Gtk.Orientation.HORIZONTAL, -1)[0] <= width - 10
            assert window.get_content().measure(Gtk.Orientation.VERTICAL, width - 10)[0] <= height - 10
            assert window.get_surface().get_width() == width, (window.get_surface().get_width(), width)
            assert window.get_surface().get_height() == height, (window.get_surface().get_height(), height)
            if width <= 600:
                assert application.conversation_workspace.split.get_collapsed()
                assert not application.conversation_workspace.split.get_show_sidebar()
                assert application.conversation_workspace.split.get_content().get_width() >= width - 30
            subprocess.run(
                ["import", "-window", str(window.get_surface().get_xid()), str(output / "workspace.png")], check=True
            )
            png_size = struct.unpack(">II", (output / "workspace.png").read_bytes()[16:24])
            scale = window.get_surface().get_scale_factor()
            assert png_size == (width * scale, height * scale), "Virtual screen clipped the scaled window"
            workspace = application.conversation_workspace
            if scenario in {"menu", "prompts", "rewrite-models", "rewrite-models-error"}:
                button = (
                    application.main_menu_button
                    if scenario == "menu"
                    else application.rewrite_settings
                    if scenario.startswith("rewrite-models")
                    else workspace.saved_prompts
                )
                render_widget(button.get_popover()).save_to_png(str(output / "menu.png"))
            if os.environ.get("MLUVA_UI_ALPHA_CHECK") == "1":
                capture_alpha(window, workspace, output)
            aligned = {}
            for name, widget in (
                ("heading", workspace.heading),
                ("live", workspace.live_box),
                ("messages", workspace.messages),
                ("composer", workspace.composer),
                ("recording", application.capture_action_bar),
            ):
                if widget.get_mapped():
                    success, bounds = widget.compute_bounds(window)
                    assert success
                    aligned[name] = bounds.get_x()
            assert "heading" in aligned or "live" in aligned, "The capture workspace must be mapped for layout checks"
            assert max(aligned.values()) - min(aligned.values()) <= 1, aligned
            if workspace.live_header.get_visible():
                assert application.header_bar.get_title_widget() is workspace.live_header
                assert workspace.live_header.get_mapped() and not workspace.heading.get_mapped()
                _, header_bounds = workspace.live_header.compute_bounds(window)
                _, live_bounds = workspace.live_box.compute_bounds(window)
                assert header_bounds.get_y() + header_bounds.get_height() <= live_bounds.get_y()
            elif not workspace.split.get_collapsed():
                _, sidebar_bounds = workspace.sidebar_heading.compute_bounds(window)
                _, heading_bounds = workspace.heading.compute_bounds(window)
                assert abs(sidebar_bounds.get_y() - heading_bounds.get_y()) <= 1
                (output / "alignment.json").write_text(json.dumps(aligned))
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
        patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
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
