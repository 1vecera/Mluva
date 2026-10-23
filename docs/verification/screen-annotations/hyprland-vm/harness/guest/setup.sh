#!/usr/bin/env bash
# Guest side: synthetic Mluva configuration and boundaries. Idempotent; run as the test user.
set -euo pipefail
M=$HOME/mluva
mkdir -p "$M/tools" "$M/run" "$HOME/.config/mluva"
chmod 700 "$HOME/.config/mluva" "$M/run"
ln -sf "$M/tools/audio-fixture-peer" "$M/tools/pw-record"
printf '#!/bin/sh\nexit 1\n' >"$M/tools/secret-tool"
printf '#!/bin/sh\nprintf "[]\\n"\n' >"$M/tools/pw-dump"
chmod 700 "$M/tools/secret-tool" "$M/tools/pw-dump"
python3 - "$M" "${SPEECH_PORT:?}" <<'PY'
import json, sys
m, port = sys.argv[1:]
home = m.rsplit("/", 1)[0]
config = f"{home}/.config/mluva"
# 0.1 s of quiet synthetic PCM, repeated at real-time pace until Stop.
json.dump({"pcm_hex": "01000200" * 800, "fragments": [3200], "bytes_per_second": 32000,
           "repeat": True, "interval_ms": 0, "wait": True}, open(f"{m}/tools/test-config.json", "w"))
json.dump({"transcription_provider": "litellm", "transcription_base_url": f"http://127.0.0.1:{port}/v1",
           "transcription_api_key_env": "MLUVA_UNUSED_SYNTHETIC_KEY", "welcome_completed": True,
           "transcription_chunk_seconds": 3, "audio_retention_policy": "never", "auto_paste": True},
          open(f"{config}/config.json", "w"))
json.dump({"enabled": True, "socket": f"{m}/run/voice.sock"}, open(f"{config}/agent-voice.json", "w"))
json.dump({"enabled": True}, open(f"{config}/screen-annotations.json", "w"))
json.dump({"agents": [{"guard": "a" * 64, "agent": {"name": "Synthetic agent", "pane_id": "w1:p1",
           "agent": "claude", "agent_status": "idle", "cwd": "/synthetic/project",
           "terminal_title_stripped": "Synthetic agent"}}]}, open(f"{m}/run/herdr-state.json", "w"))
PY
echo configured
