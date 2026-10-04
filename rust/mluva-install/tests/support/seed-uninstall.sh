#!/usr/bin/env bash
# Common synthetic inputs for independent released/native process observations.
set -euo pipefail
case_root=$1
case_name=$2
asset_root=$3
[[ "$case_root" == "$OFFSCREEN_SESSION_ROOT/"* ]]
[[ "$(readlink /proc/self/ns/net)" != "$MLUVA_HOST_NET_NS" ]]
target_home="$case_root/home"
[[ "$case_name" != unicode ]] || target_home="$case_root/Žluťoučký home"
data_root="$target_home/.local/share"
config_root="$target_home/.config"
live_home="$case_root/live-home"
case "$case_name" in live-*|xdg-*) live_home="$target_home" ;; esac
if [[ "$case_name" == xdg-custom ]]; then data_root="$case_root/custom-data"; config_root="$case_root/custom-config"; fi
app_dir="$data_root/mluva/app"
bin_dir="$target_home/.local/bin"
extension_dir="$data_root/gnome-shell/extensions/recording-status@mluva.local"
desktop_entry="$data_root/applications/com.mluva.Linux.desktop"
icon="$data_root/icons/hicolor/scalable/apps/com.mluva.Linux.svg"
autostart="$config_root/autostart/com.mluva.Linux.desktop"
mkdir -p "$app_dir" "$bin_dir" "$config_root/mluva" "$config_root/autostart" \
    "$data_root/mluva/recordings" "$data_root/mluva/screenshots" "$data_root/mluva/models" "$data_root/mluva/tensaku" \
    "$data_root/mluva-migration-backups/upgrade-owned" "${extension_dir%/*}" "${desktop_entry%/*}" "${icon%/*}" \
    "$case_root/units" "$case_root/peer" "$case_root/live-home" "$case_root/foreign"
printf 'name = "mluva-linux"\n' > "$app_dir/pyproject.toml"
printf 'application payload\n' > "$app_dir/payload"
printf '{"language_code":"ces"}\n' > "$config_root/mluva/config.json"
for path in history.sqlite3 scratchpad-draft.json recordings/audio.wav screenshots/image.png models/model.onnx tensaku/preferences; do
    printf 'opaque retained user bytes\000\377\n' > "$data_root/mluva/$path"
done
printf 'retained backup\n' > "$data_root/mluva-migration-backups/upgrade-owned/state"
printf 'unrelated executable\n' > "$bin_dir/unrelated"
printf '#!/bin/sh\napplication_dir="%s"\nexec python -m mluva_linux.app "$@"\n' "$app_dir" > "$bin_dir/mluva"
chmod 0755 "$bin_dir/mluva"
for pair in 'mluva-input-helper:configure-input-helper.sh' 'mluva-overlay:configure-recording-overlay.sh' \
    'mluva-uninstall:uninstall.sh' 'mluva-shell:mluva-shell' 'mluva-narrate:mluva-narrate' \
    'mluva-screenshot-editor:mluva-screenshot-editor' 'tensaku-edit:mluva-screenshot-editor'; do
    ln -s "$app_dir/${pair#*:}" "$bin_dir/${pair%%:*}"
done
printf '[Desktop Entry]\nName=Mluva\nExec=%s/mluva\n' "$bin_dir" > "$desktop_entry"
printf '[Desktop Entry]\nName=Mluva\nExec=mluva\nX-GNOME-Autostart-enabled=false\n' > "$autostart"
printf '<svg><title id="title">Mluva</title></svg>\n' > "$icon"
cp -a "$asset_root/gnome-extension/recording-status@mluva.local" "$extension_dir"
printf 'foreign sentinel\n' > "$case_root/foreign/sentinel"
case "$case_name" in
    app-foreign) printf 'name = "other"\n' > "$app_dir/pyproject.toml" ;;
    app-absent) rm -r "$app_dir" ;;
    app-file) rm -r "$app_dir"; printf 'foreign application path\n' > "$app_dir" ;;
    app-link) mv "$app_dir" "$case_root/foreign/app"; ln -s "$case_root/foreign/app" "$app_dir" ;;
    parent-link) mv "$data_root/mluva" "$case_root/foreign/mluva"; ln -s "$case_root/foreign/mluva" "$data_root/mluva" ;;
    launcher-foreign) printf 'foreign launcher\n' > "$bin_dir/mluva" ;;
    launcher-link) rm "$bin_dir/mluva"; ln -s "$case_root/foreign/sentinel" "$bin_dir/mluva" ;;
    helper-foreign) rm "$bin_dir/mluva-shell"; printf 'foreign helper\n' > "$bin_dir/mluva-shell" ;;
    helper-link) rm "$bin_dir/mluva-shell"; ln -s "$case_root/foreign/sentinel" "$bin_dir/mluva-shell" ;;
    desktop-foreign) printf '[Desktop Entry]\nName=Other\nExec=mluva\n' > "$desktop_entry" ;;
    desktop-link) rm "$desktop_entry"; ln -s "$case_root/foreign/sentinel" "$desktop_entry" ;;
    autostart-foreign) printf '[Desktop Entry]\nName=Mluva\nExec=other\n' > "$autostart" ;;
    icon-foreign) printf '<svg>other</svg>\n' > "$icon" ;;
    icon-link) mv "$icon" "$case_root/foreign/icon"; ln -s "$case_root/foreign/icon" "$icon" ;;
    extension-changed) printf 'local change\n' >> "$extension_dir/extension.js" ;;
    extension-extra) printf 'local file\n' > "$extension_dir/extra" ;;
    extension-missing) rm "$extension_dir/stylesheet.css" ;;
    extension-link) rm "$extension_dir/extension.js"; ln -s "$case_root/foreign/sentinel" "$extension_dir/extension.js" ;;
    extension-directory) mv "$extension_dir" "$case_root/foreign/extension"; ln -s "$case_root/foreign/extension" "$extension_dir" ;;
esac
case "$case_name" in
    live-input|live-input-failure) cp "$asset_root/resources/mluva-input@.service" "$case_root/units/mluva-input@.service" ;;
    live-input-modified) printf 'foreign unit\n' > "$case_root/units/mluva-input@.service" ;;
esac
install_home="$target_home"
case "$case_name" in home-root) install_home=/ ;; home-relative) install_home=relative ;; esac
env_data="$data_root"
env_config="$config_root"
case "$case_name" in xdg-root) env_data=/ ;; xdg-relative) env_config=relative ;; staged-ignore-xdg) env_data=/; env_config=relative ;; esac
jq -n --arg home "$target_home" --arg live "$live_home" --arg install "$install_home" --arg data "$env_data" --arg config "$env_config" \
    '{home:$home,live:$live,install:$install,data:$data,config:$config}' > "$case_root/layout.json"
