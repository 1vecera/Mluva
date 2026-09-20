#!/usr/bin/env bash
# Master a rendered film to broadcast loudness without touching the picture.
# Usage: script/master.sh out/mluva-launch.mp4
# Output: out/mluva-launch-master.mp4 measured at -16 LUFS integrated, TP <= -1.5 dBTP.
set -euo pipefail
src="${1:?final mp4 path}"
cd "$(dirname "$0")/.."
dst="${src%.mp4}-master.mp4"
ffmpeg -v error -y -i "$src" -af loudnorm=I=-16:TP=-1.5:LRA=11 -c:v copy -c:a aac -b:a 192k "$dst"
echo "mastered $dst"
ffmpeg -hide_banner -i "$dst" -af ebur128=peak=true -f null - 2>&1 | grep -E '^    I:|^    LRA:|True peak' | head -n 4 || true
