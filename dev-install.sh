#!/usr/bin/env bash
set -euo pipefail

# Build and install loom locally for development
# This compiles the release binary and runs install.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Kill any running loom daemon
if pgrep -x loom &>/dev/null; then
    echo "Killing loom daemon..."
    pkill -x loom 2>/dev/null || true
    sleep 1
fi

echo "Building release binary..."
cd "$SCRIPT_DIR/loom"
cargo build --release

echo ""
echo "Running install.sh..."
cd "$SCRIPT_DIR"
bash ./install.sh "$@"
