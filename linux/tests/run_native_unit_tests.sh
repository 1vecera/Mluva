#!/usr/bin/env bash
# Keep unit-test discovery away from the user's sessions and desktop.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
mkdir -p "$root/tmp"
private="$(mktemp -d "$root/tmp/native-tests.XXXXXXXX")"
trap 'rm -rf -- "$private"' EXIT
mkdir -p "$private"/{home,config,data,runtime}
chmod 700 "$private/runtime"
cd "$root"
env -i PATH="$PATH" HOME="$private/home" CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/tmp/native-build}" LANG=C.UTF-8 \
    XDG_CONFIG_HOME="$private/config" XDG_DATA_HOME="$private/data" \
    XDG_RUNTIME_DIR="$private/runtime" TMPDIR="$root/tmp" \
    cargo test --locked --workspace
