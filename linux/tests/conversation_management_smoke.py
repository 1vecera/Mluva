"""Exercise sidebar management and inline titles in the isolated production GTK application."""

import json
import os
import sys
import traceback
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication, render_widget
from gi.repository import Gdk, GLib, Gtk
from live_workspace_smoke import paint, settle

from mluva_linux.conversation_titles import save_generated_title
from mluva_linux.pipewire import PipeWireDeviceCatalog


def descendants(widget):
    """Find actual dialog buttons without assuming their private GTK layout."""
    yield widget
    child = widget.get_first_child()
    while child is not None:
        yield from descendants(child)
        child = child.get_next_sibling()


def main():
    """Test local writes, stale actions, keyboard edits and compact layouts without desktop side effects."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ or os.environ.get("GDK_BACKEND") != "x11":
        raise RuntimeError("Use dev/run-isolated.sh")
    app = IsolatedApplication()
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    width = int(os.environ.get("MLUVA_UI_WIDTH", "1060"))
    height = int(os.environ.get("MLUVA_UI_HEIGHT", "780"))
    errors = []
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def action(identifier, label):
        """Open a real sidebar popover and activate one of its visible actions."""
        workspace = app.conversation_workspace
        workspace.split.set_show_sidebar(True)
        menu = workspace.row_menus[identifier]
        menu.popup()
        settle(lambda: menu.get_popover().get_mapped())
        button = next(
            widget
            for widget in descendants(menu.get_popover())
            if isinstance(widget, Gtk.Button) and widget.get_label() == label
        )
        assert button.is_sensitive()
        button.emit("clicked")

    def respond(label):
        """Click the dialog's production response control and dispatch its asynchronous completion."""
        dialog = app.window.get_visible_dialog()
        button = next(
            widget for widget in descendants(dialog) if isinstance(widget, Gtk.Button) and widget.get_label() == label
        )
        assert button.is_sensitive()
        button.emit("clicked")
        settle(lambda: app.window.get_visible_dialog() is None)

    def select_target():
        """Find an older destination by search and select its native row."""
        dialog = app.window.get_visible_dialog()
        search = dialog.get_extra_child().get_first_child()
        search.set_text("Roadmap")
        search.emit("search-changed")
        choices = next(widget for widget in descendants(dialog.get_extra_child()) if isinstance(widget, Gtk.ListBox))
        assert choices.get_row_at_index(1) is None
        choices.select_row(choices.get_row_at_index(0))
        assert dialog.get_response_enabled("merge")

    def exercise():
        try:
            Gtk.Settings.get_default().set_property("gtk-enable-animations", False)
            app.window.set_default_size(width, height)
            app.window.set_size_request(width, height)
            workspace = app.conversation_workspace
            history, store = app.history_store, app.conversation_store
            target = history.add("First original", "First original", "dictation", "eng", None, "copied")
            source = history.add("Second original", "Second original", "dictation", "eng", None, "copied")
            reply = store.append(source.identifier, "Polish", "Second polished", "fixture")
            history.update_title(target.identifier, "Roadmap")
            history.update_title(source.identifier, "Meeting notes")
            workspace.refresh_history()
            app.history_page.refresh()
            workspace.show_conversation(history.find(source.identifier), [reply])
            editor = workspace.editors[(source.identifier, None)]
            editor.get_buffer().set_text("Edited source")
            workspace.prompt.get_buffer().set_text("Unsent follow-up")
            workspace.title_button.emit("clicked")
            assert workspace.title_stack.get_visible_child_name() == "edit"
            workspace.title_entry.set_text("  Renamed meeting  ")
            paint(app.window, output / "inline-title.png")
            assert (app.window.get_surface().get_width(), app.window.get_surface().get_height()) == (width, height)
            workspace.title_entry.emit("activate")
            assert history.find(source.identifier).title == "Renamed meeting"
            assert workspace.editors[(source.identifier, None)] is editor
            assert editor.get_text() == "Edited source" and workspace.prompt_text() == "Unsent follow-up"
            assert workspace.row_titles[source.identifier].get_label() == "Renamed meeting"
            assert app.history_page.entry_rows[source.identifier].get_title() == "Renamed meeting"
            assert not save_generated_title(history, source.identifier, "Late generated title", "Renamed meeting")

            workspace.title_button.emit("clicked")
            workspace.title_entry.set_text(" ")
            workspace.title_entry.emit("activate")
            assert workspace.title_entry.has_css_class("error")
            workspace.title_entry.set_text("Discard me")
            controllers = workspace.title_entry.observe_controllers()
            controller = next(
                controllers.get_item(i)
                for i in range(controllers.get_n_items())
                if isinstance(controllers.get_item(i), Gtk.EventControllerKey)
            )
            controller.emit("key-pressed", Gdk.KEY_Escape, 0, Gdk.ModifierType(0))
            assert history.find(source.identifier).title == "Renamed meeting"
            assert workspace.renaming_identifier is None

            action(target.identifier, "Rename")
            assert workspace.entry.identifier == target.identifier
            workspace.title_entry.set_text("Wrong destination")
            workspace.show_conversation(history.find(source.identifier), store.replies(source.identifier))
            workspace.title_entry.emit("activate")
            assert history.find(target.identifier).title == "Roadmap"
            assert history.find(source.identifier).title == "Renamed meeting"

            workspace.split.set_show_sidebar(True)
            menu = workspace.row_menus[source.identifier]
            menu.popup()
            settle(lambda: menu.get_popover().get_mapped())
            paint(app.window, output / "sidebar-menu.png")
            render_widget(menu.get_popover()).save_to_png(str(output / "sidebar-actions.png"))
            menu.popdown()
            action(source.identifier, "Merge with…")
            assert not app.window.get_visible_dialog().get_response_enabled("merge")
            select_target()
            paint(app.window, output / "merge-dialog.png")
            respond("Cancel")
            assert len(store.search()) == 2

            action(source.identifier, "Merge with…")
            select_target()
            workspace.set_busy(True, "Synthetic active rewrite")
            respond("Merge")
            assert len(store.search()) == 2
            assert not workspace.row_menus[source.identifier].is_sensitive()
            workspace.set_busy(False, "Ready")
            action(source.identifier, "Merge with…")
            select_target()
            respond("Merge")
            assert workspace.entry.identifier == target.identifier
            assert store.source_text(workspace.entry) == "First original\n\nEdited source"
            assert workspace.prompt_text() == "Unsent follow-up"
            assert len(workspace.rows) == 1 and source.identifier not in workspace.drafts
            assert store.replies(target.identifier)[-1].text == "First original\n\nSecond polished"
            assert not workspace.edit_drafts

            action(target.identifier, "Delete…")
            app.incognito_switch.set_active(True)
            respond("Delete")
            assert len(history.recent()) == 2
            assert not workspace.title_button.is_sensitive()
            assert not app._rename_conversation(target.identifier, "Private rename must not persist")
            assert history.find(target.identifier).title == "Roadmap"
            app.incognito_switch.set_active(False)
            workspace.show_conversation(history.find(target.identifier), store.replies(target.identifier))
            workspace.refresh_history()

            workspace.set_live("Recording", "Volatile words")
            assert not workspace.row_menus[target.identifier].is_sensitive()
            assert not app._delete_conversation(target.identifier)
            workspace.finish_live()
            action(target.identifier, "Delete…")
            respond("Cancel")
            assert len(history.recent()) == 2
            action(target.identifier, "Delete…")
            respond("Delete")
            assert history.recent() == [] and store.replies(target.identifier) == []
            assert workspace.entry is None and not workspace.title_button.is_sensitive()
            assert target.identifier not in workspace.drafts
            paint(app.window, output / "deleted-empty-state.png")
            (output / "receipt.json").write_text(
                json.dumps(
                    {
                        "width": width,
                        "height": height,
                        "display": os.environ["DISPLAY"],
                        "inline_rename": True,
                        "merge": True,
                        "delete": True,
                        "stale_action_rejected": True,
                        "no_provider_or_clipboard_calls": True,
                    }
                )
            )
        except Exception:
            errors.append(traceback.format_exc())
        app.quit()
        return GLib.SOURCE_REMOVE

    with (
        patch("mluva_linux.app.FocusedTextTargetTracker", return_value=None),
        patch.object(PipeWireDeviceCatalog, "from_system", return_value=PipeWireDeviceCatalog()),
        patch.object(app, "_new_rewrite_client", side_effect=AssertionError("Unexpected provider request")) as requests,
        patch("mluva_linux.app.deliver_text", side_effect=AssertionError("Unexpected clipboard change")) as copies,
    ):
        GLib.idle_add(exercise)
        app.run(None)
        assert not requests.called and not copies.called
    if errors:
        (output / "errors.txt").write_text("\n".join(errors))
        sys.excepthook = sys.__excepthook__
        raise RuntimeError("; ".join(errors))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
