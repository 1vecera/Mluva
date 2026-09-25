# Mluva showreel '26

A 15-second, 1920×1080, 60 fps motion-design reel: eight chapters, one bar each of a 128 BPM score. It follows the chapter structure of a motion-design showreel (kinetic type, UI animation, motion systems, 3D particles, logo animation, data visualisation, type montage, end card) and tells Mluva's story through it.

| Frames | Chapter | What it shows |
| --- | --- | --- |
| 0–113 | 01 Kinetic type | The recording light, then *Speak freely · Shape it · Keep every word*, each ending in the glossy mark as its full stop |
| 113–226 | 02 UI animation | Real footage of the production Omarchy widget: live words, F9 start and stop, Transcribing, Review |
| 226–338 | 03 Motion systems | Speak → Transcribe → Copy, then Rewrite on demand, one node per beat |
| 338–451 | 04 3D / particles | A sphere of the real widget in all 22 installed Omarchy themes, counted one by one |
| 451–563 | 05 Logo animation | The drop: the light bursts into the mark, which settles into the large-mark lockup |
| 563–676 | 06 Data visualisation | Local or cloud: the speech and rewrite engines from the provider guide |
| 676–788 | 07 Type montage | Eight cuts on eighth notes: F9, Live rewrite, Polish/Structure, history, Incognito, Ctrl+P, export, open source |
| 788–900 | 08 End card | Lockup, "The most delightful dictation for Omarchy." and github.com/1vecera/Mluva |

`src/showreel/timing.ts` holds the beat grid (`0.014 s + n × 0.46875 s`); every chapter derives its keyframes from it. `theme.ts` holds the identity: black, #F5F5F5 ink, one red accent, Adwaita Sans on a five-step scale and JetBrains Mono labels.

## Real UI and honest claims

No part of the app is redrawn. `capture_widget.py` drives the production QML widget, bridge and Omarchy controls on the private X11 display from `dev/run-isolated.sh`; only the recognition events are scripted, over the app's own D-Bus signal. The take uses Omarchy's Vantablack theme at `QT_SCALE_FACTOR=3` and is recorded losslessly at 60 fps. Words stream about six times faster than speech, but the widget's timer runs at natural pace (about 156 words per minute), so the chapter is a time-lapse of a seven-second note and says so on screen. `MLUVA_CAPTURE_MODE=themes` photographs the recording and review states under every theme in `/usr/share/omarchy/themes`.

The reel shows the take as its original PNG frames; a per-frame RGB MD5 comparison against the capture matches all 308 frames. The transcript in chapter 03 is a crop of one of those frames. Callout leaders stop outside the widget, so no captured pixel is covered.

The logo animates two clipped copies of `mluva-logo-large-mark-on-dark.svg`. Whenever mark and wordmark are at rest the reel draws that file directly: `ShowreelLockupCheck` renders the resting frames of chapters 05 and 08 against the brand SVG, and the pixel diff is zero in both.

Feature names and limits come from the repository docs: the providers from `docs/provider-selection.md`, and the Experimental tags on Live rewrite, Incognito, the local model and compatible APIs from `docs/feature-maturity.md` and the README.

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
npm run showreel:prepare   # stage public/showreel/ from tmp/showreel/
npm run showreel:render    # silent picture → out/mluva-showreel.mp4
npm run showreel:master    # mix, master and mux → out/mluva-showreel-master.mp4
```

`prepare.py` also builds `AdwaitaSans-Black-NoOverlap.ttf`, a static instance (wght 900, opsz 32) with overlaps removed, because `-webkit-text-stroke` on the variable font draws every contour overlap as a seam. Chrome renders display sizes at opsz 32, so outlined and solid type match.

The render is silent. `mix.py` places the score and effects from `src/showreel/cues.json` sample-accurately (each cue marks where the effect peaks), masters to −14 LUFS with a −1.5 dBTP ceiling using FFmpeg's two-pass loudnorm plus a small trim, and muxes 320 kbit/s AAC. Remotion Studio plays the same cue sheet for previews.

## Sound

The score and effects were generated with ElevenLabs Music v2 and Sound Effects v2 on Daniel's account ([flow](https://elevenlabs.io/app/flows/Ilr5SlTt7Blt0Xay6hmO)). The score is take 1 of the first batch; its drop falls on beat 16 and its final impact on beat 28, which is why the logo lands mid-reel. Effect levels were set by analysis, not by ear, and live in `cues.json`. Audio files stay out of Git with the other staged media (`public/showreel/` is ignored); keep the downloaded takes with the local media archive.
