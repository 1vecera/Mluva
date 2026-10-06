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
    conversation) suites=(conversation_page application_shell) ;;
    live-components) suites=(conversation_page document_surfaces) ;;
    live-controllers) suites=(live_controller review_controller) ;;
    compact) suites=(application); compact_case="${2:-}" ;;
    providers) suites=(application); provider_case="${2:-}" ;;
    management) suites=(application); management_case="${2:-}" ;;
    prompts) suites=(application); prompt_case="${2:-}" ;;
    onboarding) suites=(application); onboarding_case="${2:-wide}"; onboarding_assets="${3:-${MLUVA_TEST_ONBOARDING_ASSETS:-}}" ;;
    prompt-editor) suites=(prompt_editor) ;;
    images) suites=(application); image_case="${2:-}"; editor="${3:-${MLUVA_TEST_EDITOR:-}}" ;;
    screenshots) suites=(application_screenshots) ;;
    text-targets) suites=(text_target) ;;
    browser-targets) suites=(browser_target); browser_case="${2:-}"; browser_engine="${3:-firefox}" ;;
    browser-application) suites=(application_shortcuts); browser_case=default ;;
    *) echo "Unknown application verification group: $mode" >&2; exit 2 ;;
esac
test_arguments=()
for suite in "${suites[@]}"; do
    test_arguments+=(--test "$suite")
done

if "$inside"; then
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    if [[ "$mode" == browser-targets || "$mode" == browser-application ]]; then
        export MLUVA_TEXT_READ_AUDIT="$OFFSCREEN_SESSION_ROOT/text-read-audit.jsonl"
        export LD_PRELOAD="$CARGO_TARGET_DIR/debug/examples/libatspi_read_audit.so"
        if [[ "$browser_case" == default ]]; then
            unset ATSPI_DISABLE_P2P ATSPI_IN_TESTS ATSPI_NO_CACHE PYATSPI_NOCACHE
            export MLUVA_BROWSER_DEFAULT_TRANSPORT=1
        else
            test "$browser_case" == bus
        fi
    fi
    if [[ "$mode" == browser-targets ]]; then
        export MLUVA_BROWSER_ENGINE="$browser_engine"
    fi
    if [[ "$mode" == browser-application ]]; then
        export PATH="$OFFSCREEN_SESSION_ROOT/application-shortcut-tools:$PATH"
        export MLUVA_APPLICATION_BROWSER=1 TZ=UTC CARGO_NET_OFFLINE=true APP_SHORTCUT_SYNTHETIC_KEY=synthetic-key
        unset MLUVA_DISABLE_GLOBAL_SHORTCUT
        exec cargo test --locked -p mluva-gtk "${test_arguments[@]}" -- --ignored --test-threads=1 --nocapture
    fi
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
    if [[ "$mode" == onboarding ]]; then
        export MLUVA_ONBOARDING_CASE="$onboarding_case" GDK_SCALE=1 GSETTINGS_BACKEND=memory
        export MLUVA_TEST_ONBOARDING_ASSETS="$onboarding_assets"
        export CREDENTIAL_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/provider-keyring"
        export SSL_CERT_FILE="$OFFSCREEN_SESSION_ROOT/artifact-server/cert.pem"
        mkdir "$OFFSCREEN_SESSION_ROOT/artifact-server"
        openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj /CN=huggingface.co \
            -addext subjectAltName=DNS:huggingface.co,DNS:github.com \
            -keyout "$OFFSCREEN_SESSION_ROOT/artifact-server/key.pem" -out "$SSL_CERT_FILE" \
            > "$OFFSCREEN_SESSION_ROOT/artifact-server/certificate.log" 2>&1
        export https_proxy=http://127.0.0.1:48118 HTTPS_PROXY=http://127.0.0.1:48118
        export no_proxy=localhost,127.0.0.1 NO_PROXY=localhost,127.0.0.1
    fi
    if [[ "$mode" == images ]]; then
        export MLUVA_IMAGE_CASE="$image_case" MLUVA_TEST_EDITOR="$editor" GDK_SCALE=1 GSETTINGS_BACKEND=memory
    fi
    if [[ "$mode" == screenshots ]]; then
        export MLUVA_SCREENSHOT_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/application-screenshot-case"
        export PATH="$MLUVA_SCREENSHOT_FIXTURE_ROOT/tools:$PATH"
    fi
    if [[ "$mode" == prompts ]]; then
        export MLUVA_PROMPT_CASE="$prompt_case" GDK_SCALE=1
        cargo test --locked -p mluva-gtk "${test_arguments[@]}" -- --ignored --test-threads=1 --nocapture
        export MLUVA_PROMPT_REOPEN=1 PATH="$OFFSCREEN_SESSION_ROOT/application-reopened-tools:$PATH"
        exec cargo test --locked -p mluva-gtk "${test_arguments[@]}" -- --ignored --test-threads=1 --nocapture
    fi
    if [[ "$mode" == compact ]]; then
        export MLUVA_COMPACT_CASE="$compact_case" GDK_SCALE=1
        if [[ "$compact_case" == tiled ]]; then export GDK_SCALE=2; fi
    fi
    if [[ "$mode" == providers ]]; then
        export MLUVA_PROVIDER_CASE="$provider_case"
        export CREDENTIAL_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/provider-keyring"
        export FIXTURE_SPEECH_KEY=synthetic-http-key FIXTURE_REWRITE_KEY=synthetic-http-key
    fi
    if [[ "$mode" == management ]]; then
        export MLUVA_MANAGEMENT_CASE="$management_case" GDK_SCALE=1
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
if [[ "$mode" == onboarding ]]; then
    command -v openssl >/dev/null || { echo "Missing onboarding HTTPS prerequisite: openssl." >&2; exit 3; }
    onboarding_assets="$(realpath -m -- "${onboarding_assets:-$project_root/tmp/onboarding-assets}")"
    for asset in speech.wav llama-b11011-bin-ubuntu-x64.tar.gz Qwen3-ASR-1.7B-Q4_0.gguf mmproj-Qwen3-ASR-1.7B-Q8_0.gguf; do
        test -f "$onboarding_assets/$asset" || {
            echo "Missing pinned public onboarding artifact: $asset. Set MLUVA_TEST_ONBOARDING_ASSETS." >&2
            exit 3
        }
    done
    cargo build --locked -p mluva-gtk --example artifact_https_peer -p mluva-asr --bin mluva-asr-worker
fi
if [[ "$mode" == images ]]; then
    editor="$(realpath -m -- "${editor:-$project_root/tmp/narrated-editor/tensaku}")"
    if [[ ! -x "$editor" ]]; then
        echo "Set MLUVA_TEST_EDITOR to the verified v1.6.0 Tensaku artifact before this check." >&2
        exit 3
    fi
fi
if [[ "$mode" == text-targets ]]; then
    cargo build --locked -p mluva-gtk --example text_target_peer
elif [[ "$mode" == browser-targets ]]; then
    [[ "$browser_engine" == firefox || "$browser_engine" == chromium ]]
    if [[ "$browser_engine" == chromium ]]; then
        for prerequisite in chromium chromedriver; do
            command -v "$prerequisite" >/dev/null || { echo "Missing private Chromium prerequisite: $prerequisite." >&2; exit 3; }
        done
    fi
    cargo build --locked -p mluva-gtk --example firefox_text_peer --example atspi_read_audit
elif [[ "$mode" != live-components ]]; then
    cargo build --locked -p mluva-gtk --example private_input \
        -p mluva-audio --bin mluva-audio-cleanup --bin audio-fixture-peer \
        -p mluva-providers --bin codex-fixture-peer --bin credential-fixture-peer
fi
if [[ "$mode" == browser-application ]]; then
    cargo build --locked -p mluva-gtk --example firefox_text_peer --example atspi_read_audit
fi
if [[ "$mode" == images || "$mode" == screenshots ]]; then
    cargo build --locked -p mluva-workflows --bin screenshot-picker-fixture-peer --bin mluva-narrate \
        -p mluva-shell --bin mluva-shell -p mluva-gtk --example screenshot_editor_peer
fi
cargo test --locked -p mluva-gtk "${test_arguments[@]}" --no-run
mkdir -p tmp/application
evidence="$(mktemp -d "$project_root/tmp/application/run.XXXXXX")"
if [[ "$mode" == browser-targets || "$mode" == browser-application ]]; then
    browser_cases=(bus default)
    if [[ "$mode" == browser-application ]]; then browser_cases=(default); fi
    if [[ -n "$browser_case" ]]; then browser_cases=("$browser_case"); fi
    for browser_case in "${browser_cases[@]}"; do
        [[ "$browser_case" == bus || "$browser_case" == default ]]
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$browser_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode" "$browser_case" "${browser_engine:-firefox}"
    done
    exit
fi
if [[ "$mode" == onboarding ]]; then
    exec bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
        bash dev/run-isolated-browser.sh "$evidence/$onboarding_case" -- \
        bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session onboarding "$onboarding_case" "$onboarding_assets"
fi
if [[ "$mode" == images ]]; then
    image_cases=(minimum narrow wide preparation-failure)
    if [[ -n "$image_case" ]]; then image_cases=("$image_case"); fi
    for image_case in "${image_cases[@]}"; do
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$image_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session images "$image_case" "$editor"
    done
    exit
fi
if [[ "$mode" == prompts ]]; then
    prompt_cases=(minimum narrow wide bad-config)
    if [[ -n "$prompt_case" ]]; then prompt_cases=("$prompt_case"); fi
    for prompt_case in "${prompt_cases[@]}"; do
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$prompt_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session prompts "$prompt_case"
    done
    exit
fi
if [[ "$mode" == management ]]; then
    management_cases=(minimum narrow wide)
    if [[ -n "$management_case" ]]; then management_cases=("$management_case"); fi
    for management_case in "${management_cases[@]}"; do
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$management_case" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session management "$management_case"
    done
    exit
fi
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
