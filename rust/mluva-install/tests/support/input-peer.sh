#!/usr/bin/env bash
# Unshipped command peers; service operations are receipts only. rm acts on
# the test's bind-mounted unit directory after its inode was independently checked.
set -euo pipefail
case "${0##*/}" in
    id) printf '12345\n' ;;
    sudo)
        printf '%s\n' "$*" >> "$MLUVA_INPUT_PEER_DIR/commands"
        if [[ "$1" == rm ]]; then
            [[ "$#" == 4 && "$2" == -f && "$3" == -- && "$4" == /etc/systemd/system/mluva-input@.service ]] || exit 95
            shift
            exec /usr/bin/rm "$@"
        fi
        [[ "$1" == systemctl ]] || exit 95
        ;;
    *)
        printf 'unexpected interpreter\n' > "$MLUVA_INPUT_PEER_DIR/python-used"
        exit 99
        ;;
esac
