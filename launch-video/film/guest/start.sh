#!/bin/bash
. ~/henv.sh
M=$HOME/film; R=$M/run
unit() { local n=$1; shift; systemctl --user stop "$n" 2>/dev/null; systemctl --user kill --kill-whom=all -s KILL "$n" 2>/dev/null; systemctl --user reset-failed "$n" 2>/dev/null; systemd-run --user --quiet --collect --unit="$n" -p "StandardOutput=append:$R/$n.log" -p "StandardError=append:$R/$n.log" --setenv=WAYLAND_DISPLAY=$WAYLAND_DISPLAY --setenv=HYPRLAND_INSTANCE_SIGNATURE=$HYPRLAND_INSTANCE_SIGNATURE --setenv=XDG_CURRENT_DESKTOP=Hyprland --setenv=XDG_SESSION_TYPE=wayland --setenv=OMARCHY_PATH=$OMARCHY_PATH "$@"; }
case $1 in
 peer) sudo systemctl stop film-peer 2>/dev/null; sudo systemctl reset-failed film-peer 2>/dev/null; sudo systemd-run --quiet --unit=film-peer --uid=0 -p "StandardOutput=append:$R/peer.log" -p "StandardError=append:$R/peer.log" /usr/bin/python3 $M/scribe_peer.py $M/script.json $R/speech.jsonl $M/pki/peer.crt $M/pki/peer.key ;;
 provider) cp -f $M/provider/bin/mluva $HOME/.local/share/mluva/app/bin/mluva.provider 2>/dev/null; unit mluva -p KillMode=process --setenv=PATH=$M/tools:$HOME/.local/bin:/usr/share/omarchy/bin:/usr/bin --setenv=ELEVENLABS_API_KEY=synthetic-loopback --setenv=GSETTINGS_BACKEND=memory --setenv=GSK_RENDERER=gl $HOME/.local/share/mluva/app/bin/mluva.provider
   for _ in $(seq 100); do gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.List 2>/dev/null | grep -q record && break; sleep 0.2; done; echo started-provider ;;
 mluva) unit mluva -p KillMode=process --setenv=PATH=$M/tools:$HOME/.local/bin:/usr/share/omarchy/bin:/usr/bin --setenv=ELEVENLABS_API_KEY=synthetic-loopback --setenv=GSETTINGS_BACKEND=memory --setenv=GSK_RENDERER=gl --setenv=SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt $HOME/.local/bin/mluva
   for _ in $(seq 100); do gdbus call --session --dest com.mluva.Linux --object-path /com/mluva/Linux --method org.gtk.Actions.List 2>/dev/null | grep -q record && break; sleep 0.2; done; echo started ;;
esac
