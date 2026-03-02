#!/bin/bash
# =============================================================================
# vault-unseal.sh — Unseal Vault after a restart
#
# Vault seals itself on restart. This script reads the unseal key from
# config/vault/init.json and unseals automatically.
#
# Usage:
#   ./scripts/vault-unseal.sh
# =============================================================================
set -euo pipefail

VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8200}"
INIT_FILE="$(dirname "$0")/../config/vault/init.json"

export VAULT_ADDR

if [ ! -f "$INIT_FILE" ]; then
  echo "ERROR: $INIT_FILE not found — run vault-init.sh first."
  exit 1
fi

# Wait for Vault container
echo "Waiting for Vault..."
attempts=0
until curl -sf "${VAULT_ADDR}/v1/sys/health" >/dev/null 2>&1; do
  attempts=$((attempts + 1))
  if [ "$attempts" -ge 30 ]; then
    echo "ERROR: Vault not reachable"
    exit 1
  fi
  sleep 2
done

# Check if sealed
SEALED=$(curl -sf "${VAULT_ADDR}/v1/sys/health" 2>/dev/null | jq -r '.sealed // empty' 2>/dev/null || echo "true")
if [ "$SEALED" = "false" ]; then
  echo "Vault is already unsealed."
  exit 0
fi

UNSEAL_KEY=$(jq -r '.unseal_keys_b64[0]' "$INIT_FILE")
docker compose exec -T vault vault operator unseal "$UNSEAL_KEY" >/dev/null
echo "Vault unsealed successfully."
