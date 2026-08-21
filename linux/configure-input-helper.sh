#!/usr/bin/env bash
set -euo pipefail

script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
source_dir="$(cd -- "$(dirname -- "${script_path}")" && pwd)"
unit_source="${source_dir}/resources/voice-scribe-input@.service"
unit_destination="/etc/systemd/system/voice-scribe-input@.service"
action="${1:-status}"
user_id="$(id -u)"
runtime_dir="/run/user/${user_id}"
socket_path="${runtime_dir}/.ydotool_socket"
unit_name="voice-scribe-input@${user_id}.service"

if [[ "${user_id}" == "0" ]]; then
    echo "Run this command as the desktop user, not as root; it invokes sudo only for the system service." >&2
    exit 2
fi

[[ -f "${unit_source}" ]] || {
    echo "Missing packaged service template: ${unit_source}" >&2
    exit 1
}

case "${action}" in
    install)
        command -v sudo >/dev/null 2>&1 || {
            echo "sudo is required to install the narrowly scoped input service." >&2
            exit 1
        }
        command -v systemctl >/dev/null 2>&1 || {
            echo "systemd is required to install the input service." >&2
            exit 1
        }
        [[ -x /usr/bin/ydotoold ]] || {
            echo "Install the Fedora ydotool package first: sudo dnf install ydotool" >&2
            exit 1
        }
        [[ -e /dev/uinput ]] || {
            echo "The kernel uinput device is unavailable at /dev/uinput." >&2
            exit 1
        }
        if [[ -S "${socket_path}" ]] && ! systemctl is-active --quiet "${unit_name}"; then
            echo "Another process already owns ${socket_path}; stop it before installing this service." >&2
            exit 1
        fi
        sudo install -m 0644 "${unit_source}" "${unit_destination}"
        sudo systemctl daemon-reload
        sudo systemctl reset-failed "${unit_name}" 2>/dev/null || true
        sudo systemctl enable "${unit_name}"
        sudo systemctl restart "${unit_name}"
        for _attempt in {1..30}; do
            [[ -S "${socket_path}" ]] && break
            sleep 0.1
        done
        [[ -S "${socket_path}" ]] || {
            sudo systemctl disable --now "${unit_name}" >/dev/null 2>&1 || true
            echo "The input service started without creating its owner-only socket." >&2
            exit 1
        }
        [[ "$(stat -Lc '%u:%a' "${socket_path}")" == "${user_id}:600" ]] || {
            echo "The input service socket does not have the required owner and 0600 permissions." >&2
            exit 1
        }
        echo "Mluva keyboard paste is ready for user ${user_id}."
        echo "The service exposes keyboard-only synthetic input to processes owned by this user; remove it when that tradeoff is unwanted."
        ;;
    remove)
        command -v sudo >/dev/null 2>&1 || {
            echo "sudo is required to remove the input service." >&2
            exit 1
        }
        sudo systemctl disable --now "${unit_name}" 2>/dev/null || true
        sudo rm -f -- "${unit_destination}"
        sudo systemctl daemon-reload
        sudo systemctl reset-failed "${unit_name}" 2>/dev/null || true
        echo "Mluva keyboard paste helper removed for user ${user_id}."
        ;;
    status)
        if systemctl is-active --quiet "${unit_name}" \
            && [[ -S "${socket_path}" ]] \
            && [[ "$(stat -Lc '%u:%a' "${socket_path}")" == "${user_id}:600" ]]; then
            echo "Mluva keyboard paste helper is ready."
            exit 0
        fi
        echo "Mluva keyboard paste helper is not ready." >&2
        exit 1
        ;;
    *)
        echo "Usage: mluva-input-helper {install|status|remove}" >&2
        exit 2
        ;;
esac
