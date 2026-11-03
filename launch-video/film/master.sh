#!/usr/bin/env bash
# Attach the approved mix to the silent render without touching the picture, then check duration.
#   master.sh PICTURE.mp4 MIX.wav OUT.mp4
set -euo pipefail
picture=$1 mix=$2 out=$3
ffmpeg -nostdin -v error -y -i "$picture" -i "$mix" -map 0:v:0 -map 1:a:0 -c:v copy -c:a aac -b:a 256k -movflags +faststart -map_metadata -1 -shortest "$out"
ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,duration -of compact "$out"
