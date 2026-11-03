#!/bin/bash
set -euxo pipefail
cp -rn /etc/skel/.config/. ~/.config/ || true
cp -rn /etc/skel/.local/. ~/.local/ 2>/dev/null || true
mkdir -p ~/.local/state/omarchy ~/.local/state/omarchy/migrations
touch ~/.local/state/omarchy/first-run-user
sudo mkdir -p /etc/systemd/system/getty@tty1.service.d
sudo cp ~/autologin.conf /etc/systemd/system/getty@tty1.service.d/autologin.conf
cat > ~/.bash_profile <<'P'
[[ -f ~/.bashrc ]] && . ~/.bashrc
if [[ -z ${WAYLAND_DISPLAY:-} && $(tty) == /dev/tty1 && ! -e ~/.no-hyprland ]]; then
  . /etc/profile.d/omarchy.sh
  export XDG_CURRENT_DESKTOP=Hyprland XDG_SESSION_TYPE=wayland
  exec start-hyprland >"$HOME/hyprland-session.log" 2>&1
fi
P
cat > ~/.config/hypr/monitors.lua <<'L'
hl.monitor({ output = "", mode = "2560x1440@60", position = "auto", scale = 2 })
L
gsettings set org.gnome.desktop.interface toolkit-accessibility true || true
echo "setup done"
