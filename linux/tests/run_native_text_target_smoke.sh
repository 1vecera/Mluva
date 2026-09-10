#!/usr/bin/env bash
set -euo pipefail

run_private_session() {
    local artifact_dir=$1
    local launcher_log="${artifact_dir}/atspi-bus-launcher.log"
    local registry_log="${artifact_dir}/atspi-registry.log"
    launcher_pid=""
    registry_pid=""

    cleanup_accessibility() {
        if [[ -n "${registry_pid}" ]] && kill -0 "${registry_pid}" 2>/dev/null; then
            kill -TERM -- "${registry_pid}" 2>/dev/null || true
            wait "${registry_pid}" 2>/dev/null || true
        fi
        if [[ -n "${launcher_pid}" ]] && kill -0 "${launcher_pid}" 2>/dev/null; then
            kill -TERM -- "${launcher_pid}" 2>/dev/null || true
            wait "${launcher_pid}" 2>/dev/null || true
        fi
    }
    trap cleanup_accessibility EXIT

    gsettings set org.gnome.desktop.interface toolkit-accessibility true
    "${ATSPI_LIBEXEC}/at-spi-bus-launcher" --launch-immediately --a11y=1 >"${launcher_log}" 2>&1 &
    launcher_pid=$!

    local accessibility_address=""
    local address_result=""
    for _attempt in $(seq 1 100); do
        address_result="$(
            gdbus call \
                --session \
                --dest org.a11y.Bus \
                --object-path /org/a11y/bus \
                --method org.a11y.Bus.GetAddress \
                2>/dev/null || true
        )"
        accessibility_address="$(sed -n "s/^('\\([^']*\\)',)$/\\1/p" <<<"${address_result}")"
        if [[ "${accessibility_address}" == unix:* ]]; then
            break
        fi
        sleep 0.05
    done
    if [[ "${accessibility_address}" != unix:* ]]; then
        echo "The private AT-SPI broker did not publish an address; inspect ${launcher_log}." >&2
        exit 1
    fi

    local accessibility_bus_ready=false
    for _attempt in $(seq 1 100); do
        if gdbus call \
            --address "${accessibility_address}" \
            --dest org.freedesktop.DBus \
            --object-path /org/freedesktop/DBus \
            --method org.freedesktop.DBus.ListNames \
            >/dev/null 2>&1; then
            accessibility_bus_ready=true
            break
        fi
        sleep 0.05
    done
    if [[ "${accessibility_bus_ready}" != "true" ]]; then
        echo "The private AT-SPI broker published an address but did not accept connections; inspect ${launcher_log}." >&2
        exit 1
    fi

    export AT_SPI_BUS_ADDRESS="${accessibility_address}"
    "${ATSPI_LIBEXEC}/at-spi2-registryd" --use-gnome-session >"${registry_log}" 2>&1 &
    registry_pid=$!
    local registry_ready=false
    for _attempt in $(seq 1 100); do
        if gdbus call \
            --address "${AT_SPI_BUS_ADDRESS}" \
            --dest org.a11y.atspi.Registry \
            --object-path /org/a11y/atspi/cache \
            --method org.a11y.atspi.Cache.GetItems \
            >/dev/null 2>&1; then
            registry_ready=true
            break
        fi
        sleep 0.05
    done
    if [[ "${registry_ready}" != "true" ]]; then
        echo "The private AT-SPI registry did not expose its cache; inspect ${registry_log}." >&2
        exit 1
    fi

    cd "${LINUX_ROOT}"
    if [[ "${MLUVA_SMOKE:-}" == conversation ]]; then
        local scenario width height scenario_dir
        for specification in conversation:1060:780 lifecycle:1060:780 rewrite-settings-lifecycle:1060:780 \
            rewrite-models:420:520 rewrite-models-error:480:640 scrollbars:480:640 scrollbars-dark:1060:780 empty:480:640 \
            recording:420:520 processing:480:640 rewriting:480:640 error:480:640 incognito:480:640 dark:1060:780; do
            IFS=: read -r scenario width height <<<"${specification}"
            scenario_dir="${artifact_dir}/${scenario}"
            mkdir -p "${scenario_dir}/config" "${scenario_dir}/data"
            OFFSCREEN_SESSION_ROOT="${artifact_dir}/session" OFFSCREEN_ARTIFACT_DIR="${scenario_dir}" \
                XDG_CONFIG_HOME="${scenario_dir}/config" XDG_DATA_HOME="${scenario_dir}/data" \
                PYTHONPATH=. GTK_A11Y=atspi ADW_DISABLE_PORTAL=1 GSK_RENDERER=cairo \
                MLUVA_DISABLE_GLOBAL_SHORTCUT=1 MLUVA_UI_SCENARIO="${scenario}" \
                MLUVA_UI_WIDTH="${width}" MLUVA_UI_HEIGHT="${height}" \
                uv run --locked python tests/conversation_ui_smoke.py
        done
    elif [[ "${MLUVA_SMOKE:-}" == providers ]]; then
        OFFSCREEN_SESSION_ROOT="${artifact_dir}/session" OFFSCREEN_ARTIFACT_DIR="${artifact_dir}" \
            PYTHONPATH=.:tests GTK_A11Y=atspi ADW_DISABLE_PORTAL=1 GSK_RENDERER=cairo \
            MLUVA_DISABLE_GLOBAL_SHORTCUT=1 \
            uv run --locked python tests/provider_settings_smoke.py
    elif [[ "${MLUVA_SMOKE:-}" == live ]]; then
        OFFSCREEN_SESSION_ROOT="${artifact_dir}/session" OFFSCREEN_ARTIFACT_DIR="${artifact_dir}" \
            PYTHONPATH=.:tests GTK_A11Y=atspi ADW_DISABLE_PORTAL=1 GSK_RENDERER=cairo \
            MLUVA_DISABLE_GLOBAL_SHORTCUT=1 \
            uv run --locked python tests/live_workspace_smoke.py
    else
        OFFSCREEN_ARTIFACT_DIR="${artifact_dir}" PYTHONPATH=. GTK_A11Y=atspi \
            uv run --locked python tests/native_text_target_smoke.py
    fi

    cleanup_accessibility
    trap - EXIT
}

script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
script_dir="$(cd -- "$(dirname -- "${script_path}")" && pwd -P)"
LINUX_ROOT="$(cd -- "${script_dir}/.." && pwd -P)"
project_root="$(cd -- "${LINUX_ROOT}/.." && pwd -P)"
export LINUX_ROOT
ATSPI_LIBEXEC=/usr/libexec
if [[ ! -x "${ATSPI_LIBEXEC}/at-spi-bus-launcher" && -x /usr/lib/at-spi-bus-launcher ]]; then
    ATSPI_LIBEXEC=/usr/lib
fi
export ATSPI_LIBEXEC

if [[ "${1:-}" == "--private-session" ]]; then
    [[ $# -eq 2 ]] || {
        echo "Private session usage: ${script_path} --private-session <artifact-dir>" >&2
        exit 2
    }
    run_private_session "$2"
    exit
fi

output_argument="${1:-${project_root}/tmp/native-text-target-smoke}"
mkdir -p -- "${output_argument}"
output_dir="$(realpath -- "${output_argument}")"
case "${output_dir}" in
    "${project_root}/tmp" | "${project_root}/tmp/"*) ;;
    *)
        echo "Evidence must stay below ${project_root}/tmp/." >&2
        exit 2
        ;;
esac

for command_name in dbus-run-session gdbus gsettings import realpath seq uv xvfb-run; do
    command -v "${command_name}" >/dev/null 2>&1 || {
        echo "Missing native text-target smoke dependency: ${command_name}" >&2
        exit 1
    }
done
[[ -x "${LINUX_ROOT}/.venv/bin/python" ]] || {
    echo "Missing the prepared Linux environment; run make linux-setup first." >&2
    exit 1
}
for executable_path in "${ATSPI_LIBEXEC}/at-spi-bus-launcher" "${ATSPI_LIBEXEC}/at-spi2-registryd"; do
    [[ -x "${executable_path}" ]] || {
        echo "Missing native text-target smoke dependency: ${executable_path}" >&2
        exit 1
    }
done

artifact_dir="$(mktemp -d "${output_dir}/run.XXXXXX")"
session_root="${artifact_dir}/session"
runtime_dir="$(mktemp -d /tmp/mluva-target-runtime.XXXXXX)"
mkdir -p -- \
    "${session_root}/home" \
    "${session_root}/config" \
    "${session_root}/data" \
    "${session_root}/state" \
    "${session_root}/cache" \
    "${session_root}/tmp"
chmod 0700 "${session_root}"/* "${runtime_dir}"

cleanup_runtime() {
    rm -r -- "${runtime_dir}"
}
trap cleanup_runtime EXIT

export HOME="${session_root}/home"
export XDG_CONFIG_HOME="${session_root}/config"
export XDG_DATA_HOME="${session_root}/data"
export XDG_STATE_HOME="${session_root}/state"
export XDG_CACHE_HOME="${session_root}/cache"
export XDG_RUNTIME_DIR="${runtime_dir}"
export TMPDIR="${session_root}/tmp"
export GDK_BACKEND=x11
export GIO_USE_VFS=local
unset DISPLAY WAYLAND_DISPLAY AT_SPI_BUS_ADDRESS DBUS_SESSION_BUS_ADDRESS XAUTHORITY

display_args=(-a)
if [[ -n "${OFFSCREEN_DISPLAY_NUMBER:-}" ]]; then
    [[ "${OFFSCREEN_DISPLAY_NUMBER}" =~ ^[0-9]+$ ]] || {
        echo "OFFSCREEN_DISPLAY_NUMBER must be a display number." >&2
        exit 2
    }
    display_args=(-n "${OFFSCREEN_DISPLAY_NUMBER}")
    # Some xvfb-run versions return immediately after SIGTERM. Allow the prior
    # isolated run to release this reserved display; never remove another server's lock.
    for _attempt in $(seq 1 100); do
        if [[ ! -e "/tmp/.X${OFFSCREEN_DISPLAY_NUMBER}-lock" \
            && ! -e "/tmp/.X11-unix/X${OFFSCREEN_DISPLAY_NUMBER}" ]]; then
            break
        fi
        sleep 0.05
    done
    if [[ -e "/tmp/.X${OFFSCREEN_DISPLAY_NUMBER}-lock" \
        || -e "/tmp/.X11-unix/X${OFFSCREEN_DISPLAY_NUMBER}" ]]; then
        echo "Reserved Xvfb display is still in use; choose another unused display." >&2
        exit 1
    fi
fi
xvfb-run "${display_args[@]}" -e "${artifact_dir}/xvfb.log" -s "-screen 0 1280x900x24" \
    dbus-run-session -- "${script_path}" --private-session "${artifact_dir}"

printf 'Native text-target smoke passed; evidence: %s\n' "${artifact_dir}"

cleanup_runtime
trap - EXIT
