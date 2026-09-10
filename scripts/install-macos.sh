#!/usr/bin/env bash
# Installer-only migration boundary for the retired VoiceScribe app bundles.
set -euo pipefail

source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
applications_dir="${MLUVA_APPLICATIONS_DIR:-/Applications}"
backup_root="${MLUVA_APP_BACKUP_DIR:-${HOME}/Library/Application Support/Mluva-migration-backups}"
app="${applications_dir}/Mluva.app"
staging="${applications_dir}/.Mluva.installing.$$.app"
plist_buddy=/usr/libexec/PlistBuddy
old_apps=()
for name in Mluva 'Voice Scribe' VoiceScribeMac; do
    candidate="${applications_dir}/${name}.app"
    if [[ -e "${candidate}" || -L "${candidate}" ]]; then
        if [[ -L "${candidate}" || ! -d "${candidate}" ]]; then
            echo "Refusing an unexpected application path: ${candidate}" >&2
            exit 1
        fi
        identity="$("${plist_buddy}" -c 'Print :CFBundleIdentifier' "${candidate}/Contents/Info.plist")"
        if [[ "${identity}" != com.mluva.mac && "${identity}" != com.voicescribe.mac ]]; then
            echo "Refusing an unrelated application: ${candidate}" >&2
            exit 1
        fi
        old_apps+=("${candidate}")
    fi
done
if pgrep -x MluvaMac >/dev/null || pgrep -x VoiceScribeMac >/dev/null; then
    echo "Close Mluva before installing; its settings and drafts must not change during migration." >&2
    exit 1
fi
if [[ -e "${staging}" || -L "${staging}" || -L "${backup_root}" ]]; then
    echo "Refusing an unexpected staging or backup path." >&2
    exit 1
fi
mkdir -p "${applications_dir}"
mkdir -p "${backup_root}"
chmod 0700 "${backup_root}"
backup="$(mktemp -d "${backup_root}/apps.XXXXXX")"
complete=false
primary="${old_apps[0]:-}"
rollback() {
    local status=$?
    trap - EXIT HUP INT TERM
    if [[ "${complete}" != true ]]; then
        if [[ -n "${primary}" && "${primary}" != "${app}" && ! -e "${primary}" && -d "${app}" ]]; then
            mv "${app}" "${primary}"
        fi
        if [[ -n "${primary}" && -d "${staging}/PreviousContents" ]]; then
            if [[ -d "${primary}/Contents" ]]; then
                rm -rf -- "${primary}/Contents"
            fi
            mv "${staging}/PreviousContents" "${primary}/Contents"
        fi
        for candidate in "${old_apps[@]}"; do
            if [[ "${candidate}" != "${primary}" && -d "${backup}/$(basename "${candidate}")" ]]; then
                mv "${backup}/$(basename "${candidate}")" "${candidate}"
            fi
        done
        rm -rf -- "${staging}"
    fi
    exit "${status}"
}
trap rollback EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
ditto "${source_dir}/build/Mluva.app" "${staging}"
codesign --verify --deep --strict "${staging}"
for candidate in "${old_apps[@]}"; do
    if [[ "${candidate}" == "${primary}" ]]; then
        ditto "${candidate}" "${backup}/$(basename "${candidate}")"
    else
        mv "${candidate}" "${backup}/"
    fi
done
if [[ -n "${primary}" ]]; then
    # Keep the bundle directory's file identity so existing Finder/Dock aliases
    # follow its rename. Replace all contents, leaving no retired executable.
    mv "${primary}/Contents" "${staging}/PreviousContents"
    mv "${staging}/Contents" "${primary}/Contents"
    if [[ "${primary}" != "${app}" ]]; then
        mv "${primary}" "${app}"
    fi
else
    mv "${staging}" "${app}"
fi
complete=true
rm -rf -- "${staging}"
echo "Installed ${app}; previous app bundles are backed up at ${backup}."
echo "First launch migrates settings, history and saved drafts before opening Mluva."
