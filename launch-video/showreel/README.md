# Mluva showreel '26

A 15-second, 1920×1080, 60 fps motion-design reel: eight chapters, one bar each of a 128 BPM score. It follows the chapter structure of a motion-design showreel (kinetic type, UI animation, motion systems, 3D particles, logo animation, data visualisation, type montage, end card) and tells Mluva's story through it.

| Frames | Chapter | What it shows |
| --- | --- | --- |
| 0–113 | 01 Kinetic type | The recording light, then *Speak freely · Shape it · Keep every word*, each ending in the glossy mark as its full stop |
| 113–226 | 02 UI animation | Real footage of the production Omarchy widget: live words, F9 start and stop, Transcribing, Review |
| 226–338 | 03 Motion systems | Speak → Transcribe → Copy, then Rewrite on demand, one node per beat |
| 338–451 | 04 3D / particles | A sphere of the real widget in all 22 installed Omarchy themes, counted one by one |
| 451–563 | 05 Logo animation | The drop: the light bursts into the mark, which settles into the large-mark lockup |
| 563–676 | 06 Data visualisation | Local or cloud: the speech and rewrite engines from the provider guide |
| 676–788 | 07 Type montage | Eight cuts on eighth notes: F9, Live rewrite, Polish/Structure, history, Incognito, Ctrl+P, export, open source |
| 788–900 | 08 End card | Lockup, "The most delightful dictation for Omarchy." and github.com/1vecera/Mluva |

`src/showreel/timing.ts` holds the beat grid (`0.014 s + n × 0.46875 s`); every chapter derives its keyframes from it. `theme.ts` holds the identity: black, #F5F5F5 ink, one red accent, Adwaita Sans on a five-step scale and JetBrains Mono labels.

## Real UI and honest claims

No part of the app is redrawn. `capture_widget.py` drives the production QML widget, bridge and Omarchy controls on the private X11 display from `dev/run-isolated.sh`; only the recognition events are scripted, over the app's own D-Bus signal. The take uses Omarchy's Vantablack theme at `QT_SCALE_FACTOR=3` and is recorded losslessly at 60 fps. Words stream about six times faster than speech, but the widget's timer runs at natural pace (about 156 words per minute), so the chapter is a time-lapse of a seven-second note and says so on screen. `MLUVA_CAPTURE_MODE=themes` photographs the recording and review states under every theme in `/usr/share/omarchy/themes`.

The reel shows the take as its original PNG frames; a per-frame RGB MD5 comparison against the capture matches all 308 frames. The transcript in chapter 03 is a crop of one of those frames. Callout leaders stop outside the widget, so no captured pixel is covered.

The logo animates two clipped copies of `mluva-logo-large-mark-on-dark.svg`. Whenever mark and wordmark are at rest the reel draws that file directly: `ShowreelLockupCheck` renders the resting frames of chapters 05 and 08 against the brand SVG, and the pixel diff is zero in both.

Feature names and limits come from the repository docs: the providers from `docs/provider-selection.md`, and the Experimental tags on Live rewrite, Incognito, the local model and compatible APIs from `docs/feature-maturity.md` and the README.

## Build

Requires Node/npm, FFmpeg, ImageMagick, uv, and for captures Xvfb, D-Bus and Quickshell with Omarchy installed.

Historical Python preparation recipes were retired in the 2.0.0 cutover. Committed outputs remain; the original recipes are available in the v1.6.0 tag.

`prepare.py` also builds `AdwaitaSans-Black-NoOverlap.ttf`, a static instance (wght 900, opsz 32) with overlaps removed, because `-webkit-text-stroke` on the variable font draws every contour overlap as a seam. Chrome renders display sizes at opsz 32, so outlined and solid type match.

The render is silent. `mix.py` places the score and effects from `src/showreel/cues.json` sample-accurately (each cue marks where the effect peaks), masters to −14 LUFS with a −1.5 dBTP ceiling using FFmpeg's two-pass loudnorm plus a small trim, and muxes 320 kbit/s AAC. Remotion Studio plays the same cue sheet for previews.

## Sound

The score and effects were generated with ElevenLabs Music v2 and Sound Effects v2 on Daniel's account ([flow](https://elevenlabs.io/app/flows/Ilr5SlTt7Blt0Xay6hmO)). The score is take 1 of the first batch; its drop falls on beat 16 and its final impact on beat 28, which is why the logo lands mid-reel. Effect levels were set by analysis, not by ear, and live in `cues.json`. Audio files stay out of Git with the other staged media (`public/showreel/` is ignored); keep the downloaded takes with the local media archive.

# Mluva showreel '26 · universal

A 30-second, 1920×1080, 60 fps reel built on the first one: the same identity, HUD, type scale, logo animation and effects, on 16 bars of a new 128 BPM score. Its story is *speak, any agent, delivered*: one key opens the session wheel, and a dictation reaches Claude Code, Claude Desktop or Codex, with or without Herdr, at the level each route supports. Composition `MluvaShowreelUniversal` (`src/universal/`); `MluvaShowreel` is unchanged.

| Frames | Chapter | What it shows |
| --- | --- | --- |
| 0–226 | 01 Kinetic type | The recording light, then *Speak · Any agent · Delivered*, each ending in the glossy mark |
| 226–451 | 02 Infographics | Before: one way to send. Only Herdr's guarded prompt sent; Claude Code elsewhere was draft only, Codex outside Herdr and Claude Desktop chats were not listed. Then *Rebuilt from scratch* |
| 451–676 | 03 Motion systems | One target model: kind, name, state, delivery; four routes light up one per beat with the level each delivers (Send, Queue, Send if idle, Prefill), Draft only otherwise |
| 676–901 | 04 UI animation | Real footage of the production session wheel: F12 opens it and the hand moves one session per beat; the column beside it names the product, place and delivery level |
| 901–1239 | 05 Screen capture | The drop. Real Mluva records, transcribes, counts down and sends to a disposable Codex TUI, which takes it as its next turn |
| 1239–1576 | 06 Type montage | The re-drop: a disposable Claude Code session takes a queued dictation from its own inbox; then four cuts on the beat (Claude Code, Codex, Claude Desktop, *Any agent*) and *One key* on the score's break |
| 1576–1800 | 07 Logo · end card | The final impact: the light bursts into the mark, the lockup settles; *One key. Any agent.* and github.com/1vecera/Mluva |

`src/universal/timing.ts` holds the 16-bar grid (`0.018 s + n × 0.46875 s`); chapters derive every keyframe from it. `claims.ts` holds every routing claim the reel makes; it follows `docs/agent-routing.md` and the target model (`rust/mluva-core/src/agent_target.rs`) of the routing release. Codex Desktop appears only as a listed session on the wheel: its delivery has not been exercised live, so no shot or caption claims it.

## Real UI, captured offscreen

Nothing in the reel redraws Mluva, Codex or Claude Code. Both takes come from the repository's isolated runners: a private Xvfb screen, D-Bus, network namespace (loopback only), HOME and runtime directory. Annotations sit outside the captured pixels: the wheel and the delivery take are shown through panels whose captions live in the margins, and the HUD bands never overlap them.

**Wheel take.** `capture/showreel_wheel_take.rs` opens the production `SessionWheel` (the overlay styling, Omarchy's Vantablack theme, a 3840×2160 screen at scale 2) with fixture sessions of every kind and route, then moves the choice once per beat. GTK paints the wheel at a few frames a second in software, so the take steps time instead of racing it: `capture/take_clock.c`, preloaded into the driver only, makes the process's monotonic clock return a time the driver sets; the driver advances it in 1 ms ticks and grabs the screen with FFmpeg's `x11grab` after every frame GDK paints. GDK's own frame times in the take's record step exactly 16,667 µs, so the spring and fades play at their real speed, every frame a real paint. The reel shows a crop at half size, a clean 2:1 downsample. Fixture names match the delivery take's sessions, and the hand never stops on Codex Desktop.

**Delivery take.** `capture/delivery_take.sh` runs inside `dev/verify.sh session` with the real Mluva of the routing release: agent key F12 through the private global-shortcut portal fixture, the PCM fixture peer as microphone and a loopback speech endpoint (`capture/speech.py`) returning a scripted dictation. A disposable Codex TUI (`codex`, attached to its own daemon) and a disposable Claude Code session (`claude -n "Portal test"`) run in tmux in Ghostty on loopback models (`capture/models.py`), with no account or network. Both are configured with the model name `loopback-model`, so their own status lines say so (Claude Code also warns that it does not recognise the model). The script presses F12 to open the wheel, again to record and again to stop, for each session in turn, and records the whole 3840×2160 screen in real time, losslessly, while Mluva counts down and delivers: *Sent* for the Codex thread, *Queued* then *Delivered* for Claude Code. The reel plays recording and countdown faster than real time and the rest at real speed, and says *REAL APP · SCRIPTED DEMO · SPED UP · LOOPBACK MODELS* beside it. The agents' replies are the loopback model's scripted text.

**Frame checks.** `stage-universal.mjs` stages both takes as PNG and compares every staged frame's decoded RGB MD5 with the same frame of the lossless take; it refuses to stage on any mismatch. `src/universal/wheel-take.json` and `delivery-take.json` record the takes' timelines, fixtures and check results.

The resting lockup is the brand SVG itself: `UniversalLockupCheck` renders chapter 07 at rest against `mluva-logo-large-mark-on-dark.svg` for a pixel diff.

## Make the takes

Both takes need a checkout of the routing release (the target model and session wheel). In that checkout:

```sh
cp launch-video/showreel/capture/showreel_wheel_take.rs rust/mluva-gtk/examples/
git apply launch-video/showreel/capture/harness.patch   # screen size, GDK scale and binaries into dev/verify.sh
export CARGO_TARGET_DIR="$PWD/tmp/native-build"
cargo build --locked --release -p mluva-gtk --bin mluva --bin global-shortcut-fixture-peer \
  --example mluva_verify --example showreel_wheel_take -p mluva-audio --bin audio-fixture-peer --bin mluva-audio-cleanup
cc -O2 -shared -fPIC -o tmp/take_clock.so launch-video/showreel/capture/take_clock.c
```

The wheel take runs through `dev/run-isolated.sh` with `OFFSCREEN_SCREEN_SPEC=3840x2160x24`, `GDK_SCALE=2`, Mesa's software GL and `LD_PRELOAD=tmp/take_clock.so` on the driver only. The delivery take needs the `codex` package and `claude` binary copied into `capture/tools/` (`dev/verify.sh` hides `/home`), then:

```sh
MLUVA_VERIFY_BINARIES="$CARGO_TARGET_DIR/release" OFFSCREEN_SCREEN_SPEC=3840x2160x24 OFFSCREEN_GDK_SCALE=2 \
  bash dev/verify.sh session tmp/delivery -- bash launch-video/showreel/capture/delivery_take.sh "$CARGO_TARGET_DIR/release"
```

Stage and render from `launch-video/`:

```sh
node showreel/stage-universal.mjs <wheel take dir>
node showreel/stage-universal.mjs delivery <delivery take dir>
npm run universal:render
```

## Sound

The score is one 30-second ElevenLabs Music v2 take from the same flow, prompted in bars for the first reel's style (instrumental electronic trailer cue at 128 BPM). By analysis it sits on the 128 BPM grid 18 ms in, with an eight-bar build, a drop on bar 9 (15.0 s), a break on bar 12 and an impact on bar 13 that rings out for four bars. It is re-arranged on bar boundaries, cutting 25 ms before a downbeat with 20 ms equal-power crossfades: bars 1–11, bars 9–10 again as a re-drop, then bars 12–14. The drop stays at 15.0 s (beat 32), the re-drop lands at 20.6 s (beat 44) and the final impact at 26.25 s (beat 56), with a 3.75 s tail faded over the last 0.6 s. Sixteenth-note spacing stays at 0.117 s across both splices, and the largest sample steps there match the source's own downbeats. A second take had a near-silent 14-second intro and no ending and was not used.

Effects are the first reel's, placed by `src/universal/cues.json` so each peak lands on its frame; the effects on the drop and the final impact sit lower than the rest because the score already hits there. `showreel/master-universal.mjs` mixes them with the score sample-accurately and masters to −14 LUFS with a −1.5 dBTP ceiling: the score arrives mastered to 0 dBFS, so a gentle sample-peak limiter (−2 dBFS) holds its hits, FFmpeg's two-pass loudnorm then applies one linear gain (its LRA target, 20, sits above the mix's own 15 LU) and a measured trim lands the encoded file on −14.0 LUFS. It muxes 320 kbit/s AAC onto the silent render without re-encoding the video. The arrangement points and every effect level were chosen by analysis, not by ear.
