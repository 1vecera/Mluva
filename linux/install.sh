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
application_dir="${data_home}/voice-scribe/app"
application_parent="$(dirname -- "${application_dir}")"
application_backup="${application_parent}/.app.previous.$$"
bin_dir="${install_home}/.local/bin"
applications_dir="${data_home}/applications"
icons_dir="${data_home}/icons/hicolor/scalable/apps"
legacy_extension_uuid="right-alt@voicescribe.local"
legacy_extension_install_dir="${data_home}/gnome-shell/extensions/${legacy_extension_uuid}"

launcher_is_owned() {
    local launcher_path=$1
    [[ -f "${launcher_path}" && ! -L "${launcher_path}" ]] \
        && grep -Fxq "application_dir=\"${application_dir}\"" "${launcher_path}" \
        && grep -Fq -- "-m voice_scribe_linux.app" "${launcher_path}"
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
legacy_launcher="${bin_dir}/voice-scribe"
if [[ -L "${canonical_launcher}" ]] \
    || { [[ -e "${canonical_launcher}" ]] && ! launcher_is_owned "${canonical_launcher}"; }; then
    echo "Refusing to replace an unrelated command: ${canonical_launcher}" >&2
    exit 1
fi
if [[ -L "${legacy_launcher}" ]]; then
    if [[ "$(readlink -- "${legacy_launcher}")" != "mluva" ]]; then
        echo "Refusing to replace an unrelated command: ${legacy_launcher}" >&2
        exit 1
    fi
elif [[ -e "${legacy_launcher}" ]] && ! launcher_is_owned "${legacy_launcher}"; then
    echo "Refusing to replace an unrelated command: ${legacy_launcher}" >&2
    exit 1
fi
require_managed_link "${bin_dir}/mluva-input-helper" "${application_dir}/configure-input-helper.sh"
require_managed_link \
    "${bin_dir}/voice-scribe-input-helper" \
    "${application_dir}/configure-input-helper.sh" \
    "mluva-input-helper"
require_managed_link "${bin_dir}/mluva-overlay" "${application_dir}/configure-recording-overlay.sh"
require_managed_link \
    "${bin_dir}/voice-scribe-overlay" \
    "${application_dir}/configure-recording-overlay.sh" \
    "mluva-overlay"
require_managed_link "${bin_dir}/mluva-uninstall" "${application_dir}/uninstall.sh"

if [[ -L "${application_dir}" || ( -e "${application_dir}" && ! -d "${application_dir}" ) ]]; then
    echo "Refusing to replace an unexpected application path: ${application_dir}" >&2
    exit 1
fi
if [[ -d "${application_dir}" ]] \
    && { [[ ! -f "${application_dir}/pyproject.toml" ]] \
        || ! grep -Fxq -e 'name = "mluva-linux"' -e 'name = "voice-scribe-linux"' \
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
if pgrep -f -- "${application_dir}/.venv/bin/python -m voice_scribe_linux.app" >/dev/null 2>&1; then
    echo "Mluva is running from ${application_dir}. Close it before installation so its environment is not replaced in place." >&2
    exit 1
fi

if [[ "${staged_install}" == "false" ]] \
    && command -v gnome-extensions >/dev/null 2>&1 \
    && gnome-extensions info "${legacy_extension_uuid}" >/dev/null 2>&1; then
    gnome-extensions disable "${legacy_extension_uuid}" >/dev/null 2>&1 || true
    gnome-extensions uninstall "${legacy_extension_uuid}" >/dev/null 2>&1 || true
fi
if test -d "${legacy_extension_install_dir}"; then
    legacy_extension_backup="${data_home}/voice-scribe/retired-right-alt-extension.$(date +%s).$$"
    install -d -m 0700 "${data_home}/voice-scribe"
    mv -- "${legacy_extension_install_dir}" "${legacy_extension_backup}"
    echo "Retired Right Alt helper moved to ${legacy_extension_backup}; AltGr is no longer reserved."
fi

if [[ "${staged_install}" == "true" ]]; then
    secret_config_dir="${config_home}/daniel-ai-skills"
else
    secret_config_dir="${DAS_CONF_DIR:-${config_home}/daniel-ai-skills}"
fi
if test -x "${secret_config_dir}/bin/das-mcp-launch" && test -f "${secret_config_dir}/env/agent.env"; then
    DAS_CONF_DIR="${secret_config_dir}" bash "${source_dir}/configure-secret-profile.sh"
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
    "${application_dir}/gnome-extension/recording-status@voicescribe.local" \
    "${application_dir}/resources" \
    "${bin_dir}" \
    "${applications_dir}" \
    "${icons_dir}"
install -d -m 0755 "${application_dir}/voice_scribe_linux"
rm -f -- \
    "${application_dir}/enable-right-alt.sh" \
    "${application_dir}/disable-right-alt.sh" \
    "${application_dir}/set-right-alt-enabled.js" \
    "${application_dir}/voice_scribe_linux/hotkey_gestures.py"
if test -d "${application_dir}/voice_scribe_linux/__pycache__"; then
    rm -rf -- "${application_dir}/voice_scribe_linux/__pycache__"
fi
install -m 0644 "${source_dir}/pyproject.toml" "${source_dir}/uv.lock" "${application_dir}/"
install -m 0644 "${source_dir}/voice_scribe_linux/"*.py "${application_dir}/voice_scribe_linux/"
install -m 0644 \
    "${source_dir}/gnome-extension/recording-status@voicescribe.local/"*.js \
    "${source_dir}/gnome-extension/recording-status@voicescribe.local/"*.json \
    "${source_dir}/gnome-extension/recording-status@voicescribe.local/"*.css \
    "${application_dir}/gnome-extension/recording-status@voicescribe.local/"
install -m 0644 \
    "${source_dir}/resources/voice-scribe-input@.service" \
    "${application_dir}/resources/voice-scribe-input@.service"
install -m 0755 "${source_dir}/configure-input-helper.sh" "${application_dir}/configure-input-helper.sh"
install -m 0755 "${source_dir}/configure-recording-overlay.sh" "${application_dir}/configure-recording-overlay.sh"
install -m 0755 "${source_dir}/uninstall.sh" "${application_dir}/uninstall.sh"
uv venv --clear --system-site-packages --python /usr/bin/python3 "${application_dir}/.venv"
uv sync --project "${application_dir}" --no-dev --frozen
"${application_dir}/.venv/bin/python" -c 'import gi; gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1"); gi.require_version("Atspi", "2.0"); gi.require_version("DBus", "1.0"); gi.require_version("cairo", "1.0"); from gi.repository import Adw, Atspi, DBus, Gtk, cairo'

sed "s|@APPLICATION_DIR@|${application_dir}|g" "${source_dir}/resources/mluva.in" > "${bin_dir}/mluva"
chmod 0755 "${bin_dir}/mluva"
rm -f -- "${bin_dir}/voice-scribe"
ln -sfn "mluva" "${bin_dir}/voice-scribe"
ln -sfn "${application_dir}/configure-input-helper.sh" "${bin_dir}/mluva-input-helper"
ln -sfn "mluva-input-helper" "${bin_dir}/voice-scribe-input-helper"
ln -sfn "${application_dir}/configure-recording-overlay.sh" "${bin_dir}/mluva-overlay"
ln -sfn "mluva-overlay" "${bin_dir}/voice-scribe-overlay"
ln -sfn "${application_dir}/uninstall.sh" "${bin_dir}/mluva-uninstall"
sed "s|@EXECUTABLE@|${bin_dir}/mluva|g" "${source_dir}/resources/com.voicescribe.Linux.desktop.in" \
    > "${applications_dir}/com.voicescribe.Linux.desktop"
chmod 0644 "${applications_dir}/com.voicescribe.Linux.desktop"
install -m 0644 "${source_dir}/resources/com.voicescribe.Linux.svg" "${icons_dir}/com.voicescribe.Linux.svg"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "${applications_dir}"

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
    if ! command -v gnome-extensions >/dev/null 2>&1 \
        || ! gnome-extensions info "recording-status@voicescribe.local" >/dev/null 2>&1; then
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
    if test -x "${secret_config_dir}/bin/das-mcp-launch" && test -s "${secret_config_dir}/env/voice-scribe.env"; then
        echo "The launcher will resolve only the reviewed ElevenLabs credential reference at runtime."
    else
        echo "No managed secret launcher was found; set ELEVENLABS_API_KEY in the application process environment."
    fi
fi
