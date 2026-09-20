# Mluva app intro

A 46.73-second, 1920×1080/60 fps intro built in Remotion from production GTK captures. It follows dictation, editing, experimental Live rewrite, saved history, provider choices and Omarchy themes. A continuous narration take drives the edit; an original Blender animation provides the backdrop.

The UI reconstruction preserves native raster assets, with registered native clock patches and a recorded sidebar transition. It is not an independently redrawn vector UI. Capture content and provider responses are synthetic; timings are edited and do not measure recognition or rewrite latency. The film labels Live rewrite as Experimental and carries a demonstration disclosure.

## Render and verify

Requires Node/npm, FFmpeg/ffprobe and uv. The checked environment used Node 26.8.1, FFmpeg 9.0.1 and uv 0.12.5. Capture additionally requires the repository's Linux GTK environment and isolated desktop runner; Blender is needed only to regenerate the supplied background.

```sh
cd launch-video
npm ci --ignore-scripts
npm run lint
npm run fidelity
npm run render -- --crf=16
npm run master
```

The review copy is `out/mluva-intro-master.mp4`. `npm run preview` makes a 960×540/30 fps working copy; `npx remotion studio src/index.ts` opens a local editor when wanted. Renders and temporary evidence are ignored by Git. The committed [review](review/final.md), [iteration log](review/iterations.md), [quality method](review/quality.md) and [asset record](reference/ASSETS.md) explain the result and its limits.

## Small source map

| File | Responsibility |
| --- | --- |
| `src/MluvaIntro.tsx` | Seven story beats, camera, annotations, continuous narration, music and four action clicks. |
| `src/Surfaces.tsx` | Intact native image, black capture backing and registered clock patch. |
| `script/build-intro.mjs` | Checks script/transcript agreement, derives word-based cues and captions, writes `src/generated/intro.json`. |
| `script/capture-app.py` | Production GTK navigation with synthetic device/provider boundaries, private history and capture metadata. |
| `script/import-capture.py` | Validates capture hashes/dimensions before importing; derives the clock patch from actual glyph changes. |
| `script/render-references.mjs`, `script/verify-fidelity.py` | Renders source-size browser output and compares it against untouched native PNGs independently composited with Pillow. |
| `script/create-background.py` | Reproducible six-second Blender wave loop, played at half speed. |
| `script/master.py` | Two-pass loudness normalization, then verification of the encoded audio. |

## Refresh native assets

Run from the repository root after preparing the Linux environment with `make linux-setup`. Choose a free private display number. The runner supplies private display, configuration, history and theme state; the capture script refuses a normal desktop invocation.

```sh
OFFSCREEN_DISPLAY_NUMBER=196 OFFSCREEN_SCREEN_SPEC=2560x1600x24 dev/run-isolated.sh tmp/intro-capture -- env ADW_DISABLE_PORTAL=1 GTK_A11Y=none GDK_SCALE=2 GDK_DPI_SCALE=1 GSK_RENDERER=cairo PYTHONPATH=linux uv run --project linux python launch-video/script/capture-app.py
uv run launch-video/script/import-capture.py tmp/intro-capture
ffmpeg -ss 7.7 -i tmp/intro-capture/native-navigation.mp4 -t 1.2 -c:v libx264 -preset slow -crf 12 -an launch-video/public/ui/history-open.mp4
```

Inspect the new transition's start/end before replacing it: capture scheduling can vary with machine load. The PNGs use the paintable's intrinsic 1040×640 logical size at 2× density. The manifest records source hashes, state times, dimensions and palette hashes. Do not substitute widget allocation dimensions or stretch captures to a different aspect ratio. Run fidelity and review the film again after importing.

## Change the voice or background

The canonical text and Fish delivery directions are in `script/narration-intro.txt`. The selected take uses `s2.1-pro-free`, speed 0.78, three tone cues and three breaks. Keep the take continuous, remux its streaming WAV into a normal PCM WAV, then transcribe the chosen file with Scribe v2 into `reference/narration-scribe.json`. Updating the wording requires reviewing the caption word ranges and cue indices in `build-intro.mjs`; it deliberately rejects an unreviewed transcript or word-count change. Generation credentials are never required to render the committed assets.

Regenerate the original background from the repository root in a disposable Blender process (verified with Blender 5.2.0 LTS):

```sh
blender --background --factory-startup --python launch-video/script/create-background.py -a
ffmpeg -framerate 30 -i tmp/background/frame-%04d.png -c:v libx264 -crf 16 -pix_fmt yuv420p -movflags +faststart launch-video/public/background.mp4
```

The background generator owns that fresh scene. Do not run it inside an existing project. Check the last/first frame seam and the composite behind text after any material or motion change.
