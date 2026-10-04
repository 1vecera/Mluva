#!/usr/bin/env bash
# Observe native publisher/bridge/QML only on an isolated desktop and private bus.
set -euo pipefail

if [[ "${1:-}" == --inside-session ]]; then
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    test "$(readlink /proc/self/ns/net)" != "$MLUVA_HOST_NET_NS"
    mkdir -m 0700 "$OFFSCREEN_SESSION_ROOT/home"
    export HOME="$OFFSCREEN_SESSION_ROOT/home"
    selection=()
    if [[ "${MLUVA_PANEL_REPLAY:-0}" == 1 ]]; then selection=(released_preview_contraction_replay); fi
    exec cargo test --locked -p mluva-gtk --test omarchy_widget "${selection[@]}" -- \
        --ignored --test-threads=1 --nocapture
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
for prerequisite in cargo bwrap dbus-run-session xvfb-run xdotool quickshell import ffmpeg ffprobe cp realpath mktemp; do
    command -v "$prerequisite" >/dev/null || {
        echo "Missing native widget check prerequisite: $prerequisite." >&2
        exit 3
    }
done
test -d /usr/share/omarchy/shell/Commons
test -d /usr/share/omarchy/shell/Ui
CARGO_HOME="$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")"
RUSTUP_HOME="$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")"
CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$project_root/tmp/native-build}")"
export CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR
cd -- "$project_root"
cargo test --locked -p mluva-gtk --test omarchy_widget --no-run
bash linux/mluva-shell --help >/dev/null
mkdir -p tmp/omarchy-widget
evidence="$(mktemp -d "$project_root/tmp/omarchy-widget/run.XXXXXX")"
exec env -i PATH="$PATH" HOME="$HOME" LC_ALL=C.UTF-8 \
    CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" CARGO_NET_OFFLINE=true \
    LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" XDG_DATA_DIRS="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
    OFFSCREEN_ENABLE_ATSPI=1 OFFSCREEN_DISPLAY_NUMBER="${OFFSCREEN_DISPLAY_NUMBER:-174}" \
    MLUVA_PANEL_REPLAY="${MLUVA_PANEL_REPLAY:-0}" MLUVA_HOST_NET_NS="$(readlink /proc/self/ns/net)" \
    GDK_SCALE=1 GDK_DPI_SCALE=1 GSK_RENDERER=cairo GTK_A11Y=atspi ATSPI_DISABLE_P2P=1 \
    __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
    __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
    bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
        --tmpfs /tmp --tmpfs /usr/share/dbus-1/services --tmpfs /run/dbus --proc /proc -- \
        bash "$project_root/dev/run-isolated.sh" "$evidence" -- \
        bash "$project_root/linux/tests/run_omarchy_widget_smoke.sh" --inside-session
