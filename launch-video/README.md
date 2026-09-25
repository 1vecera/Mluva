# Mluva app intro

A 72.37-second, 1920×1080, 60 fps intro narrated by Daniel Vecera. His presenter cutout sits inside the native Omarchy desktop, and short captions at the top highlight selected spoken words. [Watch the published film](../README.md#meet-mluva).

The original 83.27-second camera recording is shortened by 10.91 seconds using matched picture/audio cuts. All 194 spoken tokens remain at their original speed and pitch. The original Figma feature order is retained. [Motion and voice decisions](MOTION.md) describe the treatment.

## Render

Requires Node/npm, FFmpeg/ffprobe and uv. Extract the private `mluva-intro-daniel-media-v7.tar.gz` at the worktree root; it restores these four inputs. A source checkout alone cannot render the film.

| Input | Content |
| --- | --- |
| `public/live/desktop-daniel.mp4` | Retimed native desktop, 1920×1200 at 60 fps. |
| `public/live/daniel-cutout.webm` | Synchronized 30 fps presenter footage with an alpha channel. |
| `public/live/edit.json` | Caption/word timing, keyboard cues and composition duration. |
| `public/audio/narration-mix.wav` | Approved 48 kHz stereo voice and music master. |

```sh
cd launch-video
npm ci --ignore-scripts
npm run lint
npm run render -- --crf=16
npm run master
```

The result is `out/mluva-intro-live-master.mp4`. Remotion renders silent picture; `script/master.py` checks its duration and attaches the approved mix as AAC without changing the video. This keeps sound aligned and avoids repeated voice processing or renderer-specific audio-delay corrections. The Remotion composition still plays the mix during interactive preview.

For a smaller full-length preview, use `npm run preview`, then `uv run script/master.py out/mluva-intro-live-preview.mp4`.

`Root.tsx` reads the local edit plan and sets the exact frame count. `src/MluvaIntro.tsx` lays out the scene; `src/PoppyCaptions.tsx` renders captions. The desktop fills a 1728×1080 area at (96, 0), preserving its 16:10 aspect. The cutout has no backdrop, outline, shadow or added edge fades.

## Showreel

The same project renders `MluvaShowreel`, a 15-second motion-design reel built from real widget captures and the brand files. [Its README](showreel/README.md) covers capture, staging, render and mastering.

## Sources and review

The v7 private archive holds the four current inputs, the final voice stem, processing scripts and verification records. Earlier private archives retain the original camera recording, raw transcript, preparation sources and native capture takes. These files stay outside Git. The approved finished movie is the public artifact; [input hashes](reference/media.json), [asset provenance](reference/ASSETS.md) and [final review](review/final.md) document it.

[Capture receipts](reference/capture-verification.json) establish actual recorder tile/float/pin behavior and the switch to Firefox/Hacker News. The old capture fixture, theme snapshots and superseded storyboard timing export have been retired. The [Figma storyboard](../docs/design/video-kit/README.md) remains the narrative reference.

Provider responses and demonstration history are prepared examples. The retained transfer into Chrome uses native F9 start/stop and Ctrl+V; it does not establish service latency, recognition accuracy or automatic insertion into every app. Live rewrite remains visibly Experimental, and its footage retains PR #45's short-draft scroll correction. This revision changes no application code.
