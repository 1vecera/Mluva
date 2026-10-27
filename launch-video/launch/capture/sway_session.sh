#!/usr/bin/env bash
# Run one command inside a private headless sway, for takes that need a real compositor: the
# see-through session wheel is a layer-shell overlay, and only a compositor shows the desktop
# behind it, as Hyprland does on Omarchy.
#
#   sway_session.sh EVIDENCE_DIR SWAY_CONFIG -- CMD...
#
# The session runs in bwrap with its own PID and network namespaces (loopback only), a tmpfs
# /home, /tmp and /run, a private HOME, XDG runtime directory and D-Bus session bus with no service
# activation, and sway's headless backend with the pixman renderer: no seat, input device, GPU,
# host display or host bus. The checkout is read-only; EVIDENCE_DIR is the only writable path.
# CMD runs with WAYLAND_DISPLAY and SWAYSOCK set; when it exits, the session is torn down.
set -euo pipefail

if [[ ${1:-} == --inside ]]; then
    shift
    evidence=$1 config=$2
    shift 3
    export HOME=/home/take XDG_RUNTIME_DIR=/run/user/take
    export XDG_CONFIG_HOME=$HOME/.config XDG_DATA_HOME=$HOME/.local/share XDG_STATE_HOME=$HOME/.local/state XDG_CACHE_HOME=$HOME/.cache
    mkdir -p "$HOME" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
    mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
    [[ "$(readlink /proc/self/ns/net)" != "$TAKE_HOST_NET_NS" ]] || { echo "network namespace not private" >&2; exit 3; }
    cat > "$XDG_RUNTIME_DIR/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path=$XDG_RUNTIME_DIR/bus</listen>
  <auth>EXTERNAL</auth>
  <policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy>
</busconfig>
EOF
    dbus-daemon --nofork --nopidfile --config-file="$XDG_RUNTIME_DIR/bus.conf" > "$evidence/dbus.log" 2>&1 &
    for _ in {1..100}; do [[ -S $XDG_RUNTIME_DIR/bus ]] && break; sleep 0.05; done
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1 \
        LIBSEAT_BACKEND=seatd SEATD_SOCK="$XDG_RUNTIME_DIR/no-seatd.sock" \
        sway -c "$config" > "$evidence/sway.log" 2>&1 &
    sway_pid=$!
    for _ in {1..100}; do
        WAYLAND_DISPLAY=$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'wayland-[0-9]' -printf '%f\n' | head -1)
        SWAYSOCK=$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'sway-ipc.*.sock' | head -1)
        [[ -n $WAYLAND_DISPLAY && -n $SWAYSOCK ]] && break
        sleep 0.1
    done
    [[ -n ${WAYLAND_DISPLAY:-} && -n ${SWAYSOCK:-} ]] || { echo "sway did not start" >&2; tail -20 "$evidence/sway.log" >&2; exit 4; }
    export WAYLAND_DISPLAY SWAYSOCK GDK_BACKEND=wayland XDG_CURRENT_DESKTOP=sway XDG_SESSION_TYPE=wayland
    status=0
    "$@" || status=$?
    swaymsg exit > /dev/null 2>&1 || kill "$sway_pid" 2>/dev/null || true
    wait "$sway_pid" 2>/dev/null || true
    exit "$status"
fi

evidence=$(realpath -m "$1") config=$(realpath "$2")
[[ ${3:-} == -- ]] || { sed -n '6p' "$0" >&2; exit 2; }
shift 3
checkout=$(cd "$(dirname "$0")/../../.." && pwd -P)
[[ ! -e $evidence ]] || { echo "evidence directory exists: $evidence" >&2; exit 2; }
mkdir -p "$evidence"
# Take settings pass through; nothing else from the calling environment does.
mapfile -t take_settings < <(env | grep '^MLUVA_TAKE_' || true)
exec env -i "${take_settings[@]}" PATH=/usr/bin:/bin LANG=C.UTF-8 LC_ALL=C.UTF-8 TERM=xterm-256color \
    TAKE_HOST_NET_NS="$(readlink /proc/self/ns/net)" TAKE_CHECKOUT="$checkout" TAKE_EVIDENCE="$evidence" \
    LD_LIBRARY_PATH="${TAKE_LD_LIBRARY_PATH:-}" \
    __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    GSK_RENDERER=cairo GDK_DEBUG=no-portals GTK_USE_PORTAL=0 ADW_DISABLE_PORTAL=1 \
    bwrap --die-with-parent --unshare-pid --unshare-net --ro-bind / / \
        --tmpfs /home --tmpfs /root --ro-bind "$checkout" "$checkout" --bind "$evidence" "$evidence" \
        --dev /dev --tmpfs /run --tmpfs /tmp --proc /proc -- \
    bash "$0" --inside "$evidence" "$config" -- "$@"
