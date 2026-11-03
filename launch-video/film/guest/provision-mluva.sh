#!/bin/bash
# Guest side, once: install the bundle as a user would, then point its speech provider and recorder at loopback fixtures.
set -euo pipefail
M=$HOME/film
mkdir -p "$M/tools" "$M/run" "$M/pki" "$HOME/.config/mluva"
chmod 700 "$HOME/.config/mluva" "$M/run"
cd /tmp/native-bundle && { bash install.sh --yes 2>&1 | tail -6 || true; }
cd "$M/pki"
if [[ ! -f ca.crt ]]; then
  openssl req -x509 -newkey rsa:2048 -nodes -days 30 -keyout ca.key -out ca.crt -subj "/CN=Film VM CA" 2>/dev/null
  openssl req -newkey rsa:2048 -nodes -keyout peer.key -out peer.csr -subj "/CN=api.elevenlabs.io" 2>/dev/null
  printf 'subjectAltName=DNS:api.elevenlabs.io\n' > ext
  openssl x509 -req -in peer.csr -CA ca.crt -CAkey ca.key -CAcreateserial -days 30 -out peer.crt -extfile ext 2>/dev/null
  sudo trust anchor --store ca.crt
fi
grep -q api.elevenlabs.io /etc/hosts || echo "127.0.0.1 api.elevenlabs.io" | sudo tee -a /etc/hosts >/dev/null
ln -sf "$M/tools/audio-fixture-peer" "$M/tools/pw-record"
printf '#!/bin/sh\nexit 1\n' > "$M/tools/secret-tool"; chmod 755 "$M/tools/secret-tool"
python3 - "$M" <<'PY'
import json, sys
m = sys.argv[1]
# Quiet synthetic PCM at real-time pace: only the provider's words matter.
json.dump({"pcm_hex": "01000200" * 800, "fragments": [3200], "bytes_per_second": 32000, "repeat": True, "interval_ms": 0, "wait": True},
          open(f"{m}/tools/test-config.json", "w"))
PY
omarchy-done mark first-run-user
pacman -Q | wc -l
