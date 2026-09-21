"""Verify private creation and atomic replacement under permissive user defaults."""

import os
from pathlib import Path

import pytest

from mluva_linux.private_files import atomic_write_private_text
from mluva_linux.scratchpad import ScratchpadDraft, ScratchpadDraftStore


def test_scratchpad_is_private_before_content_is_written(tmp_path, monkeypatch):
    """Observe the actual open descriptor before writing, including an upgraded 0755 parent."""
    directory = tmp_path / "mluva"
    directory.mkdir(mode=0o755)
    original = os.fdopen
    observed = []

    def inspect(descriptor, *args, **kwargs):
        observed.append((os.fstat(descriptor).st_mode & 0o777, os.fstat(descriptor).st_size))
        assert directory.stat().st_mode & 0o777 == 0o700
        return original(descriptor, *args, **kwargs)

    monkeypatch.setattr(os, "fdopen", inspect)
    previous = os.umask(0o022)
    try:
        path = directory / "scratchpad.json"
        draft = ScratchpadDraft("fixture", None, "2026-09-21", "private raw", "private edit", None)
        ScratchpadDraftStore(path).save(draft)
    finally:
        os.umask(previous)
    assert observed == [(0o600, 0)]
    assert ScratchpadDraftStore(path).draft == draft


def test_predictable_temporary_symlink_cannot_overwrite_another_file(tmp_path):
    """A stale name from the old persistence scheme cannot redirect the new write."""
    outside = tmp_path / "unrelated"
    outside.write_text("preserve me")
    path = tmp_path / "private" / "scratchpad.json"
    path.parent.mkdir()
    path.with_suffix(".tmp").symlink_to(outside)
    atomic_write_private_text(path, "new private content")
    assert outside.read_text() == "preserve me"
    assert path.read_text() == "new private content"
    assert path.stat().st_mode & 0o777 == 0o600


def test_failed_replace_preserves_original_and_removes_partial(tmp_path, monkeypatch):
    """A failed commit cannot lose the last saved draft or leave sensitive temporary text."""
    path = tmp_path / "draft.json"
    path.write_text("previous")

    def fail(*_args):
        raise OSError("synthetic failure")

    monkeypatch.setattr(Path, "replace", fail)
    with pytest.raises(OSError, match="synthetic failure"):
        atomic_write_private_text(path, "new private content")
    assert path.read_text() == "previous"
    assert list(tmp_path.iterdir()) == [path]
