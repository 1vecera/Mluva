#!/usr/bin/env bash
# Real browser/clipboard/input checks only inside private display, network, PID and device boundaries.
set -euo pipefail

if [[ "${1:-}" == "--inside-session" ]]; then
    shift
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    test "$(readlink /proc/self/ns/net)" != "$MLUVA_HOST_NET_NS"
    test "${XDG_SESSION_TYPE:-}" = x11
    test -z "${WAYLAND_DISPLAY:-}${HYPRLAND_INSTANCE_SIGNATURE:-}"
    for MLUVA_TASK_DEVICE in /dev/uinput /dev/input /dev/snd /dev/dri; do
        test ! -e "$MLUVA_TASK_DEVICE"
    done
    mkdir -m 0700 "$OFFSCREEN_SESSION_ROOT/home"
    export HOME="$OFFSCREEN_SESSION_ROOT/home"
    cat > "$OFFSCREEN_SESSION_ROOT/openbox.xml" <<'XML'
<?xml version="1.0"?>
<openbox_config xmlns="http://openbox.org/3.4/rc">
  <focus><focusNew>yes</focusNew><followMouse>no</followMouse></focus>
  <theme><name>Clearlooks</name></theme>
  <desktops><number>1</number></desktops>
</openbox_config>
XML
    openbox --sm-disable --config-file "$OFFSCREEN_SESSION_ROOT/openbox.xml" > "$OFFSCREEN_SESSION_ROOT/openbox.log" 2>&1 &
    MLUVA_PRIVATE_WM_PID=$!
    export MLUVA_PRIVATE_WM_PID
    trap 'kill "$MLUVA_PRIVATE_WM_PID" 2>/dev/null || true; wait "$MLUVA_PRIVATE_WM_PID" 2>/dev/null || true' EXIT
    MLUVA_TASK_WM_READY=0
    for _mluva_task_attempt in {1..100}; do
        kill -0 "$MLUVA_PRIVATE_WM_PID"
        if xprop -root _NET_SUPPORTING_WM_CHECK | rg -q 'window id'; then
            MLUVA_TASK_WM_READY=1
            break
        fi
        sleep 0.02
    done
    test "$MLUVA_TASK_WM_READY" = 1
    printf 'offscreen_window_manager=openbox\n'
    "$@"
    exit "$?"
fi

if (( $# < 3 )) || [[ "$2" != "--" ]]; then
    echo "Usage: dev/run-isolated-browser.sh <[cwd]/tmp/evidence-dir> -- <command> [args...]" >&2
    exit 2
fi
MLUVA_TASK_EVIDENCE="$1"
shift 2
for MLUVA_TASK_COMMAND in bwrap firefox openbox xdotool xclip xprop rg; do
    command -v "$MLUVA_TASK_COMMAND" >/dev/null
done
MLUVA_TASK_ROOT="$(pwd -P)"
MLUVA_TASK_RUNNER="$MLUVA_TASK_ROOT/dev/run-isolated.sh"
MLUVA_TASK_BROWSER_RUNNER="$MLUVA_TASK_ROOT/dev/run-isolated-browser.sh"
MLUVA_TASK_HOST_NET_NS="$(readlink /proc/self/ns/net)"
exec env -i \
    PATH="$PATH" HOME="$HOME" LC_ALL=C.UTF-8 \
    CARGO_HOME="${CARGO_HOME:-}" RUSTUP_HOME="${RUSTUP_HOME:-}" CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-}" \
    LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" XDG_DATA_DIRS="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
    MLUVA_HOST_NET_NS="$MLUVA_TASK_HOST_NET_NS" \
    OFFSCREEN_ENABLE_ATSPI=1 OFFSCREEN_DISPLAY_NUMBER="${OFFSCREEN_DISPLAY_NUMBER:-174}" \
    XDG_CURRENT_DESKTOP=offscreen GDK_SCALE=1 GDK_DPI_SCALE=1 GSK_RENDERER=cairo \
    GTK_A11Y=atspi ATSPI_DISABLE_P2P=1 GDK_DEBUG=no-portals GTK_USE_PORTAL=0 ADW_DISABLE_PORTAL=1 MOZ_ENABLE_WAYLAND=0 \
    __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
    __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
        --tmpfs /tmp --tmpfs /usr/share/dbus-1/services --proc /proc -- \
    bash "$MLUVA_TASK_RUNNER" "$MLUVA_TASK_EVIDENCE" -- \
    bash "$MLUVA_TASK_BROWSER_RUNNER" --inside-session "$@"
