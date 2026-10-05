# Native Rust release highlight

A 23.5-second, 1920×1080, 60 fps H.264 film with AAC sound about one thing: the native Rust release opens sooner. It reuses the showreel's identity (black surface, Adwaita Sans, JetBrains Mono, one red accent) and cuts on the drops of a generated score.

| Scene | Seconds | Content |
| --- | --- | --- |
| Title | 0–3.75 | "Opens faster." and the glossy mark, building to the first drop |
| First window | 3.75–11.25 | 1.75× sooner: 456 → 260 ms |
| Ready on the bus | 11.25–15.5 | 5.2× sooner: 217 → 42 ms |
| On disk | 15.5–19.25 | 3.3× smaller: about 161 → 49 MiB |
| End card | 19.25–23.5 | Lockup and `github.com/1vecera/Mluva` |

## What each number rests on

All three compare Python 1.6.0 with published Rust 2.0.0. They are startup and footprint figures, not inference speed.

| On screen | Source and arithmetic |
| --- | --- |
| First window 456 → 260 ms (−43%, 1.75×) | Median of six measured starts per version in [release-startup.json](../../docs/verification/rust-release-performance/release-startup.json): 455.6 → 259.8 ms (455.6 / 259.8 = 1.75). Same host, warm cache. Rust's slowest start (298 ms) beat Python's fastest (402 ms), per the report's ranges |
| Ready on D-Bus 217 → 42 ms (5.2×) | Same file, `bus_ms` medians: 216.5 → 41.8 ms. This is D-Bus name ownership, which precedes the visible window; the report says it is not recording readiness |
| Install about 161 → 49 MiB (3.3×) | Measured for this film, see below. Models and caches are excluded on both sides |
| No Python runtime | The shipped application is native Rust ([install notes](../../README.md#install)) |

Install size: 1.6.0 is its app files plus the Python environment its installer builds (`uv sync --no-dev --frozen`, verified in `linux/install.sh` at the `release/1.6.0` tag). I built that environment offline from the tag's lockfile and measured it with `du -sm`: 151 MiB environment, plus 4 MiB of `mluva_linux` and 6 MiB of `resources`, so about 161 MiB. 2.0.0 is the unpacked published bundle (`mluva-2.0.0-omarchy-x86_64.tar.gz`, 17 MB compressed) at 49 MiB. The 1.6.0 environment reuses the system PyGObject, which is not counted; the Rust bundle likewise relies on system GTK. This is one measurement on one machine, not a statistical sample.

The film does not claim 2.0.1 as public (2.0.0 is the latest GitHub release), universal inference speedups or complete platform parity.

## Sound

The score is ElevenLabs Music v2.5 (take B of two, prompted for instrumental electronic at 128 BPM; it came out at about 130 BPM). A Scribe pass found no speech. `analyze_score.py` and the energy/onset checks put its drops at 3.75 s (first hit from silence), 11.25 s (bass returns four bars later), 15.5 s (low-end jump) and 19.25 s (final hit after a 0.7 s gap). The composition reads those times from `src/highlight/score.json`, so every scene change and flash lands on a drop, and the kick pulse on the multiplier and Rust bar follows the 0.4615 s beat. The render is silent; `master.sh` trims the score to the film, fades the last 1.1 s, masters it to about −14 LUFS and muxes AAC without touching the video stream. The score file stays out of Git. If you regenerate the music, run `analyze_score.py`, update `score.json` and re-render; the structure of a new take will differ.

## Render

Needs Node/npm, FFmpeg, uv, the local score at `public/highlight/score.mp3` and the showreel's staged `public/showreel/{brand,fonts}`.

```sh
cd launch-video
npm ci --ignore-scripts
uv run --with numpy highlight/analyze_score.py public/highlight/score.mp3   # check the drops against score.json
npm run lint
npm run highlight:render
```

The result is `out/mluva-rust-highlight.mp4`.
