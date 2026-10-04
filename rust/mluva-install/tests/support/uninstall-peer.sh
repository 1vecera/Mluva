#!/usr/bin/env bash
# External desktop/service endpoints only; never contact the host services.
set -euo pipefail
name="${0##*/}"
if [[ "$name" == id ]]; then printf '42424\n'; exit; fi
printf '%s %s\n' "$name" "$*" >> "$MLUVA_UNINSTALL_PEER/commands"
case "$name" in
    sudo)
        if [[ "$1" == rm ]]; then
            [[ "$#" == 4 && "$2" == -f && "$3" == -- && "$4" == /etc/systemd/system/mluva-input@.service ]] || exit 95
            [[ "$MLUVA_UNINSTALL_CASE" != live-input-failure ]] || exit 23
            shift
            exec /usr/bin/rm "$@"
        fi
        [[ "$1" == systemctl ]] || exit 95
        ;;
    gnome-extensions)
        if [[ "$1" == info && "$MLUVA_UNINSTALL_CASE" == live-no-overlay ]]; then exit 1; fi
        if [[ "$1" == uninstall && "$MLUVA_UNINSTALL_CASE" == live-overlay-failure ]]; then exit 24; fi
        ;;
    update-desktop-database)
        if [[ "$MLUVA_UNINSTALL_CASE" == database-signal ]]; then kill -TERM "$$"; fi
        [[ "$MLUVA_UNINSTALL_CASE" != database-failure ]] || exit 25
        ;;
    *) printf 'unexpected interpreter\n' > "$MLUVA_UNINSTALL_PEER/python-used"; exit 99 ;;
esac
