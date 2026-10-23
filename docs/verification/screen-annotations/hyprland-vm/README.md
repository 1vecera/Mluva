# Screen annotation on real Hyprland in a virtual machine

This harness runs Mluva on real Hyprland 0.56.2 inside a QEMU/KVM guest. The guest has its own virtual GPU, seat, USB keyboard and USB tablet. The [results](../README.md#real-hyprland-in-a-virtual-machine) and the [evidence](evidence/) come from it.

## Isolation

- **Display.** QEMU runs headless: `-display egl-headless` renders the guest's virgl GPU on the host's Intel render node, with Mesa's EGL only. No window, VNC or host display is used.
- **Input.** QMP `input-send-event` reaches only the guest's USB keyboard and tablet, which the guest's libinput sees as physical devices. It never names a device: the optional `device` argument names a display console, and QEMU 11.1.1 aborts when given an input device id there.
- **Network.** User-mode networking. Only the guest's SSH is forwarded, to `127.0.0.1:2223`, with a key made for the VM.
- **Content.** Speech, Herdr and the microphone are the repository's loopback fixtures. The guest holds only synthetic content: no host home, configuration or credentials.
- **State.** Everything lives under the checkout's ignored `tmp/hyprland-vm/`, or `VM_DIR`.

## Host

- KVM, plus QEMU 11.1.1 with `qemu-hw-display-virtio-gpu`, `-gpu-gl`, `-vga`, `-vga-gl`, `qemu-ui-opengl`, `qemu-ui-egl-headless`, `qemu-img` and `virglrenderer`.
- On Arch, the GL modules load on top of the base `virtio-gpu` and `virtio-vga` modules, which pacman does not list as dependencies. Install them too.
- `VM_RENDER_NODE` selects the render node; the default is `/dev/dri/renderD129`.

## Provisioning

1. **Image.** Download `Arch-Linux-x86_64-cloudimg-20260815.573966.qcow2` from `https://geo.mirror.pkgbuild.com/images/v20260815.573966/` and check it against its `.SHA256`.
2. **Disk and key.** Under `tmp/hyprland-vm/`:
   - create the disk: `qemu-img create -f qcow2 -F qcow2 -b ../image/<image> disk/hyprvm.qcow2 40G`;
   - create a key without a passphrase: `ssh/id_ed25519`.
3. **First boot.** Serve a NoCloud seed on the host's loopback, for example with `python3 -m http.server --bind 127.0.0.1 8123`, then run `VM_SEED_URL=http://10.0.2.2:8123/ vm.sh start`. The seed's `user-data`:
   - creates the user `tester` with that key and passwordless sudo;
   - writes `/etc/pacman.d/mirrorlist` as `Server = https://archive.archlinux.org/repos/2026/08/25/$repo/os/$arch`;
   - adds `bootcmd: [[systemctl, enable, --now, --no-block, sshd.service]]`. Without `--no-block` the boot deadlocks.
4. **Packages.** In the guest:
   - mask `systemd-time-wait-sync.service`, because user networking has no NTP and it blocks `sshd`;
   - run `pacman-key --init` and `pacman-key --populate`;
   - run `pacman -Syu`, then install `hyprland xdg-desktop-portal-hyprland xdg-desktop-portal xdg-desktop-portal-gtk pipewire wireplumber pipewire-pulse grim wl-clipboard foot python-gobject gtk4 gtk4-layer-shell libadwaita at-spi2-core ttf-dejavu noto-fonts jq socat wtype polkit dconf sqlite bc`;
   - `touch /etc/cloud/cloud-init.disabled`.

   The snapshot gives Hyprland 0.56.2-1 (commit `efb5099`), xdg-desktop-portal-hyprland 1.4.1-1, aquamarine 0.14.0-2, GTK 4.22.4 and kernel 7.1.9, matching the workstation.
5. **Session.**
   - Install `guest/autologin.conf` as `/etc/systemd/system/getty@tty1.service.d/autologin.conf`, `guest/bash_profile` as `~/.bash_profile` and `guest/hyprland.lua` as `~/.config/hypr/hyprland.lua`.
   - Write `~/.config/mluva/hyprland-shortcuts.lua` containing only `-- Mluva managed portal bindings v1`.
   - Run `gsettings set org.gnome.desktop.interface toolkit-accessibility true`; automatic insertion needs it.
   - **Two outputs.** Add `video=Virtual-2:1920x1080@60e` to the kernel command line, because QEMU's headless display enables only the first head. Install `guest/monitors-two.lua` as `~/.config/hypr/monitors.lua`, then run `vm.sh start 2`.
6. **Mluva.** Build the debug binaries as in [`dev/verify.sh`](../../../../dev/verify.sh), adding `--bin herdr-voice-fixture-peer`. Then:
   - copy `mluva` and `mluva-audio-cleanup` to `~/mluva/bundle/bin/`, and `rust/mluva-gtk/resources` to `~/mluva/bundle/resources`;
   - copy `audio-fixture-peer` and `herdr-voice-fixture-peer` to `~/mluva/tools/`;
   - copy `../offscreen/*.py` to `~/mluva/offscreen/`, and `guest/*.py`, `guest/setup.sh` and `guest/session.sh` to `~/mluva/`;
   - copy `guest/henv.sh` to `~/henv.sh`.
   - **Desktop entry.** xdg-desktop-portal 1.22 accepts the host app id only with a desktop entry. Install `linux/resources/com.mluva.Linux.desktop.in` as `~/.local/share/applications/com.mluva.Linux.desktop`, with `@EXECUTABLE@` replaced by `/home/tester/mluva/bundle/bin/mluva`.

## Running

`run-checks.sh EVIDENCE_DIR PHASE…` drives the guest from the host. It writes screenshots, bindings, the probe's own log, the bundle and logs to `EVIDENCE_DIR`.

| Phase | What it does |
| --- | --- |
| `setup` | Starts a fresh session: the loopback speech and Herdr peers, the probe text app, a stub agent in `foot`, the synthetic settings page floating over both, and Mluva with its window on workspace 2 |
| `f9` | F9 on the USB keyboard: bindings, then capitals while Shift is bound, a circle and an underline with Shift held, the fade, an ordinary click, pins on a banner and on text, then Stop with F9 into the probe's text field |
| `agent` | Selects the synthetic agent, then F8 twice to record and F8 to send through the guarded Herdr route |
| `cancel`, `killed`, `portal` | Bindings after Ctrl+Alt+Escape; after SIGKILL of Mluva, then of Mluva and its watchdog; and with `MLUVA_DISABLE_GLOBAL_SHORTCUT=1` |
| `click` | Five clicks without movement after Shift release, against five after a 2 px move; then the fade, timed on the probe's clock |
| `insertion` | The open insertion item, with annotation on and off |
| `two`, `starts` | With two outputs: marks on the scale-1.5 output and where strokes begin when motion starts with the Shift press |
| `collect` | Copies logs, speech requests, the Herdr wire log and the newest bundle |

Example: `vm.sh start && run-checks.sh tmp/vm-run setup f9 collect`. Use a fresh evidence directory per run. Stop other heavy host work during runs: a 20-job build starved the guest once and cost a stroke its start.
