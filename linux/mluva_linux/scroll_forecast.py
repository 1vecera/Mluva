"""Predict the next spoken line without moving a following viewport backward."""

from collections import deque


class SpeechScrollForecast:
    """Estimate recent net speech growth; provider corrections never count as new speech."""

    def __init__(self) -> None:
        """Start with a modest reading-rate prior until real arrival samples exist."""
        self.samples: deque[tuple[float, int]] = deque()
        self.high_water = 0

    def observe(self, characters: int, now: float) -> None:
        """Keep a short rolling history using monotonic time and cumulative character growth."""
        self.high_water = max(self.high_water, characters)
        self.samples.append((now, self.high_water))
        while len(self.samples) > 2 and self.samples[1][0] < now - 4:
            self.samples.popleft()

    def reserve(self, columns: float, fill: float, horizon: float, limit: int) -> float:
        """Return line space needed by the forecast, capped by the user's lookahead preference."""
        rate = 12.0
        if len(self.samples) > 1:
            elapsed = self.samples[-1][0] - self.samples[0][0]
            if elapsed >= 0.25:
                rate = min(80.0, max(0.0, (self.samples[-1][1] - self.samples[0][1]) / elapsed))
        projected = fill + rate * max(0.2, horizon) / max(1, columns)
        return min(max(0, limit), max(0.0, projected - 1))
