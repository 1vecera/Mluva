# Mluva app intro

A 72.37-second, 1920×1080, 60 fps intro narrated by Daniel Vecera. His camera recording replaces synthesized speech. A small presenter cutout stays beside the native app footage, and his name appears during the introduction. The original Figma feature order remains: Welcome/setup, dictation, recorder, Polish, history, themes, Live rewrite, task questions and closing.

The original 83.27-second recording is shortened by 10.91 seconds using matched cuts in picture and sound. Every spoken word remains; delivery and pitch stay at their original speed. The app footage is retimed around the narration. Background removal preserves the recorded face and movement, and captions are burned into the movie. [Motion and voice decisions](MOTION.md) explain the treatment.

## Render

Requires Node/npm, FFmpeg/ffprobe and uv. Extract `mluva-intro-daniel-media-v5.tar.gz` at the worktree root before rendering; it restores the five inputs below under `launch-video/`, with private preparation sources under `tmp/`. Recordings, transcripts, caption text and downloaded music remain outside Git; a source checkout alone cannot render the film.

| Input | Content |
| --- | --- |
| `public/live/desktop-daniel.mp4` | Retimed native desktop, 1920×1200 at 60 fps. |
| `public/live/daniel-cutout.webm` | Daniel’s synchronized 30 fps portrait with an alpha channel. |
| `public/live/daniel.wav` | Edited and cleaned 48 kHz mono voice. |
| `public/live/edit.json` | Private caption text, timing, keyboard cues and composition duration. |
| `public/audio/music-bed.wav` | The existing user-selected music bed. |

```sh
cd launch-video
npm ci --ignore-scripts
npm run lint
npm run preview
npm run render -- --crf=16
npm run master
```

The master is `out/mluva-intro-live-master.mp4`. Mastering corrects the measured 2,048-sample render delay and matches sound duration to picture before loudness normalization. `Root.tsx` reads the local edit plan before rendering and sets the exact frame count; the scene layout lives in `src/MluvaIntro.tsx`. The 1536×960 desktop area leaves room for the presenter while retaining app controls, the pinned widget and subtitle clearance.

The private media archive includes the original camera recording, raw Scribe transcript and word timing, pause-cut plan, preparation scripts and review evidence. `reference/edit.json` records timing without transcript text. [Media hashes](reference/media.json), [asset provenance](reference/ASSETS.md) and [final review](review/final.md) describe the inputs and checks. These commands do not publish the film.

## Native capture

The retained take records actual Omarchy windows and wallpaper. Welcome/setup and recorder footage came from a separate native Hyprland compositor with GTK/Quickshell, Chromium on FT, VS Code, Ghostty, htop, Linear’s sign-in page and Firefox/Hacker News on desktop 2. Native receipts in [capture-verification.json](reference/capture-verification.json) establish actual tile/float/pin behavior. The earlier v3 archive preserves those source takes and capture tools.

`script/capture-storyboard.py` remains the production GTK fixture for future pickups. Use the installed offscreen verification runner for rehearsal. Host recording requires both `--host` and `MLUVA_AUTHORIZED_HOST_CAPTURE=1`, explicit desktop authorization, private demonstration app data and restoration of desktop state. The fixture does not synthesize global host input.

Provider responses and demonstration history are prepared fixtures. The retained transfer into Chrome used native F9 start/stop and Ctrl+V; it does not validate automatic insertion into every app or service response speed. Live rewrite stays visibly Experimental, and its retained footage includes the PR #45 short-draft scroll correction. This revision changes no app code.
