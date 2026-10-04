"""Exercise screenshot capture ownership, model context and privacy in a disposable native workspace."""

import json
import os
import subprocess
import sys
import traceback
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from conversation_ui_smoke import IsolatedApplication, paint, settle
from gi.repository import GLib
from screenshot_fixture import png

from mluva_linux.codex_client import CodexAppServerClient
from mluva_linux.config import AudioRetentionPolicy
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.workflow import DictationWorkflow


class SyntheticRecorder:
    """Replace microphone IO while running the production capture and completion lifecycle."""

    process = None
    audio_level = 0
    path = None

    def start(self, path, _callback=None):
        """Create an owner-local audio stand-in without touching sound devices."""
        self.path = path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"synthetic audio")
        self.process = object()

    def stop(self):
        """Hand the same finalized path to the real workflow."""
        self.process = None
        return self.path

    def cancel(self):
        """Erase only this fixture's current audio."""
        self.process = None
        if self.path:
            self.path.unlink(missing_ok=True)


class SyntheticSpeech:
    """Return a complete transcript without uploading audio."""

    def transcribe(self, *_args, **_kwargs):
        """Add predictable words for each continuation."""
        return TranscriptionResult("More words.", "eng", None, None)


def main() -> int:
    """Drive the real app, bridge, picker subprocess, stores and app-server transport without a host device."""
    if "OFFSCREEN_SESSION_ROOT" not in os.environ:
        raise RuntimeError("Use dev/run-isolated.sh")
    output = Path(os.environ["OFFSCREEN_ARTIFACT_DIR"])
    fixture_bin = output / "fixture-bin"
    fixture_bin.mkdir()
    first = output / "earlier.png"
    current = output / "current.png"
    first.write_bytes(png(40))
    current.write_bytes(png(170))
    gate, marker = output / "selection-ready", output / "picker-started"
    editor_receipt = output / "editor.json"
    picker = fixture_bin / "omarchy"
    picker.write_text(
        f"#!{sys.executable}\n"
        "import os, sys, time\nfrom pathlib import Path\n"
        "assert sys.argv[1:] == ['screenshot', 'region', 'save']\n"
        f"Path({str(marker)!r}).touch()\n"
        f"while not Path({str(gate)!r}).exists(): time.sleep(0.02)\n"
        "path = Path(os.environ['OMARCHY_SCREENSHOT_DIR']) / 'selection.png'\n"
        f"path.write_bytes(Path({str(current)!r}).read_bytes())\nprint(path)\n"
    )
    picker.chmod(0o700)
    editor = fixture_bin / "tensaku-edit"
    editor.write_text(
        f"#!{sys.executable}\nimport json, sys\nfrom pathlib import Path\n"
        "assert Path(sys.argv[1]).is_file()\n"
        f"Path({str(editor_receipt)!r}).write_text(json.dumps(sys.argv[1:]))\n"
    )
    editor.chmod(0o700)
    os.environ["PATH"] = str(fixture_bin) + os.pathsep + os.environ["PATH"]
    app = IsolatedApplication()
    errors, evidence = [], {}
    sys.excepthook = lambda *error: errors.append("".join(traceback.format_exception(*error)))

    def exercise() -> bool:
        try:
            workspace = app.conversation_workspace
            app.config = replace(
                app.config,
                auto_copy_dictation=False,
                auto_copy_rewrite=False,
                live_rewrite_enabled=False,
                audio_retention_policy=AudioRetentionPolicy.NEVER,
            )
            app.recorder = SyntheticRecorder()
            app.cleanup_switch.set_active(True)
            parent = app.history_store.add(
                "Earlier narration.", "Earlier narration.", "dictation", "eng", None, "saved"
            )
            other = app.history_store.add(
                "Other conversation.", "Other conversation.", "dictation", "eng", None, "saved"
            )
            earlier = app.screenshot_store.add(parent.identifier, first.read_bytes())
            workspace.show_conversation(parent, [])
            server = Path(__file__).with_name("fake_app_server.py")

            def client(_config=None, **_kwargs):
                """Have an independent process verify both real image inputs on every transformation."""
                return CodexAppServerClient(
                    command=(sys.executable, str(server), "--expect-image", str(first), "--expect-image", str(current))
                )

            transport = client()
            app.workflow = DictationWorkflow(
                app.config, SyntheticSpeech(), transport, app.history_store, app.codex_workspace
            )

            def prepare(session, path, *_args):
                """Signal readiness through the production capture callback without opening a sound device."""
                GLib.idle_add(app._capture_prepared, session, path, None, None, "gpt-5.4", 0.01, None, 0.01)

            def bridge_capture() -> None:
                """Activate the exported action from a separate process on this private bus."""
                child = subprocess.Popen([sys.executable, "-m", "mluva_linux.shell_bridge", "screenshot"])
                settle(lambda: child.poll() is not None)
                assert child.returncode == 0

            with (
                patch.object(app, "_prepare_capture", side_effect=prepare),
                patch.object(app, "_new_rewrite_client", side_effect=client),
            ):
                app._continue_recording(parent.identifier)
                settle(lambda: app.recorder.process is not None)
                capture = app.pending_session_identifier
                bridge_capture()
                settle(marker.exists)
                workspace.show_conversation(other, [], preserve_live=True)
                app._stop_capture()
                settle(lambda: app.recorder.process is None)
                assert app.capture_processing
                assert transport.process is None, "Provider must wait for an outstanding screenshot selection"
                gate.touch()
                settle(lambda: not app.capture_processing and app.screenshot_picker is None)
                settle(editor_receipt.exists)
                images = app.screenshot_store.recent(parent.identifier)
                assert len(images) == 2
                assert app.screenshot_store.recent(other.identifier) == []
                assert app.screenshot_store.recent(capture, capture=True) == []
                assert images[1].captured_after_seconds is not None
                assert images[1].path.read_bytes() == current.read_bytes()
                assert app.history_store.continuations(parent.identifier)[0].enhancement_context_sources == (
                    "screenshots",
                )
                assert (
                    app.conversation_store.source_text(app.history_store.find(parent.identifier))
                    == "Earlier narration.\n\nMore words."
                )
                evidence["capture_waits_for_picker_and_keeps_frozen_conversation"] = True
                workspace.show_conversation(app.history_store.find(parent.identifier), [])
                assert workspace.screenshot_shelf.get_visible()
                assert json.loads(editor_receipt.read_text()) == [str(images[1].path)]
                current.write_bytes(png(220))
                images[1].path.write_bytes(current.read_bytes())
                app._begin_rewrite(parent.identifier, "Use these screenshots to explain the visible controls.")
                settle(lambda: app.rewrite_client is None)
                assert app.conversation_store.replies(parent.identifier)[-1].text == "Clean text."
                evidence["follow_up_receives_saved_editor_pixels"] = True
                paint(app.window, output / "screenshot-workspace.png")
                gate.unlink()
                marker.unlink()
                bridge_capture()
                settle(marker.exists)
                app.incognito_switch.set_active(True)
                settle(lambda: app.screenshot_picker is None)
                assert len(app.screenshot_store.recent(parent.identifier)) == 2
                assert not workspace.screenshot_shelf.get_visible()
                assert all(not button.get_sensitive() for button in workspace.screenshot_buttons)
                evidence["incognito_cancels_picker_without_creating_or_uploading_an_image"] = True
                app.incognito_switch.set_active(False)
                app._remove_screenshot(images[1].identifier)
                assert not images[1].path.exists()
                assert app._delete_conversation(parent.identifier)
                assert not earlier.path.exists()
                evidence["native_remove_and_delete_erase_owned_images"] = True
                failed_capture = "screenshot-preparation-failure"
                interrupted = app.screenshot_store.add(failed_capture, png(210), capture=True)
                app.continuation_identifier = None
                app.pending_session_identifier = failed_capture
                app.audio_path = output / "not-recorded.wav"
                app.capture_preparing = True
                app._capture_preparation_failed(failed_capture, app.audio_path, "Synthetic readiness failure", 0)
                assert interrupted.path.exists()
                assert not app.screenshot_store.pending_captures()
                saved_owner = app.history_store.recent()[0].identifier
                assert app.screenshot_store.recent(saved_owner)[0].identifier == interrupted.identifier
                evidence["preparation_failure_preserves_images_with_reviewable_history_owner"] = True
            (output / "results.json").write_text(json.dumps(evidence, indent=2) + "\n")
        except Exception:
            errors.append(traceback.format_exc())
        finally:
            app.quit()
        return GLib.SOURCE_REMOVE

    app.connect("activate", lambda _app: GLib.timeout_add(100, exercise))
    app.run([])
    if errors:
        (output / "errors.log").write_text("\n".join(errors))
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(json.dumps(evidence))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
