#!/bin/bash
# Smoke test: build the app, launch it, verify it runs, kill it.
# Exit 0 on success, 1 on failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "Mluva smoke test"

echo "Running unit tests..."
swift test 2>&1 | tail -1
echo ""

echo "Building release..."
swift build -c release 2>&1 | grep "Build complete"
echo ""

echo "Launching binary..."
.build/release/VoiceScribeMac &
PID=$!
sleep 2

if kill -0 "$PID" 2>/dev/null; then
    echo "  Process $PID is running"

    # Check it's a background-only app (no dock icon)
    BG_CHECK=$(osascript -e 'tell application "System Events" to get name of every process whose background only is true' 2>/dev/null || echo "")
    if echo "$BG_CHECK" | grep -q "VoiceScribeMac"; then
        echo "  Running as background menu bar process"
    else
        echo "  WARNING: Not detected as background process"
    fi
else
    echo "  FAIL: Process not running after 2 seconds"
    exit 1
fi

echo "Stopping..."
kill "$PID" 2>/dev/null
wait "$PID" 2>/dev/null || true
sleep 0.5

if kill -0 "$PID" 2>/dev/null; then
    echo "  FAIL: Process didn't stop"
    kill -9 "$PID" 2>/dev/null
    exit 1
else
    echo "  Clean shutdown"
fi

echo ""
echo "Smoke test passed"
