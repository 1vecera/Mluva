# Motion and voice decisions

Follow the original eleven shots: desktop → brand → onboarding/providers → F9/talk/paste → tile/float → Polish → history → themes → Live rewrite → Grilling → closing brand. Hold readable results instead of adding headlines over the app. All app UI and desktop motion are native recorded pixels.

The 1920×1200 desktop fits inside 1920×1080 at 0.9× with quiet side margins. Camera close-ups reach 1.53× relative to that fit, using cubic Bézier `(0.42, 0, 0.2, 1)` and 0.6–0.65-second ramps. The full app stays in frame. Theme changes retain the live compositor border colours; the downloaded animated background is already part of the capture. Do not replace it with a separately moving image or redraw translucent app surfaces.

Keyboard overlays identify F9, Ctrl+V, Super+T and Ctrl+P at the recorded actions. The cut at 42 seconds introduces the corrected Live rewrite pickup; 55 seconds returns to the main take beneath the closing brand. Native 60 fps capture is retained at 60 fps. No optical-flow frames are invented.

## Fish Audio Sarah

Use Fish Audio's official **Sarah**, reference ID `933563129e564b19a115bedd57b7406a`, model `s2.1-pro-free`, speed `0.94`. This is the voice selected in the earlier production session. ElevenLabs is used only for Scribe transcription, never for narration generation.

Write **Ohmarchi** in speech input to guide the requested “omarči” sound; keep **Omarchy** in visible copy. Use `[emphasis] Mluva` so the opening M remains clear. The script uses restrained freeform bracket directions and short pauses supported by [Fish Audio's speech-control documentation](https://docs.fish.audio/developer-guide/core-features/emotions). Do not append a trailing pause direction: an earlier take added unwanted speech after it.

The 37.022-second source take is divided at phrase boundaries and positioned in the 59-second film using `reference/narration-cues.json`. The F9, Talk and Paste phrases align with the corresponding actions. No speech is stretched or pitch-shifted; each excerpt has a 12 ms entrance and 25 ms exit fade. The source transcript preserves the script and contains no trailing ad-lib. A transcript cannot certify acting or precise pronunciation; the rendered audio remains available for listening review.

Use the user-selected Bombinsound “Upbeat Background Music Version 5 — Rise” download. A 0.7-second crossfade joins its repeated section. In the composition, the music gain falls from 0.32 to 0.15 around speech, with a 0.6-second opening and 1.5-second closing fade. The final encoded master must measure −16 ±0.5 LUFS integrated with true peak no higher than −1.5 dBTP; the normalization target is −1.8 dBTP.

## Review method

Inspect native captures first, then the final encoded frames at every story beat and around camera ramps, keyboard cues and pickup cuts. Confirm the source stays readable, native controls remain inside frame, theme transitions settle, and short dictated text starts at the top. Review movement as sequences of adjacent frames; attractive stills alone do not establish smooth animation.

Check duration, frame cadence, decoding errors, black frames and encoded audio loudness independently. Transcribe the final mix to catch clipped or missing phrases. Those checks establish technical integrity and script coverage; natural delivery and musical taste still benefit from a human sound-on pass.
