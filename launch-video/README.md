# Mluva app intro

An 84-second, 1920×1080, 60 fps intro following the feature order of the [original Figma storyboard](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=3-10). It opens directly on Mluva's native Welcome screen while Sarah introduces “The most delightful dictation for Omarchy.” Remotion reframes the desktop recording and adds short keyboard cues, burned-in subtitles and a closing brand card.

The main take records the actual Omarchy desktop, theme changes, clipboard paste and animated wallpaper together. New Welcome/setup footage at 0–18 seconds and recorder footage at 30–44 seconds use a separate real Hyprland compositor, with native GTK/Quickshell, Chromium on FT, VS Code, Ghostty, htop and Linear's dark sign-in page arranged like the supplied reference. The recorder genuinely tiles, floats again and remains pinned while switching to Firefox/Hacker News on desktop 2. This is continuous native compositor footage; no desktop still or recreated widget is used in that scene. The isolated session leaves the visible desktop alone.

Live rewrite and Grilling at 63–79 seconds retain the take with the short-draft scroll correction. Microphone/provider responses and demonstration history are prepared fixtures; this film does not measure speech recognition quality or service latency. The source remains visible beside the draft, and the app labels Live rewrite as Experimental.

## Render

Requires Node/npm, FFmpeg/ffprobe and uv. The three media inputs below are intentionally local and ignored by Git; a source checkout alone does not include them. Restore them from the accompanying local media archive before rendering.

| Input | Content |
| --- | --- |
| `public/live/desktop.mp4` | Edited 84-second native footage, 1920×1200 at 60 fps. |
| `public/live/sarah.wav` | Fish Audio Sarah, positioned against the storyboard in an 84-second PCM track. |
| `public/audio/music-bed.wav` | User-selected downloaded music, crossfaded to 84 seconds and normalized before ducking. |

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
| `src/MluvaIntro.tsx` | Native footage, eased reframing, closing brand card, keyboard cues, subtitles and music ducking. |
| `reference/storyboard.json` | Original Figma shot order and readback. |
| `reference/edit.json` | Revised scene timing, source ranges and playback rates. |
| `script/capture-storyboard.py` | Production GTK states with prepared device/provider boundaries and private demonstration history. |
| `script/narration-storyboard.txt` | Energetic Sarah script with delivery tags and phoneme controls for Mluva and Live. |
| `reference/narration-cues.json` | Source-to-film timing and exact Fish voice identity. |
| `reference/narration-scribe.json` | Word timing and speech-to-text check of the Sarah source take. |
| `reference/capture-verification.json` | Native tiling, floating, pinning and desktop-switch evidence. |
| `script/master.py` | Measured two-pass loudness normalization and verification of encoded AAC. |

## Refresh app footage

Use the repository's Linux environment and the installed offscreen verification runner for rehearsal. The fixture refuses ordinary desktop execution unless both `--host` and `MLUVA_AUTHORIZED_HOST_CAPTURE=1` are supplied. Host recording requires explicit authorization, isolated demonstration app data, a prepared Chrome Guest window and restoration of desktop state afterward.

The fixture supports `--start-at` and `--end-at` for pickups, plus `--wait-for-start` to synchronize the app with a recorder through its output directory's `go` and `started.json` files. It records native snapshots and source/draft scroll measurements in `manifest.json`. For a complete host take, a separate recorder must capture the compositor and send actual F9 and window-manager shortcuts; the fixture deliberately does not synthesize global host input. Offscreen mode records its private X11 rehearsal automatically.

The v3 input archive also contains the isolated Hyprland pickup scripts, source recordings and capture receipts. These run a nested compositor with a private runtime, bus and application profiles, then record its output with wf-recorder. Recorder actions dispatch the same Hyprland operations used by Omarchy's Super+T and workspace shortcuts. The capture's incorrect full-range H.264 flag is corrected by stream copy; decoded colors were compared with native PNG captures. Blur was enabled only in the private compositor for legibility over the tiled windows.

The old image-patching pipeline, recreated desktop surfaces, generated background and alternate compositions have been removed. Existing Git history preserves those earlier versions.
