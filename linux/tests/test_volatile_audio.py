"""Prove that private dictation and meeting staging disappears after SIGKILL."""

import json
import os
import selectors
import signal
import subprocess
import sys
import time
from pathlib import Path
from unittest.mock import MagicMock

import pytest

import mluva_linux.app as app_module
import mluva_linux.volatile_audio as volatile_module
from mluva_linux.app import MluvaApplication
from mluva_linux.config import AppConfig, AudioRetentionPolicy
from mluva_linux.volatile_audio import VolatileAudioStore, _memory_backed


@pytest.mark.parametrize("meeting", [False, True])
def test_crash_removes_real_recorder_files_without_restart(tmp_path, meeting):
    """Kill only an isolated fixture group; the independent janitor removes every audio source."""
    code = """
import json, sys, time
from pathlib import Path
from mluva_linux.audio import PipeWireRecorder, PipeWireMeetingRecorder
from mluva_linux.volatile_audio import volatile_audio_directory
directory = volatile_audio_directory()
recorder = (PipeWireMeetingRecorder if sys.argv[2] == 'meeting' else PipeWireRecorder)(sys.argv[1])
recorder.start(directory / 'capture.wav')
while not any(p.stat().st_size > 44 for p in directory.glob('*.wav')):
    time.sleep(.01)
print(json.dumps(str(directory)), flush=True)
sys.stdin.read()
"""
    child = subprocess.Popen(
        [
            sys.executable,
            "-c",
            code,
            str(Path(__file__).with_name("fake_meeting_pw_record.py" if meeting else "fake_pw_record.py")),
            "meeting" if meeting else "dictation",
        ],
        cwd=Path(__file__).parents[1],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )
    directory = None
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            assert selector.select(timeout=10), "Synthetic capture never became ready"
        line = child.stdout.readline()
        assert line, child.stderr.read()
        directory = Path(json.loads(line))
        assert _memory_backed(directory)
        assert directory.stat().st_mode & 0o777 == 0o700
        assert all(p.stat().st_mode & 0o777 == 0o600 for p in directory.glob("*.wav"))
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=5)
        deadline = time.monotonic() + 5
        while directory.exists() and time.monotonic() < deadline:
            time.sleep(0.02)
        assert not directory.exists()
        assert not list(tmp_path.rglob("*.wav"))
    finally:
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=5)
        for stream in (child.stdin, child.stdout, child.stderr):
            stream.close()
        if directory is not None and directory.exists():
            import shutil

            shutil.rmtree(directory)


def test_refuses_disk_backed_staging(monkeypatch):
    """Do not silently fall back to persistent XDG data, runtime or temporary storage."""
    monkeypatch.setattr(volatile_module, "_memory_backed", lambda _path: False)
    with pytest.raises(OSError, match="memory-backed"):
        VolatileAudioStore()


@pytest.mark.parametrize("incognito", [False, True])
@pytest.mark.parametrize("meeting", [False, True])
def test_capture_selects_private_staging_before_recorder_start(tmp_path, monkeypatch, incognito, meeting):
    """Exercise both production capture entrypoints without opening devices or a desktop session."""
    context = MagicMock()
    for name in (
        "capture_preparing",
        "capture_processing",
        "meeting_processing",
        "meeting_retry_in_progress",
        "retry_in_progress",
        "capture_allows_auto_paste",
    ):
        setattr(context, name, False)
    context._meeting_capture_active.return_value = False
    context.pending_command_result = None
    context.scratchpad_store.draft = None
    context.recorder.process = None
    context.config = AppConfig(incognito_mode=incognito, rewrite_provider="none")
    context.data_directory = tmp_path / "durable"
    context.meeting_store.persistence_error = None
    context.mode.get_selected.return_value = 0
    context._capture_mode_for_application.return_value = "dictation"
    context.incognito_switch.get_active.return_value = incognito
    context._selected_audio_retention.return_value = AudioRetentionPolicy.NEVER
    context.segment_cleanup_session = None
    monkeypatch.setattr(app_module, "volatile_audio_directory", lambda: tmp_path / "volatile")
    monkeypatch.setattr(app_module, "set_button_content", lambda *_args: None)
    monkeypatch.setattr(app_module.threading, "Thread", MagicMock())
    monkeypatch.setattr(app_module.GLib, "timeout_add", lambda *_args: 0)
    if meeting:
        MluvaApplication._start_meeting_capture(context)
        path = context.meeting_recorder.start.call_args.args[0]
    else:
        MluvaApplication._start_capture(context)
        path = context.audio_path
    assert path.is_relative_to(tmp_path / ("volatile" if incognito else "durable"))
