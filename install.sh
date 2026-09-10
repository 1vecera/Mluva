#!/usr/bin/env bash
# Install the native app and its Omarchy widget in one pass.
set -euo pipefail

source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
assume_yes=false
app_only=false

usage() {
    cat <<'USAGE'
Usage: bash install.sh [--yes] [--app-only]

Installs desktop dependencies and Mluva for the current user. On Omarchy
Quattro, also installs or updates and enables the Mluva shell plugin.
Fedora GNOME compatibility is retained but has not been tested recently.

  --yes       Accept the installation plan without an interactive prompt.
  --app-only  Install the native app without changing shell plugins.

Provider accounts, credentials and optional local models are configured
in Mluva after installation. Run as your normal user; packages may need sudo.
USAGE
}

fail() {
    echo "Mluva setup: $*" >&2
    exit 1
}

for argument in "$@"; do
    case "${argument}" in
        --yes | -y) assume_yes=true ;;
        --app-only) app_only=true ;;
        --help | -h) usage; exit 0 ;;
        *) fail "Unknown option: ${argument}. Use --help." ;;
    esac
done

[[ "$(id -u)" != 0 ]] || fail "Run without sudo; only system package installation needs privileges."
[[ "${MLUVA_INSTALL_HOME:-${HOME}}" == "${HOME}" ]] \
    || fail "For an isolated staged install, use MLUVA_INSTALL_HOME with bash linux/install.sh."
[[ -f "${source_root}/linux/install.sh" ]] || fail "Run this script from a complete Mluva source checkout."

install_plugin=false
plugin_present=false
plugin_dir="${HOME}/.config/omarchy/plugins/mluva.dictation"
plugin_url="https://github.com/1vecera/omarchy-mluva.git"

if command -v omarchy >/dev/null 2>&1; then
    platform=omarchy
    if [[ "${app_only}" == false ]]; then
        omarchy plugin add --help >/dev/null 2>&1 \
            || fail "The widget needs Omarchy Quattro's plugin manager. Use --app-only for the native app."
        install_plugin=true
        if [[ -e "${plugin_dir}" || -L "${plugin_dir}" ]]; then
            [[ ! -L "${plugin_dir}" && -d "${plugin_dir}/.git" ]] \
                || fail "An unmanaged plugin exists at ${plugin_dir}. Back it up, then remove it with omarchy plugin remove mluva.dictation before retrying."
            plugin_origin="$(git -C "${plugin_dir}" remote get-url origin)"
            case "${plugin_origin}" in
                "${plugin_url}" | "${plugin_url%.git}" | git@github.com:1vecera/omarchy-mluva.git) ;;
                *) fail "Preserved a plugin with a different source: ${plugin_dir}. Use --app-only to keep it." ;;
            esac
            [[ -z "$(git -C "${plugin_dir}" status --porcelain)" ]] \
                || fail "Preserved local plugin changes in ${plugin_dir}. Commit or back them up before updating, or use --app-only."
            plugin_present=true
        fi
    fi
elif command -v dnf >/dev/null 2>&1; then
    platform=fedora
else
    fail "Automatic dependency setup is available for Omarchy and Fedora GNOME. See linux/README.md for the runtime requirements."
fi

echo "Install desktop dependencies and the native Mluva app for ${USER:-the current user}."
if [[ "${install_plugin}" == true ]]; then
    echo "Install or update and enable the Omarchy plugin: mluva.dictation."
elif [[ "${platform}" == fedora ]]; then
    echo "Fedora GNOME compatibility has not been tested in recent releases."
fi
if [[ "${assume_yes}" == false ]]; then
    [[ -t 0 ]] || fail "Run in a terminal to review the plan, or pass --yes."
    read -r -p "Continue? [Y/n] " answer
    case "${answer}" in
        "" | y | Y | yes | YES) ;;
        *) echo "Installation cancelled."; exit 0 ;;
    esac
fi

if [[ "${plugin_present}" == true ]]; then
    GIT_TERMINAL_PROMPT=0 git -C "${plugin_dir}" fetch --quiet origin HEAD \
        || fail "Could not check the plugin update. No installation was started; retry when its source is reachable."
    git -C "${plugin_dir}" merge-base --is-ancestor HEAD FETCH_HEAD \
        || fail "The plugin has local commits or a divergent history. Use --app-only to preserve it, or reconcile it before updating. No installation was started."
fi

if [[ "${platform}" == omarchy ]]; then
    omarchy pkg add git uv python python-gobject python-cairo gtk4 libadwaita \
        at-spi2-core gobject-introspection dbus pipewire pipewire-audio wl-clipboard procps-ng
else
    sudo dnf install -y git uv python3-gobject gtk4 libadwaita at-spi2-core \
        gobject-introspection dbus-daemon pipewire-utils wl-clipboard procps-ng
fi

bash "${source_root}/linux/install.sh"

install_widget() {
    if [[ "${plugin_present}" == true ]]; then
        omarchy plugin update mluva.dictation --yes \
            && omarchy plugin enable mluva.dictation
    else
        omarchy plugin add "${plugin_url}" --enable --yes
    fi
}

if [[ "${install_plugin}" == true ]]; then
    if install_widget; then
        echo "Omarchy widget is ready."
    else
        plugin_status=$?
        echo "Mluva's native app is installed, but widget setup failed. The app can be used on its own." >&2
        echo "Resolve the plugin error above, then rerun bash install.sh. Settings and conversations are preserved." >&2
        exit "${plugin_status}"
    fi
fi

echo "Setup complete. Launch ${HOME}/.local/bin/mluva or use the application menu."
echo "In Settings → Providers, choose speech and rewriting and connect the accounts or local models you want to use."
