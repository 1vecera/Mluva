#!/usr/bin/env bash
# Take A, F9 into a field, run by dev/verify.sh session inside the private desktop.
#
#   dev/verify.sh session EVIDENCE -- bash launch-video/launch/capture/field_take.sh BINARIES
#
# Real Mluva (record key F9 through the fixture shortcut portal, paced PCM fixture as microphone,
# a loopback speech peer with a scripted sentence), the production Omarchy widget in Quickshell
# with its real bridge, and Firefox showing an invented notes page whose reply field has focus.
# F9 starts, the widget grows the live words chunk by chunk, F9 stops, and Mluva delivers the
# final text into the field through its own captured-target path. The whole private screen is
# recorded losslessly in real time.
set -euo pipefail
: "${MLUVA_VERIFY_CLI:?run through dev/verify.sh session}" "${OFFSCREEN_SESSION_ROOT:?private desktop only}"
here=$(cd "$(dirname "$0")" && pwd)
checkout=$(cd "$here/../../.." && pwd)
binaries=$1
cli=$MLUVA_VERIFY_CLI root=$OFFSCREEN_SESSION_ROOT ev=$OFFSCREEN_ARTIFACT_DIR
t0=$(date +%s.%N)
mark() { printf '{"t":%s,"event":"%s"}\n' "$(awk -v a="$(date +%s.%N)" -v b="$t0" 'BEGIN{printf "%.3f", a-b}')" "$1" >> "$ev/timeline.jsonl"; }
"$cli" action quit > /dev/null

# No window decorations: the take shows the applications' own pixels only. New windows do not take
# focus, as on Hyprland, where Omarchy's widget is a layer-shell surface that never takes the keyboard;
# Firefox is activated explicitly below.
python3 - "$root/openbox.xml" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1]); s = p.read_text()
s = s.replace("</openbox_config>", '<applications><application class="*"><decor>no</decor><focus>no</focus></application></applications></openbox_config>')
p.write_text(s)
PY
openbox --reconfigure || true

python3 "$here/speech_field.py" 18767 "$ev/speech-requests.jsonl" > "$root/speech.log" 2>&1 &
# Quiet synthetic PCM at real-time pace until Stop, so 3 s preview chunks arrive as speech would.
python3 - "$root/tools/test-config.json" <<'PY'
import json, sys
json.dump({"pcm_hex": "01000200" * 800, "fragments": [3200], "bytes_per_second": 32000,
           "repeat": True, "interval_ms": 0, "wait": True}, open(sys.argv[1], "w"))
PY
python3 - <<'PY'
import json, os
cfg = os.path.join(os.environ["XDG_CONFIG_HOME"], "mluva/config.json")
data = json.load(open(cfg))
data.update(transcription_base_url="http://127.0.0.1:18767/v1", transcription_chunk_seconds=3,
            widget_position="bottom-center", widget_lines=3, global_recording_key="F9")
json.dump(data, open(cfg, "w"))
PY
# Mluva and the widget in Omarchy's Vantablack theme.
mkdir -p "$XDG_STATE_HOME/omarchy/current/theme" "$HOME/.config/omarchy/current"
cp /usr/share/omarchy/themes/vantablack/colors.toml "$XDG_STATE_HOME/omarchy/current/theme/colors.toml"
ln -sfn "$XDG_STATE_HOME/omarchy/current/theme" "$HOME/.config/omarchy/current/theme"

# The accessibility bus's session-bus front, so Firefox exposes its fields and Mluva tracks them.
python3 "$here/a11y_status.py" > "$root/a11y-status.log" 2>&1 &
for _ in {1..100}; do rg -q a11y-status-ready "$root/a11y-status.log" && break; sleep 0.05; done
"$binaries/global-shortcut-fixture-peer" > "$root/portal.log" 2>&1 &
for _ in {1..100}; do rg -q portal-ready "$root/portal.log" && break; sleep 0.05; done
portal() { gdbus call --session --dest org.freedesktop.portal.Desktop --object-path /org/freedesktop/portal/desktop \
    --method com.mluva.TestPortal.Drive "\"{\\\"op\\\":\\\"$1\\\",\\\"id\\\":\\\"$2\\\"}\"" > /dev/null; }
"$cli" launch --global-shortcuts > /dev/null

# The production widget and bridge, as Omarchy's shell loads them.
widget=$root/widget
mkdir -p "$widget"
cp -r "$checkout/linux/quickshell/mluva.dictation" "$widget/Mluva"
cp -r /usr/share/omarchy/shell/Commons "$widget/Commons"
cp -r /usr/share/omarchy/shell/Ui "$widget/Ui"
cat > "$widget/shell.qml" <<'QML'
import QtQuick
import Quickshell
import "Mluva" as Mluva
ShellRoot {
    Mluva.Widget { settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")}) }
}
QML
MLUVA_SHELL_COMMAND="$binaries/mluva-shell" QT_SCALE_FACTOR="${GDK_SCALE:-1}" QT_QPA_PLATFORM=xcb \
    quickshell --no-color -p "$widget/shell.qml" > "$root/widget.log" 2>&1 &

# The target: an invented notes page in Firefox, browser chrome hidden, its reply field focused.
profile=$root/firefox
mkdir -p "$profile/chrome"
cat > "$profile/user.js" <<'JS'
user_pref("accessibility.force_disabled", 0);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.newtabpage.enabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("network.captive-portal-service.enabled", false);
user_pref("network.connectivity-service.enabled", false);
user_pref("network.trr.mode", 5);
user_pref("browser.tabs.inTitlebar", 0);
user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);
user_pref("ui.systemUsesDarkTheme", 1);
user_pref("layout.css.devPixelsPerPx", "2.0");
user_pref("widget.use-xdg-desktop-portal.settings", 0);
user_pref("widget.use-xdg-desktop-portal.file-picker", 0);
user_pref("widget.use-xdg-desktop-portal.mime-handler", 0);
user_pref("widget.use-xdg-desktop-portal.open-uri", 0);
JS
printf '%s\n' '#navigator-toolbox, #titlebar, #TabsToolbar, #nav-bar { visibility: collapse !important; }' > "$profile/chrome/userChrome.css"
cp "$here/fixtures/notes.html" "$root/notes.html"
firefox --no-remote --new-instance --profile "$profile" "file://$root/notes.html" > "$root/firefox.log" 2>&1 &

read -r W H < <(xdotool getdisplaygeometry)
for _ in {1..200}; do xdotool search --onlyvisible --class firefox > /dev/null 2>&1 && break; sleep 0.1; done
sleep 4
ff=$(xdotool search --onlyvisible --class firefox | head -1)
# Omarchy's 10 px gaps at scale 2.
xdotool windowmove "$ff" 20 20 windowsize "$ff" $((W - 40)) $((H - 40))
xdotool windowactivate --sync "$ff"
sleep 2
# A person clicks into the reply field: first the page, then the field, so the field's focus
# event reaches Mluva's target tracker after Firefox's accessibility tree is live.
xdotool mousemove --sync $((W / 2)) $((H * 14 / 100)) click 1
sleep 0.6
xdotool mousemove --sync $((W / 2)) $((H * 527 / 1000)) click 1
sleep 0.6
xdotool mousemove 0 0
sleep 1
for _ in {1..100}; do pgrep -f "quickshell.*shell.qml" > /dev/null && break; sleep 0.1; done
"$cli" screenshot before > /dev/null || true

record_id=toggle-recording-f9
# Lossless and light enough to keep up at 4K60 on a loaded machine; transcoded to FFV1 afterwards.
ffmpeg -nostdin -v error -progress "$ev/ffmpeg-progress.txt" -f x11grab -thread_queue_size 256 -draw_mouse 0 -framerate 60 \
  -video_size "${W}x${H}" -i "$DISPLAY+0,0" -c:v ffvhuff -threads 8 "$ev/field-capture.mkv" &
ffmpeg_pid=$!
t0=$(date +%s.%N)
mark recording-started
sleep "${MLUVA_TAKE_LEAD:-2.5}"
portal Activated "$record_id"; mark f9-start; sleep 0.15; portal Deactivated "$record_id"
sleep "${MLUVA_TAKE_SPEAK:-9.4}"
portal Activated "$record_id"; mark f9-stop; sleep 0.15; portal Deactivated "$record_id"
last=""
for _ in {1..200}; do
    outcome=$(python3 -c 'import sqlite3, sys; r = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True).execute("SELECT delivery_outcome FROM transcription_history ORDER BY created_at DESC LIMIT 1").fetchone(); print(r[0] if r else "")' \
        "$XDG_DATA_HOME/mluva/history.sqlite3" 2> /dev/null || true)
    [[ "$outcome" != "$last" && -n "$outcome" ]] && { mark "delivery-$outcome"; last=$outcome; }
    [[ -n "$outcome" ]] && break
    sleep 0.1
done
sleep "${MLUVA_TAKE_TAIL:-3.5}"
mark tail-end
kill -INT "$ffmpeg_pid"; wait "$ffmpeg_pid" || true
"$cli" state > "$ev/final-state.json" 2>&1 || true
"$cli" screenshot after > /dev/null || true
xclip -selection clipboard -o > "$ev/clipboard.txt" 2> /dev/null || true
