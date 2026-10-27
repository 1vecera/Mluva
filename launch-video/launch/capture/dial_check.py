"""Measure the session wheel in a lossless take: the hand's bearing at rest and the ticks' bearings.

    uv run --with numpy python dial_check.py TAKE.mkv REGION SPEC.json > dial-check.json

REGION is the crop holding the wheel, `WxH+X+Y` in screen pixels. SPEC lists the selections to
check: `[{"label", "time", "selected", "count", "baseline_time"?}]`, `time` in seconds into the
take at a moment the wheel is at rest. With `baseline_time`, a frame of the same screen without the
wheel, only what the wheel adds over it is measured, so a busy desktop behind it cannot mislead. For each, the frame and the two frames 5 and 10 later are read from the
take; the ring is found by a RANSAC circle fit to its pixels; the hand is the brightest ray
between the hub text and the ring; ticks are the bright marks just inside the ring. Bearings are
clockwise from twelve o'clock, as `clock_angle` in `rust/mluva-gtk/src/agent_session_wheel.rs`
defines them, and each is compared with `clock_angle(selected, count)`.
"""

import json
import subprocess
import sys

import numpy as np

START_DEGREES = 30.0


def clock_angle(index: int, count: int) -> float:
    return (START_DEGREES + 360.0 * index / max(count, 1)) % 360.0


def frame(take: str, number: int, region: str) -> np.ndarray:
    w, h, x, y = (int(v) for v in region.replace("+", "x").split("x"))
    raw = subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-i",
            take,
            "-vf",
            f"select=eq(n\\,{number}),crop={w}:{h}:{x}:{y}",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-",
        ],
        check=True,
        capture_output=True,
    ).stdout
    return np.frombuffer(raw, np.uint8).reshape(h, w).astype(np.float32)


def ring(
    image: np.ndarray, rng: np.random.Generator
) -> tuple[float, float, float, int]:
    ys, xs = np.nonzero((image > 35) & (image < 200))
    points = np.stack([xs, ys], 1).astype(np.float64)
    best: tuple[float, float, float] | None = None
    best_count = 0
    for _ in range(6000):
        a, b, c = points[rng.choice(len(points), 3, replace=False)]
        d = 2 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]))
        if abs(d) < 1e-6:
            continue
        sa, sb, sc = a @ a, b @ b, c @ c
        cx = (sa * (b[1] - c[1]) + sb * (c[1] - a[1]) + sc * (a[1] - b[1])) / d
        cy = (sa * (c[0] - b[0]) + sb * (a[0] - c[0]) + sc * (b[0] - a[0])) / d
        r = np.hypot(a[0] - cx, a[1] - cy)
        # A dial's radius at scale 2 runs from 300 px (one session) to 580 px (twenty or more).
        if not 280 < r < 600:
            continue
        near = np.abs(np.hypot(points[:, 0] - cx, points[:, 1] - cy) - r) < 1.5
        # Score by how much of the full circle has ring pixels, in 2 degree bins: text and window
        # edges behind a see-through dial give dense but partial arcs, the ring a whole circle.
        angles = (
            np.degrees(np.arctan2(points[near, 1] - cy, points[near, 0] - cx)) % 360
        )
        count = len(np.unique((angles // 2).astype(int)))
        if count > best_count:
            best, best_count = (cx, cy, r), count
    assert best is not None, "no ring found"
    cx, cy, r = best
    inliers = points[np.abs(np.hypot(points[:, 0] - cx, points[:, 1] - cy) - r) < 2.0]
    # Algebraic least-squares refinement on the inliers.
    a = np.c_[2 * inliers, np.ones(len(inliers))]
    sol = np.linalg.lstsq(a, (inliers**2).sum(1), rcond=None)[0]
    cx, cy = sol[0], sol[1]
    return cx, cy, float(np.sqrt(sol[2] + cx * cx + cy * cy)), best_count


def profile(image, cx, cy, radii, step=0.1):
    bearings = np.arange(0, 360, step)
    t = np.radians(bearings)[:, None]
    xs = cx + np.sin(t) * radii[None, :]
    ys = cy - np.cos(t) * radii[None, :]
    xi = np.clip(np.round(xs).astype(int), 0, image.shape[1] - 1)
    yi = np.clip(np.round(ys).astype(int), 0, image.shape[0] - 1)
    return bearings, image[yi, xi].mean(1)


def measure(image, fit):
    cx, cy, r, _ = fit
    bearings, along = profile(image, cx, cy, np.linspace(0.40 * r, 0.80 * r, 80))
    hand = float(bearings[int(np.argmax(along))])
    # A tick is a narrow bright mark just inside the ring; the light trace is a broad glow, so a
    # tick must stand out from the ring 4 degrees either side of it.
    bearings, inside = profile(image, cx, cy, np.linspace(r - 14, r - 5, 10), step=0.5)
    sides = (np.roll(inside, 8) + np.roll(inside, -8)) / 2
    ticks: list[float] = []
    for i in np.argsort(inside - sides)[::-1]:
        if inside[i] - sides[i] < 25:
            break
        b = float(bearings[i])
        if all(min(abs(b - t), 360 - abs(b - t)) > 6 for t in ticks):
            ticks.append(b)
    return {
        "centre": [round(cx, 1), round(cy, 1)],
        "radius": round(r, 1),
        "hand_bearing": round(hand, 1),
        "tick_bearings": sorted(round(t, 1) for t in ticks),
    }


def main() -> None:
    take, region = sys.argv[1], sys.argv[2]
    with open(sys.argv[3]) as file:
        spec = json.load(file)
    rng = np.random.default_rng(7)
    results = []
    for check in spec:
        first = round(check["time"] * 60)
        numbers = (first, first + 5, first + 10)
        images = [frame(take, n, region) for n in numbers]
        if "baseline_time" in check:
            baseline = frame(take, round(check["baseline_time"] * 60), region)
            images = [np.clip(image - baseline, 0, 255) for image in images]
        # The dial does not move at rest: fit the ring in each frame and keep the best-supported fit.
        fit = max((ring(image, rng) for image in images), key=lambda f: f[3])
        reads = [
            measure(image, fit) | {"frame": n} for image, n in zip(images, numbers)
        ]
        hands: list[float] = [float(r["hand_bearing"]) for r in reads]
        expected = clock_angle(check["selected"], check["count"])
        error = max(min(abs(h - expected), 360 - abs(h - expected)) for h in hands)
        ticks: list[float] = list(reads[0]["tick_bearings"])
        expected_ticks = [clock_angle(i, check["count"]) for i in range(check["count"])]
        results.append(
            {
                "label": check["label"],
                "selected": check["selected"],
                "count": check["count"],
                "clock_angle": expected,
                "hand_bearing_at_rest": hands,
                "hand_error_degrees": round(error, 2),
                "hand_steady": max(hands) - min(hands) <= 0.5,
                "tick_bearings": ticks,
                "expected_tick_bearings": expected_ticks,
                "ticks_match": len(ticks) == len(expected_ticks)
                and all(
                    min(min(abs(t - e), 360 - abs(t - e)) for t in ticks) <= 1.5
                    for e in expected_ticks
                ),
                "reads": reads,
            }
        )
    json.dump(results, sys.stdout, indent=2)
    print()


main()
