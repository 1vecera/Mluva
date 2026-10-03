#!/usr/bin/env bash
# Mask only the optional system renderer inside an existing private session.
set -euo pipefail
test -n "$OFFSCREEN_SESSION_ROOT"
test "$(readlink /proc/self/ns/net)" != "$MLUVA_HOST_NET_NS"
mask="$OFFSCREEN_SESSION_ROOT/without-webkit"
mkdir -p "$mask"
: > "$mask/unavailable.so"
library="$(readlink -f /usr/lib/libwebkitgtk-6.0.so.4)"
test -f "$library"
exec bwrap --die-with-parent --bind / / --dev /dev \
    --ro-bind "$mask/unavailable.so" "$library" \
    --setenv MLUVA_TEST_WITHOUT_WEBKIT 1 -- "$@"
