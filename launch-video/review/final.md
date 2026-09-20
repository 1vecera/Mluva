# App intro review — 19 September 2026

The review copy is `launch-video/out/mluva-intro-master.mp4`: **46.733 seconds of 1920×1080 video at 60 fps**, with a 46.800-second container including AAC padding. The final file is 7,720,416 bytes; SHA-256 `03a53e3239db264a27609cb7e38948a0373b2d661188a309c961c98d9911584d`. It remains a local review artifact. No release or public video upload was made.

The intro now uses actual GTK typography, controls and layouts, a native sidebar recording, word-aligned continuous narration and an original animated Blender backdrop. The old eleven-scene replica framework is replaced by seven beats in one composition: runtime source decreased from 27 files/2,032 lines to 5 files/609 lines, excluding generated timing data. Unused media, fonts, scene code and four unused Remotion packages were removed. [Twelve completed passes](iterations.md) record observed defects, changes, evidence and decisions.

## Measured checks

| Check | Actual result |
| --- | --- |
| Native capture provenance | Forty 2080×1280 PNGs, no capture errors; asset hashes, capture script hash and five production source hashes verified. Private synthetic history and provider responses. |
| Independent pixel fidelity | **43 comparisons, maximum RGB channel error 0**, including 40 complete states and three timer substitutions. Expected pixels come directly from untouched PNGs with independent Pillow compositing, not a second call to the same React component. [Full results](fidelity.json), [comparison](comparison.png). |
| Final media integrity | **2,804 video frames** and the complete audio stream decode without errors. Presentation timestamps are strictly increasing; maximum deviation from a 1/60-second step is 0.000000667 seconds, consistent with timestamp rounding. |
| Narration and mix | Canonical text, selected voice transcript and a fresh Scribe v2 transcript of the mastered mix agree after case/punctuation/F9 normalization: **83 words**. The final encoded audio stream matches the transcribed pass 11 stream by SHA-256. |
| Mastered audio | **−16.14 LUFS integrated, −1.81 dBTP**, LRA 2.8 LU. Measured after AAC encoding. Voice is continuous, with restrained music and four quiet action clicks. |
| Background | Six-second source loop, played at half speed. Wrap-frame difference 0.468/255 versus mean adjacent 0.621/255; no wrap outlier in that diagnostic. A fresh factory-startup Blender frame matches the original first frame exactly. |
| Build and code | Clean `npm ci --ignore-scripts`, audit reports zero vulnerabilities, TypeScript, Prettier, Python Ruff check/format and `git diff --check` pass. No application runtime source is changed by this film branch. |

[Metrics](metrics.json) preserve the measurements and their scope. The only hosted workflow is manually dispatched; hosted CI was not triggered or represented as passing.

## Visual and temporal review

The [24-frame storyboard](storyboard.jpg) covers the full story. Twenty additional timestamped temporal sheets and six pairs of 1080p/720p focal frames are retained locally under `out/final-review/`. Reviewed ordered windows include the opening handoff, F9, Polish/Structure, camera moves, Live result, history cut/video-to-still handoff, search, provider cut, theme changes and closing dissolve. These are sampled-frame inspections, not a claim of sound-on real-time playback.

The editing shot visibly selects the original text and shows the actual rewrite result. The structured note is now in view at 16.1 seconds. The [720p Grilling frame](grilling-720.png) shows both the pinned question and Architecture body; the camera finishes at 22.2 seconds before the result appears at 23.3 seconds, leaving about 5.82 seconds of stationary question visibility. The provider shot shows both ElevenLabs and Codex, and history shows the query alongside its filtered result. The 720p focal frames have no overlapping captions or clipped target text; smaller secondary native labels are not intended reading targets.

Frame-difference diagnostics located the abrupt bright-theme and closing cuts. Short dissolves reduced the maximum reduced-resolution grayscale step from 95.47 to 12.80/255. The remaining large changes correspond to intentional context cuts and that brightness transition. This is evidence of a removed discontinuity, not a universal smoothness score. No remaining concrete defect was identified in the inspected windows and focal states.

## Boundaries and review status

This is an **asset-backed raster reconstruction in Remotion**, with real sampled app pixels, a native recording and presentation motion. It does not contain independently redrawn vector widgets. Exact equality applies at source resolution with matching backing; scaling and H.264 encoding necessarily alter final-video pixels. The native sidebar is 30 fps and the backdrop is a 30 fps source played at half speed, even though the master timeline is 60 fps.

Demonstration text and response timings are fixtures. They do not establish recognition speed, external model quality, microphone behavior or actual shortcut integration. Live rewrite is visibly labelled Experimental, the theme beat names Omarchy, and the closing card discloses demonstration content and edited timing. Asset sources and generation details are recorded in [the provenance file](../reference/ASSETS.md).

The narration uses supported Fish delivery directions and a phoneme override, but exact transcription and loudness cannot certify natural acting, timbre or Czech pronunciation. A human sound-on/muted playback sign-off has not been performed. The [quality method and primary-source research](quality.md) distinguish these subjective judgments from the checks completed here. The film is ready as a draft for that review, with no claim of release approval.

The separate [silent-capture fix](https://github.com/1vecera/Mluva/pull/42) treats successful empty recognition as a quiet completion, preserves the viewed conversation or visible edited Live draft, and skips rewriting, delivery and blank history. Its 474 tests, Ruff, isolated native GTK scenarios, private shortcut portal gate and ShellCheck pass. That fix was installed locally with the prior installation backed up; it remains a draft PR and is not merged.
