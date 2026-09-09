"""Guard immediate provisional drafts, bounded updates and mandatory committed final reconciliation."""

import json
from dataclasses import replace
from types import SimpleNamespace
from unittest.mock import patch

from voice_scribe_linux.app import MluvaApplication, _smooth_motion_enabled
from voice_scribe_linux.config import AppConfig
from voice_scribe_linux.live_rewrite import LiveRewriteSchedule, live_prompt
from voice_scribe_linux.realtime import RealtimePreview


def test_first_phrase_starts_once_without_waiting_for_the_later_character_threshold():
    """Opening filler waits for a short phrase, independent of a conservative saved later threshold."""
    schedule = LiveRewriteSchedule(4000, 60)
    assert schedule.take("  ", 0) is None
    assert schedule.take("Okay, uh", 0) is None
    assert schedule.take("x" * 39, 1) is None
    assert schedule.take("x" * 40, 2) == "x" * 40
    assert all(schedule.take("Go " + "x" * size, size) is None for size in range(1, 200))
    schedule.finish(True)
    assert schedule.take("Go " + "x" * 199, 199) is None
    assert schedule.take("Go " + "x" * 199, 259) is not None
    schedule.finish(True)
    assert schedule.take("Go " + "x" * 199, 1000) is None


def test_short_initial_utterance_starts_after_a_pause_or_immediately_at_stop():
    """Short speech cannot wait forever for 40 characters, and Stop never waits for the pause."""
    schedule = LiveRewriteSchedule(160, 4)
    assert schedule.take("Go", 0) is None
    assert schedule.take("Go now", 1) is None
    assert schedule.take("Go now", 4.9) is None
    assert schedule.take("Go now", 5) == "Go now"
    other = LiveRewriteSchedule(160, 4)
    assert other.take("Go", 0) is None
    assert other.take("Go", 0.1, final=True) == "Go"


def test_short_tail_and_equal_length_correction_flush_after_a_pause():
    """Small additions and batch corrections cannot wait indefinitely for character growth."""
    schedule = LiveRewriteSchedule(160, 4)
    assert schedule.take("Ship", -4) is None
    assert schedule.take("Ship", 0) == "Ship"
    schedule.finish(True)
    assert schedule.take("Ship today", 1) is None
    assert schedule.take("Ship today", 4) is None
    assert schedule.take("Ship today", 5) == "Ship today"
    schedule.finish(True)
    assert schedule.take("Ship later", 6) is None
    assert schedule.take("Ship later", 10) == "Ship later"


def test_manual_edit_retry_cannot_bypass_the_request_interval():
    """An invalidated draft stays rate-limited even when its last accepted text is cleared."""
    schedule = LiveRewriteSchedule(40, 4)
    assert schedule.take("x" * 200, 0)
    schedule.finish(True)
    schedule.last_text = ""
    assert schedule.take("x" * 200, 0.1) is None
    assert schedule.take("x" * 200, 4)
    assert schedule.take("Final words", 4.1, final=True) is None
    schedule.finish(True)
    assert schedule.take("Final words", 4.2, final=True) == "Final words"
    schedule.finish(True)
    assert schedule.take("Final words", 4.3, final=True) is None


def test_provider_failure_stops_automatic_retries():
    """A bad provider cannot produce an automatic request storm while more speech arrives."""
    schedule = LiveRewriteSchedule(40, 4)
    assert schedule.take("Start the task with the requirements supplied so far.", 0)
    schedule.finish(False)
    assert schedule.take("More speech " * 100, 90) is None
    assert schedule.take("Final speech", 100, final=True) is None


def test_preview_callback_queues_changed_provisional_text_only_for_the_active_session():
    """Early words can feed Live, while duplicate, cancelled and finalizing callbacks cannot start work."""
    queued = []
    requested = []
    app = SimpleNamespace(
        live_session_identifier="capture",
        capture_processing=False,
        live_final_entry=None,
        _live_preview_arrived=object(),
        _maybe_live_rewrite=requested.append,
    )
    callback = MluvaApplication._live_preview_callback(app, "capture")
    with patch("voice_scribe_linux.app.GLib.idle_add", side_effect=lambda *args: queued.append(args)):
        callback(RealtimePreview("", "Early provisional words"))
        callback(RealtimePreview("Committed words", "Volatile words"))
        callback(RealtimePreview("Committed words", "Changed volatile words"))
        callback(RealtimePreview("Committed words", "Changed volatile words"))
        assert [args[-1] for args in queued] == [
            "Early provisional words",
            "Committed words Volatile words",
            "Committed words Changed volatile words",
        ]
        app.live_session_identifier = "new-capture"
        callback(RealtimePreview("Late committed words", ""))
        assert len(queued) == 3
        MluvaApplication._live_preview_arrived(app, "capture", "Committed words")
        assert requested == []
        app.live_session_identifier = "capture"
        app.capture_processing = True
        MluvaApplication._live_preview_arrived(app, "capture", "Committed words")
        assert requested == []
        app.capture_processing = False
        MluvaApplication._live_preview_arrived(app, "capture", "Committed words")
        assert requested == ["Committed words"]


def test_final_recognition_always_reconciles_even_when_preview_text_matches():
    """A provisional draft never becomes copyable merely because its last text matches the final transcript."""
    schedule = LiveRewriteSchedule(160, 4)
    assert schedule.take("Same words", -4) is None
    assert schedule.take("Same words", 0) == "Same words"
    schedule.finish(True)
    assert schedule.take("Same words", 1, final=True) == "Same words"
    assert schedule.last_final
    schedule.finish(True)
    assert schedule.take("Same words", 2, final=True) is None


def test_prompt_marks_provisional_input_and_final_reconciliation():
    """The model can distinguish unsettled recognition from the canonical text supplied at Stop."""
    for final in (False, True):
        prompt = live_prompt(
            AppConfig(), "Canonical words" if final else "Provisional words", "Manual edit", final=final
        )
        context = json.loads(prompt.split("\n", 1)[1])
        assert context["transcript_status"] == (
            "final committed recognition" if final else "provisional recognition; may change"
        )
        assert "remove facts introduced by earlier recognition errors" in prompt
        assert context["current_draft"] == "Manual edit"


def test_desktop_reduced_motion_and_app_preferences_disable_shell_motion():
    """The QML bridge receives the same effective animation preference as the GTK app."""
    config = AppConfig()
    settings = SimpleNamespace(get_property=lambda _name: False)
    with patch("voice_scribe_linux.app.Gtk.Settings.get_default", return_value=settings):
        assert not _smooth_motion_enabled(config)
    settings.get_property = lambda _name: True
    with patch("voice_scribe_linux.app.Gtk.Settings.get_default", return_value=settings):
        assert _smooth_motion_enabled(config)
        assert not _smooth_motion_enabled(replace(config, smooth_scrolling=False))
        assert not _smooth_motion_enabled(replace(config, scroll_duration_ms=0))
