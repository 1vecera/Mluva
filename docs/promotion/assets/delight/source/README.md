# Launch audio sources

All spoken audio here is synthetic Fish Audio output. The files are retained unchanged; the final film uses the phrase cuts declared in [the edit plan](../launch.plan.json). No voice impersonation or physical microphone recording is claimed.

| Source | Purpose | Preparation |
| --- | --- | --- |
| [narration.mp3](narration.mp3), [script](narration.txt) | Main launch narration | One Fish generation, 36.806 seconds |
| [pronunciation.mp3](pronunciation.mp3), [script](pronunciation-script.txt) | Targeted opening and theme phrases | One additional Fish generation, 7.680 seconds; canonical on-screen spelling remains Mluva and Omarchy |
| [dictation.mp3](dictation.mp3), [script](dictation-script.txt) | Meaningful 54-word shop-demo input for actual Mluva recognition | One Fish generation, 20.532 seconds |
| [dictation-input.wav](dictation-input.wav) | Input played through the isolated PipeWire graph | Mono PCM16 at 16 kHz, with three seconds of trailing silence; no tempo change or speech removal |
| [sound-bed.flac](sound-bed.flac), [recipe](sound-bed.recipe.json) | Quiet original stereo pulse beneath narration | 55 seconds of declared oscillators; no sampled music |

[Narration provenance](narration-provenance.json) and [additional voice provenance](additional-voice-provenance.json) retain source hashes and generation details. The [main word timing](narration-timing.json) and [replacement word timing](pronunciation-timing.json) come from Scribe alignment of those already generated audio files. They support whole-word editing and captions; their automatic spelling of proper names is not a listening review or a Mluva recognition result.

The completed [installed real take](../capture/real/workflow.mp4) retains its unchanged recognition, actual provider replies and recorder PCM. Its [export receipt](../capture/real/export.json) documents the measured software timestamp offset. The [55-second film](../mluva-delight-launch.mp4) mutes captured source audio beneath the prepared narration and original bed. Full decoding, loudness and word-boundary checks passed; auditory listening was not supported by this runner and is not inferred from those checks.
