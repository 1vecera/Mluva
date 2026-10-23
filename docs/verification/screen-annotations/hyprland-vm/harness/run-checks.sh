#!/usr/bin/env bash
# Screen-annotation checks on real Hyprland in the test VM, driven from the host.
#
#   run-checks.sh EVIDENCE_DIR PHASE…
#
# Phases: setup, f9 (bindings, capitals, drawing, pins, Stop into a text field), agent (Stop to a
# stub agent), cancel, killed, portal, click (absorbed first click and fade timing), and with two
# outputs two and starts; collect copies logs and the bundle. Input goes only to the VM's USB keyboard
# and tablet through QMP; every observation is read inside the guest (hyprctl, grim, the probe's log).
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
VM=${VM_DIR:-$(cd "$HERE/../../../../.." && pwd)/tmp/hyprland-vm}
export VM_DIR=$VM
EV=$(realpath -m "$1")
shift
mkdir -p "$EV"

g() { "$HERE/vm.sh" ssh "$@"; }
q() { "$HERE/vm.sh" qmp "$@"; }
s() { g "cd ~/mluva && ./session.sh $*"; }
log() { printf '%s %s\n' "$(date +%T.%3N)" "$*" | tee -a "$EV/driver.log"; }
fetch() { scp -q -F "$VM/run/ssh_config" "hyprvm:$1" "$2"; }
shot() {
  g '. ~/henv.sh; grim -t png /tmp/shot.png'
  fetch /tmp/shot.png "$EV/$1.png"
}
binds() { s binds >"$EV/$1.json"; log "bindings ($1): $(jq -c '[.[].description]' "$EV/$1.json")"; }
probe_state() {
  g 'pkill -USR1 -f "mluva/prob[e].py"; sleep 0.3; tail -1 ~/mluva/run/probe.jsonl'
}
primary() { g '. ~/henv.sh; timeout 2 wl-paste --primary --no-newline 2>/dev/null || true'; }
red() { g ". ~/henv.sh; grim -t ppm /tmp/r.ppm; python3 ~/mluva/redcount.py /tmp/r.ppm $*"; }
wait_annotation() {
  for _ in $(seq 40); do
    [[ $(s binds | jq '[.[] | select(.description | startswith("Mluva annotation"))] | length') == 2 ]] && return 0
    sleep 0.25
  done
  return 1
}

# Layout: probe tiled left, stub agent right, synthetic settings floating over both,
# Mluva's own window on workspace 2 (window rules in the guest's hyprland.lua).
setup() {
  log "fresh session"
  s stop-all
  # Fixture outputs from an earlier run make the audio fixture and the Herdr peer refuse to start.
  g 'rm -rf ~/mluva/run/*.log ~/mluva/run/*.jsonl ~/mluva/run/banner ~/mluva/run/voice.sock \
    ~/mluva/tools/*.ready.json ~/mluva/tools/release-pcm ~/mluva/tools/durable ~/mluva/tools/capture.wav \
    ~/.local/share/mluva ~/.local/state/mluva /tmp/fade /tmp/fade.jsonl'
  local port
  port=$(s speech)
  g "cd ~/mluva && SPEECH_PORT=$port ./setup.sh" >/dev/null
  s herdr
  q move 300 500
  s probe
  sleep 1.5
  q move 1500 500
  s agent-terminal
  sleep 1.5
  s settings
  sleep 1.5
  s mluva >/dev/null
  sleep 2
  g '. ~/henv.sh; hyprctl -j clients' | jq '[.[] | {class, at, size, floating, workspace: .workspace.id}]' >"$EV/clients.json"
  g '. ~/henv.sh; hyprctl -j monitors' | jq '[.[] | {name, width, height, x, y, scale}]' >"$EV/monitors.json"
  g '. ~/henv.sh; hyprctl version | head -1; pacman -Q hyprland xdg-desktop-portal-hyprland xdg-desktop-portal aquamarine gtk4 linux' >"$EV/guest-versions.txt"
  log "windows: $(jq -c '[.[] | [.class, .at, .size]]' "$EV/clients.json")"
}

circle() {
  python3 - "$@" <<'EOF'
import math, sys
cx, cy, rx, ry = map(float, sys.argv[1:5])
for i in range(73):
    a = i / 72 * 2 * math.pi + 0.3
    print(round(cx + rx * math.cos(a)), round(cy + ry * math.sin(a)))
EOF
}

f9() {
  circle 1230 811 120 52 >"$VM/run/circle.txt"
  log "focus the probe's text with an ordinary click"
  q move 400 300 sleep 150 click sleep 300
  binds 01-bindings-before
  shot 01-before-recording
  log "F9: start dictation into the focused text field"
  q key f9
  wait_annotation || { log "annotation bindings did not appear"; binds 02-bindings-recording; return 1; }
  binds 02-bindings-recording

  log "check 2: type 'Hello World' with Shift on the virtual keyboard"
  g '. ~/henv.sh; grim -t ppm /tmp/ref.ppm'
  q type ' Hello World'
  g 'tail -40 ~/mluva/run/probe.jsonl' | jq -c 'select(.kind == "key" and (.name == "H" or .name == "W"))' >"$EV/capitals-keys.jsonl"
  probe_state >"$EV/capitals-state.json"
  log "red pixels in the probe after typing: $(red 10 10 954 1070)"

  log "check 3: hold Shift, circle the Save button (floating settings), release"
  g '. ~/henv.sh; grim -t ppm /tmp/ref.ppm'
  q move 1350 811 sleep 100 down shift sleep 200 path "$VM/run/circle.txt" 12 sleep 80
  shot 03-drawing-circle
  g '. ~/henv.sh; nohup python3 ~/mluva/sampler.py /tmp/fade 2.5 /tmp/ref.ppm 700 150 1400 910 >/tmp/fade.jsonl 2>&1 &'
  sleep 0.3
  q up shift
  sleep 3
  log "ordinary click with Shift up into the probe's text"
  q move 400 300 sleep 150 click sleep 300
  g 'tail -4 ~/mluva/run/probe.jsonl' | jq -c 'select(.kind == "click")' >"$EV/ordinary-click.jsonl"

  log "underline the sync folder label"
  q move 770 540 sleep 100 down shift sleep 200 line 770 540 1262 542 40 12 sleep 80 up shift
  sleep 1.5
  log "banner appears"
  g 'touch ~/mluva/run/banner'
  sleep 1.2
  log "pin the banner"
  q move 1000 300 sleep 300 down shift sleep 150 press left sleep 80 release left sleep 200 up shift
  sleep 1.5

  log "check 4: Shift-click without moving on the word 'kappa' in the probe"
  probe_state >"$EV/pin-before-state.json"
  primary >"$EV/pin-primary-before.txt"
  local before
  before=$(g 'wc -l < ~/mluva/run/probe.jsonl')
  q move 265 80 sleep 400 down shift sleep 150 press left sleep 80 release left sleep 40
  shot 04-pin-on-text
  q sleep 200 up shift sleep 400
  probe_state >"$EV/pin-after-state.json"
  primary >"$EV/pin-primary-after.txt"
  g "tail -n +$((before + 1)) ~/mluva/run/probe.jsonl" >"$EV/pin-probe-events.jsonl"
  sleep 2

  log "Stop with F9; Mluva delivers into the captured text field"
  probe_state >"$EV/delivery-before-state.json"
  q key f9
  for _ in $(seq 60); do
    g 'grep -q "Here is the synthetic" ~/mluva/run/probe.jsonl' && break
    sleep 0.5
  done
  sleep 1.5
  probe_state >"$EV/delivery-after-state.json"
  binds 05-bindings-after-stop
  shot 05-delivered-text-field
  status 05-mluva-status
}

# Mluva's own window (workspace 2) and its stage outcomes, without transcript text.
status() {
  g ". ~/henv.sh; hyprctl dispatch 'hl.dsp.focus({ workspace = 2 })' >/dev/null; sleep 0.7; grim -t png /tmp/shot.png; hyprctl dispatch 'hl.dsp.focus({ workspace = 1 })' >/dev/null"
  fetch /tmp/shot.png "$EV/$1.png"
  g "sqlite3 ~/.local/share/mluva/diagnostics.sqlite3 'select stage, provider, outcome from diagnostic_events order by rowid'" >"$EV/$1-diagnostics.txt"
}

# Agent dictation with F8 to the stub agent in the terminal; delivery through the guarded send.
agent() {
  g "cd ~/mluva && bundle/bin/mluva herdr select 'Synthetic agent' --socket ~/mluva/run/voice.sock" >"$EV/agent-select.json"
  log "agent selected: $(jq -c .ok "$EV/agent-select.json" 2>/dev/null || cat "$EV/agent-select.json")"
  q move 1500 600 sleep 200
  log "F8 opens the session wheel; F8 again records to the selected agent"
  q key f8 sleep 1200
  shot 06-agent-wheel
  q key f8
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  binds 06-agent-bindings-recording
  sleep 3
  log "underline the stub agent's status line; circle the floating window's Save button"
  q move 970 40 sleep 100 down shift sleep 200 line 970 40 1366 42 30 12 sleep 80 up shift
  sleep 3
  q move 1350 811 sleep 100 down shift sleep 200 path "$VM/run/circle.txt" 12 sleep 80 up shift
  sleep 4
  log "F8: stop and send"
  q move 1500 600 sleep 100 key f8
  for _ in $(seq 60); do
    g 'grep -q prompt_guarded ~/mluva/run/herdr-wire.jsonl 2>/dev/null' && break
    sleep 0.5
  done
  sleep 1.5
  binds 07-agent-bindings-after-stop
  shot 07-agent-received
}

cancel() {
  q move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  binds 08-cancel-bindings-recording
  log "Ctrl+Alt+Escape: cancel"
  q key ctrl alt esc
  sleep 1.5
  binds 08-cancel-bindings-after
}

# A killed Mluva: its watchdog removes the bindings; with the watchdog killed too, the next start does.
killed() {
  q move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  s pids | tee "$EV/09-kill-pids.txt"
  binds 09-kill-bindings-recording
  log "SIGKILL Mluva's main process only"
  g "kill -KILL \$(systemctl --user show -p MainPID --value mluva)"
  sleep 3
  binds 09-kill-bindings-after-main-killed
  s pids >>"$EV/09-kill-pids.txt"
  g 'pkill -f -- "--annotation-cleanu[p]"; rm -f ~/mluva/tools/*.ready.json'
  s mluva >/dev/null
  sleep 2
  q move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  s pids | tee "$EV/10-kill-pids.txt"
  log "SIGKILL Mluva and its watchdog together"
  g "kill -KILL \$(systemctl --user show -p MainPID --value mluva) \$(pgrep -f -- '--annotation-cleanu[p]')"
  sleep 3
  binds 10-kill-bindings-after-both-killed
  # shellcheck disable=SC2016 # expanded in the guest
  g 'ls -d "$XDG_RUNTIME_DIR"/mluva/mluva-annotation-* 2>/dev/null | wc -l' >"$EV/10-frame-directories-after-kill.txt"
  log "restart Mluva: startup cleanup"
  g 'rm -f ~/mluva/tools/*.ready.json'
  s mluva >/dev/null
  sleep 3
  binds 10-kill-bindings-after-restart
  # shellcheck disable=SC2016 # expanded in the guest
  g 'ls -d "$XDG_RUNTIME_DIR"/mluva/mluva-annotation-* 2>/dev/null | wc -l' >"$EV/10-frame-directories-after-restart.txt"
}

# Without the portal: annotation stays off with a status and Shift-click keeps selecting text.
portal() {
  log "restart Mluva with MLUVA_DISABLE_GLOBAL_SHORTCUT=1"
  s quit
  g 'rm -f ~/mluva/tools/*.ready.json'
  s mluva MLUVA_DISABLE_GLOBAL_SHORTCUT=1 >/dev/null
  sleep 2
  q move 400 300 sleep 150 click sleep 300
  s action global-record
  sleep 2
  binds 11-portal-off-bindings-recording
  probe_state >"$EV/11-portal-off-before-state.json"
  log "Shift-click on 'kappa' with the portal off"
  q move 265 80 sleep 300 down shift sleep 150 press left sleep 80 release left sleep 200 up shift sleep 400
  probe_state >"$EV/11-portal-off-after-state.json"
  primary >"$EV/11-portal-off-primary.txt"
  g ". ~/henv.sh; hyprctl dispatch 'hl.dsp.focus({ workspace = 2 })' >/dev/null; sleep 0.7; grim -t png /tmp/shot.png; hyprctl dispatch 'hl.dsp.focus({ workspace = 1 })' >/dev/null"
  fetch /tmp/shot.png "$EV/11-portal-off-status.png"
  s action cancel
  sleep 1
  s quit
  g 'rm -f ~/mluva/tools/*.ready.json'
  s mluva >/dev/null
}

# Measure the first click after Shift release without movement, against a moved click.
click() {
  g 'grep -c "\"button\"" ~/mluva/run/probe.jsonl || true' >"$EV/12-clicks-before.txt"
  q move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  sleep 1
  local n
  for n in 1 2 3 4 5; do
    log "absorbed-click trial $n: stroke ending on the button, release Shift, click without moving"
    q move 300 1013 sleep 100 down shift sleep 150 line 300 1013 480 1013 15 12 sleep 80 up shift sleep 400 click sleep 600
    g "echo trial $n \$(grep -c '\"button\"' ~/mluva/run/probe.jsonl)" >>"$EV/12-absorbed-clicks.txt"
    log "control trial $n: same stroke, move 2 px after release, then click"
    q move 300 1013 sleep 100 down shift sleep 150 line 300 1013 480 1013 15 12 sleep 80 up shift sleep 400 move 482 1013 sleep 100 click sleep 600
    g "echo control $n \$(grep -c '\"button\"' ~/mluva/run/probe.jsonl)" >>"$EV/12-absorbed-clicks.txt"
  done
  log "fade: stroke over the probe, frames on the probe's clock"
  circle 450 600 180 110 >"$VM/run/circle-probe.txt"
  g '. ~/henv.sh; grim -t ppm /tmp/ref.ppm; rm -rf /tmp/fade'
  q move 660 600 sleep 100 down shift sleep 200 path "$VM/run/circle-probe.txt" 12 sleep 80
  g '. ~/henv.sh; setsid python3 ~/mluva/sampler.py /tmp/fade 2.6 /tmp/ref.ppm 200 400 700 800 >/tmp/fade.jsonl 2>&1 < /dev/null &'
  sleep 0.6
  q up shift
  sleep 6
  g 'grep "\"Shift_L\"" ~/mluva/run/probe.jsonl | tail -1' >"$EV/13-fade-release.json"
  # The sampler measures every frame after capturing; wait for it before reading its output.
  g 'while pgrep -f "mluva/sample[r].py" >/dev/null; do sleep 1; done'
  g 'cat /tmp/fade.jsonl' >"$EV/13-fade.jsonl"
  q key ctrl alt esc
  sleep 1
}

# Two outputs, the second at scale 1.5: overlay and crops on each. Needs `vm.sh start 2`.
two() {
  q --size 3200x1080 move 2500 360 sleep 300
  s notes
  sleep 2
  g '. ~/henv.sh; hyprctl -j clients' | jq '[.[] | {class, at, size, floating, workspace: .workspace.id, monitor}]' >"$EV/two-clients.json"
  g '. ~/henv.sh; hyprctl -j monitors' | jq '[.[] | {name, width, height, x, y, scale, activeWorkspace: .activeWorkspace.id}]' >"$EV/two-monitors.json"
  log "windows: $(jq -c '[.[] | [.class, .at, .size, .monitor]]' "$EV/two-clients.json")"
  local cx cy
  read -r cx cy < <(jq -r '.[] | select(.class == "synthetic.notes") | "\(.at[0] + 170) \(.at[1] + 70)"' "$EV/two-clients.json")
  circle "$cx" "$cy" 150 40 >"$VM/run/circle-two.txt"
  q --size 3200x1080 move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  sleep 2
  log "circle the notes' first bullet on Virtual-2 (scale 1.5)"
  q --size 3200x1080 move $((cx + 150)) "$cy" sleep 100 down shift sleep 200 path "$VM/run/circle-two.txt" 12 sleep 80
  g '. ~/henv.sh; grim -t png /tmp/shot.png'
  fetch /tmp/shot.png "$EV/14-two-outputs-drawing.png"
  q --size 3200x1080 up shift sleep 2500
  log "pin on Virtual-2, underline on Virtual-1"
  q --size 3200x1080 move $((cx - 40)) $((cy + 30)) sleep 400 down shift sleep 150 press left sleep 80 release left sleep 200 up shift sleep 2500
  q --size 3200x1080 move 34 66 sleep 100 down shift sleep 200 line 34 66 330 67 30 12 sleep 80 up shift sleep 2500
  q --size 3200x1080 move 400 300 sleep 300 key f9
  for _ in $(seq 60); do
    g 'grep -q "Screen annotation" ~/mluva/run/probe.jsonl' && break
    sleep 0.5
  done
  binds 14-two-bindings-after-stop
}

# Where strokes begin when motion starts with the Shift press, or 50 ms after it, on both
# outputs. Needs `vm.sh start 2`. Lines run 400 px in 33 steps of 12 ms from local x 80 and 300.
starts() {
  g 'rm -f ~/mluva/tools/*.ready.json'
  q --size 3200x1080 move 400 300 sleep 150 click sleep 300 key f9
  wait_annotation || { log "annotation bindings did not appear"; return 1; }
  sleep 2
  local delay
  for delay in 0 0 0 50 50; do
    log "lines on Virtual-2 and Virtual-1, motion ${delay} ms after Shift"
    q --size 3200x1080 move 2000 600 sleep 400 down shift sleep "$delay" line 2000 600 2400 600 33 12 sleep 80 up shift sleep 2800
    q --size 3200x1080 move 300 700 sleep 400 down shift sleep "$delay" line 300 700 700 700 33 12 sleep 80 up shift sleep 2800
  done
  q --size 3200x1080 move 400 300 sleep 300 key f9
  sleep 8
}

# Open item: with annotation on, a click on a button in the target and back into its text field
# during the recording sometimes made insertion fall back to the clipboard. Compared with annotation off.
insertion() {
  local enabled trial before after
  for enabled in true false; do
    g "echo '{\"enabled\": $enabled}' > ~/.config/mluva/screen-annotations.json"
    s quit
    g 'rm -f ~/mluva/tools/*.ready.json'
    s mluva >/dev/null
    sleep 2
    for trial in 1 2; do
      before=$(g 'pkill -USR1 -f "mluva/prob[e].py"; sleep 0.3; tail -1 ~/mluva/run/probe.jsonl' | jq '.text | [scan("Here is the synthetic settings page")] | length')
      q move 400 300 sleep 150 click sleep 300 key f9 sleep 3000 move 480 1013 sleep 200 click sleep 300 move 400 300 sleep 150 click sleep 3000 key f9
      sleep 8
      after=$(g 'pkill -USR1 -f "mluva/prob[e].py"; sleep 0.3; tail -1 ~/mluva/run/probe.jsonl' | jq '.text | [scan("Here is the synthetic settings page")] | length')
      echo "annotation $enabled trial $trial inserted $((after - before))" | tee -a "$EV/15-insertion-after-button.txt"
      g 'rm -f ~/mluva/tools/*.ready.json'
    done
  done
  g "echo '{\"enabled\": true}' > ~/.config/mluva/screen-annotations.json"
}

collect() {
  g 'cat ~/mluva/run/probe.jsonl' >"$EV/probe.jsonl"
  g 'cat /tmp/fade.jsonl 2>/dev/null' >"$EV/fade.jsonl" || true
  g 'cat ~/mluva/run/speech-requests.jsonl' >"$EV/speech-requests.jsonl"
  g 'cat ~/mluva/run/herdr-wire.jsonl 2>/dev/null' >"$EV/herdr-wire.jsonl" || true
  g 'cat ~/mluva/run/mluva.log' >"$EV/mluva.log"
  local bundle
  bundle=$(g 'ls -td ~/.local/share/mluva/annotations/*/ 2>/dev/null | head -1')
  if [[ -n $bundle ]]; then
    mkdir -p "$EV/bundle"
    g "cd $bundle && tar -cf - ." | tar -C "$EV/bundle" -xf -
    g "stat -c '%a %n' $bundle $bundle* $bundle/frames/*" >"$EV/bundle-modes.txt"
  fi
}

for phase in "$@"; do
  "$phase"
done
