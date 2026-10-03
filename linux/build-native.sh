#!/usr/bin/env bash
# Build the production Rust executables and assemble one fresh native bundle.
set -euo pipefail

source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if (( $# != 1 )) || [[ "$1" != /* || "$1" == / ]]; then
    echo "Usage: bash linux/build-native.sh ABSOLUTE_OUTPUT_DIRECTORY" >&2
    exit 2
fi
output_dir=$1
if [[ -e "$output_dir" || -L "$output_dir" ]]; then
    echo "The native build output already exists; it was left untouched." >&2
    exit 1
fi
for prerequisite in cargo cc pkg-config realpath; do
    command -v "$prerequisite" >/dev/null || {
        echo "Missing native build prerequisite: $prerequisite. See CONTRIBUTING.md." >&2
        exit 3
    }
done

# Preserve caller-relative Cargo locations before selecting the source checkout's
# pinned toolchain. Build products stay outside the maintained source tree.
target_dir="$(realpath -m -- "${CARGO_TARGET_DIR:-$source_root/tmp/native-build}")"
if [[ -n "${CARGO_HOME:-}" ]]; then
    CARGO_HOME="$(realpath -m -- "$CARGO_HOME")"
    export CARGO_HOME
fi
if [[ -n "${RUSTUP_HOME:-}" ]]; then
    RUSTUP_HOME="$(realpath -m -- "$RUSTUP_HOME")"
    export RUSTUP_HOME
fi
cd -- "$source_root"
cargo build --locked --release --workspace --target-dir "$target_dir" \
    --bin mluva --bin mluva-shell --bin mluva-narrate --bin mluva-asr-worker \
    --bin mluva-audio-cleanup --bin mluva-install-widget --bin mluva-screenshot-editor \
    --bin mluva-uninstall --bin mluva-install --bin mluva-package
# Cargo selects the executable directory, including a configured build target.
# The builder uses its own siblings, never stale files in a guessed cache path.
cargo run --locked --release -p mluva-install --bin mluva-package \
    --target-dir "$target_dir" -- "$source_root" "$output_dir"
