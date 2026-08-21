"""Bounded, capture-ordered cleanup for immutable realtime transcript segments."""

from __future__ import annotations

import threading
import time
import uuid
from collections.abc import Callable
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path
from typing import Protocol

from voice_scribe_linux.personalization import integrity_violations

DEFAULT_ATTEMPT_TIMEOUT_SECONDS = 8.0
DEFAULT_CONCURRENCY_LIMIT = 2
DEFAULT_PENDING_CAPACITY = 8
DEFAULT_STOP_DRAIN_TIMEOUT_SECONDS = 2.0
MAX_SEGMENT_CHARACTERS = 8_000
MAX_RESPONSE_CHARACTERS = 8_000

DICTATION_CLEANUP_PROMPT = (
    "Faithfully clean this dictated text. Remove obvious filler and repair punctuation only. "
    "Preserve every fact, number, name, URL, path, identifier, command, and negation. "
    "Return only the cleaned text.\n\nDICTATION:\n{text}"
)


class SegmentCleanupFailure(StrEnum):
    """Name controlled non-content reasons for selecting immutable raw segment text."""

    CANCELLED = "cancelled"
    INPUT_TOO_LARGE = "input-too-large"
    MALFORMED_OUTPUT = "malformed-output"
    OUTPUT_TOO_LARGE = "output-too-large"
    PROCESSING = "processing"
    PROVIDER = "provider"
    SAFETY = "safety"
    SKIPPED_CAPACITY = "skipped-capacity"
    TIMEOUT = "timeout"


class SegmentCleanupState(StrEnum):
    """Expose a display-safe lifecycle without provider payloads or transcript mutation."""

    RAW = "raw"
    REWRITING = "rewriting"
    WAITING = "waiting"
    CLEANED = "cleaned"
    FALLBACK = "fallback"
    CANCELLED = "cancelled"


class SegmentCleanupLifecycle(StrEnum):
    """Prevent late attempts from mutating a stopped or cancelled session."""

    ACTIVE = "active"
    STOPPING = "stopping"
    FINISHED = "finished"
    CANCELLED = "cancelled"


class SegmentCleanupAttempt(Protocol):
    """Describe one isolated, cancellable provider attempt."""

    def transform(self, prepared_text: str) -> str:
        """Return one candidate replacement for a bounded prepared segment."""
        ...

    def close(self) -> None:
        """Cancel or release the isolated provider boundary idempotently."""
        ...


class TextTransformer(Protocol):
    """Describe the Codex client surface used by one isolated segment attempt."""

    def transform(self, prompt: str, cwd: Path, model: str | None = None) -> str:
        """Return one replacement string through the frozen model."""
        ...

    def cancel(self) -> None:
        """Prevent startup or interrupt the isolated app-server child."""
        ...


@dataclass(slots=True)
class CodexSegmentCleanupAttempt:
    """Adapt one isolated Codex app-server client to a faithful cleanup attempt."""

    client: TextTransformer
    cwd: Path
    model_identifier: str

    def transform(self, prepared_text: str) -> str:
        """Submit only the bounded prepared segment and faithful cleanup instruction."""
        return self.client.transform(
            DICTATION_CLEANUP_PROMPT.format(text=prepared_text),
            cwd=self.cwd,
            model=self.model_identifier,
        )

    def close(self) -> None:
        """Close the attempt's independent app-server process."""
        self.client.cancel()


@dataclass(frozen=True, slots=True)
class SegmentCleanupConfiguration:
    """Freeze bounded scheduling and payload policy for one capture."""

    concurrency_limit: int = DEFAULT_CONCURRENCY_LIMIT
    pending_capacity: int = DEFAULT_PENDING_CAPACITY
    attempt_timeout_seconds: float = DEFAULT_ATTEMPT_TIMEOUT_SECONDS
    stop_drain_timeout_seconds: float = DEFAULT_STOP_DRAIN_TIMEOUT_SECONDS
    segment_character_limit: int = MAX_SEGMENT_CHARACTERS
    response_character_limit: int = MAX_RESPONSE_CHARACTERS

    def __post_init__(self) -> None:
        """Reject unusable bounds rather than silently changing a frozen session policy."""
        if self.concurrency_limit <= 0:
            raise ValueError("Cleanup concurrency must be positive.")
        if self.pending_capacity < 0:
            raise ValueError("Cleanup pending capacity cannot be negative.")
        if self.attempt_timeout_seconds <= 0 or self.stop_drain_timeout_seconds <= 0:
            raise ValueError("Cleanup timeouts must be positive.")
        if self.segment_character_limit <= 0 or self.response_character_limit <= 0:
            raise ValueError("Cleanup request and response bounds must be positive.")


@dataclass(frozen=True, slots=True)
class SegmentCleanupProjection:
    """Expose one immutable segment's ordered cleanup state for display or tests."""

    identifier: str
    sequence: int
    raw_text: str
    revised_text: str | None
    state: SegmentCleanupState
    failure: SegmentCleanupFailure | None


@dataclass(frozen=True, slots=True)
class SegmentCleanupTerminalSegment:
    """Select either a validated candidate or the exact immutable raw segment."""

    identifier: str
    sequence: int
    raw_text: str
    selected_text: str
    failure: SegmentCleanupFailure | None


@dataclass(frozen=True, slots=True)
class SegmentCleanupTerminalSnapshot:
    """Return capture-ordered terminal cleanup with controlled provenance."""

    session_identifier: str
    provider_identifier: str
    model_identifier: str
    segments: tuple[SegmentCleanupTerminalSegment, ...]
    stop_drain_seconds: float

    @property
    def raw_text(self) -> str:
        """Join immutable provider segments with the realtime transport's boundary rule."""
        return _join_segments(tuple(segment.raw_text for segment in self.segments))

    @property
    def selected_text(self) -> str:
        """Join validated candidates and exact raw fallbacks in capture order."""
        return _join_segments(tuple(segment.selected_text for segment in self.segments))

    @property
    def successful_segments(self) -> int:
        """Count provider attempts that reached a validated terminal candidate."""
        return sum(segment.failure is None for segment in self.segments)

    @property
    def failed_segments(self) -> int:
        """Count segments that selected immutable raw text for a controlled reason."""
        return sum(segment.failure is not None for segment in self.segments)

    @property
    def enhancement_outcome(self) -> str:
        """Map segment results to the controlled durable History outcomes."""
        if self.failed_segments == 0:
            return "completed"
        if self.successful_segments == 0:
            return "raw-fallback"
        return "safe-fallback"


@dataclass(slots=True)
class _SegmentRecord:
    """Hold mutable state only behind the cleanup session lock."""

    identifier: str
    sequence: int
    raw_text: str
    state: SegmentCleanupState = SegmentCleanupState.RAW
    revised_text: str | None = None
    failure: SegmentCleanupFailure | None = None
    attempt_token: str | None = None
    settled: bool = False


@dataclass(slots=True)
class _ActiveAttempt:
    """Correlate one worker and timeout while allowing cancellation before startup."""

    token: str
    timer: threading.Timer
    attempt: SegmentCleanupAttempt | None = None


class SegmentCleanupSession:
    """Run isolated segment attempts concurrently and publish only an ordered prefix."""

    def __init__(
        self,
        session_identifier: str,
        provider_identifier: str,
        model_identifier: str,
        prepare_text: Callable[[str], str],
        protected_vocabulary: tuple[str, ...],
        attempt_factory: Callable[[], SegmentCleanupAttempt],
        configuration: SegmentCleanupConfiguration | None = None,
    ) -> None:
        """Freeze provider identity, preparation, vocabulary, and scheduling before capture."""
        if not session_identifier or not provider_identifier or not model_identifier:
            raise ValueError("Cleanup session, provider, and model identifiers are required.")
        self.session_identifier = session_identifier
        self.provider_identifier = provider_identifier
        self.model_identifier = model_identifier
        self._prepare_text = prepare_text
        self._protected_vocabulary = protected_vocabulary
        self._attempt_factory = attempt_factory
        self._configuration = configuration or SegmentCleanupConfiguration()
        self._condition = threading.Condition(threading.Lock())
        self._lifecycle = SegmentCleanupLifecycle.ACTIVE
        self._accepted_identifiers: set[str] = set()
        self._segments: list[_SegmentRecord] = []
        self._pending_sequences: list[int] = []
        self._active_attempts: dict[int, _ActiveAttempt] = {}
        self._next_publication_sequence = 0

    @property
    def has_stable_segments(self) -> bool:
        """Return whether any non-empty committed provider segment was accepted."""
        with self._condition:
            return bool(self._segments)

    def accept_stable_segment(self, identifier: str, raw_text: str) -> bool:
        """Accept one unique committed segment without waiting for preparation or provider work."""
        normalized_raw = raw_text.strip()
        if not normalized_raw:
            return False
        with self._condition:
            if self._lifecycle is not SegmentCleanupLifecycle.ACTIVE:
                return False
            if identifier in self._accepted_identifiers:
                return False
            self._accepted_identifiers.add(identifier)
            sequence = len(self._segments)
            record = _SegmentRecord(identifier=identifier, sequence=sequence, raw_text=normalized_raw)
            self._segments.append(record)
            if len(normalized_raw) > self._configuration.segment_character_limit:
                self._settle_record_locked(record, None, SegmentCleanupFailure.INPUT_TOO_LARGE)
                self._publish_available_locked()
            elif len(self._active_attempts) < self._configuration.concurrency_limit:
                self._start_attempt_locked(record)
            elif len(self._pending_sequences) < self._configuration.pending_capacity:
                record.state = SegmentCleanupState.WAITING
                self._pending_sequences.append(sequence)
            else:
                self._settle_record_locked(record, None, SegmentCleanupFailure.SKIPPED_CAPACITY)
                self._publish_available_locked()
            self._condition.notify_all()
        return True

    def projection(self) -> tuple[SegmentCleanupProjection, ...]:
        """Return the ordered display state without exposing unvalidated late results."""
        with self._condition:
            return tuple(
                SegmentCleanupProjection(
                    identifier=record.identifier,
                    sequence=record.sequence,
                    raw_text=record.raw_text,
                    revised_text=record.revised_text,
                    state=record.state,
                    failure=record.failure,
                )
                for record in self._segments
            )

    def stop_and_drain(self) -> SegmentCleanupTerminalSnapshot:
        """Cancel queued work and drain active attempts within one fixed stop bound."""
        started_at = time.monotonic()
        attempts_to_close: list[SegmentCleanupAttempt] = []
        with self._condition:
            if self._lifecycle in {SegmentCleanupLifecycle.FINISHED, SegmentCleanupLifecycle.CANCELLED}:
                return self._terminal_snapshot_locked(time.monotonic() - started_at)
            if self._lifecycle is SegmentCleanupLifecycle.ACTIVE:
                self._lifecycle = SegmentCleanupLifecycle.STOPPING
                for sequence in self._pending_sequences:
                    self._settle_record_locked(
                        self._segments[sequence],
                        None,
                        SegmentCleanupFailure.CANCELLED,
                    )
                self._pending_sequences.clear()
                self._publish_available_locked()
            deadline = started_at + self._configuration.stop_drain_timeout_seconds
            while self._active_attempts:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    break
                self._condition.wait(timeout=remaining)
            for sequence, active in tuple(self._active_attempts.items()):
                active.timer.cancel()
                if active.attempt is not None:
                    attempts_to_close.append(active.attempt)
                self._settle_record_locked(
                    self._segments[sequence],
                    None,
                    SegmentCleanupFailure.CANCELLED,
                )
                self._active_attempts.pop(sequence, None)
            self._publish_available_locked()
            self._lifecycle = SegmentCleanupLifecycle.FINISHED
            self._condition.notify_all()
            snapshot = self._terminal_snapshot_locked(time.monotonic() - started_at)
        for attempt in attempts_to_close:
            _close_attempt_async(attempt)
        return snapshot

    def cancel(self) -> None:
        """Immediately select raw text and reject every queued or late provider result."""
        attempts_to_close: list[SegmentCleanupAttempt] = []
        with self._condition:
            if self._lifecycle in {SegmentCleanupLifecycle.FINISHED, SegmentCleanupLifecycle.CANCELLED}:
                return
            self._lifecycle = SegmentCleanupLifecycle.CANCELLED
            self._pending_sequences.clear()
            for active in self._active_attempts.values():
                active.timer.cancel()
                if active.attempt is not None:
                    attempts_to_close.append(active.attempt)
            self._active_attempts.clear()
            for record in self._segments:
                if record.state not in {SegmentCleanupState.CLEANED, SegmentCleanupState.FALLBACK}:
                    self._settle_record_locked(record, None, SegmentCleanupFailure.CANCELLED)
                    record.state = SegmentCleanupState.CANCELLED
            self._next_publication_sequence = len(self._segments)
            self._condition.notify_all()
        for attempt in attempts_to_close:
            _close_attempt_async(attempt)

    def _start_attempt_locked(self, record: _SegmentRecord) -> None:
        """Reserve one concurrency slot before starting its daemon worker and timer."""
        token = str(uuid.uuid4())
        record.state = SegmentCleanupState.REWRITING
        record.attempt_token = token
        record.settled = False
        timer = threading.Timer(
            self._configuration.attempt_timeout_seconds,
            self._attempt_timed_out,
            args=(record.sequence, token),
        )
        timer.daemon = True
        self._active_attempts[record.sequence] = _ActiveAttempt(token=token, timer=timer)
        worker = threading.Thread(
            target=self._run_attempt,
            args=(record.sequence, token),
            name=f"codex-segment-cleanup-{record.sequence}",
            daemon=True,
        )
        worker.start()
        timer.start()

    def _run_attempt(self, sequence: int, token: str) -> None:
        """Prepare and transform one segment away from recognition and GTK threads."""
        attempt: SegmentCleanupAttempt | None = None
        candidate: str | None = None
        failure: SegmentCleanupFailure | None = None
        try:
            with self._condition:
                raw_text = self._segments[sequence].raw_text
            try:
                prepared_text = self._prepare_text(raw_text).strip()
            except Exception:
                prepared_text = ""
                failure = SegmentCleanupFailure.PROCESSING
            if not prepared_text:
                failure = failure or SegmentCleanupFailure.PROCESSING
            elif len(prepared_text) > self._configuration.segment_character_limit:
                failure = SegmentCleanupFailure.INPUT_TOO_LARGE
            else:
                attempt = self._attempt_factory()
                with self._condition:
                    active = self._active_attempts.get(sequence)
                    if active is None or active.token != token:
                        return
                    active.attempt = attempt
                result = attempt.transform(prepared_text)
                if isinstance(result, str):
                    candidate = result.strip()
                    failure = self._validate_candidate(prepared_text, candidate)
                else:
                    failure = SegmentCleanupFailure.MALFORMED_OUTPUT
        except Exception:
            failure = SegmentCleanupFailure.PROVIDER
        finally:
            try:
                self._complete_attempt(sequence, token, candidate, failure)
            finally:
                if attempt is not None:
                    _close_attempt(attempt)

    def _validate_candidate(self, prepared_text: str, candidate: str) -> SegmentCleanupFailure | None:
        """Reject malformed, oversized, or meaning-unsafe provider output before publication."""
        if not candidate:
            return SegmentCleanupFailure.MALFORMED_OUTPUT
        if len(candidate) > self._configuration.response_character_limit:
            return SegmentCleanupFailure.OUTPUT_TOO_LARGE
        if integrity_violations(prepared_text, candidate, self._protected_vocabulary):
            return SegmentCleanupFailure.SAFETY
        return None

    def _complete_attempt(
        self,
        sequence: int,
        token: str,
        candidate: str | None,
        failure: SegmentCleanupFailure | None,
    ) -> None:
        """Accept only the current attempt token and release the next bounded queued segment."""
        with self._condition:
            active = self._active_attempts.get(sequence)
            if active is None or active.token != token:
                return
            active.timer.cancel()
            self._active_attempts.pop(sequence, None)
            record = self._segments[sequence]
            self._settle_record_locked(record, candidate, failure)
            self._publish_available_locked()
            self._start_next_pending_locked()
            self._condition.notify_all()

    def _attempt_timed_out(self, sequence: int, token: str) -> None:
        """Select raw text at the attempt deadline and interrupt the isolated provider child."""
        attempt_to_close = None
        with self._condition:
            active = self._active_attempts.get(sequence)
            if active is None or active.token != token:
                return
            attempt_to_close = active.attempt
            self._active_attempts.pop(sequence, None)
            self._settle_record_locked(
                self._segments[sequence],
                None,
                SegmentCleanupFailure.TIMEOUT,
            )
            self._publish_available_locked()
            self._start_next_pending_locked()
            self._condition.notify_all()
        if attempt_to_close is not None:
            _close_attempt_async(attempt_to_close)

    def _start_next_pending_locked(self) -> None:
        """Fill available capacity only while capture remains active."""
        while (
            self._lifecycle is SegmentCleanupLifecycle.ACTIVE
            and self._pending_sequences
            and len(self._active_attempts) < self._configuration.concurrency_limit
        ):
            sequence = self._pending_sequences.pop(0)
            self._start_attempt_locked(self._segments[sequence])

    def _settle_record_locked(
        self,
        record: _SegmentRecord,
        candidate: str | None,
        failure: SegmentCleanupFailure | None,
    ) -> None:
        """Stage one terminal result without publishing past an earlier incomplete segment."""
        record.revised_text = candidate if failure is None else None
        record.failure = failure
        record.attempt_token = None
        record.state = SegmentCleanupState.WAITING
        record.settled = True

    def _publish_available_locked(self) -> None:
        """Publish only the contiguous capture-ordered prefix of settled results."""
        while self._next_publication_sequence < len(self._segments):
            record = self._segments[self._next_publication_sequence]
            if record.state is not SegmentCleanupState.WAITING or not record.settled:
                break
            record.state = SegmentCleanupState.CLEANED if record.failure is None else SegmentCleanupState.FALLBACK
            self._next_publication_sequence += 1

    def _terminal_snapshot_locked(self, stop_drain_seconds: float) -> SegmentCleanupTerminalSnapshot:
        """Convert any unfinished internal state to immutable raw fallback in capture order."""
        terminal_segments = []
        for record in self._segments:
            failure = record.failure
            selected_text = record.revised_text
            if record.state is not SegmentCleanupState.CLEANED or selected_text is None:
                selected_text = record.raw_text
                failure = failure or SegmentCleanupFailure.CANCELLED
            terminal_segments.append(
                SegmentCleanupTerminalSegment(
                    identifier=record.identifier,
                    sequence=record.sequence,
                    raw_text=record.raw_text,
                    selected_text=selected_text,
                    failure=failure,
                )
            )
        return SegmentCleanupTerminalSnapshot(
            session_identifier=self.session_identifier,
            provider_identifier=self.provider_identifier,
            model_identifier=self.model_identifier,
            segments=tuple(terminal_segments),
            stop_drain_seconds=stop_drain_seconds,
        )


def _join_segments(segments: tuple[str, ...]) -> str:
    """Match realtime committed-segment joining without mutating segment content."""
    return " ".join(segment.strip() for segment in segments if segment.strip())


def _close_attempt(attempt: SegmentCleanupAttempt) -> None:
    """Keep provider cancellation failure outside cleanup state and capture threads."""
    try:
        attempt.close()
    except Exception:
        pass


def _close_attempt_async(attempt: SegmentCleanupAttempt) -> None:
    """Interrupt a provider child without extending the fixed stop-drain bound."""
    threading.Thread(
        target=_close_attempt,
        args=(attempt,),
        name="codex-segment-cleanup-cancel",
        daemon=True,
    ).start()
