#!/usr/bin/env bash
set -euo pipefail

PLIST_NAME="me.l3s.foundry"
PLIST_SRC="$(cd "$(dirname "$0")/../config" && pwd)/${PLIST_NAME}.plist"
PLIST_DST="$HOME/Library/LaunchAgents/${PLIST_NAME}.plist"
BINARY_SRC="$(cd "$(dirname "$0")/.." && pwd)/target/release/foundryd"
BINARY_DST="/usr/local/bin/foundryd"
FOUNDRY_DIR="$HOME/.foundry"

# Unload existing service if loaded
if launchctl list "$PLIST_NAME" &>/dev/null; then
    echo "==> Unloading existing service..."
    launchctl unload "$PLIST_DST" 2>/dev/null || true
fi

# Create data directory
echo "==> Creating ${FOUNDRY_DIR}..."
mkdir -p "$FOUNDRY_DIR"

# Install binary
if [ ! -f "$BINARY_SRC" ]; then
    echo "ERROR: Binary not found at ${BINARY_SRC}"
    echo "       Run 'cargo build --release' first."
    exit 1
fi
echo "==> Copying foundryd to ${BINARY_DST}..."
sudo cp "$BINARY_SRC" "$BINARY_DST"
sudo chmod 755 "$BINARY_DST"

# Install plist
echo "==> Installing plist to ${PLIST_DST}..."
mkdir -p "$HOME/Library/LaunchAgents"
cp "$PLIST_SRC" "$PLIST_DST"

# Remind about passphrase
if grep -q 'CHANGE_ME' "$PLIST_DST"; then
    echo ""
    echo "WARNING: FOUNDRY_SECRETS_PASSPHRASE is still set to 'CHANGE_ME'."
    echo "         Edit ${PLIST_DST} and set the real passphrase before loading."
    echo ""
    read -rp "Continue loading anyway? [y/N] " answer
    if [[ ! "$answer" =~ ^[Yy]$ ]]; then
        echo "Aborted. Edit the plist, then run:"
        echo "  launchctl load ${PLIST_DST}"
        exit 0
    fi
fi

# Load service
echo "==> Loading service..."
launchctl load "$PLIST_DST"

# Show status
echo ""
echo "==> Service status:"
launchctl list "$PLIST_NAME"
echo ""
echo "Logs:"
echo "  stdout: ${FOUNDRY_DIR}/foundry.log"
echo "  stderr: ${FOUNDRY_DIR}/foundry.err"
