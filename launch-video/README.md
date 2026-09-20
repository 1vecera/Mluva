# Mluva app intro

A 59-second, 1920×1080, 60 fps intro following the [original Figma storyboard](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=3-10). The picture is recorded on the actual Omarchy desktop with native Mluva, Chrome, Ghostty and Herdr. Remotion reframes that footage and adds two brand cards and short keyboard cues.

The app pixels, window movement, theme changes, clipboard paste and animated wallpaper are recorded together. The Live rewrite and Grilling section uses a replacement take at 42–55 seconds after fixing the short-draft scroll origin. Microphone/provider responses and demonstration history are prepared fixtures; this film does not measure speech recognition quality or service latency. The original source remains visible beside the draft, and the app labels Live rewrite as Experimental.

## Render

Requires Node/npm, FFmpeg/ffprobe and uv. The three media inputs below are intentionally local and ignored by Git; a source checkout alone does not include them. Restore them from the accompanying local media archive before rendering.

| Input | Content |
| --- | --- |
| `public/live/desktop.mp4` | Edited 59-second native desktop recording, 1920×1200 at 60 fps. |
| `public/live/sarah.wav` | Fish Audio Sarah, positioned against the storyboard in a 59-second PCM track. |
| `public/audio/music-bed.wav` | User-selected downloaded music, crossfaded to cover 59 seconds. |

```sh
cd launch-video
npm ci --ignore-scripts
npm run lint
npm run preview
npm run render -- --crf=16
npm run master
```

The final file is `out/mluva-intro-live-master.mp4`; preview is 960×540 at 60 fps. The original 16:10 desktop is contained in the 16:9 composition, with gentle close-ups for app details. [Final review](review/final.md), [asset provenance](reference/ASSETS.md), [motion and voice decisions](MOTION.md), and [media hashes](reference/media.json) describe the delivery and its limits. Nothing in these commands publishes the video.

## Source map

| File | Responsibility |
| --- | --- |
| `src/MluvaIntro.tsx` | Native footage, eased reframing, brand cards, keyboard cues and music ducking. |
| `reference/storyboard.json` | Original Figma shot order and readback. |
| `script/capture-storyboard.py` | Production GTK states with prepared device/provider boundaries and private demonstration history. |
| `script/narration-storyboard.txt` | Fish Audio script with spoken pronunciation and direction tags. |
| `reference/narration-cues.json` | Source-to-film timing and exact Fish voice identity. |
| `reference/narration-scribe.json` | Speech-to-text check of the final Sarah source take. |
| `script/master.py` | Measured two-pass loudness normalization and verification of encoded AAC. |

## Refresh app footage

Use the repository's Linux environment and the installed offscreen verification runner for rehearsal. The fixture refuses ordinary desktop execution unless both `--host` and `MLUVA_AUTHORIZED_HOST_CAPTURE=1` are supplied. Host recording requires explicit authorization, isolated demonstration app data, a prepared Chrome Guest window and restoration of desktop state afterward.

The fixture supports `--start-at` and `--end-at` for pickups, plus `--wait-for-start` to synchronize the app with a recorder through its output directory's `go` and `started.json` files. It records native snapshots and source/draft scroll measurements in `manifest.json`. For a complete host take, a separate recorder must capture the compositor and send actual F9 and window-manager shortcuts; the fixture deliberately does not synthesize global host input. Offscreen mode records its private X11 rehearsal automatically.

The old image-patching pipeline, recreated desktop surfaces, generated background and alternate compositions have been removed. Existing Git history preserves those earlier versions.
