#!/usr/bin/env bash
# Shared input construction, never the migration implementation or expected result.
set -euo pipefail
case_root=$1
case_name=$2
asset_root=$3
support=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
seed=owned
[[ "$case_name" != legacy-unicode ]] || seed=unicode
[[ "$case_name" != live-xdg ]] || seed=xdg-custom
bash "$support/seed-install.sh" "$case_root" "$seed" "$asset_root"
target_home=$(jq -r .home "$case_root/layout.json")
data_root="$target_home/.local/share"
config_root="$target_home/.config"
[[ "$case_name" != live-xdg ]] || { data_root="$case_root/custom-data"; config_root="$case_root/custom-config"; }
old_data="$data_root/voice-scribe"
old_config="$config_root/voice-scribe"
mv "$data_root/mluva" "$old_data"
mv "$config_root/mluva" "$old_config"
printf 'name = "voice-scribe-linux"\n' > "$old_data/app/pyproject.toml"
printf '{"language_code":"ces","transcription_provider":"voxtype","voxtype_model":"base","unicode":"🧪"}\n' > "$old_config/config.json"
printf '{"custom_styles":[],"dictionary":[]}\n' > "$old_config/personalization.json"
rm "$old_data/history.sqlite3"
sqlite3 "$old_data/history.sqlite3" < "$support/legacy-history.sql"
sql_data=${old_data//\'/\'\'}
sqlite3 "$old_data/history.sqlite3" <<SQL
INSERT INTO transcription_history (identifier,created_at,raw_text,delivered_text,mode,language_code,delivery_outcome,retained_audio_path,title,title_revision)
VALUES ('existing','2026-09-10','VoiceScribe mentioned verbatim','Žluťoučký 🧪','scratchpad','ces','copied','$sql_data/recordings/audio.wav','Human title',4),
('external','2026-09-11','voice-scribe stays in text','Edited external','dictation','eng','pasted','/external/voice-scribe/audio.wav','External',2),
('null','2026-09-12','No audio','No audio','dictation','eng','copied',NULL,NULL,0);
SQL
jq -n --arg audio "$old_data/recordings/audio.wav" '{identifier:"draft",history_identifier:"existing",created_at:"2026-09-10",raw_text:"VoiceScribe mentioned verbatim",text:"Žluťoučký 🧪",audio_path:$audio}' > "$old_data/scratchpad-draft.json"
bin_dir="$target_home/.local/bin"
printf '#!/bin/sh\napplication_dir="%s/app"\nexec python -m voice_scribe_linux.app "$@"\n' "$old_data" > "$bin_dir/mluva"
for pair in 'mluva-input-helper:configure-input-helper.sh' 'mluva-overlay:configure-recording-overlay.sh' 'mluva-uninstall:uninstall.sh' 'mluva-shell:mluva-shell'; do
    ln -sfn "$old_data/app/${pair#*:}" "$bin_dir/${pair%%:*}"
done
ln -s mluva "$bin_dir/voice-scribe"
ln -s mluva-input-helper "$bin_dir/voice-scribe-input-helper"
ln -s mluva-overlay "$bin_dir/voice-scribe-overlay"
mv "$data_root/applications/com.mluva.Linux.desktop" "$data_root/applications/com.voicescribe.Linux.desktop"
mv "$data_root/icons/hicolor/scalable/apps/com.mluva.Linux.svg" "$data_root/icons/hicolor/scalable/apps/com.voicescribe.Linux.svg"
printf '<svg><title id="title">Voice Scribe</title></svg>\n' > "$data_root/icons/hicolor/scalable/apps/com.voicescribe.Linux.svg"
mv "$config_root/autostart/com.mluva.Linux.desktop" "$config_root/autostart/com.voicescribe.Linux.desktop"
printf '[Desktop Entry]\nName=Voice Scribe\nExec=voice-scribe\nX-GNOME-Autostart-enabled=false\n' > "$config_root/autostart/com.voicescribe.Linux.desktop"
extensions="$data_root/gnome-shell/extensions"
rm -r "$extensions/recording-status@mluva.local"
for name in recording-status@voicescribe.local right-alt@voicescribe.local; do
    mkdir "$extensions/$name"
    printf '{"uuid":"%s"}\n' "$name" > "$extensions/$name/metadata.json"
    printf 'retired extension\n' > "$extensions/$name/extension.js"
done
managed="$config_root/daniel-ai-skills"
[[ "$case_name" != live-custom-profile ]] || managed="$config_root/other-managed"
mkdir -p "$managed/env" "$config_root/hypr" "$config_root/omarchy/plugins"
printf 'ELEVENLABS_API_KEY=op://synthetic/elevenlabs/credential\n' > "$managed/env/voice-scribe.env"
chmod 0640 "$managed/env/voice-scribe.env"
cat > "$config_root/hypr/bindings.conf" <<'BINDINGS'
# voice-scribe is archival
bind = SUPER, D, exec, voice-scribe
bindl = , F10, exec, /tool/voice-scribe-overlay status
bind = , F11, exec, my-voice-scribe-test
$label = voice-scribe
BINDINGS
ln -s "$old_data/app/quickshell/mluva.dictation" "$config_root/omarchy/plugins/mluva.dictation"
case "$case_name" in
    conflict-data) mkdir "$data_root/mluva" ;;
    conflict-config) mkdir "$config_root/mluva" ;;
    foreign-command) printf 'unrelated command\n' > "$bin_dir/mluva" ;;
    state-link) mv "$old_config" "$case_root/foreign/config"; ln -s "$case_root/foreign/config" "$old_config" ;;
    profile-invalid) printf 'ELEVENLABS_API_KEY=synthetic-do-not-print\n' > "$managed/env/voice-scribe.env" ;;
    profile-conflict) printf 'ELEVENLABS_API_KEY=op://synthetic/other/credential\n' > "$managed/env/mluva.env" ;;
    extension-wrong) printf '{"uuid":"foreign"}\n' > "$extensions/recording-status@voicescribe.local/metadata.json" ;;
    extension-link) mv "$extensions/recording-status@voicescribe.local" "$case_root/foreign/extension"; ln -s "$case_root/foreign/extension" "$extensions/recording-status@voicescribe.local" ;;
    overlay-conflict) mkdir "$extensions/recording-status@mluva.local" ;;
    autostart-conflict) cp "$config_root/autostart/com.voicescribe.Linux.desktop" "$config_root/autostart/com.mluva.Linux.desktop" ;;
    autostart-custom) printf '[Desktop Entry]\nExec=custom-app\n' > "$config_root/autostart/com.voicescribe.Linux.desktop" ;;
    plugin-foreign) ln -sfn "$case_root/foreign" "$config_root/omarchy/plugins/mluva.dictation" ;;
    backup-link) mv "$data_root/mluva-migration-backups" "$case_root/foreign/backups"; ln -s "$case_root/foreign/backups" "$data_root/mluva-migration-backups" ;;
    database-corrupt) printf 'invalid database\n' > "$old_data/history.sqlite3" ;;
    draft-corrupt) printf '{synthetic-do-not-print\n' > "$old_data/scratchpad-draft.json" ;;
    database-link) mv "$old_data/history.sqlite3" "$case_root/foreign/history"; ln -s "$case_root/foreign/history" "$old_data/history.sqlite3" ;;
    draft-link) mv "$old_data/scratchpad-draft.json" "$case_root/foreign/draft"; ln -s "$case_root/foreign/draft" "$old_data/scratchpad-draft.json" ;;
esac
for name in gsettings systemctl sudo; do install -m 0755 "$support/legacy-peer.sh" "$case_root/fake-bin/$name"; done
cat > "$case_root/peer/settings.json" <<'JSON'
{"org.gnome.shell":{"favorite-apps":"['other.desktop', 'com.voicescribe.Linux.desktop']","enabled-extensions":"['right-alt@voicescribe.local', 'recording-status@voicescribe.local', 'other@local']","disabled-extensions":"['right-alt@voicescribe.local']"},"org.gnome.settings-daemon.plugins.media-keys":{"custom-keybindings":"['/test/legacy/', '/test/other/']"},"org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/test/legacy/":{"command":"'voice-scribe --record'"},"org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:/test/other/":{"command":"'other-app'"}}
JSON
printf '{"old_enabled":true,"old_active":true,"new_enabled":false,"new_active":false}\n' > "$case_root/peer/service.json"
if [[ "$case_name" == service-* ]]; then
    cp "$asset_root/resources/mluva-input@.service" "$case_root/units/voice-scribe-input@.service"
    case "$case_name" in
        service-disabled) printf '{"old_enabled":false,"old_active":false,"new_enabled":false,"new_active":false}\n' > "$case_root/peer/service.json" ;;
        service-modified) printf 'foreign service\n' >> "$case_root/units/voice-scribe-input@.service" ;;
        service-both) cp "$case_root/units/voice-scribe-input@.service" "$case_root/units/mluva-input@.service" ;;
        service-other-wants) mkdir "$case_root/units/multi-user.target.wants"; ln -s ../voice-scribe-input@.service "$case_root/units/multi-user.target.wants/voice-scribe-input@42424.service" ;;
    esac
fi
live_home="$case_root/live-home"
case "$case_name" in live-*|service-*|desktop-*) live_home="$target_home" ;; esac
jq -n --arg home "$target_home" --arg live "$live_home" --arg data "$data_root" --arg config "$config_root" --arg managed "$managed" '{home:$home,live:$live,install:$home,data:$data,config:$config,managed:$managed}' > "$case_root/layout.json"
