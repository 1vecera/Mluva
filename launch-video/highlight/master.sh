#!/usr/bin/env bash
# Attach the generated score to the silent Remotion render: trim to the film, fade out, master to -14 LUFS,
# AAC 320 kbit/s. The video stream is copied untouched, so the drops stay frame-aligned.
# usage: highlight/master.sh   (run from launch-video/; needs out/mluva-rust-highlight-silent.mp4 and public/highlight/score.mp3)
set -euo pipefail
duration=$(ffprobe -v error -show_entries format=duration -of csv=p=0 out/mluva-rust-highlight-silent.mp4)
filter="atrim=0:${duration},afade=t=out:st=$(python3 -c "print(round($duration-1.1,3))"):d=1.1"
measured=$(ffmpeg -hide_banner -i public/highlight/score.mp3 -af "${filter},loudnorm=I=-14:TP=-1.5:LRA=11:print_format=json" -f null - 2>&1 | sed -n '/^{/,/^}/p')
get() { python3 -c "import json,sys; print(json.loads(sys.argv[1])['$1'])" "$measured"; }
ffmpeg -y -loglevel error -i out/mluva-rust-highlight-silent.mp4 -i public/highlight/score.mp3 \
  -af "${filter},loudnorm=I=-14:TP=-1.5:LRA=11:measured_I=$(get input_i):measured_TP=$(get input_tp):measured_LRA=$(get input_lra):measured_thresh=$(get input_thresh):offset=$(get target_offset):linear=true" \
  -map 0:v -map 1:a -c:v copy -c:a aac -b:a 320k -ar 48000 -shortest -movflags +faststart out/mluva-rust-highlight.mp4
ffprobe -v error -show_entries stream=codec_name,width,height,r_frame_rate,duration -of csv=p=0 out/mluva-rust-highlight.mp4
