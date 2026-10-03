#!/usr/bin/env bash
# Build the production Rust executables and assemble one fresh native bundle.
set -euo pipefail

source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
widget_only=false
if [[ "${1:-}" == --widget-only ]]; then
    widget_only=true
    shift
fi
if (( $# != 1 )) || [[ "$1" != /* || "$1" == / ]]; then
    echo "Usage: bash linux/build-native.sh [--widget-only] ABSOLUTE_OUTPUT_DIRECTORY" >&2
    exit 2
fi
output_dir=$1
if [[ -e "$output_dir" || -L "$output_dir" ]]; then
    echo "The native build output already exists; it was left untouched." >&2
    exit 1
fi
prerequisites=(cargo rustc cc realpath)
if [[ "$widget_only" == false ]]; then prerequisites+=(pkg-config); fi
for prerequisite in "${prerequisites[@]}"; do
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
if [[ "$widget_only" == true ]]; then
    # The preflight checker must run on this host before desktop development
    # libraries are installed. An explicit native target overrides cross-build
    # configuration and identifies its executable directory unambiguously.
    native_target="$(rustc -vV | sed -n 's/^host: //p')"
    [[ -n "$native_target" ]] || { echo "The native Rust target is unavailable." >&2; exit 3; }
    cargo build --locked --release -p mluva-install --no-default-features \
        --bin mluva-install-widget --target "$native_target" --target-dir "$target_dir"
    mkdir -p -- "$output_dir/bin" "$output_dir/linux/quickshell"
    install -m 0755 "$target_dir/$native_target/release/mluva-install-widget" \
        "$output_dir/bin/mluva-install-widget"
    cp -a -- "$source_root/manifest.json" "$output_dir/manifest.json"
    cp -a -- "$source_root/linux/quickshell/mluva.dictation" "$output_dir/linux/quickshell/"
    exit 0
fi
cargo build --locked --release --workspace --target-dir "$target_dir" \
    --bin mluva --bin mluva-shell --bin mluva-narrate --bin mluva-asr-worker \
    --bin mluva-audio-cleanup --bin mluva-install-widget --bin mluva-screenshot-editor \
    --bin mluva-uninstall --bin mluva-install --bin mluva-package
# Cargo selects the executable directory, including a configured build target.
# The builder uses its own siblings, never stale files in a guessed cache path.
cargo run --locked --release -p mluva-install --bin mluva-package \
    --target-dir "$target_dir" -- "$source_root" "$output_dir"
