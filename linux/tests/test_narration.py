"""Annotation completion and cancellation without clipboard or normal dictation delivery."""

import io
from dataclasses import replace
from pathlib import Path

import pytest

from mluva_linux.config import AppConfig, save_config
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.narration import AnnotationSession
from mluva_linux.workflow import freeze_transcript_preparation


class Recorder:
    """Observe the recording destination and its stop/cancel lifecycle."""

    path: Path | None = None
    stopped = False
    cancelled = False

    def start(self, path):
        """Create fixture audio in the production volatile directory."""
        self.path = path
        path.write_bytes(b"independent audio fixture")

    def stop(self):
        """Return the same finalized fixture path."""
        self.stopped = True
        return self.path

    def cancel(self):
        """Observe cleanup without connecting to any sound device."""
        self.cancelled = True


class Speech:
    """Count upload calls and inspect the finalized audio before giving complete words."""

    calls = 0

    def transcribe(self, path, language, model):
        """Independently inspect the uploaded input and return complete words."""
        self.calls += 1
        assert path.read_bytes() == b"independent audio fixture"
        assert (language, model) == ("eng", "scribe_v2")
        return TranscriptionResult("Česká poznámka.", language, None, None)


@pytest.mark.parametrize("control", ["", "cancel\n", "stop", "stop\n"])
def test_only_explicit_editor_stop_uploads_and_always_erases_audio(tmp_path: Path, control: str) -> None:
    """EOF and malformed input cannot upload or return text; normal Stop produces one complete annotation."""
    config, recorder, speech = AppConfig(), Recorder(), Speech()
    config_path = tmp_path / "config.json"
    save_config(config, config_path)
    session = AnnotationSession(
        config, config_path, recorder, speech, freeze_transcript_preparation(config, None, "dictation", None)
    )
    text = session.run(io.StringIO(control))
    assert text == ("Česká poznámka." if control == "stop\n" else None)
    assert speech.calls == (1 if control == "stop\n" else 0)
    assert recorder.stopped is (control == "stop\n")
    assert recorder.cancelled
    assert not recorder.path.parent.exists()


def test_incognito_rejects_annotation_before_a_microphone_or_provider_is_started(tmp_path: Path) -> None:
    """Private mode cannot be bypassed by launching the editor command directly."""
    config, recorder, speech = replace(AppConfig(), incognito_mode=True), Recorder(), Speech()
    config_path = tmp_path / "config.json"
    save_config(config, config_path)
    session = AnnotationSession(
        config, config_path, recorder, speech, freeze_transcript_preparation(config, None, "dictation", None)
    )
    with pytest.raises(ValueError, match="Incognito"):
        session.run(io.StringIO("stop\n"))
    assert recorder.path is None
    assert speech.calls == 0


def test_switching_to_incognito_before_stop_prevents_the_annotation_upload(tmp_path: Path) -> None:
    """Enabling privacy after capture starts prevents submission of that annotation's audio."""
    config, recorder, speech = AppConfig(), Recorder(), Speech()
    config_path = tmp_path / "config.json"
    save_config(config, config_path)

    class Controls(io.StringIO):
        def readline(self, size=-1):
            """Enable privacy as the editor's Stop arrives."""
            save_config(replace(config, incognito_mode=True), config_path)
            return super().readline(size)

    session = AnnotationSession(
        config, config_path, recorder, speech, freeze_transcript_preparation(config, None, "dictation", None)
    )
    assert session.run(Controls("stop\n")) is None
    assert speech.calls == 0
    assert not recorder.path.parent.exists()
