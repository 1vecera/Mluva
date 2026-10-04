#!/usr/bin/env bash
# Exercise the native portal owner on a private bus without a display or devices.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
for prerequisite in cargo bwrap dbus-run-session realpath mktemp; do
    command -v "$prerequisite" >/dev/null || {
        echo "Missing native shortcut check prerequisite: $prerequisite." >&2
        exit 3
    }
done
# Resolve caller-relative caches before selecting the checkout and private HOME.
CARGO_HOME="$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")"
RUSTUP_HOME="$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")"
CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$project_root/tmp/native-build}")"
export CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR
cd -- "$project_root"
# Prepare all test dependencies before entering the network-isolated session.
cargo test --locked -p mluva-gtk --test global_shortcuts --no-run
evidence_root="${project_root}/tmp/global-shortcut-portal-smoke"
mkdir -p "${evidence_root}"
run_root="$(mktemp -d "${evidence_root}/run.XXXXXX")"
mkdir -p \
    "${run_root}/home" \
    "${run_root}/config" \
    "${run_root}/data" \
    "${run_root}/state" \
    "${run_root}/cache" \
    "${run_root}/runtime"
chmod 0700 \
    "${run_root}" \
    "${run_root}/home" \
    "${run_root}/config" \
    "${run_root}/data" \
    "${run_root}/state" \
    "${run_root}/cache" \
    "${run_root}/runtime"

env -i PATH="$PATH" HOME="${run_root}/home" LC_ALL=C.UTF-8 \
    CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
    CARGO_NET_OFFLINE=true LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
    XDG_CONFIG_HOME="${run_root}/config" XDG_DATA_HOME="${run_root}/data" \
    XDG_STATE_HOME="${run_root}/state" XDG_CACHE_HOME="${run_root}/cache" \
    XDG_RUNTIME_DIR="${run_root}/runtime" OFFSCREEN_SESSION_ROOT="$run_root" \
    MLUVA_HOST_NET_NS="$(readlink /proc/self/ns/net)" \
    bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
        --tmpfs /tmp --tmpfs /usr/share/dbus-1/services --proc /proc -- \
        dbus-run-session -- cargo test --locked -p mluva-gtk --test global_shortcuts \
        -- --ignored --test-threads=1 --nocapture | tee "${run_root}/receipt.txt"

printf 'Native global shortcut portal check passed; evidence: %s\n' "${run_root}"
