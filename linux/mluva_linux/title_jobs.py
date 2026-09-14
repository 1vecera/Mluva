"""Bounded conversation-title jobs with explicit main-thread dispatch and no GTK dependency."""

import sqlite3
import threading
from collections import deque
from collections.abc import Callable
from pathlib import Path

from mluva_linux.config import AppConfig
from mluva_linux.conversation_titles import clean_title, fallback_title, save_generated_title, title_prompt
from mluva_linux.history import HistoryEntry, HistoryStore
from mluva_linux.rewriting import RewriteClient, rewrite_client

MAX_PENDING_TITLES = 20


class ConversationTitleJobs:
    """Own queued requests and stale-result checks; the application owns navigation and settings."""

    def __init__(
        self,
        history: HistoryStore,
        workspace: Path,
        get_config: Callable[[], AppConfig],
        get_instructions: Callable[[], str],
        changed: Callable[[str], None],
        dispatch: Callable[[Callable[[], None]], object],
    ) -> None:
        """Inject stores and main-thread callbacks without giving this feature the application object."""
        self.history = history
        self.workspace = workspace
        self.get_config = get_config
        self.get_instructions = get_instructions
        self.changed = changed
        self.dispatch = dispatch
        self.pending: deque[tuple[str, str]] = deque()
        self.client: RewriteClient | None = None
        self.closed = False

    def enqueue(self, entry: HistoryEntry) -> None:
        """Write a local fallback first, then queue at most one model request per new conversation."""
        config = self.get_config()
        if self.closed or config.incognito_mode or entry.title or not entry.raw_text.strip():
            return
        fallback = fallback_title(entry.raw_text)
        try:
            saved = save_generated_title(self.history, entry.identifier, fallback)
        except sqlite3.Error:
            return
        if not saved:
            return
        self.changed(entry.identifier)
        if config.automatic_titles and len(self.pending) < MAX_PENDING_TITLES:
            self.pending.append((entry.identifier, fallback))
            self._start_next()

    def _start_next(self) -> None:
        """Skip deleted or renamed notes and freeze provider settings and prompt before dispatch."""
        config = self.get_config()
        if self.client is not None or self.closed or config.incognito_mode or not config.automatic_titles:
            return
        while self.pending:
            identifier, fallback = self.pending.popleft()
            try:
                entry = self.history.find(identifier)
            except KeyError:
                continue
            if entry.title != fallback:
                continue
            try:
                prompt = title_prompt(entry, self.get_instructions())
                client = rewrite_client(config, request_timeout_seconds=10, turn_timeout_seconds=20)
            except Exception:
                continue
            self.client = client
            model = config.litellm_model if config.rewrite_provider == "litellm" else config.codex_model
            threading.Thread(
                target=self._run,
                args=(client, identifier, fallback, prompt, model),
                name="conversation-title",
                daemon=True,
            ).start()
            break

    def _run(self, client: RewriteClient, identifier: str, fallback: str, prompt: str, model: str | None) -> None:
        """Run only the transport on a worker; schedule every durable write back on the owner thread."""
        title = None
        try:
            resolved = client.resolve_model(model)
            title = clean_title(client.transform(prompt, self.workspace, resolved, max_output_characters=128))
        except Exception:
            pass
        finally:
            try:
                client.close()
            except Exception:
                title = None
            self.dispatch(lambda: self._finished(client, identifier, fallback, title))

    def _finished(self, client: RewriteClient, identifier: str, fallback: str, title: str | None) -> None:
        """Preserve cancellation, privacy, deletion and manual naming at the commit boundary."""
        if client is not self.client:
            return
        self.client = None
        config = self.get_config()
        if self.closed or config.incognito_mode or not config.automatic_titles:
            return
        try:
            if title and save_generated_title(self.history, identifier, title, expected=fallback):
                self.changed(identifier)
        except sqlite3.Error:
            pass
        self._start_next()

    def cancel(self, *, wait: bool = False) -> None:
        """Invalidate callbacks before stopping the exact worker; never replay the cleared queue."""
        self.pending.clear()
        client, self.client = self.client, None
        if client is not None:
            if wait:
                client.cancel()
            else:
                threading.Thread(target=client.cancel, name="cancel-title", daemon=True).start()

    def close(self) -> None:
        """Permanently reject new work and late callbacks during application shutdown."""
        self.closed = True
        self.cancel(wait=True)
