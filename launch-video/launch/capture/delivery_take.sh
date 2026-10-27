#!/usr/bin/env bash
# Take C, delivered: run by dev/verify.sh session inside the private desktop (from the universal
# showreel's delivery take).
#
#   dev/verify.sh session EVIDENCE -- bash launch-video/launch/capture/delivery_take.sh BINARIES
#
# Real Mluva (agent key F12 through the fixture portal), a disposable Codex TUI and Claude Code in
# tmux in Ghostty, loopback models and speech. Records the whole private screen losslessly.
set -euo pipefail
: "${MLUVA_VERIFY_CLI:?run through dev/verify.sh session}" "${OFFSCREEN_SESSION_ROOT:?private desktop only}"
here=$(cd "$(dirname "$0")" && pwd)
binaries=$1
cli=$MLUVA_VERIFY_CLI root=$OFFSCREEN_SESSION_ROOT ev=$OFFSCREEN_ARTIFACT_DIR
mluva="$root/bundle/bin/mluva"
t0=$(date +%s.%N)
mark() { printf '{"t":%s,"event":"%s"}\n' "$(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN{printf "%.3f", a-b}')" "$1" >> "$ev/timeline.jsonl"; }
"$cli" action quit > /dev/null

# No window decorations: the take shows the applications' own pixels only.
python3 - "$root/openbox.xml" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1]); s = p.read_text()
s = s.replace("</openbox_config>", '<applications><application class="*"><decor>no</decor></application></applications></openbox_config>')
p.write_text(s)
PY
openbox --reconfigure || true

python3 "$here/models.py" 18765 "$ev/model-requests.jsonl" > "$root/models.log" 2>&1 &
python3 "$here/speech.py" 18767 "$ev/speech-requests.jsonl" > "$root/speech.log" 2>&1 &

mkdir -p "$HOME/.codex" "$HOME/release-notes"
cat > "$HOME/.codex/config.toml" <<TOML
model = "loopback-model"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "read-only"
[model_providers.fixture]
name = "Local loopback model"
base_url = "http://127.0.0.1:18765/v1"
wire_api = "responses"
[features]
responses_websockets = false
responses_websockets_v2 = false
remote_models = false
[projects."$HOME/release-notes"]
trust_level = "trusted"
TOML
python3 - <<PY
import json, os
home = os.environ["HOME"]
json.dump({"hasCompletedOnboarding": True, "theme": "dark", "projects": {f"{home}/release-notes": {"hasTrustDialogAccepted": True, "hasCompletedProjectOnboarding": True}}, "customApiKeyResponses": {"approved": ["fixture-key"], "rejected": []}}, open(f"{home}/.claude.json", "w"))
cfg = os.path.join(os.environ["XDG_CONFIG_HOME"], "mluva/config.json")
data = json.load(open(cfg))
data["transcription_base_url"] = "http://127.0.0.1:18767/v1"
json.dump(data, open(cfg, "w"))
PY
printf '%s\n' '{"enabled":true,"hold_key":"F12"}' > "$XDG_CONFIG_HOME/mluva/agent-voice.json"

"$binaries/global-shortcut-fixture-peer" > "$root/portal.log" 2>&1 &
for _ in {1..100}; do rg -q portal-ready "$root/portal.log" && break; sleep 0.05; done
portal() { gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop \
    --method com.mluva.TestPortal.Drive "\"{\\\"op\\\":\\\"$1\\\",\\\"id\\\":\\\"agent-hold\\\"}\"" > /dev/null; }
# Mluva and the agents share the private HOME, runtime directory and these tools (Mluva runs its own
# `codex app-server` helper for Codex threads).
export PATH="$here/tools/codex-pkg/bin:$here/tools:$PATH" ANTHROPIC_BASE_URL=http://127.0.0.1:18765 ANTHROPIC_API_KEY=fixture-key DISABLE_AUTOUPDATER=1 SHELL=/bin/bash
# Mluva in Omarchy's Vantablack theme, the reel's monochrome.
mkdir -p "$XDG_STATE_HOME/omarchy/current/theme"
cp /usr/share/omarchy/themes/vantablack/colors.toml "$XDG_STATE_HOME/omarchy/current/theme/colors.toml"
"$cli" launch --global-shortcuts > /dev/null

# The disposable agents: Codex on top, Claude Code below, both on loopback models.
cd "$HOME/release-notes"
tmux -L reel -f "$here/tmux.conf" new-session -d -s work -x 107 -y 52 \
  "codex --no-alt-screen 'Read CHANGELOG.md and wait for my next request.' 2> '$root/codex.err'; echo exit \$? >> '$root/codex.err'; sleep 900"
sleep 5
# Name the disposable thread through Codex's own command, as a person would.
tmux -L reel send-keys -t work.0 -l '/rename Release notes'; sleep 0.4; tmux -L reel send-keys -t work.0 Enter
sleep 1
tmux -L reel split-window -v -l 46% -t work "claude -n 'Portal test' --model loopback-model"
tmux -L reel set-option -p -t work.0 @label "codex"
tmux -L reel set-option -p -t work.1 @label "claude code"
ghostty --config-default-files=false --font-family="JetBrainsMono Nerd Font" --font-size=12 \
  --background=#0b0b0b --foreground=#e8e8e8 --window-decoration=false --window-padding-x=14 --window-padding-y=10 \
  --confirm-close-surface=false --gtk-single-instance=false \
  -e tmux -L reel attach -t work > "$root/ghostty.log" 2>&1 &
sleep 6

read -r W H < <(xdotool getdisplaygeometry)
place() { local id; id=$(xdotool search --onlyvisible --class "$1" | head -1); xdotool windowmove "$id" "$2" 0 windowsize "$id" $((W / 2)) "$H"; }
place ghostty $((W / 2))
place mluva 0 || place Mluva 0
sleep 1

for _ in {1..60}; do
    agents=$("$mluva" agents list)
    codex_id=$(printf '%s' "$agents" | jq -r '[.recipients[] | select(.route=="codex_queue")][0].session')
    claude_id=$(printf '%s' "$agents" | jq -r '[.recipients[] | select(.route=="claude_inbox")][0].session')
    [[ "$codex_id" != null && "$claude_id" != null ]] && break
    sleep 0.5
done
printf '%s\n' "$agents" > "$ev/agents-list.json"
[[ -n "$codex_id" && "$codex_id" != null && -n "$claude_id" && "$claude_id" != null ]] || { echo "agents not discovered" >&2; exit 4; }
for _ in {1..100}; do "$mluva" agents status | jq -e '.agent_shortcut.ready' > /dev/null && break; sleep 0.1; done

# Lossless and light enough to keep up at 4K60 on a loaded machine; transcoded to FFV1 afterwards.
ffmpeg -nostdin -v error -progress "$ev/ffmpeg-progress.txt" -f x11grab -thread_queue_size 256 -draw_mouse 0 -framerate 60 \
  -video_size "${W}x${H}" -i "$DISPLAY+0,0" -c:v ffvhuff -threads 8 "$ev/delivery-capture.mkv" &
ffmpeg_pid=$!
t0=$(date +%s.%N)
sleep 1.5

journey() {
    local id=$1 name=$2 until=$3
    "$mluva" agents select "$id" > /dev/null; mark "$name-selected"
    sleep 0.8
    rm -f "$root/tools/raw.ready.json"
    portal Activated; mark "$name-f12-open"; sleep 1.2; portal Deactivated
    portal Activated; mark "$name-f12-record"
    for _ in {1..100}; do [[ -f "$root/tools/raw.ready.json" ]] && break; sleep 0.05; done
    mark "$name-capturing"; sleep 0.3; portal Deactivated
    sleep 2.4
    portal Activated; mark "$name-f12-stop"; sleep 0.2; portal Deactivated
    local last="" state
    for _ in {1..300}; do
        state=$("$mluva" agents status | jq -r '.delivery')
        [[ "$state" != "$last" ]] && { mark "$name-status-$state"; last=$state; }
        [[ "$state" == "$until" ]] && break
        sleep 0.05
    done
    sleep 4
    mark "$name-done"
}
journey "$codex_id" codex sent
# Claude Code is busy when the dictation arrives: a request typed into it first, which the loopback
# model answers over ten seconds, so Claude Code queues the dictation and takes it after that turn.
tmux -L reel send-keys -t work.1 -l 'Run the portal tests and tell me what fails.'; sleep 0.3
tmux -L reel send-keys -t work.1 Enter; mark claude-busy-request
sleep 0.8
journey "$claude_id" claude delivered
kill -INT "$ffmpeg_pid"; wait "$ffmpeg_pid" || true
"$cli" screenshot delivery-end > /dev/null || true
tmux -L reel capture-pane -p -t work.0 > "$ev/codex-pane.txt" || true
tmux -L reel capture-pane -p -t work.1 > "$ev/claude-pane.txt" || true
tmux -L reel kill-server || true
