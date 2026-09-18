# Mluva launch film (Remotion)

A 59-second product film rendered entirely from code. Every screen is recreated in React, the
narration is one Fish Audio take cut by its transcript, captions follow the spoken words, and the
identity comes from `docs/brand`. No screen recordings are required.

## Render

```sh
cd launch-video
npm install
npx remotion render src/index.ts MluvaLaunch out/mluva-launch.mp4          # 1920×1080, 60 fps
npx remotion render src/index.ts MluvaLaunchPreview out/preview.mp4 --scale=0.5   # quick 30 fps check
npx remotion still src/index.ts MluvaLaunchPreview out/frame.png --frame=300      # one frame
```

`npx remotion studio` opens the interactive editor in a browser if you want to scrub.

## How the film is built

| Layer | Where | What it does |
| --- | --- | --- |
| Script | `script/narration.txt` | Eleven lines, one per scene. The film's copy lives here. |
| Voice | `public/audio/narration.wav` | Fish Audio `s2.1-pro`, one request so the voice is identical across scenes. |
| Transcript | `script/narration-scribe.json` | ElevenLabs Scribe v2 word timestamps of that WAV. |
| Cue sheet | `script/build-timing.mjs` → `src/generated/timing.json` | Aligns script lines to transcript words, cuts the WAV into eleven segments, places each scene with a lead-in and a hold. Edit the `BEATS` table to retime; the script refuses to exceed 60 s. |
| Scenes | `src/scenes/S01…S11` | One component per beat. Each reads local seconds with `useT()` and drives its own camera move. |
| UI kit | `src/components/` | Omarchy desktop chrome and windows, the recorder, keycaps, the workspace with history sidebar and panes, the welcome card, command palette and diff text. Palettes are the real Omarchy Nord, Tokyo Night and Rosé Pine values from `docs/design/video-kit/tokens.json`. |
| Background | `src/components/WaterBackground.tsx` | Procedural water: drifting gradients pushed through SVG turbulence displacement. |
| Captions | `src/components/Captions.tsx` | Pages built from the transcript words, active word highlighted. |
| Sound | `src/Soundtrack.tsx` + per-scene `<Audio>` | Narration segments, ElevenLabs Music bed ducked under the voice, small UI sound effects on key moments. |
| Identity | `src/brand.ts`, `src/components/Logo.tsx`, `public/brand/` | Copies of `docs/brand/svg`. The lockup animates as one object so the mark/wordmark ratio stays exact. |

## Change the words

1. Edit `script/narration.txt` (keep eleven paragraphs).
2. Regenerate the voice (Fish Audio helper from Daniel's skills, `--model s2.1-pro --allow-paid`, WAV output to `public/audio/narration.wav`).
3. Transcribe with Scribe v2 at word granularity to `script/narration-scribe.json`.
4. `node script/build-timing.mjs`, then render.

## Swap the identity

Copy new SVGs into `public/brand/` and point the paths in `src/brand.ts` at them. Scenes never reference files directly.

## Assets and licences

- JetBrains Mono (OFL) from `linux/quickshell/mluva.dictation/fonts`.
- Logo set from `docs/brand` (see its README).
- Music: generated with ElevenLabs Music v2 on Daniel's account (`public/audio/music-a.mp3`, a second take `music-b.mp3` is kept for comparison).
- Sound effects: `public/sfx/*.wav` from remotion.media (Remotion's free effect library).
- Remotion licence: free for individuals and companies up to three people; check before wider company use.
