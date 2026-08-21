#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
evidence_root="${project_root}/tmp/global-shortcut-portal-smoke"
mkdir -p "${evidence_root}"
run_root="$(mktemp -d "${evidence_root}/run.XXXXXX")"
mkdir -p \
    "${run_root}/config" \
    "${run_root}/data" \
    "${run_root}/state" \
    "${run_root}/cache" \
    "${run_root}/runtime"
chmod 0700 \
    "${run_root}" \
    "${run_root}/config" \
    "${run_root}/data" \
    "${run_root}/state" \
    "${run_root}/cache" \
    "${run_root}/runtime"

unset DISPLAY WAYLAND_DISPLAY AT_SPI_BUS_ADDRESS DBUS_SESSION_BUS_ADDRESS XAUTHORITY
(
    cd "${project_root}/linux"
    XDG_CONFIG_HOME="${run_root}/config" \
    XDG_DATA_HOME="${run_root}/data" \
    XDG_STATE_HOME="${run_root}/state" \
    XDG_CACHE_HOME="${run_root}/cache" \
    XDG_RUNTIME_DIR="${run_root}/runtime" \
    PYTHONPATH=. \
    dbus-run-session -- \
        uv run --locked python tests/global_shortcut_portal_smoke.py
) | tee "${run_root}/receipt.txt"

printf 'Global shortcut portal smoke passed; evidence: %s\n' "${run_root}"
