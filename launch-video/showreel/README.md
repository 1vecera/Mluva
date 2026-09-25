# Mluva showreel '26

A 15-second, 1920×1080, 60 fps motion-design reel: eight chapters, one bar each of a 128 BPM score. It follows the chapter structure of a motion-design showreel (kinetic type, UI animation, logo animation, motion systems, 3D particles, data visualisation, type montage, end card) and tells Mluva's story through it.

| Frames | Chapter | What it shows |
| --- | --- | --- |
| 0–113 | 01 Kinetic type | The recording light, then *Speak freely · Shape it · Keep every word*, each ending in the glossy mark as its period |
| 113–226 | 02 UI animation | Real footage of the production Omarchy widget: live words, F9 start and stop, Transcribing, Review |
| 226–338 | 03 Motion systems | Speak → Transcribe → Copy → Rewrite, one node per beat |
| 338–451 | 04 3D / particles | A sphere of the real widget in all 22 installed Omarchy themes |
| 451–563 | 05 Logo animation | The drop: the light bursts into the mark, which settles into the large-mark lockup |
| 563–676 | 06 Data visualisation | Local or cloud: the speech and rewrite engines from the provider guide |
| 676–788 | 07 Type montage | Eight cuts on eighth notes: F9, Live rewrite, Polish/Structure, history, Incognito, Ctrl+P, export, open source |
| 788–900 | 08 End card | Lockup, tagline and github.com/1vecera/Mluva |

`src/showreel/timing.ts` holds the beat grid (`0.014 s + n × 0.46875 s`); every chapter derives its keyframes from it.

## Real UI only

No part of the app is redrawn. `capture_widget.py` drives the production QML widget, bridge and Omarchy controls on the private X11 display from `dev/run-isolated.sh`; only the recognition events are scripted (text, timer, input level over the app's own D-Bus signal). It records a lossless 60 fps take at `QT_SCALE_FACTOR=3` in the Kanagawa theme, and in `MLUVA_CAPTURE_MODE=themes` photographs the recording and review states under every theme in `/usr/share/omarchy/themes`. The reel plays the take as its original PNG frames; `prepare.py` extracts them and a per-frame RGB MD5 comparison against the capture matches all 308 frames.

The logo animates two clipped copies of `mluva-logo-large-mark-on-dark.svg`. Whenever mark and wordmark are at rest the reel draws that file directly: `ShowreelLockupCheck` renders the resting frames of chapters 05 and 08 against the brand SVG, and the pixel diff is zero in both.

## Build

Requires Node/npm, FFmpeg, ImageMagick, uv, and for captures Xvfb, D-Bus and Quickshell with Omarchy installed.

```sh
# From the repository root: capture the widget take and the theme stills.
env -i PATH="$PATH" HOME="$HOME" USER="$USER" LANG=C.UTF-8 OFFSCREEN_ENABLE_ATSPI=1 \
  OFFSCREEN_SCREEN_SPEC=3840x2400x24 bash dev/run-isolated.sh tmp/showreel/widget -- \
  env PYTHONPATH=linux GTK_A11Y=none GSK_RENDERER=cairo QT_SCALE_FACTOR=3 \
  uv run --project linux --locked python launch-video/showreel/capture_widget.py
# Repeat with tmp/showreel/themes and MLUVA_CAPTURE_MODE=themes.

cd launch-video
npm ci --ignore-scripts
uv run showreel/prepare.py --audio ../tmp/showreel/audio   # stage public/showreel/
npx remotion render src/index.ts MluvaShowreel out/mluva-showreel.mp4 --muted --crf=14
uv run showreel/mix.py out/mluva-showreel.mp4               # → out/mluva-showreel-master.mp4
```

The render is silent. `mix.py` places the score and effects from `src/showreel/cues.json` sample-accurately (each cue marks where the effect peaks), masters to −14 LUFS with a −1.5 dBTP ceiling using FFmpeg's two-pass loudnorm, and muxes 320 kbit/s AAC. Remotion Studio plays the same cue sheet for previews.

## Sound

The score and effects were generated with ElevenLabs Music v2 and Sound Effects v2 on Daniel's account ([flow](https://elevenlabs.io/app/flows/Ilr5SlTt7Blt0Xay6hmO)). The score is take 1 of the first batch; its drop falls on beat 16 and its final impact on beat 28, which is why the logo lands mid-reel. Audio files stay out of Git with the other staged media (`public/showreel/` is ignored); keep the downloaded takes with the local media archive.
