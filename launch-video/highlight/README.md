# Native Rust release highlight

A 27-second, 1920×1080, 60 fps, silent H.264 highlight of the native Rust release. It reuses the showreel's identity (black surface, Adwaita Sans, JetBrains Mono, one red accent) and tells one story: dictate, polish, measured startup wins, local models, install.

| Scene | Seconds | Content |
| --- | --- | --- |
| Title | 0–2.5 | "Now native Rust." with the glossy mark |
| Dictate | 2.5–9.5 | Real footage of the published 2.0.0 window with Qwen3-ASR 1.7B recognizing synthetic speech |
| Polish | 9.5–14.5 | Real footage of the Polish action; the rewrite text is a prepared example |
| Measured | 14.5–20 | First window 456 → 260 ms and resident memory 260.3 → 236.5 MiB |
| On device | 20–24 | Real Settings page with the local-model choice |
| End card | 24–27 | Lockup, `github.com/1vecera/Mluva`, release 2.0.0 |

## What each claim rests on

| On screen | Source |
| --- | --- |
| First window 456 → 260 ms, −43% | Median of six measured starts per version in [release-startup.json](../../docs/verification/rust-release-performance/release-startup.json): 455.6 → 259.8 ms |
| Resident memory 260.3 → 236.5 MiB, −9% | Same file, median process-tree RSS: 260.31 → 236.46 MiB |
| Scope | Released Python 1.6.0 against published Rust 2.0.0, same host, warm cache, startup and idle memory only. [The performance report](../../docs/verification/rust-release-performance/README.md) states that it establishes no general decoder or UI speedup |
| No Python runtime | The shipped application is native Rust ([install notes](../../README.md#install)) |
| Qwen3-ASR 1.7B default, Parakeet v3, Whisper Tiny | [CHANGELOG](../../CHANGELOG.md) 2.0.1 entry and the Settings page in the footage |

The video does not claim 2.0.1 as public (2.0.0 is the latest GitHub release), universal inference speedups or complete platform parity.

## Footage

The frames are the published 2.0.0 application (`bin/mluva`, SHA-256 `7ae478a038e66f52c807783928b590161d72491bb4f8f258af7052c2d0cea080`), recorded in a disposable offscreen session: private Xvfb display, session bus and XDG state, the Vantablack Omarchy palette, no real microphone, content or provider. `capture/drive.py` runs the real window with these substitutions only:

- `capture/fake-pw-record` stands in for `pw-record` and streams a synthetic speech file in real time. The speech is `capture/dictation.txt` spoken by Fish Audio's default voice (`generate-voice-note`, free model); no private recording is used.
- Recognition is real: the local Qwen3-ASR 1.7B CPU runtime from an existing private model cache, inside a network namespace with only loopback.
- Polish calls `capture/fake_rewrite.py`, a loopback OpenAI-compatible peer that streams one prepared sentence. The film says so; it shows the workflow, not rewrite quality or provider latency.
- The recorder and rewrite status lines are cropped out. The 2.0.0 status text under recording reads "Scribe v2 realtime" even for local recognition, which would mislabel the local-model story.
- The dictation scene is a time-lapse: speech ×2, the wait for the final transcript ×4, the finished page at 1×. It shows the workflow, not recognition latency.

`xinput.py` clicks inside the private Xvfb display only (it refuses to run without `OFFSCREEN_SESSION_ROOT`).

## Render

Requires Node/npm, FFmpeg, Xvfb, D-Bus, the offscreen runner from the `run-offscreen-linux-verification` skill, a published 2.0.0 bundle and the local Qwen assets. The private model cache and clips stay outside Git; `public/highlight/` is ignored, and the showreel's `public/showreel/{brand,fonts}` staging is reused.

```sh
# From the repository root. 1. Synthetic speech: speak dictation.txt, then convert to 16 kHz s16le mono.
ffmpeg -i dictation.mp3 -ar 16000 -ac 1 -f s16le tmp/highlight/dictation.pcm

# 2. Capture (about two minutes). Paths below are the maintainer's; point them at your bundle and assets.
MLUVA_BUNDLE=/path/to/mluva-2.0.0 MLUVA_QWEN_ASSETS=/path/to/qwen-assets \
MLUVA_CAPTURE_PCM=$PWD/tmp/highlight/dictation.pcm \
HIGHLIGHT_REWRITE=1 HIGHLIGHT_SCENES=settings,history \
HIGHLIGHT_POLISHED="Release notes for Friday: the new build is a native Rust app. It opens faster, uses less memory, and runs speech recognition on this machine." \
OFFSCREEN_SCREEN_SPEC=2200x1600x24 \
  bash "$SKILL/scripts/run_isolated_x11.sh" tmp/highlight/capture -- python3 launch-video/highlight/capture/drive.py

# 3. Cut the clips, then render.
cd launch-video
npm ci --ignore-scripts
highlight/prepare.sh ../tmp/highlight/capture/session.*/out/take.mkv
npm run lint
npm run highlight:render
```

The result is `out/mluva-rust-highlight.mp4`. The clip cut times in `prepare.sh` follow the timeline of one capture; re-check them against `timeline.json` and the footage after a new take. Scene lengths and the clip frame counts are constants at the top of `src/highlight/Highlight.tsx`.
