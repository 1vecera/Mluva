#!/usr/bin/env bash
# Share native app and widget installation commands with a development checkout.
set -euo pipefail

source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
case "${1:-}" in
    install|uninstall) executable="mluva-$1"; build_options=(); shift ;;
    widget) executable=mluva-install-widget; build_options=(--widget-only); shift ;;
    *) echo "Expected a native install, uninstall or widget command." >&2; exit 2 ;;
esac
umask 077
mkdir -p -- "$source_root/tmp"
build_root="$(mktemp -d "$source_root/tmp/native-command.XXXXXXXX")"
child_pid=""
child_scope=""
cleanup() {
    local status=$?
    trap - EXIT
    if [[ -n "$child_pid" ]]; then
        if [[ "$child_scope" == group ]]; then
            kill -TERM -- "-$child_pid" 2>/dev/null || true
        else
            kill -TERM -- "$child_pid" 2>/dev/null || true
        fi
        wait "$child_pid" 2>/dev/null || true
    fi
    rm -rf -- "$build_root"
    return "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

run_owned() {
    local status=0
    child_scope=$1
    shift
    if [[ "$child_scope" == group ]]; then
        setsid --wait "$@" </dev/null &
    else
        # sudo authentication needs the caller's foreground terminal. Explicit
        # stdin also prevents Bash from replacing it with /dev/null here.
        "$@" <&0 &
    fi
    child_pid=$!
    wait "$child_pid" || status=$?
    child_pid=""
    return "$status"
}
run_owned group bash "$source_root/linux/build-native.sh" "${build_options[@]}" "$build_root/app"
run_owned process "$build_root/app/bin/$executable" "$@"
