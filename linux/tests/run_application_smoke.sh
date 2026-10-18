#!/usr/bin/env bash
# Verify application owners on a private desktop, clipboard and network.
set -euo pipefail

inside=false
if [[ "${1:-}" == --inside-session ]]; then
    inside=true
    shift
fi
mode="${1:-application}"
case "$mode" in
    application) suites=(application application_shell) ;;
    bootstrap) suites=(bootstrap) ;;
    phone) suites=(phone_microphone) ;;
    account) suites=(account_sync) ;;
    agent-voice) suites=(agent_voice) ;;
    conversation) suites=(conversation_page application_shell document_surfaces) ;;
    providers|onboarding) suites=(provider_pages) ;;
    history) suites=(history_controller) ;;
    capture) suites=(capture_lifecycle capture_factory) ;;
    meeting) suites=(meeting_controller) ;;
    screenshots) suites=(application_screenshots) ;;
    text-targets) suites=(text_target recording_target) ;;
    browser-targets) suites=(browser_target); browser_case="${2:-}"; browser_engine="${3:-firefox}"; browser_activation="${4:-enabled}" ;;
    browser-application) suites=(application_shortcuts); browser_case=default; browser_engine="${3:-firefox}" ;;
    *) echo "Unknown application verification group: $mode" >&2; exit 2 ;;
esac
if [[ "$mode" == browser-targets ]]; then
    case "$browser_activation" in
        enabled) ;;
        native|native-late) [[ "$browser_engine" == chromium && "$browser_case" == default ]] ;;
        *) echo "Unknown browser accessibility verification: $browser_activation" >&2; exit 2 ;;
    esac
fi
test_arguments=()
for suite in "${suites[@]}"; do
    test_arguments+=(--test "$suite")
done

if "$inside"; then
    test -n "${OFFSCREEN_SESSION_ROOT:-}"
    # Test executables live in deps; native process helpers are siblings in debug.
    export PATH="$CARGO_TARGET_DIR/debug:$PATH"
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
    if [[ "$mode" == browser-targets || "$mode" == browser-application ]]; then
        export MLUVA_BROWSER_ENGINE="$browser_engine"
    fi
    if [[ "$mode" == browser-targets ]]; then
        unset MLUVA_CHROMIUM_NATIVE_ACTIVATION MLUVA_CHROMIUM_LATE_START
        if [[ "$browser_activation" != enabled ]]; then
            export MLUVA_CHROMIUM_NATIVE_ACTIVATION=1
        fi
        if [[ "$browser_activation" == native-late ]]; then
            export MLUVA_CHROMIUM_LATE_START=1
        fi
    fi
    if [[ "$mode" == browser-application ]]; then
        export PATH="$OFFSCREEN_SESSION_ROOT/application-shortcut-tools:$PATH"
        export MLUVA_APPLICATION_BROWSER=1 TZ=UTC CARGO_NET_OFFLINE=true APP_SHORTCUT_SYNTHETIC_KEY=synthetic-key
        unset MLUVA_DISABLE_GLOBAL_SHORTCUT
        exec cargo test --locked -p mluva-gtk "${test_arguments[@]}" -- --ignored --test-threads=1 --nocapture
    fi

    export PATH="$OFFSCREEN_SESSION_ROOT/application-tools:$PATH"
    export MLUVA_DISABLE_GLOBAL_SHORTCUT=1 TZ=UTC CARGO_NET_OFFLINE=true AGENT_VOICE_SYNTHETIC_KEY=synthetic-key
    if [[ "$mode" == providers || "$mode" == onboarding ]]; then
        export CREDENTIAL_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/provider-keyring"
    fi
    if [[ "$mode" == capture ]]; then
        export CAPTURE_FACTORY_SYNTHETIC_KEY=synthetic-key
        # The independent factory process owns its synthetic recorder executable.
        export PATH="$OFFSCREEN_SESSION_ROOT/factory-tools:$PATH"
    fi
    if [[ "$mode" == bootstrap ]]; then
        export MLUVA_BOOTSTRAP_BINARY="$OFFSCREEN_ARTIFACT_DIR/bootstrap-bundle/bin/mluva"
    fi
    if [[ "$mode" == account ]]; then
        export MLUVA_ACCOUNT_ORIGIN=http://127.0.0.1:48824
    fi

    if [[ "$mode" == screenshots ]]; then
        export MLUVA_SCREENSHOT_FIXTURE_ROOT="$OFFSCREEN_SESSION_ROOT/application-screenshot-case"
        export PATH="$MLUVA_SCREENSHOT_FIXTURE_ROOT/tools:$PATH"
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

if [[ "$mode" == browser-targets || "$mode" == browser-application ]]; then
    [[ "$browser_engine" == firefox || "$browser_engine" == chromium ]]
    if [[ "$browser_engine" == chromium ]]; then
        for prerequisite in chromium chromedriver; do
            command -v "$prerequisite" >/dev/null || { echo "Missing private Chromium prerequisite: $prerequisite." >&2; exit 3; }
        done
    fi
fi
if [[ "$mode" == text-targets ]]; then
    cargo build --locked -p mluva-gtk --example text_target_peer --example atspi_read_audit
elif [[ "$mode" == browser-targets ]]; then
    cargo build --locked -p mluva-gtk --example firefox_text_peer --example atspi_read_audit
else
    cargo build --locked -p mluva-core --bin phone-fixture-peer -p mluva-gtk --example private_input \
        -p mluva-audio --bin mluva-audio-cleanup --bin audio-fixture-peer \
        -p mluva-providers --bin credential-fixture-peer
fi
if [[ "$mode" == browser-application ]]; then
    cargo build --locked -p mluva-gtk --example firefox_text_peer --example atspi_read_audit
fi
if [[ "$mode" == screenshots ]]; then
    cargo build --locked -p mluva-workflows --bin screenshot-picker-fixture-peer --bin mluva-narrate \
        -p mluva-shell --bin mluva-shell -p mluva-gtk --example screenshot_editor_peer
fi
if [[ "$mode" == capture ]]; then
    cargo build --locked -p mluva-providers --bin qwen-fixture-peer --bin local-asr-fixture-peer \
        -p mluva-asr --bin mluva-asr-worker -p mluva-install --bin mluva-screenshot-editor
fi
cargo test --locked -p mluva-gtk "${test_arguments[@]}" --no-run
mkdir -p tmp/application
evidence="$(mktemp -d "$project_root/tmp/application/run.XXXXXX")"
if [[ "$mode" == bootstrap ]]; then
    bash linux/build-native.sh "$evidence/bootstrap-bundle"
fi
if [[ "$mode" == phone || "$mode" == account ]]; then
    # Phone IPC never needs an editor, window manager or virtual input.
    exec env -i PATH="$PATH" HOME="$HOME" LC_ALL=C.UTF-8 \
        CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
        LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" \
        OFFSCREEN_ENABLE_ATSPI=1 OFFSCREEN_DISPLAY_NUMBER="${OFFSCREEN_DISPLAY_NUMBER:-274}" \
        XDG_CURRENT_DESKTOP=offscreen GSK_RENDERER=cairo GTK_A11Y=atspi \
        GDK_DEBUG=no-portals GTK_USE_PORTAL=0 ADW_DISABLE_PORTAL=1 \
        __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
        __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
        bwrap --unshare-net --unshare-pid --die-with-parent --bind / / --dev /dev \
            --tmpfs /run/dbus --tmpfs /tmp --tmpfs /usr/share/dbus-1/services --proc /proc -- \
        bash dev/run-isolated.sh "$evidence" -- \
        bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode"
fi
if [[ "$mode" == browser-targets || "$mode" == browser-application ]]; then
    browser_cases=(bus default)
    if [[ "$mode" == browser-application ]]; then browser_cases=(default); fi
    if [[ -n "$browser_case" ]]; then browser_cases=("$browser_case"); fi
    for browser_case in "${browser_cases[@]}"; do
        [[ "$browser_case" == bus || "$browser_case" == default ]]
        browser_evidence="$browser_case"
        if [[ "${browser_activation:-enabled}" != enabled ]]; then
            browser_evidence="$browser_case-$browser_activation"
        fi
        bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
            bash dev/run-isolated-browser.sh "$evidence/$browser_evidence" -- \
            bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode" "$browser_case" "${browser_engine:-firefox}" "${browser_activation:-enabled}"
    done
    exit
fi

exec bwrap --die-with-parent --bind / / --dev /dev --tmpfs /run/dbus -- \
    bash dev/run-isolated-browser.sh "$evidence" -- \
    bash "$project_root/linux/tests/run_application_smoke.sh" --inside-session "$mode"
