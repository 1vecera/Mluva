# Launch film 2026: real-app takes

The takes behind the launch film (under a minute; storyboard v1). Every frame of Mluva, Codex and Claude Code in them is the running app, captured offscreen from `main` at `9838bc8`, lossless at 60 fps on a 3840×2160 screen at scale 2. Sessions, pages, paths and sentences are invented; speech and models are loopback peers, so nothing reaches a provider. The records in `src/launch/` hold each take's timeline, fixtures, what is scripted and its frame checks. Media stays out of Git, under `~/Videos/Mluva/launch-2026-media/`.

| Take | Shows | Where it runs |
| --- | --- | --- |
| A · F9 into a field | The Omarchy widget grows the live words; on Stop, Mluva pastes the text into the reply field of an invented notes page in Firefox | `capture/field_take.sh` in `dev/verify.sh session` (X11) |
| B · The dial | The session wheel draws itself open from one o'clock over a busy invented desktop; the hand and its light trace move between four sessions | `capture/wheel_take.sh`: private headless sway, so the wheel is the same layer-shell overlay as on Hyprland |
| C · Delivered | F12 opens the wheel, F12 records, F12 stops; after the two-second window a disposable Codex TUI takes it as its next turn, then a disposable Claude Code session gets it through its inbox | `capture/delivery_take.sh` in `dev/verify.sh session` (X11), from the universal showreel |

## Make the takes

Build from a checkout of `main` (the release build takes about a minute when seeded from another build of the same commit):

```sh
cp launch-video/launch/capture/launch_wheel_take.rs rust/mluva-gtk/examples/
git apply launch-video/showreel/capture/harness.patch   # screen size, GDK scale and binaries into dev/verify.sh
export CARGO_TARGET_DIR="$PWD/tmp/native-build"
cargo build --locked --release -p mluva-gtk --bin mluva --bin global-shortcut-fixture-peer --example mluva_verify \
  --example launch_wheel_take -p mluva-audio --bin audio-fixture-peer --bin mluva-audio-cleanup -p mluva-shell --bin mluva-shell
```

Take C needs Codex's whole release package in `capture/tools/codex-pkg/` and the `claude` binary at `capture/tools/claude` (ignored), because `dev/verify.sh` hides `/home`. Then, with `R="$CARGO_TARGET_DIR/release"` and `S="OFFSCREEN_SCREEN_SPEC=3840x2160x24 OFFSCREEN_GDK_SCALE=2"`:

```sh
env MLUVA_VERIFY_BINARIES="$R" $S bash dev/verify.sh session "$PWD/tmp/takes/a-field" -- bash "$PWD/launch-video/launch/capture/field_take.sh" "$R"
bash launch-video/launch/capture/wheel_take.sh "$R" ~/Videos/Mluva/launch-2026-media/runs/b-dial
env MLUVA_VERIFY_BINARIES="$R" $S bash dev/verify.sh session "$PWD/tmp/takes/c-delivered" -- bash "$PWD/launch-video/launch/capture/delivery_take.sh" "$R"
```

A and C are recorded in real time with FFmpeg's `x11grab` into ffvhuff, which keeps up at 4K60 on a loaded machine where x264 and FFV1 drop frames; `ffmpeg-progress.txt` in each run reports dropped and duplicated frames. B steps the wheel's own clock (`SessionWheel::step_motion`) one 1/60 s frame at a time and grabs the composited output with `grim` after every paint, so it plays at real speed however slowly the software renderer draws.

## Stage and check

```sh
bash launch-video/launch/stage.sh NAME CAPTURE ~/Videos/Mluva/launch-2026-media > checks.json
uv run --with numpy python launch-video/launch/capture/dial_check.py TAKE REGION SPEC.json
```

`stage.sh` transcodes a real-time capture to FFV1 and compares every frame's decoded RGB MD5, writes each frame of the FFV1 take as PNG and compares every staged PNG's RGB MD5 with the take's frame, then makes a contact sheet. `dial_check.py` measures the wheel in the frames themselves: the ring by a circle fit, the hand's bearing at rest and the ticks' bearings, each against `clock_angle(index, count)` from `rust/mluva-gtk/src/agent_session_wheel.rs`. Over a busy desktop it subtracts a frame of the same screen without the wheel.
