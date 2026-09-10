#!/usr/bin/env bash
# Start one X11 application below a virtual display and private session bus.
# Isolate display, session bus and state; traps own their exact child PIDs.
# shellcheck disable=SC2329
set -euo pipefail

run_inside_private_session() {
    if [[ "${OFFSCREEN_ENABLE_ATSPI:-0}" != "1" ]]; then
        exec "$@"
    fi

    for command_name in dbus-daemon dbus-update-activation-environment gdbus; do
        command -v "${command_name}" >/dev/null 2>&1 || {
            echo "Missing required private AT-SPI command: ${command_name}" >&2
            exit 3
        }
    done

    accessibility_config="${OFFSCREEN_ATSPI_CONFIG:-/usr/share/defaults/at-spi2/accessibility.conf}"
    if [[ ! -f "${accessibility_config}" ]]; then
        echo "Missing private AT-SPI configuration: ${accessibility_config}" >&2
        exit 3
    fi

    accessibility_registry_command=()
    if [[ -n "${OFFSCREEN_ATSPI_REGISTRY:-}" ]]; then
        accessibility_registry_command=("${OFFSCREEN_ATSPI_REGISTRY}" --use-gnome-session)
    else
        accessibility_registry_service="${OFFSCREEN_ATSPI_REGISTRY_SERVICE:-/usr/share/dbus-1/accessibility-services/org.a11y.atspi.Registry.service}"
        if [[ ! -f "${accessibility_registry_service}" ]]; then
            echo "Missing private AT-SPI registry service: ${accessibility_registry_service}" >&2
            exit 3
        fi
        while IFS= read -r service_line; do
            if [[ "${service_line}" == Exec=* ]]; then
                read -r -a accessibility_registry_command <<< "${service_line#Exec=}"
                break
            fi
        done < "${accessibility_registry_service}"
    fi
    if (( ${#accessibility_registry_command[@]} == 0 )) \
        || [[ ! -x "${accessibility_registry_command[0]}" ]]; then
        echo "Missing executable private AT-SPI registry command." >&2
        exit 3
    fi

    session_token="${OFFSCREEN_SESSION_ROOT##*/}"
    accessibility_address="unix:abstract=offscreen-atspi-${session_token}-$$"

    mapfile -t accessibility_details < <(
        dbus-daemon \
            --config-file="${accessibility_config}" \
            --address="${accessibility_address}" \
            --fork \
            --print-address=1 \
            --print-pid=1
    )
    if (( ${#accessibility_details[@]} != 2 )) \
        || [[ "${accessibility_details[0]}" != "${accessibility_address}"* ]] \
        || [[ ! "${accessibility_details[1]}" =~ ^[0-9]+$ ]]; then
        echo "Private AT-SPI bus returned an unusable address or PID." >&2
        exit 5
    fi

    export AT_SPI_BUS_ADDRESS="${accessibility_details[0]}"
    accessibility_pid="${accessibility_details[1]}"
    accessibility_registry_pid=""
    stop_accessibility_stack() {
        if [[ -n "${accessibility_registry_pid}" ]]; then
            kill -TERM -- "${accessibility_registry_pid}" 2>/dev/null || true
            wait "${accessibility_registry_pid}" 2>/dev/null || true
        fi
        kill -TERM -- "${accessibility_pid}" 2>/dev/null || true
    }
    trap stop_accessibility_stack EXIT

    dbus-update-activation-environment AT_SPI_BUS_ADDRESS >/dev/null
    accessibility_registry_log="${OFFSCREEN_SESSION_ROOT}/atspi-registry.log"
    "${accessibility_registry_command[@]}" \
        >"${accessibility_registry_log}" 2>&1 &
    accessibility_registry_pid=$!

    accessibility_registry_ready=0
    for _attempt in {1..100}; do
        if ! kill -0 "${accessibility_registry_pid}" 2>/dev/null; then
            break
        fi
        registry_owner="$(
            gdbus call \
                --address "${AT_SPI_BUS_ADDRESS}" \
                --dest org.freedesktop.DBus \
                --object-path /org/freedesktop/DBus \
                --method org.freedesktop.DBus.NameHasOwner \
                org.a11y.atspi.Registry \
                2>/dev/null || true
        )"
        if [[ "${registry_owner}" == "(true,)" ]]; then
            if gdbus call \
                --address "${AT_SPI_BUS_ADDRESS}" \
                --dest org.a11y.atspi.Registry \
                --object-path /org/a11y/atspi/cache \
                --method org.a11y.atspi.Cache.GetItems \
                >/dev/null 2>&1; then
                accessibility_registry_ready=1
                break
            fi
        fi
        sleep 0.05
    done
    if [[ "${accessibility_registry_ready}" != "1" ]]; then
        echo "Private AT-SPI registry did not acquire org.a11y.atspi.Registry and expose its cache; inspect ${accessibility_registry_log}." >&2
        exit 5
    fi
    printf 'offscreen_atspi=private\n'
    printf 'offscreen_atspi_registry=private\n'

    set +e
    "$@"
    command_status=$?
    set -e
    exit "${command_status}"
}

if [[ "${1:-}" == "--offscreen-private-session" ]]; then
    shift
    if (( $# == 0 )); then
        echo "Missing command for private off-screen session." >&2
        exit 2
    fi
    run_inside_private_session "$@"
fi

usage() {
    echo "Usage: dev/run-isolated.sh <[cwd]/tmp/evidence-dir> -- <command> [args...]" >&2
}

if (( $# < 3 )) || [[ "$2" != "--" ]]; then
    usage
    exit 2
fi

evidence_argument="$1"
shift 2

command -v realpath >/dev/null 2>&1 || {
    echo "Missing required off-screen command: realpath" >&2
    exit 3
}

worktree_root="$(pwd -P)"
if [[ "${worktree_root}" == "/" ]]; then
    echo "Refusing to create off-screen evidence from the filesystem root." >&2
    exit 4
fi

evidence_dir="$(realpath -m -- "${evidence_argument}")"
case "${evidence_dir}" in
    "${worktree_root}/tmp" | "${worktree_root}/tmp/"*) ;;
    *)
        echo "Evidence must stay below ${worktree_root}/tmp/." >&2
        exit 4
        ;;
esac

for command_name in dbus-run-session xvfb-run mktemp ln unlink rmdir; do
    command -v "${command_name}" >/dev/null 2>&1 || {
        echo "Missing required off-screen command: ${command_name}" >&2
        exit 3
    }
done

mkdir -p -- "${evidence_dir}"

session_root="$(mktemp -d "${evidence_dir}/session.XXXXXX")"
mkdir -p -- \
    "${session_root}/config" \
    "${session_root}/data" \
    "${session_root}/state" \
    "${session_root}/cache" \
    "${session_root}/runtime" \
    "${session_root}/tmp"
chmod 0700 \
    "${session_root}" \
    "${session_root}/config" \
    "${session_root}/data" \
    "${session_root}/state" \
    "${session_root}/cache" \
    "${session_root}/runtime" \
    "${session_root}/tmp"

export OFFSCREEN_ARTIFACT_DIR="${evidence_dir}"
export OFFSCREEN_SESSION_ROOT="${session_root}"
export XDG_CONFIG_HOME="${session_root}/config"
export XDG_DATA_HOME="${session_root}/data"
export XDG_STATE_HOME="${session_root}/state"
export XDG_CACHE_HOME="${session_root}/cache"
export TMPDIR="${session_root}/tmp"
export GDK_BACKEND=x11
export GIO_USE_VFS=local
unset DISPLAY WAYLAND_DISPLAY HYPRLAND_INSTANCE_SIGNATURE AT_SPI_BUS_ADDRESS DBUS_SESSION_BUS_ADDRESS XAUTHORITY

runtime_alias_root="$(mktemp -d "/tmp/offscreen-xdg.XXXXXX")"
runtime_alias_path="${runtime_alias_root}/runtime"
cleanup_runtime_alias() {
    if [[ -L "${runtime_alias_path}" ]]; then
        unlink -- "${runtime_alias_path}"
    fi
    rmdir -- "${runtime_alias_root}" 2>/dev/null || true
}
trap cleanup_runtime_alias EXIT
ln -s -- "${session_root}/runtime" "${runtime_alias_path}"
export XDG_RUNTIME_DIR="${runtime_alias_path}"

screen_spec="${OFFSCREEN_SCREEN_SPEC:-1280x900x24}"
runner_path="$(realpath -- "$0")"
xvfb_arguments=(-a)
display_allocation="automatic"
if [[ -n "${OFFSCREEN_DISPLAY_NUMBER:-}" ]]; then
    xvfb_arguments=(-n "${OFFSCREEN_DISPLAY_NUMBER}")
    display_allocation="explicit:${OFFSCREEN_DISPLAY_NUMBER}"
fi
printf 'offscreen_session=%s\n' "${session_root}"
printf 'offscreen_evidence=%s\n' "${evidence_dir}"
printf 'offscreen_display_allocation=%s\n' "${display_allocation}"
printf 'offscreen_xdg_runtime=short-alias\n'

set +e
xvfb-run "${xvfb_arguments[@]}" -s "-screen 0 ${screen_spec}" \
    dbus-run-session -- \
    "${runner_path}" --offscreen-private-session "$@"
command_status=$?
set -e
exit "${command_status}"
