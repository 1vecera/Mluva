#!/usr/bin/env bash
# Cut the offscreen capture (highlight/capture.md) into the three clips the composition plays.
# usage: highlight/prepare.sh /path/to/take.mkv   (run from launch-video/)
set -euo pipefail
take="$1"; out=public/highlight; mkdir -p "$out"
clip() { # name start seconds speed crop-height crop-top (the 2120-px-wide window sits at 0,0; pixels are not rescaled)
  ffmpeg -y -loglevel error -ss "$2" -t "$3" -i "$take" \
    -vf "crop=2120:$5:0:$6,setpts=PTS/$4,fps=60,format=yuv420p" \
    -c:v libx264 -crf 12 -preset slow -movflags +faststart -an "$out/$1.mp4"
}
# Dictation is a time-lapse: speech ×2, the wait for the final transcript ×4, then the finished page at 1×.
ffmpeg -y -loglevel error -i "$take" -filter_complex "\
[0:v]crop=2120:880:0:0,split=3[a][b][c];\
[a]trim=6.8:13.8,setpts=(PTS-STARTPTS)/2[a2];\
[b]trim=13.8:18.2,setpts=(PTS-STARTPTS)/4[b2];\
[c]trim=18.2:20.4,setpts=PTS-STARTPTS[c2];\
[a2][b2][c2]concat=n=3:v=1:a=0,fps=60,format=yuv420p[v]" \
  -map "[v]" -c:v libx264 -crf 12 -preset slow -movflags +faststart -an "$out/dictation.mp4"
clip polish 20.2 4.6 1.0 1060 200
clip providers 35.9 3.6 1.0 1050 200
ls -la "$out"
