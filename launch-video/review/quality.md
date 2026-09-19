# How to review the intro

Require a timestamp, visible defect, proposed correction and a matching before/after sample. Do not collapse fidelity, motion, readability and sound into an uncalibrated overall score. Two independent research reviews of the previous film and early native capture found real defects that informed this rebuild: approximate controls, incorrect alpha backing, clipped provider choices, a malformed Grilling fixture, frozen time, camera movement during reading and unsupported claims.

## Evidence ladder

| Question | Check | Limit |
| --- | --- | --- |
| Does the UI match the actual app? | Hash untouched captures; render each Remotion state at 2080×1280; independently composite source PNGs over the capture's black backing; compare all RGB pixels, including timer substitutions. | Source-resolution settled fidelity, not independently redrawn vectors, compressed-film equality or provider behavior. |
| Does the workflow make sense? | Inspect native state, action cue, response and stable result in order; check labels and maturity against the application source. | Synthetic text and edited timing are demonstrative. |
| Is the encoded timeline intact? | Decode all frames/audio; inspect presentation timestamps and expected frame count; locate frame-difference spikes around cuts. | A decoder pass does not establish comfortable motion. Repeated frames may be deliberate holds or 30 fps sources in a 60 fps master. |
| Can viewers read the focal result? | Inspect full-size and 720p frames; measure stationary visibility; look for clipping and caption overlap. | Reading speed and comprehension still need a real-time viewer. Not every label is a reading target. |
| Is the motion purposeful? | Inspect contiguous ordered frames around entrances, camera settles, state changes and loop wraps; track the app separately from the background. | A sheet establishes event order and visible geometry, not the experience of playback speed. |
| Is the voice intelligible and intact? | Compare canonical script to Scribe words, keep continuous audio, inspect endpoints, measure source/mix/master loudness and true peak. | Transcript/metrics do not certify natural acting, timbre or Czech pronunciation. |

[VBench](https://arxiv.org/abs/2311.17982) separates appearance, temporal consistency, flicker, smoothness and dynamic degree. Its distinction matters here: a frozen screenshot can score well for stability while failing as an intro. These dimensions inform the rubric; no VBench score is claimed for this film.

[TemporalBench](https://arxiv.org/html/2410.10818v1) evaluates event order, repetition and motion magnitude; [Causality Matters](https://arxiv.org/html/2508.11576v1) reports sensitivity to frame ordering in its tested models. Use ordered temporal evidence and explicit sampling. A useful future evaluator calibration is a blinded frozen/reversed control of the same clip: failure to detect the manipulated action disqualifies that evaluator on that dimension. That diagnostic has not been run here, and these papers do not certify this model's motion judgment.

[Apple's motion guidance](https://developer.apple.com/design/human-interface-guidelines/motion) supports brief, purposeful feedback and stable spatial relationships. Its [app-preview guidance](https://developer.apple.com/app-store/app-previews/) supports captured UI, legibility and a cohesive story; its platform submission rules are not requirements for this Linux intro. The practical edit is establish → action → readable result, with restrained transitions and a steady question hold.

[Remotion's animation guidance](https://www.remotion.dev/docs/animating-properties) supports frame-driven animation so render frames do not depend on wall-clock CSS animation. The composition uses only frame time for camera, opacity and sound envelopes. Native capture and backdrop cadence are documented separately from the 60 fps output.

## Speech direction

Fish documents natural-language square-bracket cues for S2/S2.1 and pause cues such as `[break]` in its [model overview](https://docs.fish.audio/developer-guide/models-pricing/models-overview) and [emotion guide](https://docs.fish.audio/developer-guide/core-features/emotions). These are expressive hints, not exact-duration timing commands. The old S1 parentheses guidance should not override the selected model's syntax.

The final script reduces cue changes to three and retains one take. Actual word timings set the edit. Fish's [fine-grained control documentation](https://docs.fish.audio/developer-guide/core-features/fine-grained-control) describes phoneme delimiters and CMU Arpabet; parts of that page retain older-model examples, so the selected take was checked empirically by transcription. All three brand mentions normalize to Mluva. This is text recognition evidence, not an auditory sign-off.

## Repeatable improvement loop

Choose the most consequential observed defect; change one coherent aspect; render its affected interval; compare at the same time and scale; record the result and keep/revert decision. Use a full export at milestones to catch interactions that a local crop misses. The [iteration log](iterations.md) records actual completed passes separately from proposals and capture retries.

For the final perceptual review, watch once muted for story/readability and once with sound for pacing, voice/music balance and pronunciation. Listen especially to Mluva, F nine, Polish and Live rewrite. Machine verification and ordered-frame inspection are recorded in [the final report](final.md); they are not represented as a human playback or listening judgment.
