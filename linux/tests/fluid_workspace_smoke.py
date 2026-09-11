"""Exercise live navigation, settings and real offline Mermaid in an isolated GTK session."""

import json
import os
import sys
import traceback
from dataclasses import replace
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication, render_widget
from gi.repository import Adw, GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.command_palette import application_commands
from mluva_linux.config import load_config
from mluva_linux.realtime import RealtimePreview
from mluva_linux.ui import set_button_content


def main():
    """Run only synthetic audio/provider boundaries and inspect native production widgets."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    app = IsolatedApplication()
    errors = []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise():
        try:
            workspace = app.conversation_workspace
            assert not workspace.split.get_show_sidebar()
            assert app.config.live_rewrite_template == "grilling"
            old = app.history_store.add("Saved conversation", "Saved conversation", "dictation", "eng", None, "ready")
            workspace.refresh_history()
            app.pending_session_identifier = "synthetic-session"
            app.pending_mode = "dictation"
            app.pending_incognito = False
            app.recorder = SimpleNamespace(process=object())
            snapshot = RealtimePreview("I want a support portal with an API and a queue.", "")
            app.realtime_session = SimpleNamespace(snapshot=lambda: snapshot)
            app._start_live_rewrite()
            workspace.set_live("00:12", snapshot.display_text)
            set_button_content(app.record_button, "media-playback-stop-symbolic", "Stop")
            app._set_status("Microphone ready · Scribe v2 realtime")
            assert not workspace.live_draft_box.get_visible()
            with patch.object(app, "_maybe_live_rewrite") as rewrite:
                app.live_mode_switch.set_active(True)
                assert app.live_schedule is not None
                rewrite.assert_called_once_with(snapshot.display_text)
                assert load_config(app.config_path).live_rewrite_enabled
                source = (
                    "## Questions\n- Who owns failed requests?\n\n## Architecture\n"
                    "### Intent\nA support portal for the team.\n\n"
                    "```mermaid\nflowchart LR\n Portal --> API\n API --> Queue\n```\n"
                )
                app.live_updating = True
                workspace.show_live_draft(source, "Grilling")
                app.live_updating = False
                assert workspace.live_draft() == source
                assert "failed requests" in workspace.live_questions.get_text()
                settle(lambda: bool(workspace.live_draft_text.diagram_ranges), timeout=20)
                assert workspace.live_diagrams.web.get_network_session().is_ephemeral()
                assert workspace.live_draft() == source
                paint(app.window, output / "grilling-desktop.png")
                render_widget(workspace.live_diagrams).save_to_png(str(output / "mermaid.png"))
                updated = source.replace("Who owns failed requests?", "How long should requests be retained?")
                updated = updated.replace(
                    "A support portal for the team.", "A support portal owned by the support team."
                )
                app.live_updating = True
                workspace.show_live_draft(updated, "Grilling")
                app.live_updating = False
                assert "Who owns" not in workspace.live_questions.get_text()
                assert "retained" in workspace.live_questions.get_text()
                assert workspace.live_draft() == updated
                settle(lambda: workspace.live_draft_text.diagram_source == workspace.live_draft_text.get_text())
                source = updated
                workspace.show_conversation(old, [])
                assert not workspace.viewing_live and workspace.live_active
                workspace.set_live("00:13", snapshot.display_text + " More speech.")
                assert workspace.entry.identifier == old.identifier and not workspace.viewing_live
                workspace.live_navigation.emit("clicked")
                assert workspace.viewing_live and workspace.live_draft() == source
                workspace.live_draft_text.get_buffer().insert_at_cursor("\nKeep my deliberate edit.")
                edited = workspace.live_draft()
                app.live_mode_switch.set_active(False)
                assert app.live_schedule.paused
                app.live_mode_switch.set_active(True)
                assert app.live_schedule is not None and workspace.live_draft() == edited
                app.live_template_buttons["task-spec"].set_active(True)
                assert app.live_config.live_rewrite_template == "task-spec" and workspace.live_draft() == edited
                assert not app._apply_workspace_settings({"rewrite_provider": "litellm"})
                app.live_template_buttons["grilling"].set_active(True)
            commands = application_commands(app)
            for text in ("Floating widget position", "History time format", "Microphone", "Keep audio", "Template"):
                assert any(text in command.title for command in commands), text
            next(command for command in commands if command.title == "Widget position: Lower right").run()
            assert load_config(app.config_path).widget_position == "bottom-right"
            next(command for command in commands if command.title == "Time format: 12-hour · 2:30 PM").run()
            assert app.config.time_format == "12h"
            assert workspace.history_list.get_selected_row() is None
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            next(command for command in commands if command.title == "Toggle history sidebar").run()
            assert workspace.split.get_show_sidebar()
            paint(app.window, output / "grilling-sidebar.png")
            style = Adw.StyleManager.get_default()
            style.set_color_scheme(Adw.ColorScheme.FORCE_DARK)
            settle(lambda: workspace.live_diagrams.rendered_dark is True and not workspace.live_diagrams.in_flight)
            paint(app.window, output / "grilling-dark.png")
            style.set_color_scheme(Adw.ColorScheme.DEFAULT)
            command = next(command for command in commands if "Floating widget position" in command.title)
            command.run()
            settle(lambda: app.settings_dialog.get_visible_page() is app.workspace_settings_pages[0])
            paint(app.window, output / "settings.png")
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            app.settings_dialog.close()
            settle(lambda: not app.settings_dialog.get_mapped())
            app.window.set_default_size(480, 640)
            app.window.set_size_request(420, 520)
            workspace.live_draft_follower.stop()
            workspace.live_draft_scroll.get_vadjustment().set_value(0)
            paint(app.window, output / "grilling-narrow.png")
            assert not workspace.split.get_show_sidebar()
            for widget in (app.record_button, app.live_mode_switch, workspace.live_questions):
                success, bounds = widget.compute_bounds(app.window)
                assert success and bounds.get_x() >= 0
                assert bounds.get_x() + bounds.get_width() <= app.window.get_width()
            exercise_diagrams(app)
            exercise_paused_finalization(app, old)
            (output / "workspace.json").write_text(
                json.dumps(
                    {
                        "display": os.environ["DISPLAY"],
                        "live_toggle_during_recording": True,
                        "manual_edits_preserved": True,
                        "history_does_not_steal_live_state": True,
                        "mermaid_rendered_offline": True,
                        "source_roundtrip": True,
                        "answered_questions_retire": True,
                        "paused_draft_saved_without_copy": True,
                        "late_paused_result_rejected": True,
                        "finalization_respects_selected_conversation": True,
                        "sequence_and_invalid_diagrams_checked": True,
                        "settings_searchable": len(
                            [command for command in commands if command.title.startswith("Settings ·")]
                        ),
                        "narrow_viewport": [app.window.get_width(), app.window.get_height()],
                    },
                    indent=2,
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app._cancel_live_rewrite()
            app.recorder = None
            app.realtime_session = None
            app.quit()
        return GLib.SOURCE_REMOVE

    def activated(_app):
        app.disconnect(activation)
        GLib.timeout_add(300, exercise)

    activation = app.connect("activate", activated)
    app.run([])
    if errors:
        raise RuntimeError("\n".join(errors))


def exercise_diagrams(app):
    """Exercise local rendering, source fallback and diagram edits through the native live editor."""
    workspace = app.conversation_workspace
    preview = workspace.live_diagrams
    requests = []
    preview.web.connect("resource-load-started", lambda _web, _resource, request: requests.append(request.get_uri()))
    sequence = "```mermaid\nsequenceDiagram\n Speaker->>Mluva: Dictate\n Mluva-->>Speaker: Questions\n```\n"
    workspace.show_live_draft(sequence)
    settle(
        lambda: (
            preview.rendered_codes == ("sequenceDiagram\n Speaker->>Mluva: Dictate\n Mluva-->>Speaker: Questions\n",)
        )
    )
    assert workspace.live_draft_text.diagram_ranges and workspace.live_draft() == sequence
    render_widget(preview).save_to_png(str(Path(os.environ["OFFSCREEN_ARTIFACT_DIR"]) / "sequence.png"))
    revision = preview.revision
    workspace.show_live_draft("A new paragraph.\n" + sequence)
    assert preview.revision == revision and workspace.live_draft_text.diagram_ranges
    for source in (
        "```mermaid\nthis is not a diagram\n```\n",
        '```mermaid\n%%{init:{"securityLevel":"loose"}}%%\nflowchart LR\n A-->B\n```\n',
        '```mermaid\nflowchart LR\n A["<img src=https://example.invalid/private>"]\n```\n',
    ):
        workspace.show_live_draft(source)
        settle(lambda source=source: preview.source == source and not preview.in_flight and not preview.pending)
        assert workspace.live_draft() == source
    assert not any(uri.startswith(("http:", "https:")) for uri in requests), requests
    unfinished = "```mermaid\nflowchart LR\n A -->"
    workspace.show_live_draft(unfinished)
    assert not preview.get_visible() and workspace.live_draft() == unfinished
    entry = app.history_store.add("Sketch source", "Sketch source", "dictation", "eng", None, "ready")
    reply = app.conversation_store.append(entry.identifier, "Sketch", sequence, "fixture")
    workspace.show_conversation(entry, [reply])
    saved_preview = workspace.messages.get_last_child().get_last_child()
    settle(lambda: bool(saved_preview.editor.diagram_ranges), timeout=20)
    assert saved_preview.editor.get_text() == sequence
    assert app.conversation_store.replies(entry.identifier)[0].text == sequence
    saved_preview._timed_out()
    settle(lambda: saved_preview.web is None)
    assert not saved_preview.editor.diagram_ranges and saved_preview.editor.get_text() == sequence


def exercise_paused_finalization(app, old):
    """Keep history selection stable and recover a paused draft without accepting a late provider result."""
    workspace = app.conversation_workspace
    app.config = replace(app.config, auto_copy_rewrite=True)
    for return_to_live in (False, True):
        app.pending_session_identifier = "paused-fixture"
        app._start_live_rewrite()
        workspace.set_live("00:15", "The final source stays immutable.")
        draft = "## Questions\n- What remains?\n\n## Architecture\nMy deliberate edit."
        workspace.show_live_draft(draft)
        client = SimpleNamespace(cancel=lambda: None)
        app.live_rewrite_client = client
        app._pause_live_rewrite()
        app._live_rewrite_finished("paused-fixture", client, app.live_revision, "Late replacement", "fixture")
        assert workspace.live_draft() == draft
        entry = app.history_store.add("Final source", "Final source", "dictation", "eng", None, "ready")
        app.live_final_entry = entry.identifier
        workspace.show_conversation(old, [])
        if return_to_live:
            workspace.show_live()
        with patch("mluva_linux.app.deliver_text") as copy, patch.object(app, "_new_rewrite_client") as request:
            app._maybe_live_rewrite("Final source", final=True)
            copy.assert_not_called()
            request.assert_not_called()
        saved = app.conversation_store.replies(entry.identifier)
        assert len(saved) == 1 and saved[0].text == draft and "paused" in saved[0].instruction
        assert app.history_store.find(entry.identifier).raw_text == "Final source"
        assert workspace.entry.identifier == (entry.identifier if return_to_live else old.identifier)


if __name__ == "__main__":
    main()
