"""Fresh spoken text for an editor box, without clipboard delivery or a second history item."""

import contextlib
import os
import signal
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import TextIO

from mluva_linux.audio import PipeWireRecorder
from mluva_linux.config import AppConfig, default_config_dir, load_config
from mluva_linux.personalization import PersonalizationStore
from mluva_linux.providers import transcription_client
from mluva_linux.volatile_audio import VolatileAudioStore
from mluva_linux.workflow import TranscriptionClient, TranscriptPreparationSnapshot, freeze_transcript_preparation

MAX_ANNOTATION_CHARACTERS = 120_000


@dataclass
class AnnotationSession:
    """Own one short recording and the frozen speech/local-processing choices used for it."""

    config: AppConfig
    config_path: Path
    recorder: PipeWireRecorder
    speech: TranscriptionClient
    preparation: TranscriptPreparationSnapshot

    def run(self, controls: TextIO) -> str | None:
        """Upload only after an explicit stop line; EOF, dismissal and failures erase raw audio."""
        if self.config.incognito_mode:
            raise ValueError("Narrated screenshots are unavailable in Incognito.")
        store = VolatileAudioStore()
        try:
            self.recorder.start(store.path / "annotation.wav")
            if controls.readline(32) != "stop\n":
                return None
            path = self.recorder.stop()
            if load_config(self.config_path).incognito_mode:
                return None
            transcript = self.speech.transcribe(path, self.config.language_code, self.config.transcription_model)
            text = self.preparation.process(transcript.text)
            if not text.strip():
                raise ValueError("No annotation speech was detected.")
            if len(text) > MAX_ANNOTATION_CHARACTERS:
                raise ValueError("Annotation exceeds the text box limit.")
            return text
        finally:
            try:
                self.recorder.cancel()
            finally:
                store.close()


def main() -> int:
    """Run the editor's stdin/UTF-8-stdout protocol through the installed Mluva launch environment."""
    speech = None

    def cancel(_signal, _frame):
        """Unwind normal cancellation; the editor also owns the helper's complete process group."""
        raise InterruptedError("Annotation cancelled.")

    signal.signal(signal.SIGTERM, cancel)
    signal.signal(signal.SIGINT, cancel)
    try:
        config_path = default_config_dir(os.environ) / "config.json"
        config = load_config(config_path)
        if config.incognito_mode:
            print("Narrated screenshots are unavailable in Incognito.", file=sys.stderr)
            return 1
        # Local runtimes may print progress. Only the final annotation belongs on stdout.
        with contextlib.redirect_stdout(sys.stderr):
            speech = transcription_client(config)
            personalization = PersonalizationStore(default_config_dir(os.environ) / "personalization.json")
            session = AnnotationSession(
                config,
                config_path,
                PipeWireRecorder.from_system(target=config.microphone_target),
                speech,
                freeze_transcript_preparation(config, personalization, "dictation", None),
            )
            text = session.run(sys.stdin)
        if text is None:
            return 1
        sys.stdout.write(text)
        sys.stdout.flush()
        return 0
    except Exception:
        print(
            "Annotation could not finish. Check Mluva's microphone and speech provider, then try again.",
            file=sys.stderr,
        )
        return 1
    finally:
        close = getattr(speech, "close", None)
        if close is not None:
            with contextlib.suppress(Exception), contextlib.redirect_stdout(sys.stderr):
                close()


if __name__ == "__main__":
    raise SystemExit(main())
