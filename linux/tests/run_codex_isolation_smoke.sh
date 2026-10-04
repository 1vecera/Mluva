#!/usr/bin/env bash
# Compare the actual installed Codex using native clients and private loopback inference.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
for prerequisite in cargo bwrap codex realpath mktemp; do
    command -v "$prerequisite" >/dev/null || {
        echo "Missing native Codex isolation prerequisite: $prerequisite." >&2
        exit 3
    }
done
CARGO_HOME="$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")"
RUSTUP_HOME="$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")"
CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$project_root/tmp/native-build}")"
export CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR
cd -- "$project_root"
cargo test --locked -p mluva-providers --test codex --no-run

mkdir -p tmp/codex-isolation
run_root="$(mktemp -d "$project_root/tmp/codex-isolation/run.XXXXXX")"
mkdir -m 0700 "$run_root"/{home,config,data,state,cache,runtime,tmp}
env -i PATH="$PATH" HOME="$run_root/home" LC_ALL=C.UTF-8 \
    CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
    CARGO_NET_OFFLINE=true LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" TMPDIR="$run_root/tmp" \
    XDG_CONFIG_HOME="$run_root/config" XDG_DATA_HOME="$run_root/data" \
    XDG_STATE_HOME="$run_root/state" XDG_CACHE_HOME="$run_root/cache" XDG_RUNTIME_DIR="$run_root/runtime" \
    bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
        --tmpfs /tmp --tmpfs /run/dbus --proc /proc -- \
        cargo test --locked -p mluva-providers --test codex installed_codex \
        -- --ignored --test-threads=1 --nocapture | tee "$run_root/receipt.txt"

printf 'Native installed-Codex isolation check passed; evidence: %s\n' "$run_root"
