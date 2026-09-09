# Real audio, captured on Omarchy

The released **v0.3.0** speech recording and the **UNRELEASED** Live task recording have separate identities. Both use real audio, production GTK/Quickshell, actual recognition and a private desktop on Daniel's Lenovo. Controls and the surrounding artwork are automated. No recognition text or model response is substituted.

[Product introduction](assets/mluva-product-intro.mp4) · [Released speech workflow](assets/v0.3.0/speech/workflow.mp4) · [Unreleased Live workflow](assets/unreleased-live/workflow.mp4) · [Portrait recording close-up](assets/v0.3.0/speech/vertical.mp4) · [Manifest](assets/manifest.json) · [Reproduction](../../dev/README.md)

![Mluva v0.3.0 showing the complete, unchanged Moon speech transcription on the Nord desktop](assets/workspace-dark.png)

## Choose the right asset

| Asset | Identity and content |
| --- | --- |
| [Product introduction](assets/mluva-product-intro.mp4) | 44.70 seconds, 1920 × 1080, 30 fps. Captioned AI transition, released speech, unreleased Live draft and saved result, followed by a title card. Every excerpt remains at 1× speed; [edit](assets/mluva-product-intro.edit.json), [portable plan](assets/mluva-product-intro.plan.json) and [QA](assets/mluva-product-intro.qa.json) disclose cuts, fades and audio alignment. |
| [JFK speech workflow](assets/v0.3.0/speech/workflow.mp4) | v0.3.0, `4ce8dc49537da98d8f32f25726f63193774d3b91`; 1920 × 1080, 30 fps, 17.03 seconds. Full recognition/review recording with synchronized source audio. |
| [Live task workflow](assets/unreleased-live/workflow.mp4) | **UNRELEASED**, `cdc00237595f8cfb29e75cd87a980311ea3bcc98`; 1920 × 1080, 30 fps, 59.53 seconds. Full synthetic narration, provisional model input, and final reconciliation against committed recognition. |
| [Portrait speech close-up](assets/v0.3.0/speech/vertical.mp4) | 1080 × 1920 editorial crop of the released recording widget, with unchanged timing and the original audio stream. It is not a second recognition session. |
| [Portrait poster](assets/v0.3.0/speech-portrait.png) | Native 1080 × 1920 rendering of the real saved v0.3.0 speech result; no additional provider call. |
| [Workspace](assets/workspace-dark.png), [review widget](assets/widget-review.png) | Stable filenames for the real released speech result. The widget PNG retains alpha transparency. |
| [Substantial Live draft](assets/unreleased-live/live-structured.png) | Frame at video 24.0 seconds: actual Task, Intent and Requirements while the voiced narration continues. |
| [Partial saved task](assets/v0.3.0/task-saved-partial-desktop.png) | Real v0.3.0 result, explicitly **partial** because its final update failed. It is not the successful Live result. |
| [H3 Max transition](assets/h3-max/h3-max-transition-raw.mp4) | AI-generated motion from actual empty/partial-workspace reference frames. Intermediate interface text is generated; this is illustrative transition footage. See its [exact request and qualification](assets/h3-max/generation.json). |

The continuous workflow MP4s contain no time cuts or speed changes. Their captured PCM is aligned to the measured first-sample timestamp, encoded to AAC and padded with silence to the screen recording's end. Export receipts preserve the measured offset and its software-timestamp limitation. Crops and the separate edited introduction retain their own disclosures.

The introduction cuts H3 source seconds 2.8–12.8, released speech 0–16, Live 17–30, then saved Live 55.4–59.2. It uses 0.4-second crossfades and a 3.5-second title card. The wait between the during-speech draft and saved result remains visible in the full Live recording. The coordinator supplied and reviewed this composition; both real audio excerpts correlate above 0.9998 with their continuous sources at zero shift.

## What the Live recording proves

The selected take uses native Codex `gpt-5.6-luna`, low effort, Fast off and the default service tier. Scribe commits remain at 25 seconds. Opt-in Live rewriting may read explicitly provisional recognition; raw history and final reconciliation use committed text. One rewrite runs at a time. The configured later-update thresholds are 160 additional characters and four seconds; the initial gate accepts a 40-character phrase or a short utterance unchanged for four seconds.

All following offsets are relative to the first captured PCM sample. At **11.057 seconds**, a generated but incomplete Intent fragment appears. At **22.730 seconds**, Task, Intent and Requirements are visible while voiced source audio continues. The first Scribe commit arrives at **25.378 seconds**. The expanded update at **34.603 seconds** occurs after the voiced source ends at about 33.742 seconds, while recording continues through padded silence. Stop is at **40.742 seconds**; the canonical final result is painted at **54.911 seconds**. These are observations from one take, not a general latency claim.

Add **0.347045 seconds** to convert those PCM offsets into timestamps in the selected MP4. The saved final reply retains the missing-order-ID flag, preview before saving and totals matching the upload. Owner and deadline remain explicitly unset; the model expresses these as open questions rather than literal `[Missing: ...]` markers. It narrows the spoken general bad-row error criterion to missing-order-ID errors; review the unchanged [source script](assets/sources/task-script.txt), [raw recognition](assets/unreleased-live/recognition.json) and [actual reply](assets/unreleased-live/replies.json).

The raw text is preserved despite recognition artifacts: “Czech crowns” is recognized as “check crowns,” and the padded silence produces an additional sentence about the owner. No wording was corrected in raw history. The original provider events, source hashes, exact runtime identity and final output are retained alongside the video. [Boundary review](assets/unreleased-live/boundary-review.json) records the text and timing checks. [Audio QA](evidence/audio-and-boundary-qa.json) found a 64 ms PipeWire lead-in: voice ends near source +33.678 seconds and captured PCM +33.742 seconds.

The prior [native take](evidence/initial-echo-take/workflow.mp4) is development evidence. Its first provider reply exactly repeated the initial template, so the +8.148-second paint is excluded from meaningful generation; its first substantive paint was +19.120 seconds. A separate [rejected four-second commit experiment](evidence/rejected-four-second-commits/boundary-review.json) lost requirements at speech segment boundaries and hit HTTP 429 on its rewrite request. Neither is presented as the selected performance result.

## Sources and capture boundary

The JFK excerpt is from the September 12, 1962 Rice University address. The [JFK Presidential Library's primary audio catalog](https://www.jfklibrary.org/asset-viewer/archives/jfkwha-127-002) identifies its White House recording as public domain. The downloadable copy's catalog identifies the Library/White House source and Miller Center digital copy. [Provenance](assets/v0.3.0/speech-provenance.json) records both catalogs, the original download and hashes. The selected excerpt is source seconds 527.0–538.5, followed by 0.5 seconds of silence; it is recognized through `scribe_v2_realtime` without a batch fallback.

The task narration was generated once with Fish Audio's free `s2.1-pro-free` default voice. It is synthetic, not a personal recording or an impersonation. The full [original MP3](assets/sources/task-fish.mp3), script and [actual input WAV](assets/sources/task-input.wav) are retained. WAV preparation applies `atempo=0.85` and seven seconds of trailing silence, without removing any spoken content. Provider and media details are in the manifest.

Captures use Xvfb **:193**, separate D-Bus/AT-SPI and XDG state, a private PipeWire graph with no hardware devices, and disabled automatic copy/paste. Xcompmgr supplies real compositing; the theme and wallpaper come from this machine's active Nord installation. This is an isolated X11 render on an Omarchy host, not a live Hyprland desktop recording or a physical-microphone/F9 acceptance test. Cloud recognition and native Codex rewriting use the selected authenticated providers. No credential values are stored in the published evidence.

## Legacy media

All retained v0.1.1 media and its original manifest are under [legacy-v0.1.1](assets/legacy-v0.1.1/manifest.json). Those Fedora-container demonstrations use scripted text and a fake rewrite provider. They are historical UI fixtures, not v0.3.0 recognition footage. The stable `mluva-omarchy-demo.mp4` and `mluva-omarchy-vertical.mp4` filenames now contain the real released JFK recording and its portrait crop. Source licenses and model-generated material are identified separately; public-domain status is claimed only for the historical government speech recording.
