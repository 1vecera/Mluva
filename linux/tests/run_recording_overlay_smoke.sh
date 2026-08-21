#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
linux_root=$(cd -- "${script_dir}/.." && pwd)
project_root=$(cd -- "${linux_root}/.." && pwd)
extension_root="${linux_root}/gnome-extension/recording-status@voicescribe.local"
output_dir=${1:-"${project_root}/tmp/overlay-smoke"}

for executable in dbus-run-session gnome-extensions gnome-shell-test-tool; do
    if ! command -v "${executable}" >/dev/null 2>&1; then
        echo "Missing required overlay smoke dependency: ${executable}" >&2
        exit 1
    fi
done

mkdir -p -- "${output_dir}"
output_dir=$(cd -- "${output_dir}" && pwd -P)
gnome-extensions pack \
    --force \
    --extra-source recordingOverlay.js \
    --out-dir "${output_dir}" \
    "${extension_root}"
extension_archive="${output_dir}/recording-status@voicescribe.local.shell-extension.zip"

for scenario in preparing recording quiet; do
    runtime_dir=$(mktemp -d /tmp/mluva-shell.XXXXXX)
    chmod 0700 "${runtime_dir}"
    screenshot_path="${output_dir}/${scenario}-1280x720.png"
    log_path="${output_dir}/${scenario}-1280x720.log"

    set +e
    dbus-run-session -- env \
        NO_AT_BRIDGE=1 \
        XDG_RUNTIME_DIR="${runtime_dir}" \
        VOICE_SCRIBE_OVERLAY_SCENARIO="${scenario}" \
        VOICE_SCRIBE_OVERLAY_SCREENSHOT="${screenshot_path}" \
        gnome-shell-test-tool \
            --headless \
            --disable-animations \
            --extension "${extension_archive}" \
            "${script_dir}/recording_overlay_smoke.js" \
            >"${log_path}" 2>&1
    smoke_status=$?
    set -e

    rm -r -- "${runtime_dir}"
    if ((smoke_status != 0)); then
        echo "GNOME recording overlay smoke failed for ${scenario}; see ${log_path}" >&2
        exit "${smoke_status}"
    fi
    if rg -n 'recordingOverlay\.js|recording-status@voicescribe\.local.*(ERROR|CRITICAL|Exception)' "${log_path}"; then
        echo "GNOME recording overlay emitted an extension-specific error for ${scenario}" >&2
        exit 1
    fi
    if [[ ! -s "${screenshot_path}" ]]; then
        echo "GNOME recording overlay did not retain ${screenshot_path}" >&2
        exit 1
    fi
done

echo "GNOME recording overlay smoke passed; evidence: ${output_dir}"
