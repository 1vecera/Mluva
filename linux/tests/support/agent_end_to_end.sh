#!/usr/bin/env bash
set -euo pipefail
: "${MLUVA_VERIFY_CLI:?Use run_agent_end_to_end.sh}"
: "${OFFSCREEN_SESSION_ROOT:?Private desktop required}"
"$MLUVA_VERIFY_CLI" doctor >/dev/null
"$MLUVA_VERIFY_CLI" action quit >/dev/null
tools=$1
target=$2
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
"$target/debug/global-shortcut-fixture-peer" > "$OFFSCREEN_SESSION_ROOT/portal.log" 2>&1 &
portal=$!
cleanup() {
    if [[ -d /tmp/herdr-voice ]]; then cp -a /tmp/herdr-voice "$OFFSCREEN_SESSION_ROOT/native-agent"; fi
    kill "$portal" 2>/dev/null || true
    wait "$portal" 2>/dev/null || true
}
trap cleanup EXIT
for _ in {1..100}; do
    rg -q portal-ready "$OFFSCREEN_SESSION_ROOT/portal.log" && break
    sleep 0.05
done
rg -q portal-ready "$OFFSCREEN_SESSION_ROOT/portal.log"
"$MLUVA_VERIFY_CLI" launch --global-shortcuts >/dev/null
export VOICE_PROOF_ROOT=/tmp/herdr-voice
export VOICE_PROOF_HERDR="$tools/herdr"
export VOICE_PROOF_MLUVA="$OFFSCREEN_SESSION_ROOT/bundle/bin/mluva"
export VOICE_PROOF_EXPECTED_TEXT='Synthetic narration keeps 12 files.'
export PATH="$tools:$PATH"
uv run --no-project --offline python "$root/linux/tests/support/herdr_native_voice.py"
