# Motion and voice decisions

Daniel’s delivery sets the timing. The 83.27-second camera recording becomes 72.37 seconds after ten long internal pauses and unused lead/tail space are shortened. Remaining sentence gaps are generally about half a second; internal gaps retain breath room. No spoken words, pitch changes or voice synthesis are introduced. Each cut uses the same 30 fps source boundary for the portrait and audio, with 6 ms audio edge fades in the pauses.

The app follows the existing Figma feature order. Thirty-four source intervals align native actions to the new voice: local/cloud choice, rewriting tools, F9 start/finish, paste, tiling, refloating, desktop switching, Commands, history, themes and Live/task behavior. Stable holds absorb most timing changes; the tile, refloat and switch transitions have explicit timing anchors. The original captured windows remain the picture source. `reference/edit.json` records these mappings.

The desktop occupies 1536×960 at (40, 24), leaving a presenter area on the right. Welcome uses a stable 1.24× view, and app details use 1.42×, with cubic Bézier `(0.42, 0, 0.2, 1)` and 0.7-second ramps. The recorder retains the whole desktop view. The presenter remains in one position rather than crossing over the app. His name appears from 1.05 to 8.9 seconds; the product information overlay fades out by 9.1 seconds. Closing copy appears at 63.43 seconds.

[Robust Video Matting](https://github.com/PeterL1n/RobustVideoMatting) separates the recorded presenter using temporal state carried across the source recording. The camera is cropped to remove excess ceiling, then matted at 432×580 and 30 fps. Picture and audio receive identical pause edits afterward. A VP9 alpha video replaces the large ProRes intermediate. The compositor adds a soft shadow and fades the lower and side crop edges; no face or lip motion is generated.

[ElevenLabs Scribe v2](https://elevenlabs.io/docs/api-reference/speech-to-text/convert) supplies word timing. Captions are grouped by phrases, with normalized Omarchy/Live spelling and “text model” for Scribe’s “texture model.” The raw transcript remains in the private archive. No transcript text is committed to Git.

Voice treatment is light: 70 Hz high-pass, 14 kHz low-pass, 4 dB FFT noise reduction at a −64 dB noise floor, −1 dB around 250 Hz, +1 dB around 3.2 kHz and parallel 2:1 compression. Measured loudness normalization brings the voice to −18.25 LUFS with −3 dBTP peak. The original recording has no clipped samples; the main correction is level and consistency, not aggressive denoising.

The existing music bed stays at gain 0.20, about 21 dB below the voice by RMS over spoken caption intervals. Fixed low gain avoids audible pumping between phrases. Opening and closing fades are 1.2 and 1.4 seconds. The encoded master must measure −16 ±0.5 LUFS with true peak no higher than −1.5 dBTP; its limiter target is −1.8 dBTP.

Five comparisons against the voice stem found the same 2,048-sample delay in the rendered mix: 42.67 ms at 48 kHz. The mastering command advances this mix by that measured amount and trims its tail to the exact picture duration. This corrects export timing without changing delivery speed. Recheck alignment if the renderer, audio pipeline or composition changes; the generic mastering helper defaults to no advance.

Review final encoded story frames, adjacent portrait/cut and widget-transition frames, subtitle bounds, complete decoding and encoded loudness. Confirm that audio samples and portrait frame boundaries remain synchronized, and transcribe the final mix to check wording. These technical checks do not replace a human judgment of vocal tone or delivery.
