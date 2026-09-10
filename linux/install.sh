#!/usr/bin/env bash
set -euo pipefail

source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
install_home="${MLUVA_INSTALL_HOME:-${HOME}}"
if [[ "${install_home}" != /* || "${install_home}" == "/" ]]; then
    echo "MLUVA_INSTALL_HOME must be an absolute non-root directory." >&2
    exit 1
fi
staged_install=false
if [[ "${install_home}" != "${HOME}" ]]; then
    staged_install=true
    data_home="${install_home}/.local/share"
    config_home="${install_home}/.config"
else
    data_home="${XDG_DATA_HOME:-${install_home}/.local/share}"
    config_home="${XDG_CONFIG_HOME:-${install_home}/.config}"
fi
for resolved_root in "${data_home}" "${config_home}"; do
    if [[ "${resolved_root}" != /* || "${resolved_root}" == "/" ]]; then
        echo "Refusing to install through an unsafe XDG root: ${resolved_root}" >&2
        exit 1
    fi
done
if [[ "${MLUVA_MIGRATION_ACTIVE:-}" != 1 ]]; then
    exec /usr/bin/python3 "${source_dir}/migrate_legacy.py" \
        "${source_dir}" "${install_home}" "${config_home}" "${data_home}" "${staged_install}"
fi

application_dir="${data_home}/mluva/app"
application_parent="$(dirname -- "${application_dir}")"
application_backup="${application_parent}/.app.previous.$$"
bin_dir="${install_home}/.local/bin"
applications_dir="${data_home}/applications"
icons_dir="${data_home}/icons/hicolor/scalable/apps"

launcher_is_owned() {
    local launcher_path=$1
    [[ -f "${launcher_path}" && ! -L "${launcher_path}" ]] \
        && grep -Fxq "application_dir=\"${application_dir}\"" "${launcher_path}" \
        && grep -Fq -- "-m mluva_linux.app" "${launcher_path}"
}

require_managed_link() {
    local link_path=$1
    shift
    if [[ ! -e "${link_path}" && ! -L "${link_path}" ]]; then
        return
    fi
    if [[ ! -L "${link_path}" ]]; then
        echo "Refusing to replace an unrelated command: ${link_path}" >&2
        exit 1
    fi
    local actual_target
    actual_target="$(readlink -- "${link_path}")"
    local expected_target
    for expected_target in "$@"; do
        if [[ "${actual_target}" == "${expected_target}" ]]; then
            return
        fi
    done
    echo "Refusing to replace an unrelated command: ${link_path}" >&2
    exit 1
}

canonical_launcher="${bin_dir}/mluva"
if [[ -L "${canonical_launcher}" ]] \
    || { [[ -e "${canonical_launcher}" ]] && ! launcher_is_owned "${canonical_launcher}"; }; then
    echo "Refusing to replace an unrelated command: ${canonical_launcher}" >&2
    exit 1
fi
require_managed_link "${bin_dir}/mluva-input-helper" "${application_dir}/configure-input-helper.sh"
require_managed_link "${bin_dir}/mluva-overlay" "${application_dir}/configure-recording-overlay.sh"
require_managed_link "${bin_dir}/mluva-uninstall" "${application_dir}/uninstall.sh"
require_managed_link "${bin_dir}/mluva-shell" "${application_dir}/mluva-shell"

if [[ -L "${application_dir}" || ( -e "${application_dir}" && ! -d "${application_dir}" ) ]]; then
    echo "Refusing to replace an unexpected application path: ${application_dir}" >&2
    exit 1
fi
if [[ -d "${application_dir}" ]] \
    && { [[ ! -f "${application_dir}/pyproject.toml" ]] \
        || ! grep -Fxq 'name = "mluva-linux"' \
            "${application_dir}/pyproject.toml"; }; then
    echo "Refusing to replace an unrecognized application directory: ${application_dir}" >&2
    exit 1
fi

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1" >&2
        exit 1
    }
}

require_command uv
require_command pw-record
require_command pw-dump
require_command wl-copy
require_command pgrep
test -x /usr/bin/python3 || {
    echo "Mluva requires the distribution Python at /usr/bin/python3." >&2
    exit 1
}
if pgrep -f -- "${application_dir}/.venv/bin/python -m mluva_linux.app" >/dev/null 2>&1; then
    echo "Mluva is running from ${application_dir}. Close it before installation so its environment is not replaced in place." >&2
    exit 1
fi

install -d -m 0755 "${application_parent}"
previous_application=false
if [[ -e "${application_backup}" || -L "${application_backup}" ]]; then
    echo "Refusing to reuse an existing installation backup path: ${application_backup}" >&2
    exit 1
fi
if [[ -d "${application_dir}" ]]; then
    mv -- "${application_dir}" "${application_backup}"
    previous_application=true
fi
installation_complete=false

rollback_installation() {
    local exit_status=$?
    trap - EXIT HUP INT TERM
    if [[ "${installation_complete}" != "true" ]]; then
        if [[ -d "${application_dir}" ]]; then
            rm -r -- "${application_dir}"
        fi
        if [[ "${previous_application}" == "true" && -d "${application_backup}" ]]; then
            mv -- "${application_backup}" "${application_dir}"
        fi
    fi
    exit "${exit_status}"
}
trap rollback_installation EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

install -d -m 0755 \
    "${application_dir}" \
    "${application_dir}/gnome-extension/recording-status@mluva.local" \
    "${application_dir}/resources" \
    "${bin_dir}" \
    "${applications_dir}" \
    "${icons_dir}"
install -d -m 0755 "${application_dir}/mluva_linux"
install -m 0644 "${source_dir}/pyproject.toml" "${source_dir}/uv.lock" "${application_dir}/"
install -m 0644 "${source_dir}/mluva_linux/"*.py "${application_dir}/mluva_linux/"
install -m 0644 \
    "${source_dir}/gnome-extension/recording-status@mluva.local/"*.js \
    "${source_dir}/gnome-extension/recording-status@mluva.local/"*.json \
    "${source_dir}/gnome-extension/recording-status@mluva.local/"*.css \
    "${source_dir}/gnome-extension/recording-status@mluva.local/"*.svg \
    "${application_dir}/gnome-extension/recording-status@mluva.local/"
install -m 0644 \
    "${source_dir}/resources/mluva-input@.service" \
    "${application_dir}/resources/mluva-input@.service"
install -m 0755 "${source_dir}/configure-input-helper.sh" "${application_dir}/configure-input-helper.sh"
install -m 0755 "${source_dir}/configure-recording-overlay.sh" "${application_dir}/configure-recording-overlay.sh"
install -m 0755 "${source_dir}/uninstall.sh" "${application_dir}/uninstall.sh"
install -m 0755 "${source_dir}/mluva-shell" "${application_dir}/mluva-shell"
install -d -m 0755 "${application_dir}/quickshell/mluva.dictation"
install -m 0644 "${source_dir}/quickshell/mluva.dictation/"* "${application_dir}/quickshell/mluva.dictation/"
uv venv --clear --system-site-packages --python /usr/bin/python3 "${application_dir}/.venv"
uv sync --project "${application_dir}" --no-dev --frozen
"${application_dir}/.venv/bin/python" -c 'import gi; gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1"); gi.require_version("Atspi", "2.0"); gi.require_version("DBus", "1.0"); gi.require_version("cairo", "1.0"); from gi.repository import Adw, Atspi, DBus, Gtk, cairo'

sed "s|@APPLICATION_DIR@|${application_dir}|g" "${source_dir}/resources/mluva.in" > "${bin_dir}/mluva"
chmod 0755 "${bin_dir}/mluva"
ln -sfn "${application_dir}/configure-input-helper.sh" "${bin_dir}/mluva-input-helper"
ln -sfn "${application_dir}/configure-recording-overlay.sh" "${bin_dir}/mluva-overlay"
ln -sfn "${application_dir}/uninstall.sh" "${bin_dir}/mluva-uninstall"
ln -sfn "${application_dir}/mluva-shell" "${bin_dir}/mluva-shell"
sed "s|@EXECUTABLE@|${bin_dir}/mluva|g" "${source_dir}/resources/com.mluva.Linux.desktop.in" \
    > "${applications_dir}/com.mluva.Linux.desktop"
chmod 0644 "${applications_dir}/com.mluva.Linux.desktop"
install -m 0644 "${source_dir}/resources/com.mluva.Linux.svg" "${icons_dir}/com.mluva.Linux.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "${applications_dir}"

if [[ "${staged_install}" == "false" ]]; then
    secret_config_dir="${DAS_CONF_DIR:-${config_home}/daniel-ai-skills}"
    if [[ ! -s "${secret_config_dir}/env/mluva.env" ]] \
        && test -x "${secret_config_dir}/bin/das-mcp-launch" \
        && test -f "${secret_config_dir}/env/agent.env"; then
        DAS_CONF_DIR="${secret_config_dir}" bash "${source_dir}/configure-secret-profile.sh"
    fi
fi

installation_complete=true
trap - EXIT HUP INT TERM
if [[ "${previous_application}" == "true" ]]; then
    rm -r -- "${application_backup}"
fi

echo "Mluva is installed. Launch it from the application menu or run ${bin_dir}/mluva."
echo "On first launch, approve the F9 recording toggle and Ctrl+Alt+Escape cancellation shortcuts."
echo "No logout is required for the application or shortcut changes. Change the recording key between F1 and F24 from the Capture page."
if [[ "${staged_install}" == "true" ]]; then
    echo "Staged verification skipped live GNOME extension, systemd helper, accessibility, and secret-profile inspection."
else
    desktop_name="${XDG_CURRENT_DESKTOP:-}"
    if [[ "${desktop_name,,}" == *gnome* ]]; then
        if ! command -v gnome-extensions >/dev/null 2>&1 \
            || ! gnome-extensions info "recording-status@mluva.local" >/dev/null 2>&1; then
            echo "For a bottom recording bar that remains visible over other applications, install the optional display-only extension:"
            echo "  mluva-overlay install"
            echo "A newly installed GNOME Shell extension may require one logout and login before it can be enabled."
        fi
        if ! "${bin_dir}/mluva-input-helper" status >/dev/null 2>&1; then
            echo "For automatic paste in apps without native accessibility editing, install the optional keyboard-only helper:"
            echo "  mluva-input-helper install"
            echo "This requires sudo once and grants same-user processes synthetic-keyboard access through an owner-only socket."
        fi
        if command -v gsettings >/dev/null 2>&1 \
            && test "$(gsettings get org.gnome.desktop.interface toolkit-accessibility 2>/dev/null || true)" != "true"; then
            echo "Automatic insertion is unavailable while GNOME toolkit accessibility is off."
            echo "Enable it before launching Mluva with: gsettings set org.gnome.desktop.interface toolkit-accessibility true"
            echo "Applications already open when it is enabled may need to be restarted before they expose text targets."
        fi
    fi
    if test -x "${secret_config_dir}/bin/das-mcp-launch" && test -s "${secret_config_dir}/env/mluva.env"; then
        echo "The launcher will resolve only the reviewed ElevenLabs credential reference at runtime."
    elif test -x "${secret_config_dir}/bin/das-agent-launch" && test -s "${secret_config_dir}/env/agent.env"; then
        echo "The launcher will use das-agent-launch --only for the selected ElevenLabs credential at runtime."
    else
        echo "Choose your speech and rewrite providers in Settings and supply their credentials through the application environment."
    fi
fi
