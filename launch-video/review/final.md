# Live storyboard review

Review copy: `mluva-intro-live-sarah.mp4`, 59 seconds of picture, 1920×1080 at 60 fps. The finished render and input archive are local in Downloads; the public source PR contains no new footage, narration or downloaded music.

The original eleven-shot Figma sequence is restored. Native desktop footage carries the app, Chrome paste, window tiling, history and theme changes. Minimal brand cards and keyboard cues are the only graphic overlays. The old recreated-surface, clock-patching and alternate-background pipeline is removed. A replacement take at 42–55 seconds includes the short Live draft scroll correction from PR #45; the earlier source-dictation correction from PR #44 is merged.

## Verified

- All eleven story beats and seventeen selected encoded frames were visually inspected, including the action results, all three themes, source/draft alignment, and the Grilling question → answer → next question.
- Adjacent frames around the camera move and window-layout transition were inspected. The app remains inside the frame; the camera settles before the command palette and holds through Live rewrite and Grilling.
- Both host capture manifests contain no callback errors. The pickup's source and draft scroll values stay at 0 with their natural 4 px top margin.
- The final file contains 3,540 H.264 frames at 60 fps, 1920×1080. Full decoding succeeds; the black-frame scan finds no black interval of 0.1 seconds or longer. AAC padding extends container duration to 59.051 seconds.
- Encoded audio measures **−16.06 LUFS integrated**, **−2.90 dBTP** and **6.3 LU** loudness range. These meet the mastering limits.
- Scribe transcription of the final mix retains every narration phrase without a trailing ad-lib. It spells the brand “Maluva” and the spoken OS name “Omachi”; the source-take transcription spells the brand “Mluva.” Speech recognition alone does not resolve those pronunciation details.
- TypeScript, capture-script lint/format checks and both preview/full renders pass. The related app patch passes 498 tests, Ruff and native GTK regression checks.

## Review limits

This is prepared demonstration content shown through production views. Microphone samples and provider responses are fixtures, so the timing does not establish recognition accuracy or provider performance. Actual host F9, manual Ctrl+V and Super+T are captured; automatic insertion into every application is not claimed.

The official Fish Sarah voice and earlier pronunciation directions are preserved. A human sound-on review is still needed for exact pronunciation, acting and musical balance; waveform levels and transcription establish different properties. The local review copy was opened in mpv for that pass.
