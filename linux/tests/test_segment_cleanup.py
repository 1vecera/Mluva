"""Concurrency and immutable-fallback coverage for realtime segment cleanup."""

import threading
import time
from collections.abc import Callable
from dataclasses import dataclass, field

from voice_scribe_linux.segment_cleanup import (
    SegmentCleanupAttempt,
    SegmentCleanupConfiguration,
    SegmentCleanupFailure,
    SegmentCleanupSession,
    SegmentCleanupState,
)


class ControlledProvider:
    """Coordinate isolated attempts so tests choose completion order deterministically."""

    def __init__(self) -> None:
        """Create shared request, result, and concurrency state."""
        self.condition = threading.Condition()
        self.requests: list[tuple[str, ControlledAttempt]] = []
        self.results: dict[str, str | Exception] = {}
        self.active = 0
        self.maximum_active = 0

    def factory(self) -> "ControlledAttempt":
        """Return one independently closable attempt."""
        return ControlledAttempt(self)

    def wait_for_requests(self, count: int, timeout: float = 2) -> bool:
        """Wait until the expected number of provider attempts starts."""
        deadline = time.monotonic() + timeout
        with self.condition:
            while len(self.requests) < count:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return False
                self.condition.wait(timeout=remaining)
            return True

    def complete(self, prepared_text: str, result: str | Exception) -> None:
        """Release the attempt for one exact prepared segment."""
        with self.condition:
            self.results[prepared_text] = result
            self.condition.notify_all()


@dataclass(slots=True)
class ControlledAttempt:
    """Block one provider call until its controller supplies a result or closes it."""

    provider: ControlledProvider
    closed: bool = field(init=False, default=False)

    def transform(self, prepared_text: str) -> str:
        """Record concurrency and wait for a deterministic response."""
        with self.provider.condition:
            self.provider.requests.append((prepared_text, self))
            self.provider.active += 1
            self.provider.maximum_active = max(self.provider.maximum_active, self.provider.active)
            self.provider.condition.notify_all()
            try:
                while prepared_text not in self.provider.results and not self.closed:
                    self.provider.condition.wait(timeout=2)
                if self.closed:
                    raise RuntimeError("controlled attempt closed")
                result = self.provider.results[prepared_text]
            finally:
                self.provider.active -= 1
                self.provider.condition.notify_all()
        if isinstance(result, Exception):
            raise result
        return result

    def close(self) -> None:
        """Interrupt only this isolated attempt."""
        with self.provider.condition:
            self.closed = True
            self.provider.condition.notify_all()


@dataclass(slots=True)
class FixedAttempt:
    """Return or raise one immediate deterministic provider result."""

    result: str | Exception

    def transform(self, _prepared_text: str) -> str:
        """Return the fixed result or raise its fixed exception."""
        if isinstance(self.result, Exception):
            raise self.result
        return self.result

    def close(self) -> None:
        """Release no resources for an immediate fake attempt."""


def _session(
    factory: Callable[[], SegmentCleanupAttempt],
    *,
    prepare_text: Callable[[str], str] = str.upper,
    configuration: SegmentCleanupConfiguration | None = None,
    protected_vocabulary: tuple[str, ...] = (),
) -> SegmentCleanupSession:
    """Build one frozen test session around the supplied fake factory."""
    return SegmentCleanupSession(
        session_identifier="capture-session",
        provider_identifier="codex-app-server",
        model_identifier="gpt-5.4-test",
        prepare_text=prepare_text,
        protected_vocabulary=protected_vocabulary,
        attempt_factory=factory,
        configuration=configuration,
    )


def _wait_for_state(
    session: SegmentCleanupSession,
    expected: tuple[SegmentCleanupState, ...],
    timeout: float = 2,
) -> bool:
    """Poll bounded daemon work until its projection reaches one exact state tuple."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if tuple(segment.state for segment in session.projection()) == expected:
            return True
        time.sleep(0.001)
    return False


def test_concurrent_results_publish_only_in_capture_order() -> None:
    """Keep a later success waiting until the earlier provider attempt settles."""
    provider = ControlledProvider()
    session = _session(provider.factory)

    assert session.accept_stable_segment("alpha", "raw alpha")
    assert session.accept_stable_segment("beta", "raw beta")
    assert provider.wait_for_requests(2)
    assert provider.maximum_active == 2

    provider.complete("RAW BETA", "clean beta")
    assert _wait_for_state(
        session,
        (SegmentCleanupState.REWRITING, SegmentCleanupState.WAITING),
    )
    provider.complete("RAW ALPHA", "clean alpha")
    assert _wait_for_state(
        session,
        (SegmentCleanupState.CLEANED, SegmentCleanupState.CLEANED),
    )

    snapshot = session.stop_and_drain()
    assert [segment.identifier for segment in snapshot.segments] == ["alpha", "beta"]
    assert snapshot.raw_text == "raw alpha raw beta"
    assert snapshot.selected_text == "clean alpha clean beta"
    assert snapshot.enhancement_outcome == "completed"


def test_capacity_pressure_never_blocks_and_selects_exact_raw_text() -> None:
    """Reject work beyond active and pending bounds without delaying committed recognition."""
    provider = ControlledProvider()
    session = _session(
        provider.factory,
        configuration=SegmentCleanupConfiguration(concurrency_limit=1, pending_capacity=0),
    )

    assert session.accept_stable_segment("slow", "raw slow")
    assert provider.wait_for_requests(1)
    started_at = time.monotonic()
    assert session.accept_stable_segment("overflow", "raw overflow")
    assert time.monotonic() - started_at < 0.05
    assert tuple(segment.state for segment in session.projection()) == (
        SegmentCleanupState.REWRITING,
        SegmentCleanupState.WAITING,
    )

    provider.complete("RAW SLOW", "clean slow")
    assert _wait_for_state(
        session,
        (SegmentCleanupState.CLEANED, SegmentCleanupState.FALLBACK),
    )
    snapshot = session.stop_and_drain()
    assert snapshot.selected_text == "clean slow raw overflow"
    assert snapshot.segments[1].failure is SegmentCleanupFailure.SKIPPED_CAPACITY
    assert snapshot.enhancement_outcome == "safe-fallback"


def test_provider_and_safety_failures_preserve_immutable_raw_segment() -> None:
    """Discard provider errors and meaning-changing candidates instead of deterministic prepared text."""
    provider_failure = _session(lambda: FixedAttempt(RuntimeError("secret provider payload")))
    assert provider_failure.accept_stable_segment("provider", "I um said raw")
    assert _wait_for_state(provider_failure, (SegmentCleanupState.FALLBACK,))
    failed = provider_failure.stop_and_drain()
    assert failed.selected_text == "I um said raw"
    assert failed.segments[0].failure is SegmentCleanupFailure.PROVIDER
    assert failed.enhancement_outcome == "raw-fallback"

    unsafe = _session(
        lambda: FixedAttempt("Deploy now"),
        prepare_text=lambda text: text,
    )
    assert unsafe.accept_stable_segment("unsafe", "Do not deploy https://example.com now")
    assert _wait_for_state(unsafe, (SegmentCleanupState.FALLBACK,))
    rejected = unsafe.stop_and_drain()
    assert rejected.selected_text == "Do not deploy https://example.com now"
    assert rejected.segments[0].failure is SegmentCleanupFailure.SAFETY


def test_request_response_and_preparation_bounds_fail_closed() -> None:
    """Send no oversized input and reject empty or oversized output before publication."""
    factory_calls = 0

    def counted_factory() -> FixedAttempt:
        """Record whether an oversized input reached the provider boundary."""
        nonlocal factory_calls
        factory_calls += 1
        return FixedAttempt("unused")

    oversized_input = _session(
        counted_factory,
        configuration=SegmentCleanupConfiguration(segment_character_limit=3),
    )
    assert oversized_input.accept_stable_segment("large", "four")
    assert _wait_for_state(oversized_input, (SegmentCleanupState.FALLBACK,))
    assert oversized_input.stop_and_drain().segments[0].failure is SegmentCleanupFailure.INPUT_TOO_LARGE
    assert factory_calls == 0

    empty_output = _session(lambda: FixedAttempt("   "))
    assert empty_output.accept_stable_segment("empty", "raw")
    assert _wait_for_state(empty_output, (SegmentCleanupState.FALLBACK,))
    assert empty_output.stop_and_drain().segments[0].failure is SegmentCleanupFailure.MALFORMED_OUTPUT

    oversized_output = _session(
        lambda: FixedAttempt("toolong"),
        configuration=SegmentCleanupConfiguration(response_character_limit=3),
    )
    assert oversized_output.accept_stable_segment("response", "raw")
    assert _wait_for_state(oversized_output, (SegmentCleanupState.FALLBACK,))
    assert oversized_output.stop_and_drain().segments[0].failure is SegmentCleanupFailure.OUTPUT_TOO_LARGE


def test_timeout_ignores_late_success_and_releases_next_attempt() -> None:
    """Select raw at the deadline and prevent a cancellation-ignoring late result from publishing."""
    provider = ControlledProvider()
    session = _session(
        provider.factory,
        configuration=SegmentCleanupConfiguration(
            concurrency_limit=1,
            pending_capacity=1,
            attempt_timeout_seconds=0.02,
            stop_drain_timeout_seconds=0.1,
        ),
    )

    assert session.accept_stable_segment("timeout", "raw timeout")
    assert session.accept_stable_segment("next", "raw next")
    assert provider.wait_for_requests(1)
    assert provider.wait_for_requests(2)
    provider.complete("RAW NEXT", "clean next")
    reached_terminal_order = _wait_for_state(
        session,
        (SegmentCleanupState.FALLBACK, SegmentCleanupState.CLEANED),
    )
    assert reached_terminal_order, session.projection()
    provider.complete("RAW TIMEOUT", "late clean timeout")

    snapshot = session.stop_and_drain()
    assert snapshot.selected_text == "raw timeout clean next"
    assert snapshot.segments[0].failure is SegmentCleanupFailure.TIMEOUT
    assert session.projection()[0].state is SegmentCleanupState.FALLBACK


def test_stop_cancels_queued_work_and_bounds_active_drain() -> None:
    """Return within the stop bound even when an active attempt has not completed."""
    provider = ControlledProvider()
    session = _session(
        provider.factory,
        configuration=SegmentCleanupConfiguration(
            concurrency_limit=1,
            pending_capacity=1,
            attempt_timeout_seconds=2,
            stop_drain_timeout_seconds=0.02,
        ),
    )
    assert session.accept_stable_segment("active", "raw active")
    assert session.accept_stable_segment("queued", "raw queued")
    assert provider.wait_for_requests(1)

    started_at = time.monotonic()
    snapshot = session.stop_and_drain()
    elapsed = time.monotonic() - started_at

    assert elapsed < 0.1
    assert snapshot.selected_text == "raw active raw queued"
    assert [segment.failure for segment in snapshot.segments] == [
        SegmentCleanupFailure.CANCELLED,
        SegmentCleanupFailure.CANCELLED,
    ]
    assert len(provider.requests) == 1


def test_duplicate_and_late_segments_cannot_mutate_terminal_state() -> None:
    """Accept one event ID once and reject provider callbacks after cancellation."""
    provider = ControlledProvider()
    session = _session(provider.factory)
    assert session.accept_stable_segment("same", "raw")
    assert not session.accept_stable_segment("same", "mutated")
    assert provider.wait_for_requests(1)

    session.cancel()
    provider.complete("RAW", "late")

    projection = session.projection()
    assert len(projection) == 1
    assert projection[0].raw_text == "raw"
    assert projection[0].state is SegmentCleanupState.CANCELLED
    assert not session.accept_stable_segment("new", "late raw")
