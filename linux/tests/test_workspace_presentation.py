"""Protect live question/source boundaries, local diagrams and persisted presentation choices."""

import json
import threading
import wave
from dataclasses import replace

import pytest

from mluva_linux.batch_preview import BatchPreviewClient
from mluva_linux.config import AppConfig, load_config, save_config
from mluva_linux.display_time import history_timestamp
from mluva_linux.elevenlabs import TranscriptionResult
from mluva_linux.live_rewrite import initial_draft, live_prompt, split_grilling_draft
from mluva_linux.mermaid_view import mermaid_blocks, native_svg


def test_grilling_starts_empty_and_preserves_question_and_architecture_source():
    """Avoid empty form fields and make the pinned question split perfectly reversible."""
    config = AppConfig()
    assert config.live_rewrite_template == "grilling"
    assert initial_draft(config) == ""
    source = "## Questions\n- Who will use this?\n\n## Architecture\n### Intent\nA useful prototype.\n"
    questions, architecture = split_grilling_draft(source)
    assert "Who will use this?" in questions
    assert architecture.startswith("## Architecture")
    assert questions + architecture == source
    assert split_grilling_draft("## Questions\n- Who is it for?\n") == ("## Questions\n- Who is it for?\n", "")
    assert split_grilling_draft("## Architecture\nThe question is settled.") == (
        "",
        "## Architecture\nThe question is settled.",
    )
    context = json.loads(live_prompt(config, "Only for the support team.", source, final=True).split("\n", 1)[1])
    assert context["current_draft"] == source
    assert context["transcript_status"] == "final committed recognition"
    assert "remove questions answered anywhere" in context["instructions"]
    assert "hide empty fields" in context["instructions"]


def test_presentation_settings_roundtrip_and_validate(tmp_path):
    """Persist the same choices used by the widget, timestamps, sidebar and command palette."""
    config = replace(AppConfig(), widget_position="bottom-right", time_format="12h", history_sidebar_visible=True)
    path = tmp_path / "config.json"
    save_config(config, path)
    assert load_config(path) == config
    for change in ({"widget_position": "run arbitrary command"}, {"time_format": "unknown"}):
        with pytest.raises(ValueError):
            replace(config, **change)


def test_compact_local_dates_use_unpadded_day_and_optional_am_pm():
    """Keep weekday/month names short and distinguish noon from midnight."""
    assert history_timestamp("2026-09-07T14:05:00") == "Mon 7 Sep · 14:05"
    assert history_timestamp("2026-09-07T14:05:00", "12h") == "Mon 7 Sep · 2:05 PM"
    assert history_timestamp("2026-09-07T00:05:00", "12h") == "Mon 7 Sep · 12:05 AM"
    assert history_timestamp("2026-09-17T12:05:00", "12h") == "Thu 17 Sep · 12:05 PM"


def test_diagrams_wait_for_closed_fences_and_leave_unsupported_source_intact():
    """Never turn an unfinished model stream or nested code example into executable diagram input."""
    source = "Český popis\n```mermaid\nflowchart LR\n A --> B\n```\nStill editable.\n"
    start, end, code = mermaid_blocks(source)[0]
    assert source[start:end] == "```mermaid\nflowchart LR\n A --> B\n```\n"
    assert code == "flowchart LR\n A --> B\n"
    assert not mermaid_blocks(source[: end - 4])
    assert not mermaid_blocks("````text\n" + source + "````\n")
    assert not mermaid_blocks("```mermaid\n" + "x" * 12_001 + "\n```\n")
    assert len(mermaid_blocks(source * 8)) == 3
    assert mermaid_blocks("~~~mermaid\nflowchart LR\n A --> B\n~~~\n")[0][2] == code


def test_native_diagrams_cannot_load_resources_outside_the_browser_policy():
    """Retain internal markers but reject external URLs and embedded browser/image content."""
    source = '<svg xmlns="http://www.w3.org/2000/svg"><path marker-end="url(#arrow)"/></svg>'
    assert native_svg(source) == source.encode()
    for content in (
        '<image href="file:///private/example.png"/>',
        '<use href="https://example.invalid/sketch.svg"/>',
        '<style>path {fill:url("https://example.invalid/paint")}</style>',
        "<foreignObject/>",
        "<script/>",
    ):
        with pytest.raises(ValueError):
            native_svg("<svg>" + content + "</svg>")


def test_batch_live_can_start_midway_without_uploading_while_disabled(tmp_path):
    """Include pre-toggle audio and recognize the complete recording once on finalization."""
    calls = []
    preview_started = threading.Event()

    class Speech:
        def transcribe(self, path, *_args):
            with wave.open(str(path)) as audio:
                calls.append(audio.readframes(audio.getnframes()))
            preview_started.set()
            return TranscriptionResult("Synthetic speech", "eng", None, None)

    session = BatchPreviewClient(Speech, tmp_path, 3, preview_enabled=False).start("eng")
    before = b"\x01\x00" * 48_000
    after = b"\x02\x00" * 16_000
    try:
        session.submit_audio(before)
        assert not preview_started.wait(0.05)
        session.set_preview_enabled(True)
        assert preview_started.wait(2)
        session.set_preview_enabled(False)
        session.submit_audio(after)
        result = session.finish()
        assert result.transcription.text == "Synthetic speech"
        assert calls == [before, before + after]
        assert not list(tmp_path.iterdir())
    finally:
        session.cancel()
