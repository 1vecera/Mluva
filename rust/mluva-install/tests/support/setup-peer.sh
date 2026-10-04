#!/usr/bin/env bash
# Private external endpoints for combined-setup ordering; no package or desktop writes.
set -euo pipefail
name=${0##*/}
case "$name" in
    id) printf '%s\n' "${MLUVA_SETUP_UID:-1000}" ;;
    omarchy)
        printf 'omarchy %s\n' "$*" >> "$MLUVA_SETUP_LOG"
        if [[ "$1 $2" == 'plugin enable' ]]; then exit "${MLUVA_SETUP_MANAGER_STATUS:-0}"; fi
        [[ "$1 $2" == 'pkg add' ]] || exit 97
        exit "${MLUVA_SETUP_PACKAGE_STATUS:-0}" ;;
    sudo)
        [[ "$1 $2 $3" == 'dnf install -y' ]] || exit 97
        printf 'sudo %s\n' "$*" >> "$MLUVA_SETUP_LOG"
        exit "${MLUVA_SETUP_PACKAGE_STATUS:-0}" ;;
    dnf) exit 0 ;;
    python3|mluva-install-widget|install-widget.sh)
        if [[ "$name" == python3 ]]; then
            [[ "${MLUVA_SETUP_NATIVE:-0}" == 0 ]] || { touch "$MLUVA_SETUP_LOG.python-used"; exit 99; }
            [[ "$1" == */linux/install_widget.py ]] || exit 97
            shift
        fi
        if [[ "${1:-}" == --check ]]; then
            printf 'widget-check\n' >> "$MLUVA_SETUP_LOG"
            exit "${MLUVA_SETUP_PREFLIGHT_STATUS:-0}"
        fi
        printf 'widget-install\n' >> "$MLUVA_SETUP_LOG"
        exit "${MLUVA_SETUP_PLUGIN_STATUS:-0}" ;;
    install.sh|mluva-install)
        printf 'native\n' >> "$MLUVA_SETUP_LOG"
        exit "${MLUVA_SETUP_NATIVE_STATUS:-0}" ;;
    *) exit 97 ;;
esac
