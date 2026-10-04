#!/usr/bin/env bash
# Identical synthetic inputs for released and native installer processes.
set -euo pipefail
case_root=$1
case_name=$2
asset_root=$3
support=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
bash "$support/seed-uninstall.sh" "$case_root" "$case_name" "$asset_root"
target_home=$(jq -r .home "$case_root/layout.json")
data_root="$target_home/.local/share"
config_root="$target_home/.config"
if [[ "$case_name" == xdg-custom ]]; then data_root="$case_root/custom-data"; config_root="$case_root/custom-config"; fi
if [[ "$case_name" == fresh ]]; then
    rm -r "$target_home"
    mkdir "$target_home"
fi
mkdir -p "$case_root/fake-bin" "$case_root/temp"
for name in uv pw-record pw-dump wl-copy update-desktop-database gnome-extensions gsettings python-trap; do
    install -m 0755 "$support/install-peer.sh" "$case_root/fake-bin/$name"
done
case "$case_name" in
    live-profile-*|live-database-failure|live-signal|live-foreign-launcher)
        managed="$config_root/daniel-ai-skills"
        mkdir -p "$managed/env" "$managed/bin"
        chmod 0750 "$managed/env"
        printf '#!/bin/sh\nexit 99\n' > "$managed/bin/das-mcp-launch"
        chmod 0755 "$managed/bin/das-mcp-launch"
        printf 'UNRELATED=op://synthetic/unrelated/value\nDAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL=op://synthetic/elevenlabs/credential\n' > "$managed/env/agent.env"
        case "$case_name" in
            live-profile-existing) printf 'ELEVENLABS_API_KEY=op://synthetic/existing/credential\n' > "$managed/env/mluva.env"; chmod 0600 "$managed/env/mluva.env" ;;
            live-profile-invalid) printf 'DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL=synthetic-value-do-not-echo\n' > "$managed/env/agent.env" ;;
            live-profile-duplicate) cat "$managed/env/agent.env" >> "$managed/env/duplicate"; cat "$managed/env/duplicate" >> "$managed/env/agent.env"; rm "$managed/env/duplicate" ;;
            live-profile-cr) printf 'DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL=op://synthetic/elevenlabs/credential\r\n' > "$managed/env/agent.env" ;;
            live-profile-empty) touch "$managed/env/mluva.env"; chmod 0640 "$managed/env/mluva.env" ;;
        esac
        ;;
esac
if [[ "$case_name" == native-legacy ]]; then mkdir -p "$data_root/voice-scribe"; printf 'preserve\n' > "$data_root/voice-scribe/history"; fi
