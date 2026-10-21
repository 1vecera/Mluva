#!/usr/bin/env bash
# Offscreen end-to-end run of screen annotation in an isolated headless sway (wlr-layer-shell).
#
#   run.sh HARNESS EVIDENCE_DIR
#
# HARNESS is a layer-shell harness script with `start`, `run CMD…`, `env` and `stop` commands
# that runs headless sway on a private XDG_RUNTIME_DIR, private bus and private HOME, with no
# seat, input device or host display. EVIDENCE_DIR receives screenshots, logs and the bundle.
#
# The real `mluva` executable records paced synthetic PCM from a fake pw-record, receives
# scripted text from a loopback speech peer, draws through its own layer-shell overlay with
# virtual-pointer strokes, and delivers through the guarded Herdr send to a synthetic Herdr peer
# whose prompts a stub agent prints in a foot terminal. Shift is fed through Mluva's
# annotation-ink-down/up and annotation-pin application actions: the harness has no Hyprland.
set -euo pipefail

HARNESS=$(realpath "$1")
EVIDENCE=$(realpath -m "$2")
HERE=$(cd "$(dirname "$0")" && pwd)
CHECKOUT=$(cd "$HERE/../../../.." && pwd)
BIN=${MLUVA_BINARIES:-$CHECKOUT/tmp/native-build/debug}
VPTR=${VPTR:-$(dirname "$HARNESS")/nested/vptr.py}
[[ ! -e $EVIDENCE ]] || { echo "evidence directory exists: $EVIDENCE" >&2; exit 2; }
mkdir -p "$EVIDENCE"
for binary in mluva mluva-audio-cleanup audio-fixture-peer herdr-voice-fixture-peer; do
  [[ -x $BIN/$binary ]] || { echo "missing $BIN/$binary; build it first" >&2; exit 2; }
done

# Mluva's private home: short and synthetic, so delivered paths carry no host details.
APP=$(mktemp -d /tmp/mluva-e2e.XXXXXX)
chmod 700 "$APP"
STARTED=
cleanup() {
  set +e
  if [[ -n $STARTED ]]; then
    h gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux \
      --method org.gtk.Actions.Activate quit '[]' '{}' >/dev/null 2>&1
    sleep 2
    # shellcheck disable=SC2016 # expanded by the shell inside the harness
    h sh -c 'find "$XDG_RUNTIME_DIR/mluva" -maxdepth 1 -name "mluva-annotation-*" 2>/dev/null | wc -l' \
      >"$EVIDENCE/leftover-frame-directories.txt"
    bash "$HARNESS" stop >"$EVIDENCE/harness-stop.log" 2>&1
  fi
  wait 2>/dev/null
  rm -rf "$APP"
}
trap cleanup EXIT

bash "$HARNESS" start >"$EVIDENCE/harness.log" 2>&1 &
for _ in $(seq 100); do
  bash "$HARNESS" env >/dev/null 2>&1 && break
  sleep 0.2
done
STARTED=1
eval "$(bash "$HARNESS" env)"
XDG=$XDG_RUNTIME_DIR
APP_ENV=(HOME="$APP/home" XDG_CONFIG_HOME="$APP/home/.config" XDG_DATA_HOME="$APP/home/.local/share"
  XDG_STATE_HOME="$APP/home/.local/state" XDG_CACHE_HOME="$APP/home/.cache")
h() { bash "$HARNESS" run env "${APP_ENV[@]}" "$@"; }
action() {
  h gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux \
    --method org.gtk.Actions.Activate "$1" '[]' '{}' >/dev/null
}
draw() { h python3 "$VPTR" --size 1280x720 "$@"; }
shot() { h grim -t png "$EVIDENCE/$1.png"; }
log() { printf '%s %s\n' "$(date +%T.%3N)" "$*" | tee -a "$EVIDENCE/driver.log"; }

# Synthetic boundaries: paced PCM, loopback speech, no keyring, no audio devices.
mkdir -p "$APP/bundle/bin" "$APP/tools" "$APP/home/.config/mluva"
cp "$BIN/mluva" "$BIN/mluva-audio-cleanup" "$APP/bundle/bin/"
ln -s "$CHECKOUT/rust/mluva-gtk/resources" "$APP/bundle/resources"
ln -s "$BIN/audio-fixture-peer" "$APP/tools/pw-record"
printf '#!/bin/sh\nexit 1\n' >"$APP/tools/secret-tool"
printf '#!/bin/sh\nprintf "[]\\n"\n' >"$APP/tools/pw-dump"
chmod 700 "$APP/tools/secret-tool" "$APP/tools/pw-dump"
python3 - "$APP/tools/test-config.json" <<'EOF'
import json, sys
# 0.1 s of quiet synthetic PCM, repeated at real-time pace until Stop.
json.dump({"pcm_hex": "01000200" * 800, "fragments": [3200], "bytes_per_second": 32000,
           "repeat": True, "interval_ms": 0, "wait": True}, open(sys.argv[1], "w"))
EOF

h python3 "$HERE/speech_peer.py" "$EVIDENCE/speech-requests.jsonl" >"$EVIDENCE/speech-port" &
for _ in $(seq 50); do [[ -s $EVIDENCE/speech-port ]] && break; sleep 0.1; done
PORT=$(cat "$EVIDENCE/speech-port")
SOCKET=$XDG/voice.sock
python3 - "$APP" "$PORT" "$SOCKET" <<'EOF'
import json, sys
app, port, socket = sys.argv[1:]
config = f"{app}/home/.config/mluva"
json.dump({"transcription_provider": "litellm", "transcription_base_url": f"http://127.0.0.1:{port}/v1",
           "transcription_api_key_env": "MLUVA_UNUSED_SYNTHETIC_KEY", "welcome_completed": True,
           "transcription_chunk_seconds": 3, "audio_retention_policy": "never"},
          open(f"{config}/config.json", "w"))
json.dump({"enabled": True, "socket": socket}, open(f"{config}/agent-voice.json", "w"))
json.dump({"enabled": True}, open(f"{config}/screen-annotations.json", "w"))
json.dump({"agents": [{"guard": "a" * 64, "agent": {"name": "Synthetic agent", "pane_id": "w1:p1",
           "agent": "claude", "agent_status": "idle", "cwd": "/synthetic/project",
           "terminal_title_stripped": "Synthetic agent"}}]}, open(f"{app}/herdr-state.json", "w"))
EOF

h "$BIN/herdr-voice-fixture-peer" "$SOCKET" "$APP/herdr-state.json" "$APP/herdr-wire.jsonl" \
  >"$EVIDENCE/herdr-peer.log" 2>&1 &
log "starting Mluva"
h env PATH="$APP/tools:/usr/bin" MLUVA_DISABLE_GLOBAL_SHORTCUT=1 GSETTINGS_BACKEND=memory \
  "$APP/bundle/bin/mluva" >"$EVIDENCE/app.log" 2>&1 &
for _ in $(seq 150); do
  h gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux \
    --method org.gtk.Actions.List 2>/dev/null | grep -q annotation-pin && break
  sleep 0.2
done
sleep 1
# Keep Mluva's window on another workspace; the overlay covers the output regardless.
h swaymsg '[app_id="com.mluva.Linux"] move container to workspace 2' >/dev/null || true
h swaymsg workspace 1 >/dev/null
h python3 "$HERE/synthetic_settings.py" "$APP/banner" >"$EVIDENCE/synthetic-app.log" 2>&1 &
sleep 1
h foot --app-id stub-agent --font monospace:size=9 python3 "$HERE/stub_agent.py" "$APP/herdr-wire.jsonl" \
  >"$EVIDENCE/stub-agent.log" 2>&1 &
sleep 2
shot 01-before-recording
for _ in $(seq 25); do
  h "$APP/bundle/bin/mluva" herdr list --socket "$SOCKET" >"$EVIDENCE/list.json" 2>&1 || true
  h "$APP/bundle/bin/mluva" herdr select "Synthetic agent" --socket "$SOCKET" >"$EVIDENCE/select.json" 2>&1 \
    && grep -q '"ok": true' "$EVIDENCE/select.json" && break
  sleep 0.4
done
grep -q '"ok": true' "$EVIDENCE/select.json" || { log "agent selection failed"; exit 1; }

log "recording to the agent"
h "$APP/bundle/bin/mluva" herdr record >"$EVIDENCE/record-start.json"
for _ in $(seq 100); do [[ -e $APP/tools/raw.ready.json ]] && break; sleep 0.05; done
log "first PCM written"
sleep 4.5

# Mark 1: circle the Save button below the card (window-local 470,640 120x44).
python3 - "$APP/circle.txt" <<'EOF'
import math, sys
with open(sys.argv[1], "w") as out:
    for i in range(0, 73):
        a = i / 72 * 2 * math.pi + 0.3
        print(round(530 + 95 * math.cos(a)), round(662 + 40 * math.sin(a)), file=out)
EOF
log "ink down: circle"
action annotation-ink-down
sleep 0.15
draw path "$APP/circle.txt" 12
shot 02-drawing-circle
action annotation-ink-up
log "ink up"
sleep 4

# Mark 2: underline the label that runs past the card's edge.
log "ink down: underline"
action annotation-ink-down
sleep 0.15
draw line 70 386 634 388 40 12
action annotation-ink-up
log "ink up"
sleep 1.9

# A scripted screen change: the app shows an error banner, as the narration says "Now".
log "banner appears"
touch "$APP/banner"
sleep 1.3

# Mark 3: a pin on the banner. Without Hyprland's cursor query the pin goes where the pointer
# last moved over the overlay, so the pointer moves there while Shift is held.
log "ink down: pin"
action annotation-ink-down
sleep 0.15
draw move 330 300
action annotation-pin
sleep 0.4
shot 03-pin
action annotation-ink-up
log "ink up"
sleep 3

log "stopping"
h "$APP/bundle/bin/mluva" herdr record >"$EVIDENCE/record-stop.json"
for _ in $(seq 150); do
  grep -q prompt_guarded "$APP/herdr-wire.jsonl" 2>/dev/null && break
  sleep 0.2
done
log "agent prompt observed: $(grep -c prompt_guarded "$APP/herdr-wire.jsonl" 2>/dev/null || echo 0)"
sleep 1.5
shot 04-agent-received

cp "$APP/herdr-wire.jsonl" "$EVIDENCE/herdr-wire.jsonl" 2>/dev/null || true
BUNDLE=$(find "$APP/home/.local/share/mluva/annotations" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | head -1)
if [[ -n $BUNDLE ]]; then
  mkdir -p "$EVIDENCE/bundle"
  cp -r "$BUNDLE"/. "$EVIDENCE/bundle/"
  stat -c '%a %n' "$BUNDLE" "$BUNDLE"/* "$BUNDLE"/frames/* >"$EVIDENCE/bundle-modes.txt"
fi
python3 - "$APP/home/.local/share/mluva/history.sqlite3" >"$EVIDENCE/history.json" <<'EOF'
import json, sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
rows = db.execute("SELECT h.identifier, h.raw_text, h.delivered_text, a.identifier, a.keyframes FROM transcription_history h LEFT JOIN conversation_annotations a ON a.history_identifier = h.identifier").fetchall()
print(json.dumps([dict(zip(["conversation", "raw", "working", "annotation", "keyframes"], row)) for row in rows], indent=2))
EOF
echo "$APP" >"$EVIDENCE/app-home.txt"
log "done"
