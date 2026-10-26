#!/usr/bin/env bash
# Build the mluva-ffi static library for the Swift package.
# Usage: build-rust.sh [debug|release] [--universal]
# Writes macos/.rust/<profile>/libmluva_ffi.a; --universal adds x86_64 and merges with lipo.
set -euo pipefail
profile="${1:-debug}"
universal="${2:-}"
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$root/tmp/mac-target}"
out="$here/.rust/$profile"
toolchain="$(cd "$root" && rustup show active-toolchain | cut -d' ' -f1)"
# The repository pins its toolchain; put that toolchain's own binaries first.
PATH="$(dirname "$(cd "$root" && rustup which cargo)"):$PATH"
cargo=(cargo)
flags=(--locked -p mluva-ffi)
[[ "$profile" == release ]] && flags+=(--release)
export MACOSX_DEPLOYMENT_TARGET=26.0
targets=(aarch64-apple-darwin)
[[ "$universal" == --universal ]] && targets+=(x86_64-apple-darwin)
libraries=()
for target in "${targets[@]}"; do
  rustup target add --toolchain "$toolchain" "$target" >/dev/null
  (cd "$root" && CARGO_TARGET_DIR="$target_dir" "${cargo[@]}" build "${flags[@]}" --target "$target")
  libraries+=("$target_dir/$target/$profile/libmluva_ffi.a")
done
mkdir -p "$out"
if (( ${#libraries[@]} > 1 )); then
  lipo -create "${libraries[@]}" -output "$out/libmluva_ffi.a"
else
  cp "${libraries[0]}" "$out/libmluva_ffi.a"
fi
lipo -info "$out/libmluva_ffi.a"
