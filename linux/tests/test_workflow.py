"""End-to-end workflow coverage around external-service boundaries."""

import json
import threading
from collections.abc import Callable, Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest

from voice_scribe_linux.codex_client import CodexAppServerClient
from voice_scribe_linux.config import AppConfig, AudioRetentionPolicy
from voice_scribe_linux.delivery import DeliveryReceipt
from voice_scribe_linux.diagnostics import DiagnosticsStore
from voice_scribe_linux.elevenlabs import ElevenLabsClient, TranscriptionResult
from voice_scribe_linux.history import HistoryStore
from voice_scribe_linux.personalization import PersonalizationStore
from voice_scribe_linux.segment_cleanup import (
    SegmentCleanupFailure,
    SegmentCleanupTerminalSegment,
    SegmentCleanupTerminalSnapshot,
)
from voice_scribe_linux.workflow import DictationWorkflow, WorkflowFailure, reprocess_history_entry


class WorkflowScribeHandler(BaseHTTPRequestHandler):
    """Return one transcript while exercising a real HTTP upload."""

    def do_POST(self) -> None:
        """Consume uploaded audio and return a complete Scribe response."""
        self.rfile.read(int(self.headers["Content-Length"]))
        response = json.dumps(
            {
                "text": "Scratchpad thought.",
                "language_code": "eng",
                "language_probability": 0.99,
                "transcription_id": "scratchpad-scribe",
            }
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)

    def log_message(self, _format: str, *_args: object) -> None:
        """Suppress successful local contract traffic."""


class FailingElevenLabsClient:
    """Raise a deterministic recognition failure without network traffic."""

    def transcribe(self, _audio_path: Path, language_code: str, model_id: str) -> TranscriptionResult:
        """Fail after validating that workflow supplied its stable Scribe contract."""
        assert language_code == "eng"
        assert model_id == "scribe_v2"
        raise RuntimeError("deterministic Scribe failure")


class StaticTranscriptionClient:
    """Return one configured transcript without network traffic."""

    def __init__(self, text: str) -> None:
        """Store the immutable raw transcript."""
        self.text = text

    def transcribe(self, _audio_path: Path, language_code: str, model_id: str) -> TranscriptionResult:
        """Return the configured text with stable provider metadata."""
        assert language_code == "eng"
        assert model_id == "scribe_v2"
        return TranscriptionResult(self.text, language_code, 1.0, "static-transcription")


class ForbiddenTranscriptionClient:
    """Fail if a committed realtime result is accidentally uploaded through the batch route."""

    def transcribe(self, _audio_path: Path, _language_code: str, _model_id: str) -> TranscriptionResult:
        """Reject every call because successful realtime recognition must be exactly once."""
        raise AssertionError("batch transcription must not run for a committed realtime result")


class FakeDeliveryTarget:
    """Record restoration and content-free caret confirmation around one workflow delivery."""

    def __init__(self, restore_succeeds: bool = True, confirmation: bool | None = True) -> None:
        """Configure deterministic restore and confirmation results."""
        self.restore_succeeds = restore_succeeds
        self.confirmation = confirmation
        self.restore_calls = 0
        self.confirmed_text: list[str] = []
        self.inserted_text: list[str] = []

    def restore(self) -> bool:
        """Record restoration immediately before the desktop delivery boundary."""
        self.restore_calls += 1
        return self.restore_succeeds

    def confirm_insertion(self, inserted_text: str) -> bool | None:
        """Record only the already-delivered candidate supplied for caret arithmetic."""
        self.confirmed_text.append(inserted_text)
        return self.confirmation

    def insert_text(self, inserted_text: str) -> bool | None:
        """Represent one native accessibility insertion attempt."""
        self.inserted_text.append(inserted_text)
        return self.confirmation


def test_history_reprocess_uses_current_scoped_deterministic_rules_without_provider_access(tmp_path: Path) -> None:
    """Re-render immutable raw text locally and preserve recognition provenance for later review."""
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement(
        "post grass",
        "PostgreSQL",
        application_identifier="/usr/bin/code",
    )
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    entry = history.add(
        raw_text="use post grass period",
        delivered_text="Earlier output.",
        mode="dictation",
        language_code="eng",
        transcription_id="scribe-reprocess",
        delivery_outcome="pasted",
        application_identifier="/usr/bin/code",
        enhancement_provider_id="codex-app-server",
        enhancement_model_identifier="gpt-5.4",
        enhancement_outcome="completed",
        recognition_ms=25,
        enhancement_ms=40,
        delivery_ms=8,
    )

    reprocessed = reprocess_history_entry(AppConfig(), history, personalization, entry.identifier)

    assert reprocessed.raw_text == "use post grass period"
    assert reprocessed.delivered_text == "use PostgreSQL."
    assert reprocessed.application_identifier == "/usr/bin/code"
    assert reprocessed.recognition_ms == 25
    assert reprocessed.enhancement_provider_id is None
    assert reprocessed.enhancement_ms is not None
    assert reprocessed.delivery_ms is None


class CapturingCodexClient:
    """Capture one command prompt without starting the real Codex app-server."""

    def __init__(self, result: str) -> None:
        """Store the deterministic proposed replacement."""
        self.result = result
        self.prompts: list[str] = []
        self.resolved_models: list[str | None] = []

    def resolve_model(self, requested_model: str | None) -> str:
        """Resolve one deterministic test model before recognition starts."""
        self.resolved_models.append(requested_model)
        return requested_model or "gpt-5.4-test"

    def transform(self, text: str, cwd: Path, model: str | None = None) -> str:
        """Record only the disclosed prompt fields and return a fixed preview."""
        assert cwd.is_dir()
        assert model == "gpt-5.4-test"
        self.prompts.append(text)
        return self.result


@pytest.fixture
def scribe_server() -> Iterator[ThreadingHTTPServer]:
    """Serve deterministic Scribe responses without contacting ElevenLabs."""
    server = ThreadingHTTPServer(("127.0.0.1", 0), WorkflowScribeHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield server
    server.shutdown()
    server.server_close()
    thread.join()


def make_workflow(tmp_path: Path, server: ThreadingHTTPServer) -> DictationWorkflow:
    """Construct a workflow against isolated history and the local Scribe contract server."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    return DictationWorkflow(
        config=AppConfig(),
        elevenlabs=ElevenLabsClient(
            api_key="write-only-test-key",
            endpoint=f"http://127.0.0.1:{server.server_port}/v1/speech-to-text",
        ),
        codex=CodexAppServerClient(command=("not-started",)),
        history=history,
        cwd=tmp_path,
    )


def test_scratchpad_requires_explicit_delivery_and_retains_audio(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
) -> None:
    """Keep successful Scratchpad audio and leave the clipboard untouched until acceptance."""
    audio_path = tmp_path / "recordings" / "scratchpad.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-scratchpad-audio")
    workflow = make_workflow(tmp_path, scribe_server)
    result = workflow.complete(
        audio_path,
        mode="scratchpad",
        use_codex_cleanup=False,
        allow_auto_paste=False,
    )
    assert result.output_text == "Scratchpad thought."
    assert not result.delivery.copied
    assert result.retained_audio_path == audio_path
    assert audio_path.exists()
    assert result.requires_acceptance
    assert result.history_entry is not None
    assert result.history_entry.delivery_outcome == "draft"
    assert result.history_entry.retained_audio_path == str(audio_path)
    assert result.recognition_route == "scribe-v2-batch"
    assert result.recognition_fallback_reason is None
    assert result.history_entry.recognition_route == "scribe-v2-batch"


def test_spoken_structure_changes_delivered_text_but_preserves_raw_history(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Keep Scribe recognition immutable beside deterministic prepared output."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture normalized output without changing the desktop."""
        assert text == "First thought.\n\nCorrected thought."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "structured.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-structured")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    raw_text = "First thought period new paragraph wrong thought scratch that Corrected thought period"
    workflow = DictationWorkflow(
        config=AppConfig(spoken_commands_enabled=True),
        elevenlabs=StaticTranscriptionClient(raw_text),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
    )

    assert result.transcription.text == raw_text
    assert result.output_text == "First thought.\n\nCorrected thought."
    assert result.history_entry is not None
    assert result.history_entry.raw_text == raw_text
    assert result.history_entry.delivered_text == result.output_text


def test_personalization_runs_before_delivery_and_preserves_raw_history(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Apply scoped deterministic rules locally while retaining provider recognition unchanged."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture personalized output without touching the desktop."""
        assert text == "Use PostgreSQL. Best,\nDaniel"
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    raw_text = "Use post grass period snippet email signoff"
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement(
        "post grass",
        "wrong global value",
    )
    personalization.save_dictionary_replacement(
        "post grass",
        "PostgreSQL",
        application_identifier="/usr/bin/code",
    )
    personalization.save_snippet(
        "email signoff",
        "Best,\nDaniel",
        application_identifier="/usr/bin/code",
    )
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    audio_path = tmp_path / "recordings" / "personalized.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-personalized")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient(raw_text),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        personalization=personalization,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        application_identifier="/usr/bin/code",
    )

    assert result.transcription.text == raw_text
    assert result.output_text == "Use PostgreSQL. Best,\nDaniel"
    assert result.history_entry is not None
    assert result.history_entry.raw_text == raw_text
    assert result.history_entry.delivered_text == result.output_text
    assert result.history_entry.application_identifier == "/usr/bin/code"


def test_frozen_transcript_preparation_ignores_mid_capture_rule_edits(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Use the dictionary and snippet policy captured before speech rather than mutable stop-time settings."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Require the pre-capture dictionary value without changing the desktop."""
        assert text == "Use PostgreSQL."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement("post grass", "PostgreSQL")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    audio_path = tmp_path / "recordings" / "frozen-personalization.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-frozen-personalization")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient("Use post grass period"),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        personalization=personalization,
    )
    frozen = workflow.freeze_transcript_preparation("dictation", None)
    personalization.save_dictionary_replacement("post grass", "mutable stop-time value")

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        transcript_preparation=frozen,
    )

    assert result.output_text == "Use PostgreSQL."


def test_saved_style_uses_only_instructions_and_personalized_text(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Keep application identity out of a safe style request and deliver its validated candidate."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture the safely styled output without desktop mutation."""
        assert text == "Deploy PostgreSQL 17\n- Keep --dry-run enabled"
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement("post grass", "PostgreSQL")
    technical = next(style for style in personalization.styles if style.name == "Technical notes")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    codex = CapturingCodexClient("Deploy PostgreSQL 17\n- Keep --dry-run enabled")
    audio_path = tmp_path / "recordings" / "styled.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-styled")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient("deploy post grass 17 and keep --dry-run enabled"),
        codex=codex,
        history=history,
        cwd=tmp_path,
        personalization=personalization,
        diagnostics=diagnostics,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        application_identifier="/opt/private/application-identity",
        style_identifier=technical.identifier,
    )

    assert result.output_text == "Deploy PostgreSQL 17\n- Keep --dry-run enabled"
    assert len(codex.prompts) == 1
    assert codex.resolved_models == [None]
    assert "/opt/private/application-identity" not in codex.prompts[0]
    style_payload = json.loads(codex.prompts[0].split("STYLE INPUT JSON:\n", maxsplit=1)[1])
    assert style_payload == {
        "style_instructions": technical.instructions,
        "dictated_text": "deploy PostgreSQL 17 and keep --dry-run enabled",
    }
    enhancement_event = next(event for event in diagnostics.recent() if event.stage == "enhancement")
    assert enhancement_event.provider == "codex-app-server"
    assert enhancement_event.outcome == "completed"
    assert result.history_entry is not None
    assert result.history_entry.enhancement_provider_id == "codex-app-server"
    assert result.history_entry.enhancement_model_identifier == "gpt-5.4-test"
    assert result.history_entry.enhancement_context_sources == ("style-instructions",)
    assert result.history_entry.enhancement_outcome == "completed"
    assert result.history_entry.recognition_ms == result.recognition_ms
    assert result.history_entry.enhancement_ms == result.enhancement_ms
    assert result.history_entry.delivery_ms == result.delivery_ms


def test_realtime_segment_cleanup_uses_ordered_candidates_and_raw_fallback(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Deliver validated segments in capture order without repeating whole-transcript cleanup."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture ordered terminal segment output without changing the desktop."""
        assert text == "clean alpha raw beta"
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    codex = CapturingCodexClient("whole cleanup must not run")
    audio_path = tmp_path / "recordings" / "segments.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-segments")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=ForbiddenTranscriptionClient(),
        codex=codex,
        history=history,
        cwd=tmp_path,
    )
    cleanup = SegmentCleanupTerminalSnapshot(
        session_identifier="segment-session",
        provider_identifier="codex-app-server",
        model_identifier="gpt-5.4-test",
        segments=(
            SegmentCleanupTerminalSegment(
                identifier="segment-0",
                sequence=0,
                raw_text="raw alpha",
                selected_text="clean alpha",
                failure=None,
            ),
            SegmentCleanupTerminalSegment(
                identifier="segment-1",
                sequence=1,
                raw_text="raw beta",
                selected_text="raw beta",
                failure=SegmentCleanupFailure.PROVIDER,
            ),
        ),
        stop_drain_seconds=0.012,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=True,
        allow_auto_paste=False,
        session_identifier="segment-session",
        recognized_transcription=TranscriptionResult(
            "raw alpha raw beta",
            "eng",
            None,
            "realtime-session",
        ),
        recognition_duration_seconds=0.004,
        codex_model_identifier="gpt-5.4-test",
        segment_cleanup=cleanup,
    )

    assert result.output_text == "clean alpha raw beta"
    assert codex.prompts == []
    assert result.enhancement_ms >= 12
    assert result.history_entry is not None
    assert result.history_entry.raw_text == "raw alpha raw beta"
    assert result.history_entry.enhancement_provider_id == "codex-app-server"
    assert result.history_entry.enhancement_model_identifier == "gpt-5.4-test"
    assert result.history_entry.enhancement_context_sources == ()
    assert result.history_entry.enhancement_outcome == "safe-fallback"
    assert "1 cleanup segment(s) used immutable Raw Text" in result.delivery.guidance


def test_unsafe_saved_style_keeps_deterministic_text_and_records_fallback(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Reject a fluent style candidate that removes dictionary terms, numbers, flags, or negation."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Require exact safe fallback without touching the desktop."""
        assert text == "Do not deploy PostgreSQL 17 with --dry-run."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement("post grass", "PostgreSQL")
    prose = next(style for style in personalization.styles if style.name == "Prose")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    audio_path = tmp_path / "recordings" / "unsafe-style.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-unsafe-style")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient("Do not deploy post grass 17 with --dry-run period"),
        codex=CapturingCodexClient("Deploy the database now."),
        history=history,
        cwd=tmp_path,
        personalization=personalization,
        diagnostics=diagnostics,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        style_identifier=prose.identifier,
    )

    assert result.output_text == "Do not deploy PostgreSQL 17 with --dry-run."
    assert "changed protected facts or terms" in result.delivery.guidance
    assert result.history_entry is not None
    assert result.history_entry.raw_text == "Do not deploy post grass 17 with --dry-run period"
    assert result.history_entry.enhancement_model_identifier == "gpt-5.4-test"
    assert result.history_entry.enhancement_context_sources == ("style-instructions",)
    assert result.history_entry.enhancement_outcome == "raw-fallback"
    enhancement_event = next(event for event in diagnostics.recent() if event.stage == "enhancement")
    assert enhancement_event.outcome == "raw-fallback"


def test_command_uses_only_instruction_and_selection_then_waits_for_acceptance(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Keep a selected-text rewrite in preview state without touching the desktop."""

    def forbidden_delivery(_text: str, _auto_paste: bool) -> DeliveryReceipt:
        """Prove processing stops before the explicit Command acceptance boundary."""
        raise AssertionError("Command preview must not deliver")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", forbidden_delivery)
    audio_path = tmp_path / "recordings" / "command.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-command")
    workflow = make_workflow(tmp_path, scribe_server)
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow.diagnostics = diagnostics
    codex = CapturingCodexClient("Rewritten selection.")
    workflow.codex = codex

    result = workflow.complete(
        audio_path,
        mode="command",
        use_codex_cleanup=False,
        allow_auto_paste=True,
        selected_text="Original selected text.",
    )

    assert result.mode == "command"
    assert result.output_text == "Rewritten selection."
    assert result.requires_acceptance
    assert result.recognition_ms >= 0
    assert result.enhancement_ms >= 0
    assert result.delivery_ms >= 0
    assert not result.delivery.copied
    assert result.history_entry is not None
    assert result.history_entry.delivery_outcome == "pending-preview"
    assert result.history_entry.enhancement_provider_id == "codex-app-server"
    assert result.history_entry.enhancement_model_identifier == "gpt-5.4-test"
    assert result.history_entry.enhancement_context_sources == ("selected-text",)
    assert result.history_entry.enhancement_outcome == "completed"
    assert result.history_entry.delivery_ms is None
    assert not audio_path.exists()
    assert len(codex.prompts) == 1
    command_payload = json.loads(codex.prompts[0].split("COMMAND INPUT JSON:\n", maxsplit=1)[1])
    assert command_payload == {
        "spoken_instruction": "Scratchpad thought.",
        "selected_text": "Original selected text.",
    }
    assert [(event.stage, event.provider, event.outcome) for event in reversed(diagnostics.recent())] == [
        ("recognition", "elevenlabs-scribe-v2", "completed"),
        ("enhancement", "codex-app-server", "completed"),
        ("delivery", "desktop", "deferred"),
    ]


def test_command_provider_failure_records_controlled_provenance(
    tmp_path: Path,
) -> None:
    """Persist the frozen provider, model, context, outcome, and timing without its error payload."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    audio_path = tmp_path / "recordings" / "failed-command.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-failed-command")
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient("Rewrite this"),
        codex=CodexAppServerClient(command=("mluva-command-that-does-not-exist",)),
        history=history,
        cwd=tmp_path,
    )

    with pytest.raises(WorkflowFailure) as failure:
        workflow.complete(
            audio_path,
            mode="command",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            selected_text="Explicit selection",
            codex_model_identifier="gpt-5.4-test",
            audio_retention_policy=AudioRetentionPolicy.FAILURES,
        )

    assert failure.value.stage == "processing"
    assert str(failure.value) == "Processing failed: Codex Command processing failed."
    entry = failure.value.history_entry
    assert entry is not None
    assert entry.delivery_outcome == "processing-failed"
    assert entry.enhancement_provider_id == "codex-app-server"
    assert entry.enhancement_model_identifier == "gpt-5.4-test"
    assert entry.enhancement_context_sources == ("selected-text",)
    assert entry.enhancement_outcome == "failed"
    assert entry.recognition_ms is not None
    assert entry.enhancement_ms is not None
    assert entry.delivery_ms is None
    assert entry.retained_audio_path == str(audio_path)


def test_incognito_scratchpad_writes_no_history_or_audio(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
) -> None:
    """Return a memory-only editable result and erase its source WAV in Incognito."""
    audio_path = tmp_path / "recordings" / "private.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-private")
    workflow = make_workflow(tmp_path, scribe_server)
    result = workflow.complete(
        audio_path,
        mode="scratchpad",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        incognito=True,
        audio_retention_policy=AudioRetentionPolicy.ALWAYS,
    )
    assert result.incognito
    assert result.requires_acceptance
    assert result.history_entry is None
    assert result.retained_audio_path is None
    assert not audio_path.exists()
    assert workflow.history.recent() == []


def test_always_policy_retains_successful_dictation_audio(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Retain normal successful audio only when the frozen policy is Always."""

    def fake_delivery(_text: str, auto_paste: bool) -> DeliveryReceipt:
        """Avoid changing the real desktop clipboard during the workflow test."""
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "always.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-always")
    workflow = make_workflow(tmp_path, scribe_server)
    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        audio_retention_policy=AudioRetentionPolicy.ALWAYS,
    )
    assert result.history_entry is not None
    assert result.history_entry.retained_audio_path == str(audio_path)
    assert result.retained_audio_path == audio_path
    assert audio_path.exists()


def test_incognito_dictation_delivers_without_history_or_recovery_audio(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Allow cloud recognition and delivery while suppressing every local session artifact."""

    def fake_delivery(_text: str, auto_paste: bool) -> DeliveryReceipt:
        """Avoid changing the real desktop clipboard during the Incognito test."""
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "private-dictation.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-private-dictation")
    workflow = make_workflow(tmp_path, scribe_server)
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow.diagnostics = diagnostics
    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        incognito=True,
        audio_retention_policy=AudioRetentionPolicy.ALWAYS,
    )
    assert result.delivery.copied
    assert result.history_entry is None
    assert result.retained_audio_path is None
    assert not audio_path.exists()
    assert workflow.history.recent() == []
    assert diagnostics.recent() == []


def test_history_failure_after_delivery_does_not_invite_duplicate_retry(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Return a successful receipt with a warning when only post-delivery persistence fails."""

    def fake_delivery(_text: str, auto_paste: bool) -> DeliveryReceipt:
        """Represent an already completed external delivery."""
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    def fail_history_add(*_args: object, **_kwargs: object) -> None:
        """Fail only the local persistence stage after delivery completed."""
        raise OSError("history disk unavailable")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    monkeypatch.setattr(HistoryStore, "add", fail_history_add)
    audio_path = tmp_path / "recordings" / "history-failure.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-history-failure")
    workflow = make_workflow(tmp_path, scribe_server)
    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        audio_retention_policy=AudioRetentionPolicy.ALWAYS,
    )
    assert result.delivery.copied
    assert "Local history could not be saved" in result.delivery.guidance
    assert result.history_entry is None
    assert result.retained_audio_path is None
    assert not audio_path.exists()


def test_incognito_rejects_codex_before_processing_and_erases_audio(tmp_path: Path) -> None:
    """Fail closed when Command or cleanup cannot guarantee an ephemeral Codex session."""
    audio_path = tmp_path / "recordings" / "private-command.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-private-command")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=ElevenLabsClient(api_key="unused"),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )
    with pytest.raises(ValueError, match="Codex processing is disabled"):
        workflow.complete(
            audio_path,
            mode="command",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            incognito=True,
            audio_retention_policy=AudioRetentionPolicy.ALWAYS,
        )
    assert not audio_path.exists()
    assert history.recent() == []


@pytest.mark.parametrize(
    ("policy", "expected_retained"),
    [
        (AudioRetentionPolicy.NEVER, False),
        (AudioRetentionPolicy.FAILURES, True),
        (AudioRetentionPolicy.ALWAYS, True),
    ],
)
def test_failure_audio_obeys_frozen_retention_policy(
    tmp_path: Path,
    policy: AudioRetentionPolicy,
    expected_retained: bool,
) -> None:
    """Retain failed recognition audio only under Failures or Always."""
    audio_path = tmp_path / "recordings" / f"{policy.value}.wav"
    audio_path.parent.mkdir(exist_ok=True)
    audio_path.write_bytes(b"RIFF-failure")
    history = HistoryStore(tmp_path / f"{policy.value}.sqlite3")
    history.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=FailingElevenLabsClient(),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )
    with pytest.raises(RuntimeError, match="deterministic Scribe failure"):
        workflow.complete(
            audio_path,
            mode="dictation",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            audio_retention_policy=policy,
        )
    assert audio_path.exists() is expected_retained
    entries = history.recent()
    assert len(entries) == 1
    assert entries[0].delivery_outcome == "recognition-failed"
    assert entries[0].retained_audio_path == (str(audio_path) if expected_retained else None)
    assert entries[0].audio_retention_policy == policy.value


def test_incognito_recognition_failure_leaves_no_session_artifact(tmp_path: Path) -> None:
    """Override even Always retention when a private Scribe request fails."""
    audio_path = tmp_path / "recordings" / "incognito-failure.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-incognito-failure")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=FailingElevenLabsClient(),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )
    with pytest.raises(WorkflowFailure) as failure:
        workflow.complete(
            audio_path,
            mode="dictation",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            incognito=True,
            audio_retention_policy=AudioRetentionPolicy.ALWAYS,
        )
    assert failure.value.history_entry is None
    assert failure.value.retained_audio_path is None
    assert not audio_path.exists()
    assert history.recent() == []


def test_failed_recognition_retries_to_preview_without_delivery(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Recover retained audio into history without touching the clipboard or inserting text."""

    def forbidden_delivery(_text: str, _auto_paste: bool) -> DeliveryReceipt:
        """Prove that recognition retry stops before the independent delivery boundary."""
        raise AssertionError("retry_recognition must not deliver")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", forbidden_delivery)
    audio_path = tmp_path / "recordings" / "retry.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-retry")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    personalization = PersonalizationStore(tmp_path / "personalization.json")
    personalization.save_dictionary_replacement(
        "Scratchpad",
        "Recovered",
        application_identifier="/usr/bin/code",
    )
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=FailingElevenLabsClient(),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        personalization=personalization,
    )
    with pytest.raises(WorkflowFailure) as failure:
        workflow.complete(
            audio_path,
            mode="dictation",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            audio_retention_policy=AudioRetentionPolicy.FAILURES,
            application_identifier="/usr/bin/code",
        )
    failed_entry = failure.value.history_entry
    assert failed_entry is not None
    assert failed_entry.application_identifier == "/usr/bin/code"
    assert failed_entry.recognition_ms is not None
    workflow.elevenlabs = ElevenLabsClient(
        api_key="write-only-test-key",
        endpoint=f"http://127.0.0.1:{scribe_server.server_port}/v1/speech-to-text",
    )
    recovered = workflow.retry_recognition(failed_entry.identifier)
    assert recovered.raw_text == "Scratchpad thought."
    assert recovered.delivered_text == "Recovered thought."
    assert recovered.delivery_outcome == "retry-ready"
    assert recovered.recognition_route == "scribe-v2-batch-retry"
    assert recovered.recognition_fallback_reason is None
    assert recovered.recognition_ms is not None
    assert recovered.enhancement_provider_id is None
    assert recovered.enhancement_model_identifier is None
    assert recovered.enhancement_context_sources == ()
    assert recovered.enhancement_outcome is None
    assert recovered.enhancement_ms is None
    assert recovered.delivery_ms is None
    assert recovered.retained_audio_path == str(audio_path)
    assert audio_path.exists()
    delivered = history.mark_delivered(recovered.identifier, recovered.delivered_text, "copied-after-retry")
    assert delivered.retained_audio_path is None
    assert not audio_path.exists()


def test_delivery_failure_preserves_recognized_text_for_independent_retry(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Persist final text and failure audio when only the desktop delivery boundary fails."""

    def fail_delivery(_text: str, auto_paste: bool) -> DeliveryReceipt:
        """Represent a clipboard installation failure after recognition completed."""
        assert not auto_paste
        raise OSError("clipboard unavailable")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fail_delivery)
    audio_path = tmp_path / "recordings" / "delivery-failure.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-delivery-failure")
    workflow = make_workflow(tmp_path, scribe_server)
    with pytest.raises(WorkflowFailure) as failure:
        workflow.complete(
            audio_path,
            mode="dictation",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            audio_retention_policy=AudioRetentionPolicy.FAILURES,
        )
    entry = failure.value.history_entry
    assert entry is not None
    assert entry.raw_text == "Scratchpad thought."
    assert entry.delivered_text == "Scratchpad thought."
    assert entry.delivery_outcome == "delivery-failed"
    assert entry.retained_audio_path == str(audio_path)
    assert audio_path.exists()


def test_incognito_delivery_failure_keeps_text_only_in_memory(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Expose recognized text to the active UI while persisting no private session artifact."""

    def fail_delivery(_text: str, auto_paste: bool) -> DeliveryReceipt:
        """Fail the desktop boundary after cloud recognition completes."""
        assert not auto_paste
        raise OSError("clipboard unavailable")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fail_delivery)
    audio_path = tmp_path / "recordings" / "private-delivery-failure.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-private-delivery-failure")
    workflow = make_workflow(tmp_path, scribe_server)
    with pytest.raises(WorkflowFailure) as failure:
        workflow.complete(
            audio_path,
            mode="dictation",
            use_codex_cleanup=False,
            allow_auto_paste=False,
            incognito=True,
            audio_retention_policy=AudioRetentionPolicy.ALWAYS,
        )
    assert failure.value.output_text == "Scratchpad thought."
    assert failure.value.history_entry is None
    assert failure.value.retained_audio_path is None
    assert not audio_path.exists()
    assert workflow.history.recent() == []


def test_cleanup_failure_falls_back_to_raw_before_delivery(
    tmp_path: Path,
    scribe_server: ThreadingHTTPServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Prefer immutable recognition over failing the session when optional Codex cleanup fails."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture the exact raw fallback without changing the desktop clipboard."""
        assert text == "Scratchpad thought."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "cleanup-failure.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-cleanup-failure")
    workflow = make_workflow(tmp_path, scribe_server)
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow.diagnostics = diagnostics
    workflow.codex = CodexAppServerClient(command=("mluva-command-that-does-not-exist",))
    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=True,
        allow_auto_paste=False,
        codex_model_identifier="gpt-5.4-test",
    )
    assert result.output_text == "Scratchpad thought."
    assert "Codex cleanup failed" in result.delivery.guidance
    assert result.history_entry is not None
    assert result.history_entry.raw_text == result.history_entry.delivered_text
    assert result.history_entry.enhancement_provider_id == "codex-app-server"
    assert result.history_entry.enhancement_model_identifier == "gpt-5.4-test"
    assert result.history_entry.enhancement_context_sources == ()
    assert result.history_entry.enhancement_outcome == "raw-fallback"
    assert not audio_path.exists()
    enhancement_event = next(event for event in diagnostics.recent() if event.stage == "enhancement")
    assert enhancement_event.provider == "codex-app-server"
    assert enhancement_event.outcome == "raw-fallback"


def test_committed_realtime_result_skips_batch_upload_and_remains_raw_history(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Route provider-committed realtime text through processing and delivery exactly once."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Capture the committed result without touching the desktop."""
        assert text == "Committed realtime text."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "realtime.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-realtime")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=ForbiddenTranscriptionClient(),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        diagnostics=diagnostics,
    )
    transcription = TranscriptionResult(
        text="Committed realtime text.",
        language_code="eng",
        language_probability=None,
        transcription_id="realtime-session",
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        session_identifier="05e93756-6aee-43ce-bbf6-d4aa110ed333",
        recognized_transcription=transcription,
        recognition_duration_seconds=0.125,
    )

    assert result.transcription is transcription
    assert result.recognition_ms == 125
    assert not result.recognition_fallback
    assert result.recognition_route == "scribe-v2-realtime"
    assert result.recognition_fallback_reason is None
    assert result.history_entry is not None
    assert result.history_entry.raw_text == "Committed realtime text."
    assert result.history_entry.recognition_route == "scribe-v2-realtime"
    recognition_event = next(event for event in diagnostics.recent() if event.stage == "recognition")
    assert recognition_event.outcome == "completed"


def test_realtime_failure_runs_one_explicit_batch_fallback(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Expose the degraded route while retaining the normal batch result and diagnostics."""

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Return a deterministic receipt without changing the desktop."""
        assert text == "Batch recovery text."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "batch-fallback.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-batch-fallback")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(),
        elevenlabs=StaticTranscriptionClient("Batch recovery text."),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        diagnostics=diagnostics,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=False,
        session_identifier="9bb3e41f-56d4-4275-b722-c17568363164",
        recognition_used_batch_fallback=True,
        recognition_fallback_reason="realtime-stream-failed",
    )

    assert result.recognition_fallback
    assert result.recognition_route == "scribe-v2-batch"
    assert result.recognition_fallback_reason == "realtime-stream-failed"
    assert result.history_entry is not None
    assert result.history_entry.recognition_route == "scribe-v2-batch"
    assert result.history_entry.recognition_fallback_reason == "realtime-stream-failed"
    assert "completed with batch Scribe v2" in result.delivery.guidance
    recognition_event = next(event for event in diagnostics.recent() if event.stage == "recognition")
    assert recognition_event.outcome == "safe-fallback"


def test_dictation_restores_and_confirms_captured_target_immediately_before_paste(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Keep target restoration inside the final delivery boundary and dispatch only once."""
    target = FakeDeliveryTarget()

    def fake_delivery(
        text: str,
        auto_paste: bool,
        confirm_paste: Callable[[], bool | None] | None = None,
        insert_directly: Callable[[str], bool | None] | None = None,
        authorize_keyboard_paste: Callable[[], bool] | None = None,
    ) -> DeliveryReceipt:
        """Prove restoration precedes the single confirmed delivery call."""
        assert text == "Restored target text."
        assert auto_paste
        assert target.restore_calls == 1
        assert insert_directly is not None and insert_directly(text) is True
        assert authorize_keyboard_paste is not None
        assert confirm_paste is not None and confirm_paste() is True
        return DeliveryReceipt(
            copied=True,
            pasted=True,
            guidance="Inserted in test.",
            paste_dispatched=True,
            paste_confirmed=True,
        )

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "restored-target.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-restored-target")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(auto_paste=True),
        elevenlabs=StaticTranscriptionClient("Restored target text."),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=True,
        delivery_target=target,
    )

    assert target.restore_calls == 1
    assert target.inserted_text == ["Restored target text."]
    assert target.confirmed_text == ["Restored target text."]
    assert result.delivery.pasted
    assert result.history_entry is not None
    assert result.history_entry.delivery_outcome == "pasted"


def test_missing_or_stale_target_degrades_to_copy_without_paste_attempt(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Never inject into whichever application happens to be focused after target restoration fails."""
    target = FakeDeliveryTarget(restore_succeeds=False)

    def fake_delivery(text: str, auto_paste: bool) -> DeliveryReceipt:
        """Require copy-only behavior after the captured target rejects restoration."""
        assert text == "Copy-only recovery."
        assert not auto_paste
        return DeliveryReceipt(copied=True, pasted=False, guidance="Copied in test.")

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "stale-target.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-stale-target")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(auto_paste=True),
        elevenlabs=StaticTranscriptionClient("Copy-only recovery."),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=True,
        delivery_target=target,
    )

    assert target.restore_calls == 1
    assert not result.delivery.paste_dispatched
    assert "captured text target could not be restored" in result.delivery.guidance
    assert result.history_entry is not None
    assert result.history_entry.delivery_outcome == "copied"


def test_unconfirmed_target_records_safe_fallback_without_retrying_paste(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Persist an honest unconfirmed receipt while treating the one dispatch as terminal."""
    target = FakeDeliveryTarget(confirmation=False)
    delivery_calls = 0

    def fake_delivery(
        text: str,
        auto_paste: bool,
        confirm_paste: Callable[[], bool | None] | None = None,
        insert_directly: Callable[[str], bool | None] | None = None,
        authorize_keyboard_paste: Callable[[], bool] | None = None,
    ) -> DeliveryReceipt:
        """Return one uncertain receipt after consulting the content-free target callback."""
        nonlocal delivery_calls
        delivery_calls += 1
        assert text == "One dispatch only."
        assert auto_paste
        assert insert_directly is not None and insert_directly(text) is False
        assert authorize_keyboard_paste is not None
        assert confirm_paste is not None and confirm_paste() is False
        return DeliveryReceipt(
            copied=True,
            pasted=False,
            guidance="Paste sent once in test.",
            paste_dispatched=True,
            paste_confirmed=False,
        )

    monkeypatch.setattr("voice_scribe_linux.workflow.deliver_text", fake_delivery)
    audio_path = tmp_path / "recordings" / "unconfirmed-target.wav"
    audio_path.parent.mkdir()
    audio_path.write_bytes(b"RIFF-unconfirmed-target")
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    diagnostics = DiagnosticsStore(tmp_path / "diagnostics.sqlite3")
    diagnostics.initialize()
    workflow = DictationWorkflow(
        config=AppConfig(auto_paste=True),
        elevenlabs=StaticTranscriptionClient("One dispatch only."),
        codex=CodexAppServerClient(command=("must-not-start",)),
        history=history,
        cwd=tmp_path,
        diagnostics=diagnostics,
    )

    result = workflow.complete(
        audio_path,
        mode="dictation",
        use_codex_cleanup=False,
        allow_auto_paste=True,
        session_identifier="c337a32c-f3ae-4479-80a4-c48853f257fd",
        delivery_target=target,
    )

    assert delivery_calls == 1
    assert target.inserted_text == ["One dispatch only."]
    assert result.history_entry is not None
    assert result.history_entry.delivery_outcome == "paste-unconfirmed"
    delivery_event = next(event for event in diagnostics.recent() if event.stage == "delivery")
    assert delivery_event.outcome == "safe-fallback"
