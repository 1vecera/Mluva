#!/usr/bin/env bash
# Input construction shared with the unchanged released setup process.
set -euo pipefail
case_root=$1
case_name=$2
setup_script=$3
support=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$case_root/source/linux" "$case_root/source/bin" "$case_root/home" "$case_root/bin"
cp "$setup_script" "$case_root/source/install.sh"
install -m 0755 "$support/setup-peer.sh" "$case_root/source/linux/install.sh"
for name in mluva-install mluva-install-widget; do install -m 0755 "$support/setup-peer.sh" "$case_root/source/bin/$name"; done
for name in id omarchy sudo dnf python3; do install -m 0755 "$support/setup-peer.sh" "$case_root/bin/$name"; done
for name in bash dirname touch cat; do ln -s "/usr/bin/$name" "$case_root/bin/$name"; done
case "$case_name" in
    fedora*) rm "$case_root/bin/omarchy" ;;
    unknown-platform) rm "$case_root/bin/omarchy" "$case_root/bin/dnf" ;;
    incomplete) rm "$case_root/source/linux/install.sh" ;;
esac
