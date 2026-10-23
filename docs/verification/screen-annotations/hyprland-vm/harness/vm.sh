#!/usr/bin/env bash
# Isolated Arch Linux VM running real Hyprland, for screen-annotation checks.
#
#   vm.sh start [OUTPUTS]   boot headless: KVM, virtio-vga-gl on egl-headless, no window, no VNC
#   vm.sh stop              power off through QMP
#   vm.sh ssh [CMD…]        run a command in the guest as the synthetic test user
#   vm.sh qmp ARGS…         qmp.py against this VM (keys, tablet, screendump)
#
# State lives in VM_DIR (default: the checkout's ignored tmp/hyprland-vm): disk/hyprvm.qcow2, the test
# user's key in ssh/id_ed25519, and run/ for sockets and logs. VM_SEED_URL passes a cloud-init seed on
# the first boot. The README describes provisioning.
# Networking is QEMU user mode; only guest SSH is forwarded, to 127.0.0.1:${VM_SSH_PORT:-2223}.
# Input reaches only the guest's USB keyboard and tablet through QMP; nothing touches the host.
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
DIR=${VM_DIR:-$(cd "$HERE/../../../../.." && pwd)/tmp/hyprland-vm}
RUN=$DIR/run
PORT=${VM_SSH_PORT:-2223}
RENDER=${VM_RENDER_NODE:-/dev/dri/renderD129}

ssh_config() {
  cat >"$RUN/ssh_config" <<EOF
Host hyprvm
  HostName 127.0.0.1
  Port $PORT
  User tester
  IdentityFile $DIR/ssh/id_ed25519
  IdentitiesOnly yes
  StrictHostKeyChecking no
  UserKnownHostsFile $RUN/known_hosts
  LogLevel ERROR
  ConnectTimeout 5
EOF
}

start() {
  local outputs=${1:-1}
  mkdir -p "$RUN"
  chmod 700 "$RUN"
  ssh_config
  if [[ -f $RUN/qemu.pid ]] && kill -0 "$(cat "$RUN/qemu.pid")" 2>/dev/null; then
    echo "already running" >&2
    return 0
  fi
  # The first boot reads a cloud-init seed served on the host's loopback (10.0.2.2 in the guest).
  local seed=()
  [[ -z ${VM_SEED_URL:-} ]] || seed=(-smbios "type=1,serial=ds=nocloud;s=$VM_SEED_URL")
  # Mesa's EGL only: the render node is the integrated GPU, never another vendor's driver.
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json \
    qemu-system-x86_64 -name hyprvm -machine q35,accel=kvm -cpu host -smp 8 -m 8G \
    -drive "file=$DIR/disk/hyprvm.qcow2,if=virtio,cache=writeback,discard=unmap" \
    -nic "user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:$PORT-:22" \
    -vga none -device "virtio-vga-gl,id=gpu0,max_outputs=$outputs,xres=${VM_XRES:-1920},yres=${VM_YRES:-1080}" \
    -display "egl-headless,rendernode=$RENDER" \
    -device qemu-xhci,id=xhci -device usb-kbd,id=kbd0 -device usb-tablet,id=tab0 \
    -qmp "unix:$RUN/qmp.sock,server=on,wait=off" -monitor none \
    -chardev "socket,id=ser0,path=$RUN/serial.sock,server=on,wait=off,logfile=$RUN/serial.log,logappend=on" \
    -serial chardev:ser0 "${seed[@]}" \
    -pidfile "$RUN/qemu.pid" -daemonize
}

stop() {
  [[ -f $RUN/qemu.pid ]] || return 0
  local pid
  pid=$(cat "$RUN/qemu.pid")
  python3 "$HERE/qmp.py" "$RUN/qmp.sock" raw '{"execute":"system_powerdown"}' >/dev/null 2>&1 || true
  for _ in $(seq 60); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 1
  done
  python3 "$HERE/qmp.py" "$RUN/qmp.sock" raw '{"execute":"quit"}' >/dev/null 2>&1 || true
}

case ${1:-} in
  start) shift; start "$@" ;;
  stop) stop ;;
  ssh) shift; ssh_config; exec ssh -F "$RUN/ssh_config" hyprvm "$@" ;;
  qmp) shift; exec python3 "$HERE/qmp.py" "$RUN/qmp.sock" "$@" ;;
  *) sed -n '2,14p' "$0" >&2; exit 2 ;;
esac
