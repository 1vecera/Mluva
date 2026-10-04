#!/usr/bin/env bash
# External dependency/cache/GNOME endpoints; no desktop, capture or secret access.
set -euo pipefail
name=${0##*/}
case "$name" in
    uv)
        if [[ "$1" == venv ]]; then
            target=${!#}
            mkdir -p "$target/bin"
            ln -s "$MLUVA_INSTALL_DEPENDENCY_PEER" "$target/bin/python"
        elif [[ "$1" != sync ]]; then exit 96; fi
        ;;
    update-desktop-database)
        printf 'update-desktop-database %s\n' "$*" >> "$MLUVA_INSTALL_PEER/commands"
        case "$MLUVA_INSTALL_CASE" in
            *database-failure) exit 25 ;;
            live-foreign-launcher)
                rm "$MLUVA_INSTALL_TARGET/.local/bin/mluva"
                printf 'concurrent owner\n' > "$MLUVA_INSTALL_TARGET/.local/bin/mluva"
                exit 25 ;;
            live-signal)
                sleep 300 &
                printf '%s\n' "$!" > "$MLUVA_INSTALL_PEER/child"
                touch "$MLUVA_INSTALL_PEER/waiting"
                wait ;;
        esac
        ;;
    gnome-extensions) printf '%s %s\n' "$name" "$*" >> "$MLUVA_INSTALL_PEER/commands"; [[ "$MLUVA_INSTALL_CASE" == live-gnome-ready ]] ;;
    gsettings) printf '%s %s\n' "$name" "$*" >> "$MLUVA_INSTALL_PEER/commands"; if [[ "$MLUVA_INSTALL_CASE" == live-gnome-ready ]]; then printf 'true\n'; else printf 'false\n'; fi ;;
    python-trap|python3) printf 'unexpected interpreter\n' > "$MLUVA_INSTALL_PEER/python-used"; exit 99 ;;
    *) exit 97 ;;
esac
