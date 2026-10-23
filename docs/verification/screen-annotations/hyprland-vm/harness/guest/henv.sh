# shellcheck shell=bash
# Source in the guest to talk to the running Hyprland session of the test user.
XDG_RUNTIME_DIR=/run/user/$(id -u)
for instance in "$XDG_RUNTIME_DIR"/hypr/*/; do
  [[ -S $instance.socket.sock ]] && HYPRLAND_INSTANCE_SIGNATURE=$(basename "$instance")
done
for socket in "$XDG_RUNTIME_DIR"/wayland-[0-9]*; do
  [[ $socket == *.lock ]] || WAYLAND_DISPLAY=$(basename "$socket")
done
export XDG_RUNTIME_DIR HYPRLAND_INSTANCE_SIGNATURE WAYLAND_DISPLAY
export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
export XDG_CURRENT_DESKTOP=Hyprland XDG_SESSION_TYPE=wayland
