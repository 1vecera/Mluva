#!/usr/bin/env bash
# Development-only real-process verification. No live desktop fallback.
set -euo pipefail
umask 077
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
target="$(realpath -m -- "${CARGO_TARGET_DIR:-$root/tmp/native-build}")"
usage() {
    cat <<'HELP'
Usage: dev/verify.sh doctor
       dev/verify.sh smoke <new tmp/evidence-dir> [--expect-working TEXT]
       dev/verify.sh session <new tmp/evidence-dir> -- <command> [args...]

Build first (outside the private desktop):
  export CARGO_TARGET_DIR="$PWD/tmp/native-build"
  cargo build --locked -p mluva-gtk --bin mluva --example mluva_verify \
    -p mluva-audio --bin audio-fixture-peer --bin mluva-audio-cleanup

Session commands use "$MLUVA_VERIFY_CLI" doctor|discover|state|launch
  action record|cancel|latest|history|settings|quit
  edit TEXT | assert raw|working|editor|count EXPECTED | screenshot NAME
All client output is JSON. Failures exit nonzero; evidence survives cleanup.
The session is synthetic, network/device isolated and has a private clipboard.
HELP
}
case "${1:---help}" in
    --help|-h) usage; exit 0 ;;
    doctor|smoke|session) mode="$1"; shift ;;
    *) usage >&2; exit 2 ;;
esac
command -v jq >/dev/null || { echo 'Missing jq (JSON reporting prerequisite)' >&2; exit 3; }
missing=()
for dependency in bwrap firefox openbox xdotool xclip xprop rg Xvfb xvfb-run xauth dbus-run-session dbus-daemon gdbus import identify timeout git sha256sum; do
    command -v "$dependency" >/dev/null || missing+=("$dependency")
done
for binary in mluva audio-fixture-peer mluva-audio-cleanup examples/mluva_verify; do
    [[ -x "$target/debug/$binary" ]] || missing+=("$target/debug/$binary")
done
if [[ "$mode" == doctor ]] || (( ${#missing[@]} )); then
    printf '%s\n' "${missing[@]}" | jq -Rn --arg target "$target" \
        '[inputs | select(length > 0)] as $missing | {command:"doctor",ok:($missing|length==0),scope:"host prerequisites only; isolation checked inside session",target:$target,missing:$missing}'
    (( ${#missing[@]} == 0 )) || exit 3
    exit 0
fi
(( $# >= 1 )) || { usage >&2; exit 2; }
evidence="$(realpath -m -- "$1")"
shift
[[ "$evidence" == "$root/tmp/"* && ! -e "$evidence" ]] || {
    echo '{"ok":false,"error":"Choose a new evidence directory inside this checkout tmp/"}'
    exit 2
}
if [[ "$mode" == smoke ]]; then
    (( $# == 0 )) || { [[ $# == 2 && "$1" == --expect-working ]] || { usage >&2; exit 2; }; }
else
    [[ "${1:-}" == -- && $# -ge 2 ]] || { usage >&2; exit 2; }
    shift
fi
mkdir -p -- "$evidence"
cd -- "$root"
source_head="$(git rev-parse HEAD)"
source_dirty=false
[[ -z "$(git status --porcelain)" ]] || source_dirty=true
jq -n --arg head "$source_head" --argjson dirty "$source_dirty" \
    '{source_head:$head,working_tree_dirty:$dirty,build:"Cargo development binary; hashes identify actual inputs"}' > "$evidence/build.json"
sha256sum "$target/debug/mluva" "$target/debug/examples/mluva_verify" "$target/debug/audio-fixture-peer" > "$evidence/build.sha256"
# Stamp the caller namespaces before env clearing; the client also proves private
# PID 1, Xvfb/bus ownership, device absence and path containment independently.
for boundary in net pid mnt; do
    readlink "/proc/self/ns/$boundary" > "$evidence/host-$boundary"
done
set +e
env -i PATH="$PATH" HOME="$HOME" LC_ALL=C.UTF-8 \
    LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" XDG_DATA_DIRS="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" \
    OFFSCREEN_DISPLAY_NUMBER="${OFFSCREEN_DISPLAY_NUMBER:-284}" \
    bwrap --die-with-parent --unshare-pid --unshare-net --ro-bind / / \
        --tmpfs /home --tmpfs /root --ro-bind "$root" "$root" --bind "$evidence" "$evidence" \
        --dev /dev --tmpfs /run --tmpfs /tmp --proc /proc -- \
    bash "$root/dev/run-isolated-browser.sh" "$evidence" -- \
    "$target/debug/examples/mluva_verify" run "$mode" "$root" "$target/debug" "$@" \
    > "$evidence/runner.log" 2>&1
status=$?
set -e
if [[ ! -f "$evidence/result.json" ]]; then
    jq -Rn --arg evidence "$evidence" --argjson status "$status" \
        '[inputs | fromjson? | select(type == "object" and has("ok"))] |
        (if length > 0 then last else {ok:false,status:"blocked",error:"Private session did not produce a result; inspect runner.log and xvfb.log"} end) +
        {exit_code:$status,evidence:$evidence,runner_log:($evidence + "/runner.log")}' < "$evidence/runner.log" > "$evidence/result.json"
fi
cat "$evidence/result.json"
exit "$status"
