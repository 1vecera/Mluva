# Mluva app intro

A 78-second, 1920×1080, 60 fps intro following the feature order of the [original Figma storyboard](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=3-10), with longer holds and an immediate Mluva introduction. Native desktop footage shows Mluva, Chrome, Ghostty and Herdr. Remotion reframes it and adds brand cards, short keyboard cues and burned-in subtitles.

The main take records the actual Omarchy desktop, theme changes, clipboard paste and animated wallpaper together. Welcome/setup at 6–18 seconds is a native GTK pickup from a private X11 display. At 30–38 seconds, the production recording widget moves over a still from the clean Chrome/Ghostty/Herdr arrangement; this is a native widget pickup, not continuous host recording. Live rewrite and Grilling at 57–73 seconds use the take with the short-draft scroll correction. Microphone/provider responses and demonstration history are prepared fixtures; this film does not measure speech recognition quality or service latency. The source remains visible beside the draft, and the app labels Live rewrite as Experimental.

## Render

Requires Node/npm, FFmpeg/ffprobe and uv. The three media inputs below are intentionally local and ignored by Git; a source checkout alone does not include them. Restore them from the accompanying local media archive before rendering.

| Input | Content |
| --- | --- |
| `public/live/desktop.mp4` | Edited 78-second native footage, 1920×1200 at 60 fps. |
| `public/live/sarah.wav` | Fish Audio Sarah, positioned against the storyboard in a 78-second PCM track. |
| `public/audio/music-bed.wav` | User-selected downloaded music, crossfaded to 78 seconds and normalized before ducking. |

```sh
cd launch-video
npm ci --ignore-scripts
npm run lint
npm run preview
npm run render -- --crf=16
npm run master
```

The final file is `out/mluva-intro-live-master.mp4`; preview is 960×540 at 60 fps. The 16:10 desktop is contained in the 16:9 composition, with close-ups for app details and space for readable subtitles. Captions are rendered into the picture from `reference/narration-cues.json`; the local archive also includes an SRT. [Final review](review/final.md), [asset provenance](reference/ASSETS.md), [motion and voice decisions](MOTION.md), and [media hashes](reference/media.json) describe the delivery and its limits. Nothing in these commands publishes the video.

## Source map

| File | Responsibility |
| --- | --- |
| `src/MluvaIntro.tsx` | Native footage, eased reframing, brand cards, keyboard cues, subtitles and music ducking. |
| `reference/storyboard.json` | Original Figma shot order and readback. |
| `reference/edit.json` | Revised scene timing, source ranges and playback rates. |
| `script/capture-storyboard.py` | Production GTK states with prepared device/provider boundaries and private demonstration history. |
| `script/narration-{storyboard,brand}.txt` | Fish Audio script and two-phrase pronunciation pickup with direction tags. |
| `reference/narration-cues.json` | Source-to-film timing and exact Fish voice identity. |
| `reference/narration-scribe.json` | Word timing and speech-to-text checks of both Sarah source takes. |
| `script/master.py` | Measured two-pass loudness normalization and verification of encoded AAC. |

## Refresh app footage

Use the repository's Linux environment and the installed offscreen verification runner for rehearsal. The fixture refuses ordinary desktop execution unless both `--host` and `MLUVA_AUTHORIZED_HOST_CAPTURE=1` are supplied. Host recording requires explicit authorization, isolated demonstration app data, a prepared Chrome Guest window and restoration of desktop state afterward.

The fixture supports `--start-at` and `--end-at` for pickups, plus `--wait-for-start` to synchronize the app with a recorder through its output directory's `go` and `started.json` files. It records native snapshots and source/draft scroll measurements in `manifest.json`. For a complete host take, a separate recorder must capture the compositor and send actual F9 and window-manager shortcuts; the fixture deliberately does not synthesize global host input. Offscreen mode records its private X11 rehearsal automatically.

The old image-patching pipeline, recreated desktop surfaces, generated background and alternate compositions have been removed. Existing Git history preserves those earlier versions.
