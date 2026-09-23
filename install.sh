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

if command -v omarchy >/dev/null 2>&1; then
    platform=omarchy
    if [[ "${app_only}" == false ]]; then
        omarchy plugin enable --help >/dev/null 2>&1 \
            || fail "The widget needs Omarchy Quattro's plugin manager. Use --app-only for the native app."
        install_plugin=true
        python3 "${source_root}/linux/install_widget.py" --check
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

if [[ "${platform}" == omarchy ]]; then
    omarchy pkg add git uv python python-gobject python-cairo gtk4 libadwaita \
        at-spi2-core gobject-introspection dbus pipewire pipewire-audio wl-clipboard procps-ng webkitgtk-6.0 bubblewrap
else
    sudo dnf install -y git uv python3-gobject gtk4 libadwaita at-spi2-core \
        gobject-introspection dbus-daemon pipewire-utils wl-clipboard procps-ng webkitgtk6.0 bubblewrap
fi

bash "${source_root}/linux/install.sh"

if [[ "${install_plugin}" == true ]]; then
    if python3 "${source_root}/linux/install_widget.py"; then
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
