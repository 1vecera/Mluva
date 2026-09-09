# S27-464 panel stability evidence

The original demo contains two runtime defects: a shrinking recognition preview can scroll entirely out of its fixed viewport, and Stop hides the Live editor before the final rewrite returns. The intro also contains deliberate edits. This change fixes the runtime causes; the media agent owns the film edit.

Measurements use zero-based frame numbers and half-open time intervals. The inspected originals are the 44.700-second [intro](https://github.com/1vecera/Mluva/blob/18004cc98cd73f1b038883e59db6274ee9f0be4b/docs/promotion/assets/mluva-product-intro.mp4) and complete 59.533-second [Live recording](https://github.com/1vecera/Mluva/blob/18004cc98cd73f1b038883e59db6274ee9f0be4b/docs/promotion/assets/unreleased-live/workflow.mp4), both 30 fps, at source commit `18004cc98cd73f1b038883e59db6274ee9f0be4b`. These immutable links identify the measured footage even after S27-465 replaces the intro. Their SHA-256 hashes and source URLs are retained in [measurements.json](s27-464/measurements.json). The original capture identifies runtime `cdc00237595f8cfb29e75cd87a980311ea3bcc98`; the reproduction baseline is the same refreshed `origin/main` commit `18004cc`.

| Original interval | Observed behavior | Classification |
| --- | --- | --- |
| Live 26.800–27.067 s, frames 804–811; intro 35.000–35.267 s, frames 1050–1057 | Preview text vanishes for eight frames. The recording border stays at x=64, y=793, 500×127; header and timer remain. | Runtime text clipping, reproduced independently. |
| Live 41.133 s | Stop replaces the five-line preview with a header-only processing panel near the bottom edge. | Runtime publication drops the preview, contracting the panel. |
| Live 41.533–41.567 s | The shell panel is absent for one frame, then shows the raw note as ready while Live reconciliation is pending. | Runtime clear/republication at workflow completion. |
| Live 41.600–55.267 s, frames 1248–1657 | The two Live editors are replaced by the original note; the final rewrite only appears at frame 1658. | Runtime lifecycle hides Live before its final callback. |
| Live 45.567–55.267 s, frames 1367–1657 | The prematurely ready shell review expires after four seconds, then returns with the final reply. | Runtime phase starts the dismissal timer too early. |
| Intro 9.600–10.000, 25.200–25.600, 37.800–38.200, 41.200–41.600 s | Crossfades change scenes. At 37.800 s the edit jumps from Live around 29.6 s to its completed result around 55.4 s. | Edit transitions; the opening generated scene is captioned as a visualization. |

The recognition event fixture preserves the exact recorded display snapshots and their relative spacing, including the temporary duplicated segment. No recognition, raw-history or provider behavior was changed. QML now clamps the painted offset to the shortened text's new tail while retaining animated following for growth. Processing retains its existing preview. GTK keeps both Live editors mapped until reconciliation finishes or is cancelled, and reserves the Cancel button's height during recording. Shell review stays busy, with Copy disabled and dismissal paused, until finalization completes.

The [before clip](s27-464/preview-before.mp4) and [after clip](s27-464/preview-after.mp4) are disclosed offline replays of [recorded preview events](../../linux/tests/fixtures/preview-contraction.json) through production QML. Each contains all 300 frames of a five-second, 60 fps capture. Delivery copies are lossless 520×147 crops around the panel; they contain no cuts, retiming or audio. They are verification footage, not a new real-provider product recording.

| Controlled observation | Baseline | Fixed |
| --- | --- | --- |
| Blank preview pixels | 15 frames, 2.550–2.800 s | 0 of 300 frames |
| QML panel x/y/width/height | 390/749/500/127 | Identical |
| Preview viewport | 85 px = five 17 px lines | Identical |
| Pixel border rows in retained crop | 10 and 136 in every frame | Identical |
| Header/timer | Present in every sampled QML state; pixel counts constant across all video frames | Identical |
| Recording dot opacity | 0.55–1.00 | 0.55–1.00 |
| GTK during held finalization | 30 of 30 samples unmapped | 30 of 30 mapped at each measured size |
| GTK bounds, default / wide window | Hidden | 16/102/598/330 and 248/102/786/590, constant across Stop |
| GTK manual original/draft reading positions | Draft position retained but editor hidden | 90/80 px, unchanged throughout each 30-sample transition |

The [pending finalization screenshot](s27-464/finalization-pending.png) shows the real GTK editors and Cancel action with a deliberately held, local JSONL provider. The regression exercises manual edits during reconciliation, one final saved/copyable draft, immutable raw recognition, rejected provisional input for the final request, cancellation and late callbacks, and retained copyable text after save/copy failures. Ordinary successful rewriting separately asserts a ready state with no cancellation message. Review checks hold a busy widget beyond its four-second timeout. Existing QML checks also pass for smooth anticipation, reduced motion, five lines, header/timer/pulse, bounded Unicode previews, actions, focus and dismissal.

Validation on Lenovo: `make linux-test` passed 361 tests, feature-contract verification, Ruff and formatting. The full Live workspace fixture passed; focused finalization verification additionally passed after the error/cancellation status changes at the wide viewport. The ordinary conversation lifecycle passed. `make linux-text-target-test` passed private cross-process focus capture, Unicode insertion and exact target confirmation. The QML regression and both 60 fps replays passed. Local logs and raw geometry are under `tmp/s27-464/`; the durable measurements and small clips above travel with the PR.

All runtime verification used private Xvfb `:194`, D-Bus, XDG state and accessibility; device/provider/clipboard boundaries were disabled or synthetic. Xvfb does not verify a live Wayland compositor, physical shortcuts, real microphone or target-application compatibility. Private portal attempts to find absent Wayland/PipeWire services were headless service diagnostics, not live-device access. Swift is unavailable on this Linux host; `swift test`, the macOS app build and macOS launch smoke remain unrun. No installation, release, provider call or hosted-CI spending was performed.

For reproduction from the repository root, use the existing isolated harness and an unused output directory. The QML fixture additionally needs the installed Omarchy shell controls and `quickshell`/`ffmpeg`:

```bash
OFFSCREEN_DISPLAY_NUMBER=194 PYTHONPATH=linux:linux/tests \
  MLUVA_PANEL_REPLAY=1 PULSE_SERVER=unix:/nonexistent PIPEWIRE_REMOTE=mluva-disabled \
  bash dev/run-isolated.sh tmp/panel-replay -- \
  uv run --project linux --locked python linux/tests/shell_overlay_smoke.py

OFFSCREEN_DISPLAY_NUMBER=194 GDK_SCALE=1 GDK_DPI_SCALE=1 \
  PULSE_SERVER=unix:/nonexistent PIPEWIRE_REMOTE=mluva-disabled \
  make linux-live-rewrite-test
```

Omit `MLUVA_PANEL_REPLAY` to run the full QML regression. For a baseline replay, copy `linux/quickshell/mluva.dictation` from commit `18004cc` into a scratch directory and pass its absolute path as `MLUVA_OVERLAY_SOURCE`; keep the current fixture and tests. Media coordination supplied the exact intervals above: retained old real-provider footage must keep its original runtime provenance, while a cut past finalization or an offline replacement must be disclosed. The revised composer and promotional assets remain with S27-465.
