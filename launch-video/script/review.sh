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
s01a 0.6
s01b 2.57
s02a 3.77
s02b 8.59
s03a 9.79
s03b 15.31
s04a 16.51
s04b 21.81
s05a 23.01
s05b 28.97
s06a 30.17
s06b 35.13
s07a 36.33
s07b 37.53
s08a 38.73
s08b 44.43
s09a 45.63
s09b 49.83
s10a 51.03
s10b 54.41
s11a 55.61
s11b 59.09
EOF
cd "out/frames-$tag"
magick montage $(ls *.png | head -12) -tile 3x4 -geometry 640x360+6+6 -background '#111' -fill white -pointsize 18 -label '%t' "../sheet-${tag}a.png"
magick montage $(ls *.png | tail -12) -tile 3x4 -geometry 640x360+6+6 -background '#111' -fill white -pointsize 18 -label '%t' "../sheet-${tag}b.png"
cd ../..
echo "audio:"
ffmpeg -v info -i "out/preview-$tag.mp4" -af volumedetect -f null - 2>&1 | grep -E 'mean_volume|max_volume' || true
ffmpeg -hide_banner -i "out/preview-$tag.mp4" -af ebur128=peak=true -f null - 2>&1 | grep -E '^    I:|^    LRA:' | head -n 2 || true
echo "done $tag"
