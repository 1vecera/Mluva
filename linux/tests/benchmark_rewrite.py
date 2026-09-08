"""Opt-in live Codex timing with synthetic text; never read Mluva history or change user settings."""

import argparse
import json
import statistics
import time
from pathlib import Path
from tempfile import TemporaryDirectory

from voice_scribe_linux.codex_client import CodexAppServerClient, select_model

PROMPT = (
    "Rewrite the following sentence clearly, preserving its meaning: "
    "Um, we should, uh, send the release notes on Friday after the checks pass."
)


def main() -> None:
    """Alternate inherited, low/standard and low/Fast requests and report end-to-first-text latency."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--live", action="store_true", required=True, help="Send synthetic requests using Codex credits"
    )
    parser.add_argument("--model", help="Catalog identifier; omitted uses the catalog default")
    parser.add_argument("--runs", type=int, choices=range(1, 6), default=3)
    arguments = parser.parse_args()
    scratch = Path(__file__).resolve().parents[2] / "tmp" / "rewrite-benchmark"
    scratch.mkdir(parents=True, exist_ok=True)
    measurements: dict[str, list[float]] = {}
    with TemporaryDirectory(dir=scratch) as temporary:
        for repeat in range(arguments.runs):
            for mode in ("inherited", "standard", "fast"):
                client = CodexAppServerClient(turn_timeout_seconds=90)
                first_text: float | None = None
                started = time.monotonic()

                def progress(delta: str, request_started: float = started) -> None:
                    """Measure the first nonempty streamed text, including process and model resolution."""
                    nonlocal first_text
                    if delta and first_text is None:
                        first_text = time.monotonic() - request_started

                try:
                    model = select_model(client.list_models(), arguments.model)
                    if mode == "fast" and model.fast_tier is None:
                        print(json.dumps({"mode": mode, "skipped": "Model does not advertise Fast"}), flush=True)
                        continue
                    result = client.transform(
                        PROMPT,
                        Path(temporary),
                        model.identifier,
                        effort=None if mode == "inherited" else model.rewrite_effort,
                        service_tier=None if mode == "inherited" else model.fast_tier if mode == "fast" else "default",
                        on_delta=progress,
                    )
                    assert first_text is not None and result
                    measurements.setdefault(mode, []).append(first_text)
                    print(
                        json.dumps(
                            {
                                "run": repeat + 1,
                                "mode": mode,
                                "model": model.identifier,
                                "first_text_seconds": round(first_text, 3),
                                "total_seconds": round(time.monotonic() - started, 3),
                                "result": result,
                            }
                        ),
                        flush=True,
                    )
                finally:
                    client.close()
    medians = {mode: round(statistics.median(samples), 3) for mode, samples in measurements.items()}
    print(json.dumps({"median_first_text_seconds": medians}))


if __name__ == "__main__":
    main()
