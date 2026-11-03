#!/bin/bash
# Guest side: a clean set. Close the windows, restart the app and start fresh sessions, so a take shows only its own delivery.
# shellcheck source=/dev/null
. ~/henv.sh
pkill -x foot; pkill -x firefox; sleep 1.5; systemctl --user stop mluva; sleep 0.5
rm -rf ~/.claude/projects ~/.codex/sessions ~/.local/share/mluva/annotations/* 2>/dev/null
sqlite3 ~/.local/share/mluva/history.sqlite3 "delete from transcription_history" 2>/dev/null
rm -f ~/.config/mluva/agent-voice-selected.json
cp /tmp/config-before.json ~/.config/mluva/config.json 2>/dev/null
omarchy-theme-set tokyo-night >/dev/null 2>&1; sleep 1
hyprctl dispatch "hl.dsp.focus({ workspace = 1 })" >/dev/null
bash ~/film/up.sh 2>&1 | tail -1 | cut -c1-120
bash ~/film/fix.sh 2>&1 | cut -c1-100 | head -6
hyprctl dispatch "hl.dsp.focus({ workspace = 1 })" >/dev/null
hyprctl clients -j | jq -c '.[]|[.class,.workspace.id,.at,.size]'
