#!/usr/bin/env bash
# Stage one launch-film take as PNG and prove every staged frame is the take's frame.
#
#   stage.sh NAME CAPTURE MEDIA_DIR > checks.json
#
# CAPTURE is the lossless capture (FFV1, or ffvhuff from a real-time take). A non-FFV1 capture is
# transcoded to FFV1 at MEDIA_DIR/takes/NAME.mkv and both are compared frame by frame. Every frame
# of the FFV1 take is then written to MEDIA_DIR/staged/NAME/NNNNN.png, and the decoded RGB MD5 of
# each PNG is compared with the same frame of the take; any mismatch or count difference fails.
# A contact sheet (two frames a second) goes to MEDIA_DIR/contact/NAME.png. A rerun reuses an
# existing transcode and staged PNGs but recomputes every checksum.
set -euo pipefail
name=$1 capture=$(realpath "$2") media=$(realpath -m "$3")
take=$media/takes/$name.mkv staged=$media/staged/$name
mkdir -p "$media/takes" "$media/contact" "$media/checks"
md5s() { ffmpeg -nostdin -v error "$@" -pix_fmt rgb24 -f framemd5 - | awk -F', *' '!/^#/ {print $NF}'; }

codec=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 "$capture")
if [[ $codec != ffv1 ]]; then
    [[ -s $take ]] || ffmpeg -nostdin -v error -y -i "$capture" -c:v ffv1 -level 3 -threads 16 -slices 24 -slicecrc 1 -pix_fmt bgr0 "$take"
    md5s -i "$capture" > "$media/checks/$name.capture.md5"
else
    [[ $capture == "$take" ]] || cp --reflink=auto "$capture" "$take"
fi
md5s -i "$take" > "$media/checks/$name.take.md5"
transcode_ok=null
if [[ $codec != ffv1 ]]; then
    cmp -s "$media/checks/$name.capture.md5" "$media/checks/$name.take.md5" && transcode_ok=true || transcode_ok=false
fi

if [[ ! -d $staged ]]; then
    mkdir -p "$staged"
    # One PNG per decoded frame: a real-time take can have timestamp gaps where x11grab dropped
    # frames, and the image writer would otherwise fill them with duplicates.
    ffmpeg -nostdin -v error -i "$take" -fps_mode passthrough -start_number 0 "$staged/%05d.png"
fi
md5s -framerate 60 -start_number 0 -i "$staged/%05d.png" > "$media/checks/$name.staged.md5"
frames=$(wc -l < "$media/checks/$name.take.md5")
staged_frames=$(wc -l < "$media/checks/$name.staged.md5")
matching=$(paste -d' ' "$media/checks/$name.take.md5" "$media/checks/$name.staged.md5" | awk '$1 == $2' | wc -l)

# Every 30th frame, labelled with its frame number in the take.
ffmpeg -nostdin -v error -y -i "$take" -vf "select=not(mod(n\,30)),scale=640:-1,drawtext=text='%{eif\:n*30\:d}':x=8:y=8:fontcolor=white:fontsize=18:box=1:boxcolor=black@0.6,tile=6x$(( (frames / 30 + 6) / 6 )):padding=4:color=0x202020" \
    -frames:v 1 "$media/contact/$name.png"

IFS=x read -r width height < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0:s=x "$take")
jq -n --arg name "$name" --arg take "$take" --arg capture "$capture" --arg codec "$codec" --arg staged "$staged" \
    --arg contact "$media/contact/$name.png" --argjson width "$width" --argjson height "$height" \
    --argjson frames "$frames" --argjson staged_frames "$staged_frames" --argjson matching "$matching" \
    --argjson transcode_ok "$transcode_ok" --arg sha "$(sha256sum "$take" | cut -d' ' -f1)" '{
      name: $name, take: $take, take_sha256: $sha, capture: $capture, capture_codec: $codec,
      resolution: [$width, $height], fps: 60, frames: $frames, duration_seconds: ($frames / 60),
      transcode_frames_identical: $transcode_ok,
      staged: $staged, staged_frames: $staged_frames, staged_rgb_md5_matching: $matching,
      staged_check: (if $frames == $staged_frames and $matching == $frames then "pass" else "fail" end),
      contact_sheet: $contact }'
[[ $frames == "$staged_frames" && $matching == "$frames" && $transcode_ok != false ]]
