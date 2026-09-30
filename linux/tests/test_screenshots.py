"""Image ownership, privacy, editing and retention behavior for narration screenshots."""

import sqlite3
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pytest
from screenshot_fixture import png

from mluva_linux.conversation import ConversationStore
from mluva_linux.history import HistoryStore
from mluva_linux.screenshots import MAX_SCREENSHOTS, ImageInput, ScreenshotStore, validate_images


def test_capture_images_follow_the_saved_conversation_through_merge_and_delete(tmp_path: Path) -> None:
    """Preserve visual context after recording, then remove its files with the merged conversation."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    conversations = ConversationStore(history)
    conversations.initialize()
    images = ScreenshotStore(history.path)
    screenshot = images.add("capture-one", png(), capture=True, captured_after_seconds=12.5)
    first = history.add("first", "first", "dictation", "eng", None, "copied")
    images.bind_capture("capture-one", first.identifier)
    second = history.add("second", "second", "dictation", "eng", None, "copied")
    next_image = images.add(second.identifier, png(90))

    conversations.merge(first.identifier, second.identifier)

    assert [image.data for image in images.snapshot(first.identifier)] == [png(), png(90)]
    assert images.snapshot(first.identifier)[0].captured_after_seconds == 12.5
    assert images.recent(second.identifier) == []
    history.delete(first.identifier)
    assert not screenshot.path.exists()
    assert not next_image.path.exists()
    assert images.recent(first.identifier) == []


def test_provider_snapshot_is_immutable_but_next_request_sees_saved_annotations(tmp_path: Path) -> None:
    """Keep an in-flight image stable while later requests receive the editor's newest PNG."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    entry = history.add("describe this", "describe this", "dictation", "eng", None, "copied")
    images = ScreenshotStore(history.path)
    screenshot = images.add(entry.identifier, png())
    frozen = images.snapshot(entry.identifier)
    screenshot.path.write_bytes(png(170))
    assert frozen[0].data == png()
    assert images.snapshot(entry.identifier)[0].data == png(170)
    screenshot.path.write_bytes(png(170)[:-9])
    with pytest.raises(ValueError, match="incomplete"):
        images.snapshot(entry.identifier)


def test_expired_conversation_erases_images_but_active_capture_is_preserved(tmp_path: Path) -> None:
    """History retention includes images and does not erase a screenshot in the current narration."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    images = ScreenshotStore(history.path)
    entry = history.add("old", "old", "dictation", "eng", None, "copied")
    saved = images.add(entry.identifier, png())
    active = images.add("capture-two", png(90), capture=True)
    now = datetime.now(UTC)
    with sqlite3.connect(history.path) as connection:
        connection.execute(
            "UPDATE transcription_history SET created_at = ? WHERE identifier = ?",
            ((now - timedelta(days=30)).isoformat(), entry.identifier),
        )
    assert history.prune_older_than(7, now=now) == 1
    assert not saved.path.exists()
    assert active.path.exists()
    images.delete_owner("capture-two", capture=True)
    assert not active.path.exists()


def test_invalid_image_or_deleted_owner_leaves_no_attachment(tmp_path: Path) -> None:
    """Refuse damaged captures and late picker results after their conversation was deleted."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    images = ScreenshotStore(history.path)
    with pytest.raises(ValueError, match="PNG"):
        images.add("capture", b"not an image", capture=True)
    with pytest.raises(ValueError, match="no longer exists"):
        images.add("deleted", png())
    assert not images.directory.exists() or not list(images.directory.iterdir())


def test_image_budget_and_external_symlink_fail_before_provider_input(tmp_path: Path) -> None:
    """Prevent an excessive request or a replaced attachment from uploading unrelated local content."""
    with pytest.raises(ValueError, match="Too many"):
        validate_images((ImageInput(png()),) * (MAX_SCREENSHOTS + 1))
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    images = ScreenshotStore(history.path)
    screenshot = images.add("capture", png(), capture=True)
    outside = tmp_path / "unrelated.png"
    outside.write_bytes(png(220))
    screenshot.path.unlink()
    screenshot.path.symlink_to(outside)
    with pytest.raises(ValueError, match="outside"):
        images.snapshot("capture", capture=True)
    assert outside.read_bytes() == png(220)
    images.delete_owner("capture", capture=True)
    assert outside.read_bytes() == png(220)
    assert not screenshot.path.is_symlink()
    assert not images.pending_captures()
