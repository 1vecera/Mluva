# Live intro review

Review copy: `mluva-intro-live-sarah-v4.mp4`, 84 seconds of picture, 1920×1080 at 60 fps. The movie and unchanged v3 media archive are local in Downloads. The source PR contains code, narration scripts and review records; footage, generated speech and downloaded music remain local. The SRT is in the archive, away from the movie, to avoid duplicate player subtitles.

The cut starts on native Welcome with an opening information overlay: the Mluva logo, “The most delightful dictation for Omarchy,” and “Local or cloud. Your choice of tools.” It holds through 5.8 seconds and fades away by 6.4 seconds, leaving setup controls and subtitles clear and preserving the 84-second timing. Sarah uses more energetic Fish Audio directions, explicit Mluva phonemes and the long-i `L AY1 V` pronunciation for Live rewrite. The original feature order and burned subtitles remain. Recorder coverage grows to fourteen seconds so “Tile it. Float it. Take it to your next desktop” accompanies distinct actions.

## Verified

- The new desktop follows the supplied arrangement: FT on the left, VS Code above Ghostty and htop in the middle, Linear on the right. Firefox/Hacker News occupies desktop 2. Linear is visibly signed out in the private browser profile; a signed-in workspace was not captured.
- Native Hyprland receipts confirm the production recorder is floating and pinned initially, tiled and unpinned after the tile action, then floating and pinned again. It remains pinned on desktop 2 and on return. These are continuous native Wayland captures in a private nested compositor, with actual applications and playing wallpaper. The visible desktop was not manipulated.
- Native PNG and encoded capture samples exposed an incorrect full-range flag from wf-recorder. Correcting H.264 metadata by stream copy restored colors; checked decoded RGB samples are within 3 levels per channel of native captures. Private compositor blur keeps Welcome readable over the other windows.
- The v4 review inspected twenty-four encoded frames across the full feature sequence and ten adjacent opening-fade samples. The information overlay appears from frame zero, stays clear of setup controls and subtitles, and disappears before local/cloud selection.
- Nine edited scenes retain the original feature sequence. The v3 review inspected twenty-seven encoded frames across Welcome, dictation/paste, recorder states, Firefox, Polish, history, themes, Live rewrite, task and closing. Captions and native controls stay inside the frame. Eighteen caption cues are ordered, nonoverlapping and within the film.
- The v3 encoded frame sequences show the native tile/refloat animations and the following camera ramp. The window settles into its slot; refloating restores the middle column. Workspace switching preserves the recorder position. The Polish close-up settles before the command palette.
- The retained Live rewrite/task footage includes the PR #45 short-draft scroll correction. The source remains visible beside the draft, and Live rewrite stays visibly Experimental. This video revision changes no app code.
- Full decode and a black-frame scan pass: 5,040 H.264 frames, 1920×1080 at 60 fps, no black intervals of at least 0.1 seconds. Exact file hash and container duration are in `metrics.json`.
- Music is **17.08 dB below speech** by RMS over the spoken phrases before mastering, comparing stereo-averaged music at the ducked gain with the aligned mono voice. The encoded master measures **−16.40 LUFS** integrated and **−1.73 dBTP**, inside the configured limits.
- Raw ElevenLabs Scribe v2 transcription retains all eighteen phrases and both Mluva mentions without extra speech. Only punctuation/case, “F nine” versus F9 and spoken “Omarchi” versus visible Omarchy are normalized for comparison. The v3 uploaded PCM matched its delivered master. The decoded v4 audio is byte-for-byte identical to v3, so the transcription and balance checks carry forward without another synthesis or transcription pass.
- TypeScript and the full 1080p/60 render pass. The scene map, source ranges, media hashes and captions match the delivery. The app CI workflow is manual-only and was not dispatched. Merging updates the existing GitHub Pages documentation; its embedded published film and all homepage assets remain intact.

## Review limits

Microphone/provider responses and demonstration history are prepared fixtures. The film does not measure recognition accuracy, provider latency or automatic insertion into every application. The retained F9 and Ctrl+V scene was recorded on the host; the new recorder sequence exercises native tiling and pinning in isolation using the same compositor operations as Omarchy's Super+T and workspace shortcuts.

Fish Audio's official Sarah voice uses the requested energetic directions and phoneme controls. Transcription verifies wording and measurements verify levels; exact pronunciation, acting and musical taste still need human listening. No sound-on listening review is claimed.

## Repository cleanup

The complete PR removes 95 obsolete tracked files (18,727,851 bytes), including abandoned recreated UI/clock assets, old voice/music/background files, unused promotion screenshots and the superseded Figma/Blender pipeline. Required render inputs, the current logo/font, the original Figma storyboard, attribution and the shared logo-study reflow helper remain. Removed tracked sources are recoverable from Git history. The existing homepage film and its poster/captions are still in use and are retained.

All four current local media files match the SHA-256 hashes in the saved v3 archive. Local reference checks scanned 178 Markdown/HTML asset links without finding any that point to removed files. The visible desktop and unrelated app/identity work were left alone.

Stale local capture attempts, private browser profiles and obsolete previews totaling 1,575,579,025 bytes were deleted after archive verification. Current render inputs, the browser dependency cache, v4 review evidence and unrelated app-regression evidence remain.
