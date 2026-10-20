#!/usr/bin/env bash
# Real Mluva -> guarded Herdr -> native Codex, inside the private application desktop.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
[[ $# == 1 && "$1" == /* ]] || { echo 'Usage: linux/tests/run_agent_end_to_end.sh /absolute/patched/herdr' >&2; exit 2; }
for tool in codex claude uv; do command -v "$tool" >/dev/null; done
mkdir -p "$root/tmp"
tools="$(mktemp -d "$root/tmp/agent-e2e-tools.XXXXXX")"
install -m 0755 "$(realpath -- "$1")" "$tools/herdr"
for tool in codex claude; do install -m 0755 "$(realpath -- "$(command -v "$tool")")" "$tools/$tool"; done
# The copied CLIs stay inside the checkout, which the private runner exposes read-only.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/tmp/native-build}"
cargo build --locked -p mluva-gtk --bin mluva --bin global-shortcut-fixture-peer --example mluva_verify \
    -p mluva-audio --bin audio-fixture-peer --bin mluva-audio-cleanup
bash "$root/dev/verify.sh" session "$tools/evidence" -- bash "$root/linux/tests/support/agent_end_to_end.sh" "$tools" "$CARGO_TARGET_DIR"
