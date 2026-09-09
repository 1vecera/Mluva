#!/usr/bin/env bash
# Capture three independent private desktops; no live microphone, provider or desktop input.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"
output="${1:-tmp/promotion-$(date -u +%Y%m%dT%H%M%SZ)}"
[[ ! -e "$output" ]] || { echo "Use a new evidence directory: $output" >&2; exit 1; }
make linux-setup
export OFFSCREEN_ENABLE_ATSPI=1 PYTHONPATH="$root/linux"
export VOICE_SCRIBE_DISABLE_GLOBAL_SHORTCUT=1 ADW_DISABLE_PORTAL=1

for scenario in dark portrait light; do
    format=landscape
    theme=tokyo-night
    export OFFSCREEN_SCREEN_SPEC=1920x1080x24
    if [[ "$scenario" == portrait ]]; then
        format=portrait
        export OFFSCREEN_SCREEN_SPEC=1080x1920x24
    elif [[ "$scenario" == light ]]; then
        theme=rose-pine
    fi
    bash dev/run-isolated.sh "$output/$scenario" -- \
        uv run --project linux --locked python dev/promotion_capture.py --format "$format" --theme "$theme"
done
magick -background none dev/promotion-footer.svg "$output/footer.png"
printf 'Capture evidence: %s\n' "$output"
