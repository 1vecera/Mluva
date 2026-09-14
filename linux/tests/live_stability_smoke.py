"""Observe streaming words, pane startup and correction geometry with native animations enabled."""

import json
import os
import traceback
from dataclasses import replace
from pathlib import Path

from conversation_ui_smoke import IsolatedApplication
from gi.repository import GLib, Gtk
from live_layout_smoke import bounds, frames
from live_workspace_smoke import paint

from mluva_linux.live_rewrite import initial_draft


def main() -> None:
    """Exercise real GTK layout under rapid partial updates on a private display."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    app = IsolatedApplication()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    errors = []

    def exercise() -> bool:
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", True)
            app.config = replace(app.config, live_rewrite_enabled=True, scroll_duration_ms=350)
            app.window.set_default_size(1060, 780)
            app._navigate_to_page("capture")
            workspace = app.conversation_workspace
            workspace.set_config(app.config)
            app.pending_session_identifier = "stability-first"
            app.pending_mode = "dictation"
            app.pending_incognito = False
            app._start_live_rewrite()
            workspace.set_live("00:00", "")
            frames()
            assert workspace.live_draft_available and workspace.live_draft_box.get_mapped()
            panes = [bounds(pane, app.window) for pane in (workspace.live_source_box, workspace.live_draft_box)]
            adjustment = workspace.live_scroll.get_vadjustment()
            assert adjustment.get_value() == 0
            source = "The first words start at the top."
            workspace.set_live("00:01", source)
            frames()
            assert workspace.live_text.get_top_margin() == 4 and adjustment.get_value() == 0
            assert workspace.live_draft_box.get_mapped()
            paint(app.window, output / "live-before-response.png")

            positions = []
            for index in range(48):
                source += " Useful words fill each line naturally."
                workspace.set_live(f"00:{index + 2:02}", source)
                positions.extend(frames(0.035, adjustment))
                assert workspace.live_text.get_text() == source
            positions.extend(frames(0.6, adjustment))
            assert adjustment.get_value() > 100
            reverse = max((before - after for before, after in zip(positions, positions[1:], strict=False)), default=0)
            assert reverse <= 1, reverse
            stable = (adjustment.get_upper(), adjustment.get_value(), workspace.live_text.get_top_margin())
            for _index in range(100):
                workspace.set_live("00:51", source)
            frames()
            after = (adjustment.get_upper(), adjustment.get_value(), workspace.live_text.get_top_margin())
            assert all(abs(a - b) <= 1 for a, b in zip(stable, after, strict=True)), (stable, after)
            paint(app.window, output / "live-stream-overflow.png")

            workspace.show_live_draft("## Task\nKeep the source exact.\n\n## Next step\nReview on Tuesday.")
            frames()
            assert panes == [bounds(pane, app.window) for pane in (workspace.live_source_box, workspace.live_draft_box)]
            for ending in ("Wednesday.", "Wednesday morning.", "Thursday."):
                rewritten = "## Task\nKeep the source exact.\n\n## Next step\nReview on " + ending
                workspace.show_live_draft(rewritten)
                assert workspace.live_draft() == rewritten
                buffer = workspace.live_draft_text.get_buffer()
                cursor = buffer.get_start_iter()
                while not cursor.is_end():
                    for tag in cursor.get_tags():
                        if tag.get_property("foreground-set"):
                            assert tag.get_property("foreground-rgba").alpha == 1
                    cursor.forward_char()
                frames(0.04)
            paint(app.window, output / "live-correction.png")

            workspace.set_live("00:52", "A much shorter corrected recognition.")
            frames(0.6)
            end = workspace.live_text.get_iter_location(workspace.live_text.get_buffer().get_end_iter())
            visible = workspace.live_text.get_visible_rect()
            assert visible.y <= end.y < visible.y + visible.height, (end.y, visible.y, visible.height)
            workspace.finish_live()
            app.pending_session_identifier = "stability-second"
            app._start_live_rewrite()
            workspace.set_live("00:00", "A fresh recording begins here.")
            frames(0.6)
            assert workspace.live_text.get_top_margin() == 4
            assert adjustment.get_value() == 0
            assert workspace.live_draft_box.get_mapped()
            paint(app.window, output / "live-second-recording.png")

            workspace.show_live_draft("A draft that belongs only to the previous capture.")
            workspace.finish_live()
            app.config = replace(app.config, live_rewrite_enabled=False)
            app.pending_session_identifier = "stability-start-without-live"
            app._start_live_rewrite()
            workspace.set_live("00:00", "An unrelated new recording.")
            frames()
            assert not workspace.live_draft_available and not workspace.live_draft_box.get_mapped()
            fresh_draft = initial_draft(app.config, app.live_prompts)
            app.config = replace(app.config, live_rewrite_enabled=True)
            app._start_live_rewrite(preserve_draft=True)
            frames()
            assert workspace.live_draft_box.get_mapped()
            assert workspace.live_draft() == fresh_draft
            workspace.show_live_draft("A deliberate edit in the current capture.")
            app._start_live_rewrite(preserve_draft=True)
            assert workspace.live_draft() == "A deliberate edit in the current capture."
            (output / "live-stability.json").write_text(
                json.dumps(
                    {
                        "right_pane_before_first_response": True,
                        "first_and_second_capture_top_margin": workspace.live_text.get_top_margin(),
                        "maximum_reverse_scroll_during_growth": reverse,
                        "unchanged_updates_do_not_grow_extent": True,
                        "draft_revisions_opaque_immediately": True,
                        "pane_geometry_stable": panes,
                        "late_live_enable_starts_with_current_capture_draft": True,
                        "same_capture_restart_preserves_edits": True,
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app._cancel_live_rewrite()
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(300, exercise)

    activation = app.connect("activate", activated)
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))


if __name__ == "__main__":
    main()
