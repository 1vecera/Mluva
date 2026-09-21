# Daniel narration review

Review copy: `mluva-intro-daniel-v5.mp4`, 72.37 seconds, 1920×1080 at 60 fps. Daniel’s recorded voice and small presenter cutout replace Fish Audio. The name “Daniel Vecera” appears during the introduction. The product information overlay, original feature order, actual desktop footage, music and burned subtitles remain.

The movie, Markdown transcript and `mluva-intro-daniel-media-v5.tar.gz` are local in Downloads. The repository contains composition code, timing, provenance and review records. Human recordings, transcript/caption text, portrait media and music remain outside Git. The original OneDrive upload and earlier deliveries are preserved.

## Verified

- The 83.27-second camera recording becomes 72.37 seconds by shortening ten internal pauses and unused lead/tail space. All 194 source word tokens remain within retained footage. Both portrait and voice use identical source boundaries: 2,171 portrait frames at 30 fps and 3,473,600 audio samples at 48 kHz. Speech and pitch remain at their original speed.
- Twenty encoded story frames cover the opening/name, local/cloud setup, F9 dictation/paste, tiled recorder, Firefox/Hacker News, Polish, history, themes, Live rewrite/task questions and closing. Twelve transition samples show actual tile/refloat/workspace movement. Thirty adjacent portrait samples cover every pause cut. The name, portrait and captions remain clear of app controls; no cutout background leaks or missing facial regions were seen in those samples.
- Twenty-nine caption phrases are ordered, nonoverlapping, inside the film and within the available width using the bundled font. Captions follow the recorded wording, with product spelling and one text-model transcription correction. The original Scribe output remains in the private archive.
- Raw Scribe v2 transcription of the final decoded mix returns the same 194 word tokens by count. Comparison shows one two-word recognition variation in the paste clause; the original transcription supplies that caption. The underlying source segment is retained continuously. No spoken wording was generated, replaced or intentionally removed.
- Five audio comparisons across the film found a constant 2,048-sample delay in the initial Remotion render. Mastering removes that measured 42.67 ms delay. The corrected encoded mix has zero measured lag against the edited voice at 16 kHz analysis resolution in all five windows, with correlation above 0.994. Picture and sound now have the same 72.366667-second duration. This verifies export alignment with the matched source cuts; it is not a perceptual lip-reading test.
- The voice stem measures −18.25 LUFS and −3.00 dBTP after light denoising, EQ and parallel compression. Music is 21.40 dB below voice by RMS over spoken caption intervals before mastering. The encoded master measures −16.01 LUFS and −3.36 dBTP, inside the configured loudness and peak limits.
- Complete FFmpeg decoding passes for all 4,342 H.264 frames. The black-frame scan finds no intervals of at least 0.1 seconds. TypeScript and the complete 1080p/60 render pass. The actual movie hash, stream details, cue count and timing checks are recorded in `metrics.json`.

## Retained capture and review limits

The desktop follows the supplied FT/VS Code/Ghostty/htop/Linear arrangement, with Firefox/Hacker News on desktop 2. Native receipts in `reference/capture-verification.json` establish recorder tile/float/pin behavior. This revision retimes those reviewed source pixels; it does not recreate app UI or operate the visible desktop.

Provider responses and demonstration history are prepared fixtures. Linear is visibly signed out. The original F9/clipboard transfer used Ctrl+V; it does not validate automatic insertion into every application, microphone recognition accuracy or provider latency. Live rewrite remains visibly Experimental, and the retained footage includes PR #45’s short-draft scroll correction. No application code changes in this revision.

Technical audio checks and transcription do not replace human listening for taste, vocal tone or delivery. No sound-on listening review is claimed. Presenter edges were reviewed in encoded frame sequences, not with a full human playback review. Source wallpaper/music attribution and the limits on established redistribution rights remain in `reference/ASSETS.md`.

## Cleanup and delivery

The four superseded tracked Fish cue/transcript/script files are removed. Active media, the original camera recording, raw transcription, pause plan, matting model and preparation/review scripts are preserved in the private v5 archive. Large intermediate ProRes portraits were discarded after checking the VP9 alpha replacement. Earlier source takes remain available in the v3 archive.

The repository CI workflow is manual-only and was not dispatched. The source change is prepared on a task branch for a draft PR; no new film is published to the website. The local movie and source archive are the review deliverables.
