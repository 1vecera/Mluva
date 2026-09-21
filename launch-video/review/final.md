# Daniel narration review

Approved film: `mluva-intro-daniel-v7-voice.mp4`, 72.37 seconds, 1920×1080 at 60 fps. Daniel approved the enhanced voice and requested README playback, cleanup and merge. The [public movie](https://github.com/user-attachments/assets/ffac3582-3146-46f2-9244-2c28c5cbef26) is the exact approved file; its hash and stream details are recorded in `metrics.json`.

The presenter sits inside the native desktop picture, without a box, outline, shadow or added edge fades. “Daniel Vecera” appears during the introduction. The product overlay, original Figma feature order and native footage remain, with short highlighted captions at the top. The final audio uses Daniel’s recording, local noise reduction and restrained voice enhancement.

## Verified

- The 83.27-second camera recording becomes 72.37 seconds by shortening ten internal pauses and unused lead/tail space. All 194 source caption tokens remain within retained footage. Portrait and voice use identical source boundaries: 2,171 portrait frames at 30 fps and 3,473,600 audio samples at 48 kHz. Speech speed and pitch are unchanged.
- Twenty-four encoded story frames cover the introduction, setup, F9/paste, tiled recorder, Firefox/Hacker News, both presenter position changes, Polish, history, themes, Live rewrite, task questions and closing. Full-resolution light/dark samples show the plain cutout. This v6 picture review also applies to v7: the complete H.264 stream is byte-identical. Earlier motion and cut checks cover the unchanged source footage.
- Forty-eight caption phrases are ordered, nonoverlapping and within the available width. Their 194-token sequence matches the prior captions, including product spelling and one text-model recognition correction. Selected words receive coral emphasis and a restrained pulse. Raw transcript and caption text stay in the private archive.
- Fresh ElevenLabs Scribe v2 transcription of the enhanced encoded mix matches the previous normalized wording exactly: 196 tokens under that comparison’s tokenizer. No speech was generated or replaced. This transcription count uses different tokenization from the 194-token caption check.
- DeepFilterNet3 reports zero measured lag in five source comparisons. A 30 ms silent tail restores the exact sample count. Five comparisons of the final encoded mix find offsets of only two to four samples at 48 kHz, under 0.1 ms. Audio and picture start at zero and both last 72.366667 seconds.
- The enhanced voice measures −18.12 LUFS and −3.00 dBTP. Music is about 21.5 dB below voice over spoken intervals. The encoded movie measures −16.01 LUFS and −3.61 dBTP, with 3.3 LU loudness range.
- Full decoding passes for all 4,342 frames. The unchanged picture has no black intervals of at least 0.1 seconds. The current TypeScript check passes. The simplified export attaches the approved mix directly to silent picture, without repeating voice processing or applying the retired Remotion delay correction.

Daniel’s playback approval covers the delivery and vocal tone. Automated transcription, loudness and alignment checks provide technical evidence; they do not establish a perceptual lip-reading score or complete frame-by-frame human review.

## Capture scope

The desktop follows the supplied FT/VS Code/Ghostty/htop/Linear arrangement, with Firefox/Hacker News on desktop 2. Native receipts in `reference/capture-verification.json` establish actual recorder tile/float/pin behavior. Linear is visibly signed out. Provider responses and demonstration history are prepared examples; F9 and Ctrl+V demonstrate the transfer flow, not service latency, recognition accuracy or automatic insertion into every app. Live rewrite remains visibly Experimental and retains PR #45’s short-draft scroll correction. This video revision changes no application code.

## Cleanup and delivery

The README and existing website use one native GitHub video attachment. Anonymous download reproduces the approved file hash, and GitHub’s Markdown renderer produces a video element with playback controls. The repository retains a small poster and composition source, rather than another large movie binary. The superseded film, old subtitle tracks, obsolete narration files, capture helper, theme snapshots and stale storyboard timing export are removed; Git history preserves their prior versions.

The private `mluva-intro-daniel-media-v7.tar.gz` in Downloads preserves the four active render inputs, enhanced voice stem, processing scripts, composition snapshot and review evidence. Earlier archives retain original camera/transcript sources and native takes. Rejected preview treatments and reproducible temporary render bundles are removed after archive verification. Raw recordings, private transcripts and browser profiles are excluded from Git. Wallpaper/music provenance remains in `reference/ASSETS.md`.

LinkedIn and X copy remains in local drafts and has not been posted. The manual repository CI workflow was not dispatched; local checks cover TypeScript, the Linux suite, shortcut portal smoke and shell scripts. Publishing this approved film does not authorize a release or unrelated PR merge.
