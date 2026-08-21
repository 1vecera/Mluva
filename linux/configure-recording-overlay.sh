#!/usr/bin/env bash
set -euo pipefail

script_path=$(readlink -f -- "${BASH_SOURCE[0]}")
source_dir=$(cd -- "$(dirname -- "${script_path}")" && pwd)
extension_uuid="recording-status@voicescribe.local"
extension_source="${source_dir}/gnome-extension/${extension_uuid}"
recording_overlay_package_dir=""

cleanup_package_dir() {
    if [[ -n "${recording_overlay_package_dir}" && -d "${recording_overlay_package_dir}" ]]; then
        rm -r -- "${recording_overlay_package_dir}"
    fi
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

pack_extension() {
    local output_dir=$1
    gnome-extensions pack \
        --force \
        --extra-source recordingOverlay.js \
        --out-dir "${output_dir}" \
        "${extension_source}"
}

install_overlay() {
    require_command gnome-extensions
    recording_overlay_package_dir=$(mktemp -d /tmp/mluva-overlay.XXXXXX)
    trap cleanup_package_dir EXIT
    pack_extension "${recording_overlay_package_dir}"
    gnome-extensions install --force "${recording_overlay_package_dir}/${extension_uuid}.shell-extension.zip"
    if gnome-extensions info "${extension_uuid}" >/dev/null 2>&1; then
        gnome-extensions enable "${extension_uuid}"
        echo "Mluva recording overlay is installed and enabled."
    else
        echo "Mluva recording overlay is installed but this Shell session has not discovered it yet."
        echo "Log out and back in once, then run: mluva-overlay enable"
    fi
}

enable_overlay() {
    require_command gnome-extensions
    gnome-extensions enable "${extension_uuid}"
    echo "Mluva recording overlay is enabled."
}

show_status() {
    require_command gnome-extensions
    if ! gnome-extensions info "${extension_uuid}"; then
        echo "Mluva recording overlay is not installed." >&2
        exit 1
    fi
}

remove_overlay() {
    require_command gnome-extensions
    gnome-extensions disable "${extension_uuid}" >/dev/null 2>&1 || true
    gnome-extensions uninstall "${extension_uuid}"
    echo "Mluva recording overlay was removed."
}

case "${1:-}" in
    install)
        install_overlay
        ;;
    enable)
        enable_overlay
        ;;
    status)
        show_status
        ;;
    remove)
        remove_overlay
        ;;
    *)
        echo "Usage: $(basename "$0") {install|enable|status|remove}" >&2
        exit 2
        ;;
esac
