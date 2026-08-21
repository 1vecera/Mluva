#!/usr/bin/env bash
set -euo pipefail

script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
source_dir="$(cd -- "$(dirname -- "${script_path}")" && pwd -P)"
install_home="${MLUVA_INSTALL_HOME:-${HOME}}"
if [[ "${install_home}" != /* || "${install_home}" == "/" ]]; then
    echo "MLUVA_INSTALL_HOME must be an absolute non-root directory." >&2
    exit 1
fi

staged_uninstall=false
if [[ "${install_home}" != "${HOME}" ]]; then
    staged_uninstall=true
    data_home="${install_home}/.local/share"
    config_home="${install_home}/.config"
else
    data_home="${XDG_DATA_HOME:-${install_home}/.local/share}"
    config_home="${XDG_CONFIG_HOME:-${install_home}/.config}"
fi
for resolved_root in "${data_home}" "${config_home}"; do
    if [[ "${resolved_root}" != /* || "${resolved_root}" == "/" ]]; then
        echo "Refusing to uninstall through an unsafe XDG root: ${resolved_root}" >&2
        exit 1
    fi
done

application_dir="${data_home}/voice-scribe/app"
bin_dir="${install_home}/.local/bin"
desktop_entry="${data_home}/applications/com.voicescribe.Linux.desktop"
icon_path="${data_home}/icons/hicolor/scalable/apps/com.voicescribe.Linux.svg"
input_unit="/etc/systemd/system/voice-scribe-input@.service"

if pgrep -f -- "${application_dir}/.venv/bin/python -m voice_scribe_linux.app" >/dev/null 2>&1; then
    echo "Mluva is running from ${application_dir}. Close it before uninstalling." >&2
    exit 1
fi
if [[ -d "${application_dir}" ]] \
    && { [[ ! -f "${application_dir}/pyproject.toml" ]] \
        || ! grep -Fxq 'name = "mluva-linux"' "${application_dir}/pyproject.toml"; }; then
    echo "Refusing to remove an unrecognized application directory: ${application_dir}" >&2
    exit 1
fi

remove_exact_symlink() {
    local path=$1
    local expected_target=$2
    if [[ ! -L "${path}" ]]; then
        return
    fi
    if [[ "$(readlink -- "${path}")" != "${expected_target}" ]]; then
        echo "Preserved unexpected symlink at ${path}." >&2
        return
    fi
    rm -f -- "${path}"
}

if [[ "${staged_uninstall}" == "false" ]]; then
    if [[ -f "${input_unit}" ]]; then
        if ! "${source_dir}/configure-input-helper.sh" remove; then
            echo "The application remains installed because its system input helper could not be removed." >&2
            exit 1
        fi
    fi
    if command -v gnome-extensions >/dev/null 2>&1 \
        && gnome-extensions info "recording-status@voicescribe.local" >/dev/null 2>&1; then
        if ! "${source_dir}/configure-recording-overlay.sh" remove; then
            echo "Warning: the optional Mluva recording overlay could not be removed automatically." >&2
            echo "Remove it later with: gnome-extensions uninstall recording-status@voicescribe.local" >&2
        fi
    fi
else
    echo "Staged verification skipped live GNOME extension and systemd helper inspection."
fi

launcher_path="${bin_dir}/mluva"
if [[ -f "${launcher_path}" && ! -L "${launcher_path}" ]]; then
    if grep -Fxq "application_dir=\"${application_dir}\"" "${launcher_path}" \
        && grep -Fq -- "-m voice_scribe_linux.app" "${launcher_path}"; then
        rm -f -- "${launcher_path}"
    else
        echo "Preserved unexpected executable at ${launcher_path}." >&2
    fi
fi
remove_exact_symlink "${bin_dir}/voice-scribe" "mluva"
remove_exact_symlink "${bin_dir}/mluva-input-helper" "${application_dir}/configure-input-helper.sh"
remove_exact_symlink "${bin_dir}/voice-scribe-input-helper" "mluva-input-helper"
remove_exact_symlink "${bin_dir}/mluva-overlay" "${application_dir}/configure-recording-overlay.sh"
remove_exact_symlink "${bin_dir}/voice-scribe-overlay" "mluva-overlay"
remove_exact_symlink "${bin_dir}/mluva-uninstall" "${application_dir}/uninstall.sh"

if [[ -f "${desktop_entry}" ]] \
    && grep -Fxq "Name=Mluva" "${desktop_entry}" \
    && grep -Fxq "Exec=${bin_dir}/mluva" "${desktop_entry}"; then
    rm -f -- "${desktop_entry}"
fi
if [[ -f "${icon_path}" ]] && grep -Fq '<title id="title">Mluva</title>' "${icon_path}"; then
    rm -f -- "${icon_path}"
fi

if [[ -d "${application_dir}" ]]; then
    rm -r -- "${application_dir}"
fi

command -v update-desktop-database >/dev/null 2>&1 \
    && [[ -d "${data_home}/applications" ]] \
    && update-desktop-database "${data_home}/applications"

echo "Mluva application files and registered desktop integrations were removed."
echo "Settings and user-created history remain under ${config_home}/voice-scribe and ${data_home}/voice-scribe."
