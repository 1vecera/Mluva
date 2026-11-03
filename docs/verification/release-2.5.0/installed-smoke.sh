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
cp "$task_root/tmp/staged-home/.local/share/applications/com.mluva.Linux.desktop" "$XDG_DATA_HOME/applications/"
update-desktop-database "$XDG_DATA_HOME/applications"
xdg-mime default com.mluva.Linux.desktop x-scheme-handler/mluva
test "$(xdg-mime query default x-scheme-handler/mluva)" = com.mluva.Linux.desktop
cd "$OFFSCREEN_SESSION_ROOT"

# Start the installed app on one saved settings file, check what it owns, then quit it normally.
# `uri` also exercises the registered mluva:// handler and a stale sign-in callback.
run_scenario() {
    local label=$1 config=$2 uri=${3:-no}
    local settings="$XDG_CONFIG_HOME/mluva/config.json"
    printf '%s\n' "$config" >"$settings"
    cp "$settings" "$OFFSCREEN_ARTIFACT_DIR/config-$label.json"
    "$app" >"$OFFSCREEN_ARTIFACT_DIR/app-$label.log" 2>&1 &
    local app_pid=$!
    trap 'kill "$app_pid" 2>/dev/null || true; wait "$app_pid" 2>/dev/null || true' EXIT
    local ready=0
    for _attempt in {1..150}; do
        kill -0 "$app_pid"
        if gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner com.mluva.Linux | rg -q true; then
            if gdbus introspect --session --dest com.mluva.Linux --object-path /com/mluva/Linux/window/1 | rg -q org.gtk.Actions; then ready=1; break; fi
        fi
        sleep 0.05
    done
    test "$ready" = 1
    test "$(readlink "/proc/$app_pid/exe")" = "$(readlink -f "$app")"
    gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.List >"$OFFSCREEN_ARTIFACT_DIR/actions-$label.txt"
    rg -q "'account'" "$OFFSCREEN_ARTIFACT_DIR/actions-$label.txt"
    gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate account '[]' '{}' >/dev/null
    if [ "$uri" = uri ]; then
        gio open 'mluva://account?state=stale&code=stale'
        "$app" 'mluva://account?state=stale&code=stale'
    fi
    # Startup never rewrites the saved settings, whichever provider they name.
    cmp "$settings" "$OFFSCREEN_ARTIFACT_DIR/config-$label.json"
    gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.Activate quit '[]' '{}' >/dev/null
    wait "$app_pid"
    trap - EXIT
    if rg -i 'critical|panic|could not start|dbind-warning' "$OFFSCREEN_ARTIFACT_DIR/app-$label.log"; then exit 1; fi
    printf 'PASS %s: launched, owned the private bus, exported the account action, left settings untouched and quit cleanly.\n' "$label"
}

# The 2.4.0 scenario, unchanged: a local model.
run_scenario local '{"welcome_completed":true,"transcription_provider":"local","rewrite_provider":"none"}' uri
# An existing 2.4.0 installation: its saved file has none of the new settings.
run_scenario upgraded-elevenlabs '{"welcome_completed":true,"transcription_provider":"elevenlabs"}'
# The new routes, with their key and region unset: readiness is reported, the app stays up.
run_scenario microsoft-batch '{"welcome_completed":true,"transcription_provider":"microsoft","transcription_mode":"batch","microsoft_region":"swedencentral"}'
run_scenario xai-realtime '{"welcome_completed":true,"transcription_provider":"xai","transcription_mode":"realtime"}'
run_scenario mistral-batch '{"welcome_completed":true,"transcription_provider":"mistral","transcription_mode":"batch"}'
printf 'PASS staged binary launched outside checkout, owned a private bus, exported the account action, accepted the registered URI handler and started cleanly with local, upgraded ElevenLabs, Microsoft, xAI and Mistral settings.\n'
