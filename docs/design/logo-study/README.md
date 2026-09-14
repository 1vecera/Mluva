# Mluva logo study

[Compare the ten directions in Figma](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=154-14). Each direction has a transparent logo and matching Mluva wordmark, developed from a separate agent’s brief and revised after its self-critique. The existing production identity stays in place until Daniel chooses a direction.

Mluva turns speech into useful, editable text while preserving the original. It should feel calm, immediate and at home in Omarchy. The mark should suggest a thought finding form, speech becoming text, or a receptive space for words. Use a filled silhouette, clear negative space and a compact, recognizable shape. JetBrains Mono provides the common starting point for the wordmarks.

| Direction | Meaning | Source |
| --- | --- | --- |
| Brimming Tide | A thought gathering into a pool of speech | [Brief](concepts/01-tide.json) |
| Open Syllable | An open speech form with a restrained M | [Brief](concepts/02-syllable.json) |
| Soft Turn | A spoken thought turning into a page | [Brief](concepts/03-fold.json) |
| Paired Resonance | Two related forms in conversation | [Brief](concepts/04-resonance.json) |
| Word Spring | Speech rising into useful lines of text | [Brief](concepts/05-spring.json) |
| Breathing Aperture | A calm opening that receives and releases voice | [Brief](concepts/06-aperture.json) |
| Soft M | A recognizable M shaped by breath | [Brief](concepts/07-monogram.json) |
| Unfurling Voice | A thought opening into expression | [Brief](concepts/08-unfurl.json) |
| Current to Cursor | Flowing speech meeting an editing caret | [Brief](concepts/09-cursor.json) |
| Quiet Vessel | A receptive vessel for a thought | [Brief](concepts/10-vessel.json) |

[STUDIES.md](STUDIES.md) contains all ten long descriptions, app and mark identities, critiques, revisions, optical rules and motion relationships. The JSON briefs retain separate image-generation prompts. [manifest.json](manifest.json) links every asset to its Figma master and records PNG dimensions, true alpha, ink bounds and SHA-256 hashes.

The 20 PNGs in `assets/` are unchanged imagegen originals. These are raster studies; the selected identity will need final vector construction and optical refinement. Native Figma crops normalize comparison scale without changing the PNGs. Keep wordmark proportions when resizing. Inspect the 24, 48 and 128 px icon proofs alongside the large artwork: small negative spaces and the caret separation are the first details to judge.

The Figma page uses shared spacing, card width, mark size, proof-surface variables; wordmark width follows auto-layout with a locked aspect ratio. Width and spacing were changed together, inspected for overflow, and restored. The ten large study boards use auto-layout; canvas placement refreshes through the shared [reflow helper](../video-kit/reflow-canvas.mjs).

The [Breathing Mark principles](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=153-1677) and [Breathing Motion loop](https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=155-16) are separate from the logo options. The recording light starts as a small circle, expands through a fluid contour to a larger circle, then returns. Its centre and layout slot stay fixed. The full 102-pose Figma sequence uses one opaque pose at a time; the transparent 30 fps WebM is the film asset.

Source: Daniel’s September 14, 2026 feedback; ten independent Codex concept agents; OpenAI image_gen. No new production logo was selected or installed by this study.
