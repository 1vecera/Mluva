"""Exercise title scheduling, cancellation and persistence without a GTK application."""

import json
import queue
import threading
from dataclasses import replace

import pytest

from mluva_linux.config import AppConfig
from mluva_linux.history import HistoryStore
from mluva_linux.title_jobs import MAX_PENDING_TITLES, ConversationTitleJobs


class ControlledTitleClient:
    """Hold a request at a deterministic boundary so the owner can edit or cancel it."""

    def __init__(self):
        """Give each request independent completion and resource-release signals."""
        self.release = threading.Event()
        self.closed = threading.Event()
        self.started = threading.Event()
        self.cancelled = False
        self.prompt = ""
        self.model = None

    def resolve_model(self, requested):
        """Record the frozen model passed to this request."""
        self.model = requested
        return requested or "fixture-model"

    def transform(self, prompt, _cwd, _model, *, max_output_characters):
        """Return a title only after the test releases this exact worker."""
        assert max_output_characters == 128
        self.prompt = prompt
        self.started.set()
        assert self.release.wait(2), "Title worker was not released"
        if self.cancelled:
            raise RuntimeError("Cancelled fixture request")
        return "Plán pátečního vydání"

    def close(self):
        """Expose transport teardown independently of main-thread completion."""
        self.closed.set()

    def cancel(self):
        """Unblock the worker without allowing a usable result."""
        self.cancelled = True
        self.release.set()


@pytest.fixture
def harness(tmp_path, monkeypatch):
    """Use real SQLite and workers with an explicitly controlled main-thread callback queue."""
    history = HistoryStore(tmp_path / "history.sqlite3")
    history.initialize()
    config = [AppConfig()]
    instructions = ["Original title instructions"]
    callbacks = queue.Queue()
    clients = []
    changes = []

    def factory(_config, **_options):
        client = ControlledTitleClient()
        clients.append(client)
        return client

    monkeypatch.setattr("mluva_linux.title_jobs.rewrite_client", factory)
    jobs = ConversationTitleJobs(
        history,
        tmp_path,
        lambda: config[0],
        lambda: instructions[0],
        lambda identifier: changes.append((identifier, threading.get_ident())),
        callbacks.put,
    )
    yield jobs, history, config, instructions, callbacks, clients, changes
    jobs.close()
    for client in clients:
        client.release.set()
        assert client.closed.wait(2)


def add_note(history, text="Synthetic source"):
    """Persist a new note with distinct raw and delivered versions."""
    return history.add(text, "Delivered text", "dictation", "eng", None, "copied")


def finish(client, callbacks):
    """Wait for the real worker and then run its queued commit on the caller thread."""
    client.release.set()
    callbacks.get(timeout=2)()


def test_one_request_at_a_time_with_local_labels_and_owner_thread_commits(harness):
    """Duplicate queue calls do not generate twice, and worker completion alone cannot write a title."""
    jobs, history, _config, _instructions, callbacks, clients, changes = harness
    first, second = add_note(history, "First note"), add_note(history, "Second note")
    jobs.enqueue(first)
    jobs.enqueue(first)
    jobs.enqueue(second)
    assert len(clients) == 1 and len(jobs.pending) == 1
    clients[0].release.set()
    assert clients[0].closed.wait(2)
    assert history.find(first.identifier).title == "First note"
    callbacks.get(timeout=2)()
    assert len(clients) == 2
    finish(clients[1], callbacks)
    assert jobs.client is None and not jobs.pending
    assert history.find(first.identifier).title == "Plán pátečního vydání"
    assert history.find(second.identifier).title == "Plán pátečního vydání"
    assert all(thread == threading.get_ident() for _identifier, thread in changes)
    assert history.find(first.identifier).raw_text == first.raw_text
    assert history.find(first.identifier).delivered_text == first.delivered_text


@pytest.mark.parametrize(
    "action", ["rename", "clear", "same-label", "delete", "private", "disabled", "cancel", "close"]
)
def test_late_results_preserve_document_and_privacy_decisions(harness, action):
    """Exercise each commit gate after a real request starts and before its callback runs."""
    jobs, history, config, _instructions, callbacks, clients, _changes = harness
    entry = add_note(history)
    jobs.enqueue(entry)
    client = clients[0]
    assert client.started.wait(2)
    expected = history.find(entry.identifier).title
    if action in {"rename", "clear", "same-label"}:
        expected = {"rename": "Human title", "clear": None, "same-label": expected}[action]
        history.update_title(entry.identifier, expected)
    elif action == "delete":
        history.delete(entry.identifier)
    elif action == "private":
        config[0] = replace(config[0], incognito_mode=True)
    elif action == "disabled":
        config[0] = replace(config[0], automatic_titles=False)
    elif action == "cancel":
        jobs.cancel(wait=True)
    else:
        jobs.close()
    finish(client, callbacks)
    if action == "delete":
        with pytest.raises(KeyError):
            history.find(entry.identifier)
    else:
        current = history.find(entry.identifier)
        assert (current.title, current.raw_text, current.delivered_text) == (
            expected,
            entry.raw_text,
            entry.delivered_text,
        )


def test_cancelled_callback_cannot_clear_or_replace_a_new_request(harness):
    """A replaced job keeps its own identity even when an older callback arrives first."""
    jobs, history, _config, _instructions, callbacks, clients, _changes = harness
    jobs.enqueue(add_note(history, "Old note"))
    old = clients[0]
    jobs.cancel(wait=True)
    jobs.enqueue(add_note(history, "New note"))
    current = clients[1]
    finish(old, callbacks)
    assert jobs.client is current
    finish(current, callbacks)
    assert jobs.client is None


def test_transport_teardown_failure_keeps_the_fallback_and_advances_the_queue(harness, monkeypatch):
    """A close failure must not strand every later title behind a finished request."""
    jobs, history, _config, _instructions, callbacks, clients, _changes = harness
    first, second = add_note(history, "First note"), add_note(history, "Second note")
    jobs.enqueue(first)
    jobs.enqueue(second)
    client = clients[0]

    def fail_close():
        client.closed.set()
        raise OSError("Synthetic transport cleanup failure")

    monkeypatch.setattr(client, "close", fail_close)
    finish(client, callbacks)
    assert history.find(first.identifier).title == "First note"
    assert len(clients) == 2
    finish(clients[1], callbacks)
    assert history.find(second.identifier).title == "Plán pátečního vydání"
    assert jobs.client is None


def test_bounded_queue_keeps_local_labels_for_overflow_and_skips_removed_notes(harness):
    """Bound provider work without losing labels or sending deleted and renamed queued content."""
    jobs, history, _config, _instructions, callbacks, clients, _changes = harness
    entries = [add_note(history, f"Note {index}") for index in range(MAX_PENDING_TITLES + 3)]
    for entry in entries:
        jobs.enqueue(entry)
    assert len(clients) == 1 and len(jobs.pending) == MAX_PENDING_TITLES
    assert all(history.find(entry.identifier).title == entry.raw_text for entry in entries)
    history.update_title(entries[1].identifier, "A human named the queued note")
    for entry in entries[2 : MAX_PENDING_TITLES + 1]:
        history.delete(entry.identifier)
    finish(clients[0], callbacks)
    assert len(clients) == 1 and jobs.client is None and not jobs.pending


def test_request_freezes_prompt_and_model_before_the_worker_starts(harness):
    """Later settings and prompt edits affect the next request, never a running one."""
    jobs, history, config, instructions, callbacks, clients, _changes = harness
    config[0] = replace(config[0], codex_model="first-model")
    jobs.enqueue(add_note(history))
    client = clients[0]
    assert client.started.wait(2)
    config[0] = replace(config[0], codex_model="second-model")
    instructions[0] = "Changed instructions"
    assert client.model == "first-model"
    assert client.prompt.startswith("Original title instructions")
    assert json.loads(client.prompt.split("\n", 1)[1])["transcript_excerpt"] == "Synthetic source"
    finish(client, callbacks)


@pytest.mark.parametrize("private", [False, True])
def test_disabled_generation_never_contacts_a_provider(harness, private):
    """Disabled automatic titles keep a local label; Incognito creates no title at all."""
    jobs, history, config, _instructions, _callbacks, clients, _changes = harness
    config[0] = replace(config[0], automatic_titles=False, incognito_mode=private)
    entry = add_note(history)
    jobs.enqueue(entry)
    assert not clients and not jobs.pending
    assert history.find(entry.identifier).title == (None if private else "Synthetic source")
