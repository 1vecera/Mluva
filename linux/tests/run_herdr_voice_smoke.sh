#!/usr/bin/env bash
# Real TUIs with private files, processes, devices and synthetic loopback inference.
set -euo pipefail
launch="${2:---managed-codex}"
if (( $# < 1 || $# > 2 )) || [[ "$launch" != --managed-codex && "$launch" != --manual-codex && "$launch" != --manual-claude ]]; then
    echo 'Usage: linux/tests/run_herdr_voice_smoke.sh /absolute/patched/herdr [--manual-codex|--manual-claude]' >&2
    exit 2
fi
codex_launch=--managed-codex
claude_launch=--managed-claude
[[ "$launch" == --manual-codex ]] && codex_launch=--manual-codex
[[ "$launch" == --manual-claude ]] && claude_launch=--manual-claude
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
herdr_binary="$(realpath -- "$1")"
test -x "$herdr_binary"
for tool in bwrap uv codex claude; do command -v "$tool" >/dev/null; done
mkdir -p "$project_root/tmp/herdr-voice"
proof_root="$(mktemp -d "$project_root/tmp/herdr-voice/run.XXXXXX")"
mkdir -m 0700 "$proof_root/project"
env -i PATH="$PATH" HOME=/tmp/herdr-voice/home TERM=xterm-256color LANG=C.UTF-8 \
    XDG_CONFIG_HOME=/tmp/herdr-voice/config XDG_DATA_HOME=/tmp/herdr-voice/data \
    XDG_STATE_HOME=/tmp/herdr-voice/state XDG_RUNTIME_DIR=/tmp/herdr-voice/runtime \
    UV_CACHE_DIR=/tmp/herdr-voice/cache \
    VOICE_PROOF_ROOT=/tmp/herdr-voice VOICE_PROOF_HERDR="$herdr_binary" \
    VOICE_PROOF_CODEX_LAUNCH="$codex_launch" VOICE_PROOF_CLAUDE_LAUNCH="$claude_launch" \
    bwrap --unshare-net --unshare-pid --unshare-ipc --die-with-parent \
        --ro-bind / / --dev /dev --proc /proc --tmpfs /tmp --tmpfs /run \
        --bind "$proof_root" /tmp/herdr-voice --chdir /tmp/herdr-voice/project -- \
        uv run --no-project --offline python \
        "$project_root/linux/tests/support/herdr_native_voice.py" > "$proof_root/run.log" 2>&1
cat "$proof_root/result.json"
printf '\nPrivate native-agent proof: %s\n' "$proof_root"
