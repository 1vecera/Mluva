#!/usr/bin/env bash
# Verify application and Live owners on a private desktop, clipboard and network.
set -euo pipefail

inside=false
if [[ "${1:-}" == --inside-session ]]; then
    inside=true
    shift
fi
mode="${1:-application}"
case "$mode" in
    application) suites=(application application_shell) ;;
    live-components) suites=(conversation_page document_surfaces) ;;
    *) echo "Unknown application verification group: $mode" >&2; exit 2 ;;
esac
test_arguments=()
for suite in "${suites[@]}"; do
    test_arguments+=(--test "$suite")
done

if "$inside"; then
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    export PATH="$OFFSCREEN_SESSION_ROOT/application-tools:$PATH"
    export MLUVA_DISABLE_GLOBAL_SHORTCUT=1 TZ=UTC CARGO_NET_OFFLINE=true
    exec cargo test --locked -p mluva-gtk "${test_arguments[@]}" -- \
        --ignored --test-threads=1 --nocapture
fi

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
for prerequisite in cargo bwrap import realpath mktemp; do
    command -v "$prerequisite" >/dev/null || {
        echo "Missing native application check prerequisite: $prerequisite." >&2
        exit 3
    }
done
# Resolve caller-relative caches before selecting the checkout and private HOME.
CARGO_HOME="$(realpath -m -- "${CARGO_HOME:-$HOME/.cargo}")"
RUSTUP_HOME="$(realpath -m -- "${RUSTUP_HOME:-$HOME/.rustup}")"
CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$project_root/tmp/native-build}")"
export CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR
cd -- "$project_root"
if [[ "$mode" == application ]]; then
    cargo build --locked -p mluva-gtk --example private_input \
        -p mluva-audio --bin mluva-audio-cleanup --bin audio-fixture-peer \
        -p mluva-providers --bin codex-fixture-peer
fi
cargo test --locked -p mluva-gtk "${test_arguments[@]}" --no-run
mkdir -p tmp/application
evidence="$(mktemp -d "$project_root/tmp/application/run.XXXXXX")"
exec bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
    bash dev/run-isolated-browser.sh "$evidence" -- \
    bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode"
