# Live intro review

Review copy: `mluva-intro-live-sarah-v2.mp4`, 78 seconds of picture, 1920×1080 at 60 fps. The movie, subtitles and v2 input archive are local in Downloads. The public draft source PR includes code, narration scripts and review records; no footage, generated voice or downloaded music is published.

The cut follows the original Figma feature order with the requested clearer wording and longer holds. Mluva and its purpose appear immediately, followed by twelve seconds of the native Welcome flow. The small recording widget carries “Mluva floats like water” across the clean Chrome/Ghostty/Herdr arrangement. History, Live rewrite and task narration are direct. Subtitles are burned into the picture, and the music sits substantially below the voice.

## Verified

- Ten revised scenes cover the original feature sequence, with the desktop and brand opening combined. Twenty-five encoded frames were inspected across setup choices, dictation/paste, the small widget, Polish, history, themes, Live rewrite, task and closing card. Captions remain readable and inside the frame.
- Widget motion and the following camera ramp were inspected as frame sequences. The close-up starts after the widget scene ends and settles before the command palette. App content and subtitles remain visible.
- The new setup and widget pickups use production GTK/Quickshell components in a private X11 display. The widget background is a still from the actual desktop recording. These are native pickups, not continuous host footage.
- The retained Live rewrite/task pickup includes the PR #45 short-draft scroll correction. Its previously verified source and draft scroll positions stay at 0 with the natural 4 px margin. PR #44's source-dictation fix is merged; this video revision does not change the app.
- The video has 4,680 H.264 frames at 60 fps, 1920×1080. Full decoding and a black-frame scan pass. See `metrics.json` for the exact final container duration, hash and encoded audio measurements.
- The normalized music is **17.33 dB below the voice** by RMS over spoken phrases before mastering, using stereo-averaged music at the ducked gain. The final master meets −16 ±0.5 LUFS integrated and ≤−1.5 dBTP.
- Scribe transcription of the mixed master retains every phrase, including both “Mluva” mentions, without a trailing ad-lib. It spells the spoken OS name “Omachi”; the speech input deliberately retains the earlier “Ohmarchi” direction while visible text says “Omarchy.”
- TypeScript and the 1080p/60 render pass. The scene map, caption timing and source excerpts have been checked against the delivered media. Hosted CI is manual-only and was not dispatched.

## Review limits

Microphone/provider responses and demonstration history are prepared fixtures, so this does not measure recognition quality or provider performance. Actual host F9 and manual Ctrl+V are retained; automatic insertion into every application is not claimed. Live rewrite remains visibly Experimental.

Fish Audio's official Sarah voice is used with direction tags and a two-phrase phoneme pickup for Mluva. Human listening remains the check for exact pronunciation, acting and musical taste. Transcription and waveform measurements verify wording and levels, not those judgments.
