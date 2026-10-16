#!/usr/bin/env bash
set -euo pipefail
test -n "$OFFSCREEN_SESSION_ROOT"
test -z "${WAYLAND_DISPLAY:-}"
test ! -e /dev/snd
task_root="$PWD"
app="$task_root/tmp/staged-home/.local/bin/mluva"
test "$(readlink -f "$app")" = "$task_root/tmp/staged-home/.local/share/mluva/app/bin/mluva"
export MLUVA_DISABLE_GLOBAL_SHORTCUT=1 MLUVA_SECRET_PROFILE=1
mkdir -p "$XDG_CONFIG_HOME/mluva" "$XDG_DATA_HOME/applications"
printf '%s\n' '{"welcome_completed":true,"transcription_provider":"local","rewrite_provider":"none"}' >"$XDG_CONFIG_HOME/mluva/config.json"
cp "$task_root/tmp/staged-home/.local/share/applications/com.mluva.Linux.desktop" "$XDG_DATA_HOME/applications/"
update-desktop-database "$XDG_DATA_HOME/applications"
xdg-mime default com.mluva.Linux.desktop x-scheme-handler/mluva
test "$(xdg-mime query default x-scheme-handler/mluva)" = com.mluva.Linux.desktop
cd "$OFFSCREEN_SESSION_ROOT"
"$app" >"$OFFSCREEN_ARTIFACT_DIR/app.log" 2>&1 &
app_pid=$!
trap 'kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true' EXIT
ready=0
for _attempt in {1..150}; do
    kill -0 "$app_pid"
    if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner com.mluva.Linux | rg -q true; then
        if gdbus introspect --session --dest com.mluva.Linux --object-path /com/mluva/Linux/window/1 | rg -q org.gtk.Actions; then ready=1; break; fi
    fi
    sleep 0.05
done
test "$ready" = 1
test "$(readlink "/proc/$app_pid/exe")" = "$(readlink -f "$app")"
gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.List >"$OFFSCREEN_ARTIFACT_DIR/actions.txt"
rg -q "'account'" "$OFFSCREEN_ARTIFACT_DIR/actions.txt"
gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate account '[]' '{}' >/dev/null
gio open 'mluva://account?state=stale&code=stale'
"$app" 'mluva://account?state=stale&code=stale'
gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate quit '[]' '{}' >/dev/null
wait "$app_pid"
trap - EXIT
if rg -i 'critical|panic|could not start|dbind-warning' "$OFFSCREEN_ARTIFACT_DIR/app.log"; then exit 1; fi
printf 'PASS staged binary launched outside checkout, owned private bus, exported account action, accepted registered URI handler and quit cleanly.\n'
