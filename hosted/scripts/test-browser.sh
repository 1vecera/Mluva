#!/usr/bin/env bash
# Disposable display, buses, network, devices and browser profiles. Synthetic audio only.
set -euo pipefail
project_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd -- "$project_root"
runner="${MLUVA_OFFSCREEN_RUNNER:-$HOME/.agents/skills/run-offscreen-linux-verification-daniel/scripts/run_isolated_x11.sh}"
if [[ ! -f "$runner" ]]; then runner="$project_root/dev/run-isolated.sh"; fi
exec env -i PATH="$PATH" HOME="$HOME" LC_ALL=C.UTF-8 \
  OFFSCREEN_DISPLAY_NUMBER="${OFFSCREEN_DISPLAY_NUMBER:-278}" \
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
  __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
  --tmpfs /dev/shm --tmpfs /tmp --tmpfs /run/dbus --tmpfs /usr/share/dbus-1/services --proc /proc -- \
  bash "$runner" tmp/hosted-browser -- node hosted/tests/browser.mjs
