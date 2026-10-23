#!/usr/bin/env bash
# Guest side: start and stop the pieces of one test session as transient user units.
#   session.sh speech | herdr | mluva [VAR=value…] | probe | settings | notes | agent-terminal | action NAME | quit | kill | binds | stop-all
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=henv.sh
. "$HOME/henv.sh"
M=$HOME/mluva
R=$M/run
unit() {
  local name=$1; shift
  systemctl --user stop "$name" 2>/dev/null || true
  # Leftovers such as a clipboard-serving wl-copy keep the unit loaded under its name.
  systemctl --user kill --kill-whom=all -s KILL "$name" 2>/dev/null || true
  systemctl --user reset-failed "$name" 2>/dev/null || true
  systemd-run --user --quiet --collect --unit="$name" -p "StandardOutput=append:$R/$name.log" \
    -p "StandardError=append:$R/$name.log" --setenv=WAYLAND_DISPLAY="$WAYLAND_DISPLAY" \
    --setenv=HYPRLAND_INSTANCE_SIGNATURE="$HYPRLAND_INSTANCE_SIGNATURE" \
    --setenv=XDG_CURRENT_DESKTOP=Hyprland --setenv=XDG_SESSION_TYPE=wayland "$@"
}
action() {
  gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux \
    --method org.gtk.Actions.Activate "$1" '[]' '{}' >/dev/null
}
case $1 in
  speech) rm -f "$R/speech-port"; unit speech sh -c "python3 $M/offscreen/speech_peer.py $R/speech-requests.jsonl >$R/speech-port"
    for _ in $(seq 50); do [[ -s $R/speech-port ]] && break; sleep 0.1; done; cat "$R/speech-port" ;;
  herdr) unit herdr "$M/tools/herdr-voice-fixture-peer" "$R/voice.sock" "$R/herdr-state.json" "$R/herdr-wire.jsonl" ;;
  mluva) shift; env_args=()
    for pair in "$@"; do env_args+=("--setenv=$pair"); done
    # KillMode=process: like a desktop launch, Mluva's helpers outlive a killed main process.
    unit mluva -p KillMode=process "${env_args[@]}" --setenv=PATH="$M/tools:/usr/bin" --setenv=GSETTINGS_BACKEND=memory \
      "$M/bundle/bin/mluva"
    for _ in $(seq 150); do
      gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux \
        --method org.gtk.Actions.List 2>/dev/null | grep -q annotation-pin && break
      sleep 0.2
    done; echo started ;;
  probe) unit probe python3 "$M/probe.py" "$R/probe.jsonl" ;;
  settings) unit settings python3 "$M/offscreen/synthetic_settings.py" "$R/banner" ;;
  notes) unit notes foot --app-id synthetic.notes --font monospace:size=14 sh -c \
      'printf "Synthetic release notes\n\n- Fix the overflowing sync label\n- Move Save inside the card\n"; exec sleep infinity' ;;
  agent-terminal) unit agentterm foot --app-id stub-agent --font monospace:size=11 \
      python3 "$M/offscreen/stub_agent.py" "$R/herdr-wire.jsonl" ;;
  action) action "$2" ;;
  quit) action quit; sleep 2; systemctl --user stop mluva 2>/dev/null || true ;;
  kill) systemctl --user kill -s KILL mluva ;;
  pids) echo "main $(systemctl --user show -p MainPID --value mluva)"
    pgrep -f -- "--annotation-cleanu[p]" | sed 's/^/watchdog /' || true ;;
  binds) hyprctl -j binds | jq -c '[.[] | select(.description | startswith("Mluva")) |
      {description, key, modmask, mouse, non_consuming, release, dispatcher, arg}]' ;;
  stop-all) for u in mluva probe settings notes agentterm herdr speech; do systemctl --user stop "$u" 2>/dev/null || true; done ;;
  *) echo "unknown: $1" >&2; exit 2 ;;
esac
