#!/usr/bin/env bash
# Build the shared core and run the Swift tests. Command Line Tools ship Swift Testing outside
# the default search paths, so they are passed explicitly; Xcode needs no extra flags.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
"$here/scripts/build-rust.sh" debug >/dev/null
flags=()
developer="$(xcode-select -p)"
if [[ "$developer" == */CommandLineTools ]]; then
  frameworks="$developer/Library/Developer/Frameworks"
  flags=(-Xswiftc -F -Xswiftc "$frameworks" -Xlinker -F -Xlinker "$frameworks"
    -Xlinker -rpath -Xlinker "$frameworks" -Xlinker -rpath -Xlinker "$developer/Library/Developer/usr/lib")
fi
swift test --package-path "$here" "${flags[@]}" "$@"
