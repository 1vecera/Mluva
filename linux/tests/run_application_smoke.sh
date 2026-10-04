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
    live-controllers) suites=(live_controller review_controller) ;;
    compact) suites=(application); compact_case="${2:-}" ;;
    providers) suites=(application); provider_case="${2:-}" ;;
    *) echo "Unknown application verification group: $mode" >&2; exit 2 ;;
esac
test_arguments=()
for suite in "${suites[@]}"; do
    test_arguments+=(--test "$suite")
done

if "$inside"; then
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    if [[ "$mode" == live-controllers ]]; then
        for suite in "${suites[@]}"; do
            env PATH="$OFFSCREEN_SESSION_ROOT/${suite%_controller}-codex-tools:$PATH" TZ=UTC CARGO_NET_OFFLINE=true \
                cargo test --locked -p mluva-gtk --test "$suite" -- \
                --ignored --test-threads=1 --nocapture
        done
        exit
    fi
    export PATH="$OFFSCREEN_SESSION_ROOT/application-tools:$PATH"
    export MLUVA_DISABLE_GLOBAL_SHORTCUT=1 TZ=UTC CARGO_NET_OFFLINE=true
    if [[ "$mode" == compact ]]; then
        export MLUVA_COMPACT_CASE="$compact_case" GDK_SCALE=1
        if [[ "$compact_case" == tiled ]]; then export GDK_SCALE=2; fi
    fi
    if [[ "$mode" == providers ]]; then
        export MLUVA_PROVIDER_CASE="$provider_case"
        export CREDENTIAL_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/provider-keyring"
        export FIXTURE_SPEECH_KEY=synthetic-http-key FIXTURE_REWRITE_KEY=synthetic-http-key
    fi
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
if [[ "$mode" != live-components ]]; then
    cargo build --locked -p mluva-gtk --example private_input \
        -p mluva-audio --bin mluva-audio-cleanup --bin audio-fixture-peer \
        -p mluva-providers --bin codex-fixture-peer --bin credential-fixture-peer
fi
cargo test --locked -p mluva-gtk "${test_arguments[@]}" --no-run
mkdir -p tmp/application
evidence="$(mktemp -d "$project_root/tmp/application/run.XXXXXX")"
if [[ "$mode" == providers ]]; then
    provider_cases=(flow minimum narrow wide details error)
    if [[ -n "$provider_case" ]]; then provider_cases=("$provider_case"); fi
    for provider_case in "${provider_cases[@]}"; do
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$provider_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session providers "$provider_case"
    done
    exit
fi
if [[ "$mode" == compact ]]; then
    export OFFSCREEN_SCREEN_SPEC=2200x2500x24
    compact_cases=(minimum narrow tiled wide empty rewriting recording processing live-draft finalizing finalizing-empty)
    if [[ -n "$compact_case" ]]; then compact_cases=("$compact_case"); fi
    for compact_case in "${compact_cases[@]}"; do
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$compact_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session compact "$compact_case"
    done
    exit
fi
exec bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
    bash dev/run-isolated-browser.sh "$evidence" -- \
    bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode"
