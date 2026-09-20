# Motion and voice decisions

Keep the Figma feature order with a clearer opening and slower pacing: Mluva introduction → Welcome/setup → F9/talk/paste → floating recorder → Polish → history → themes → Live rewrite → task → closing brand. The 78-second cut starts with the brand and purpose immediately. Setup holds for twelve seconds and shows cloud/local speech, rewriting tools and recorder appearance. The float scene uses the small production widget rather than the app window.

The 1920×1200 desktop fits at 1600×1000 inside the 1920×1080 composition. Setup receives a 1.33× close-up; subsequent app details use 1.58×, with cubic Bézier `(0.42, 0, 0.2, 1)` and 0.85-second ramps. Native controls remain in frame. The widget scene holds the full desktop framing so its movement across the window arrangement is visible. Its native X11 window follows eased position changes over a recorded desktop still. Other scenes preserve the recorded animated wallpaper.

Keyboard overlays identify F9, Ctrl+V and Ctrl+P at the recorded actions. Plain white subtitles sit on a dark backing below the app; there is no word bouncing or karaoke animation. Each caption follows the matching voice phrase and remains briefly afterward for reading. Source excerpts are slowed with duplicated frames at 60 fps, never optical-flow interpolation. `reference/edit.json` records the exact retiming.

## Fish Audio Sarah

Use Fish Audio's official **Sarah**, reference ID `933563129e564b19a115bedd57b7406a`, model `s2.1-pro-free`, speed `0.90`. ElevenLabs is used only for Scribe transcription. The narration uses shorter, direct wording and longer spaces between feature explanations.

Write **Ohmarchi** in speech input to preserve the earlier requested “omarči” sound; keep **Omarchy** in visible copy. The two phrases naming Mluva use `<|phoneme_start|>M L UW1 V AH0<|phoneme_end|>`, following Fish's [English phoneme controls](https://docs.fish.audio/developer-guide/core-features/fine-grained-control/english). Scribe recognizes both names as “Mluva” in that pickup. The script uses restrained bracket directions and `[break]` from [Fish Audio's speech-control documentation](https://docs.fish.audio/developer-guide/core-features/emotions). Do not append a trailing pause direction: an earlier take added unwanted speech after it.

The 30.662-second main take and 3.414-second pronunciation pickup are normalized individually to −18 LUFS, divided at phrase boundaries and placed in the 78-second film using `reference/narration-cues.json`. The F9, Talk and Paste phrases align with the corresponding actions. Speech is neither stretched nor pitch-shifted; each excerpt has a 12 ms entrance and 25 ms exit fade. Main-take brand phrases are replaced by the pickup, as recorded in the cue mapping. Transcription checks wording, not acting or exact pronunciation.

Use the user-selected Bombinsound “Upbeat Background Music Version 5 — Rise” download. A 0.7-second crossfade joins its repeated section. Normalize the music stem to −26 LUFS before mixing; apply gain 0.35 under speech and 0.60 between phrases, with 1.2-second opening and 2.5-second closing fades. This places the music roughly 17 dB below the normalized voice during speech. Gain values alone are not comparable with the earlier unnormalized stem. The final encoded master must measure −16 ±0.5 LUFS integrated with true peak no higher than −1.5 dBTP; the normalization target is −1.8 dBTP.

## Review method

Inspect native captures first, then final encoded frames at every story beat and around camera ramps, keyboard cues and pickup cuts. Confirm the source and captions remain readable, native controls stay inside frame, themes settle, and short dictated text starts at the top. Inspect adjacent frames during the floating widget and camera motion; stills alone do not establish smooth animation.

Check duration, frame cadence, decoding errors, black frames and encoded audio loudness independently. Transcribe the final mix to catch clipped or missing phrases. Those checks establish technical integrity and script coverage; natural delivery and musical taste still benefit from a human sound-on pass.
