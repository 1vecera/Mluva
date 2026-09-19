# Asset provenance

| Asset | Origin and treatment |
| --- | --- |
| `public/ui/*.png` | Production Mluva GTK from this worktree in a private Xvfb session. Forty untouched 2080×1280 PNGs, synthetic demonstration text, private seeded history and fixture provider outputs. `captures.json` records each SHA-256, app source hashes, capture script hash and theme hash. No personal conversations or credentials are used. |
| `public/ui/clocks/*.png` | Crops of the original native timer states, independently composited over black and registered at integral logical coordinates. `clock.json` stores the logical rectangle. |
| `public/ui/history-open.mp4` | A 1.2-second excerpt of the same real private-desktop recording, 2080×1280/30 fps. Navigation uses the production sidebar callback. |
| `public/background.mp4` | Original Blender geometry, materials, lighting and periodic shape-key animation from `script/create-background.py`; six seconds at 1920×1080/30 fps. No stock footage. |
| `public/audio/intro-voice.wav` | Fish Audio continuous take, `s2.1-pro-free`, speed 0.78; selected take v5, 42.916313 seconds. Three tone directions, three breaks and explicit Mluva phonemes. Remuxed to a valid 48 kHz PCM WAV. Text/directions are in `script/narration-intro.txt`; Scribe v2 word timing is in `narration-scribe.json`. |
| `public/audio/music-a.mp3` | Retained from the prior film commit `8ee4ec1`; its source README records generation with ElevenLabs Music v2 on the owner's account. The separate downloaded bed and unused alternative were removed. |
| `public/sfx/mouse-click.wav` | Retained unchanged from the prior film. Remotion identifies [this effect](https://www.remotion.dev/docs/sfx/mouse-click) as Pixeliota's Mouse Click Sound under CC0. |
| `public/brand/mluva-logo-large-mark-on-dark.svg` | Exact repository identity asset from `docs/brand/svg/`, used as a complete lockup. Other unused logo copies were removed. |
| `public/fonts/JetBrainsMono-Regular.ttf` | Repository JetBrains Mono font; its SIL Open Font License is included alongside as `OFL.txt`. Unused weights/styles were removed. |
| `reference/{nord,tokyo-night,rose-pine}.toml` | Omarchy theme colour inputs applied through Mluva's actual theme controller inside private capture state. |

The film is an asset-backed Remotion reconstruction: native typography, icons, labels, wrapping and layout remain the captured pixels. Film headings, captions, camera motion, keyboard/pointer cues and backdrop are presentation layers. The fixtures demonstrate UI states; they do not validate external provider quality, F9 portal integration, measured response times or production microphone behavior.
