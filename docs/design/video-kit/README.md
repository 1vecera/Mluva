# Mluva video kit

The [editable Figma kit](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj) contains the assets for a 59-second Mluva film, updated alongside the app on `feat/mluva-video-brief` from main `fbd793e`. Start with Getting started, follow each page guide, and use the [Delivery directory](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=69-558) to find an asset.

The final read-back contains 12 pages, 120 variables, 60 components, eight variant families, 12 editable app screens, six real runtime capture masters, 11 annotated shots and eight native motion studies with 96 tracks. Frame notes explain each state’s purpose outside export bounds. The assembled film and sound mix are subsequent production work.

| Source | Purpose |
| --- | --- |
| [tokens.json](tokens.json) | Palette source, semantic aliases and layout/type/motion/content controls |
| [asset-manifest.json](asset-manifest.json) | Master links, exposed properties, shot timing and exact demonstration text |
| [figma-snapshot.json](figma-snapshot.json) | Native read-back and variable, font, overlap and timing evidence |
| [reflow-canvas.mjs](reflow-canvas.mjs) | Apply canvas/stage spacing and pair purpose notes with assets |
| [retime-motion.mjs](retime-motion.mjs) | Apply Motion variables to all 96 existing native tracks |
| [breathing-light.blend](breathing-light.blend) | Editable breathing surface with period/amplitude/deformation controls |
| [create-breathing-light.py](create-breathing-light.py) | Rebuild or render the transparent Blender loop |
| [capture-provenance.json](capture-provenance.json) | Environment, source/media hashes and runtime evidence |
| [verify.py](verify.py) | Check every source token and asset contract against the captured state |
| [contrast-review.json](contrast-review.json) | Modeled color pairs and rejected low-opacity text treatment |
| [ATTRIBUTION.md](ATTRIBUTION.md) | Brand, font, symbol, footage and desktop sources |

## Editing shared assets

Change the master first, then use exposed text/boolean properties for a particular shot. Live properties hide Original or Draft and its divider; the app prevents hiding both. Completed/history views retain the app’s vertical source/reply structure. `Commands / Surface` supplies both the complete palette screen and storyboard close-up. Motion-local copies remain separate where native timelines require editable descendants.

Theme modes are Nord, Tokyo Night and Rosé Pine examples. Semantic colors alias primitives; source JSON retains checked hexadecimal values and OKLCH. Any installed Omarchy theme supplies the app’s real palette. Raster captures retain their recorded pixels and cannot recolor from a variable.

Both `font/ui` and `font/display` default to JetBrains Mono. Five shared text styles bind family, size and leading. The custom wordmark is outlined artwork derived from JetBrains Mono Medium; changing a text variable does not reshape it. [sculpt_wordmark.py](../../../dev/sculpt_wordmark.py) regenerates its optical spacing and narrowed “l”. The app bundles unchanged Regular, Medium, Bold and Italic fonts.

Layout variables bind repeated dimensions, padding, gaps, strokes, radii and surface opacity. Figma opacity values are percentages: `opacity/window = 82` resolves to alpha `0.82`. JSON retains both units. Surface transparency does not reduce text opacity.

Canvas x/y and native keyframe timestamps do not remain bound to variables. Load the Figma use/motion skills before running the helpers through the Plugin API. Both default to a dry run; review changes, then pass `dryRun: false`. Run canvas reflow once per page in separate calls. Bound component dimensions update directly; reflow arranges the surrounding boards and notes. Figma edits do not change application constants.

```js
await reflowMluvaCanvas(figma, { pageId: "3:6" });
await retimeMluvaMotion(figma);
```

## Motion and footage

Recorder overflow reveals 66 words in seven naturally wrapped lines and moves only forward. The app predicts scrolling from recent speech rate; Figma uses an authored cadence. Local previous/current diffs fade changed runs and keep unchanged middle text stable. The spoken correction changes Tuesday to Wednesday; Polish preserves Tuesday. Grilling retains the answer while replacing its question.

The retimer handles every native track, preserving keyframe identities, easing and end holds, and extending timelines only when necessary. Eight timing-variable changes updated 90 tracks; restoring defaults left a zero-change dry run. Eleven fresh-instance checks exercised width, padding, controls, icons, sidebar, content, pane visibility, typography and opacity. All 57 semantic aliases resolved across three modes. A recorder-width stress pass also reflowed all eight variants without overlaps.

The connector cannot import native video, and its GIF renderer did not display the animation. Water therefore uses a full-resolution poster and editable camera motion in Figma. Use the privately supplied licensed 10-second 1080p/24 fps MP4 for actual ripples. The Figma breathing study uses four poses; its transparent 384×384/30 fps WebM supplies the continuous loop. The desktop timeline crossfades captured endpoints; the separate 8-second 1440p/30 fps MP4 contains actual Omarchy compositor movement.

The Blender source is an art-directed metaball surface, not a physical liquid simulation. Select `Controls / Breath` and change period, amplitude or deformation. After changing period in Blender’s UI, set playback end to `round(period_seconds * fps)`. The generator does this automatically:

```sh
blender --background --factory-startup --python-exit-code 1 \
  --python docs/design/video-kit/create-breathing-light.py -- \
  --output tmp/breathing-light.blend --period 3.4 --fps 30 --render-dir tmp/breath-frames
```

Use a fresh background Blender 5.2 process. The generator refuses to replace an output unless `--force` is explicit. The saved scene uses a relative frame path. The app uses a lightweight harmonic silhouette with the same 3.4-second cadence and exact peak bounds. Reduced motion uses a steady light and immediate text updates.

## Verification and handoff

```sh
uv run --no-project python docs/design/video-kit/verify.py
node --check docs/design/video-kit/retime-motion.mjs
node --check docs/design/video-kit/reflow-canvas.mjs
git diff --check
```

All 12 pages passed canvas/section overlap and font checks. The sole text overrun is an intentional seven-line transcript inside a clipped five-line viewport. Motion exports were sampled across phases; final reviews corrected selected-row contrast, narrow pane-status labels and desktop framing. The audit verifies captured evidence, not subsequent Figma edits.

App checks cover 440 unit tests, Ruff, GTK conversation/Live/provider/prompt flows, minimum-size welcome, full-window settings, pane shortcuts, exact Unicode delivery, revision animation, manual scrolling after contractions and reduced motion. A disposable Omarchy ARM VM supplied real Browser/Ghostty/Herdr captures, same-window float/tile checks, palette/border checks and the WebKit Markdown/workspace gate. Host desktop input was not used. Synthetic examples do not establish provider accuracy or latency. Live rewrite and automatic pasting remain Experimental; automatic pasting is off by default in Settings → Capture → Behavior.

Use full-opacity primary ink. Modeled contrast on opaque surfaces is 9.25:1 Nord, 8.10:1 Tokyo Night and 6.66:1 Rosé Pine. Transparent light-theme panes need a suitable wallpaper or opaque reading surface; inspect actual compositions after palette/opacity edits. Final narration timing, the representative Blender pilot, full-film assembly and a listened sound-mix review remain production work.
