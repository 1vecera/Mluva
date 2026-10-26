#!/usr/bin/env bash
# Build Mluva.app: the shared core, the Swift app, the icon and an ad-hoc signature.
# Usage: build-app.sh [debug|release] [--universal] [output-directory]
# --universal adds Intel (x86_64) to Apple Silicon. Ad-hoc signing runs locally only;
# Developer ID signing and notarization need an Apple Developer account.
set -euo pipefail
profile="${1:-release}"
universal="${2:-}"
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/.." && pwd)"
out="${3:-$here/.build/app}"
"$here/scripts/build-rust.sh" "$profile" "$universal"
archs=(arm64)
[[ "$universal" == --universal ]] && archs+=(x86_64)
binaries=()
for arch in "${archs[@]}"; do
  MLUVA_RUST_PROFILE="$profile" swift build --package-path "$here" -c "$profile" \
    --triple "$arch-apple-macosx26.0" --product Mluva
  binaries+=("$here/.build/$arch-apple-macosx/$profile/Mluva")
done
app="$out/Mluva.app"
rm -rf -- "${app:?}"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if (( ${#binaries[@]} > 1 )); then
  lipo -create "${binaries[@]}" -output "$app/Contents/MacOS/Mluva"
else
  cp "${binaries[0]}" "$app/Contents/MacOS/Mluva"
fi
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
build="$(git -C "$root" rev-list --count HEAD 2>/dev/null || echo 1)"
sed -e "s/__VERSION__/$version/" -e "s/__BUILD__/$build/" "$here/Resources/Info.plist" > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
cp "$root/docs/brand/svg/mluva-mark-symbolic.svg" "$app/Contents/Resources/MenuBarMark.svg"
iconset="$(mktemp -d /tmp/mluva-iconset.XXXXXX)"
swift "$here/scripts/make-icon.swift" "$root/docs/brand/png/mluva-mark-1024.png" "$iconset/AppIcon.iconset"
iconutil -c icns "$iconset/AppIcon.iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf -- "${iconset:?}"
codesign --force --options runtime --entitlements "$here/Resources/Mluva.entitlements" --sign - "$app"
codesign --verify --strict "$app"
lipo -info "$app/Contents/MacOS/Mluva"
echo "$app"
