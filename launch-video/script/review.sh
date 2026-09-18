#!/usr/bin/env bash
# Render a half-size 30 fps preview, pull labelled frames at review timestamps and build two
# contact sheets plus audio stats. Usage: script/review.sh v3
set -euo pipefail
tag="${1:?tag such as v2}"
cd "$(dirname "$0")/.."
npx tsc --noEmit
npx remotion render src/index.ts MluvaLaunchPreview "out/preview-$tag.mp4" --scale=0.5 --log=error
dur=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "out/preview-$tag.mp4")
echo "duration $dur"
mkdir -p "out/frames-$tag"
while read -r name at; do
  ffmpeg -v error -y -ss "$at" -i "out/preview-$tag.mp4" -frames:v 1 "out/frames-$tag/$name.png"
done <<'EOF'
s01a 0.8
s01b 2.6
s02a 5.0
s02b 7.4
s03a 9.4
s03b 11.2
s03c 12.6
s04a 17.4
s04b 19.5
s05a 22.0
s05b 23.6
s05c 26.4
s06a 29.4
s06b 31.2
s06c 33.6
s07a 36.2
s08a 39.0
s08b 41.3
s09a 46.2
s09b 48.4
s10a 51.0
s10b 53.1
s11a 56.5
s11b 58.9
EOF
cd "out/frames-$tag"
magick montage $(ls *.png | head -12) -tile 3x4 -geometry 640x360+6+6 -background '#111' -fill white -pointsize 18 -label '%t' "../sheet-${tag}a.png"
magick montage $(ls *.png | tail -12) -tile 3x4 -geometry 640x360+6+6 -background '#111' -fill white -pointsize 18 -label '%t' "../sheet-${tag}b.png"
cd ../..
echo "audio:"
ffmpeg -v info -i "out/preview-$tag.mp4" -af volumedetect -f null - 2>&1 | grep -E 'mean_volume|max_volume' || true
echo "done $tag"
