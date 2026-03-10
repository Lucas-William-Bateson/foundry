#!/bin/bash
# =============================================================================
# vault-auto-unseal.sh — Entrypoint wrapper for the Vault container
#
# Starts vault server, waits for it to be ready, then auto-unseals using the
# unseal key from init.json (mounted into the container). Runs as a background
# process alongside vault.
#
# This is configured as an additional container in docker-compose.yml that
# runs after vault starts. It watches for seal status and auto-unseals.
# =============================================================================
set -euo pipefail

VAULT_ADDR="${VAULT_ADDR:-http://vault:8200}"
INIT_FILE="/vault/keys/init.json"

export VAULT_ADDR

if [ ! -f "$INIT_FILE" ]; then
  echo "[auto-unseal] No init.json found at $INIT_FILE — skipping auto-unseal"
  echo "[auto-unseal] Run vault-init.sh first to initialize Vault"
  exit 0
fi

UNSEAL_KEY=$(cat "$INIT_FILE" | python3 -c "import json,sys; print(json.load(sys.stdin)['unseal_keys_b64'][0])" 2>/dev/null || true)
if [ -z "$UNSEAL_KEY" ]; then
  echo "[auto-unseal] Could not read unseal key from $INIT_FILE"
  exit 1
fi

echo "[auto-unseal] Waiting for Vault to be reachable..."
ATTEMPTS=0
until wget -qO- "$VAULT_ADDR/v1/sys/health" >/dev/null 2>&1; do
  ATTEMPTS=$((ATTEMPTS + 1))
  if [ "$ATTEMPTS" -ge 60 ]; then
    echo "[auto-unseal] Vault not reachable after 60 attempts, giving up"
    exit 1
  fi
  sleep 1
done

# Check if vault needs unsealing
SEALED=$(wget -qO- "$VAULT_ADDR/v1/sys/health" 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('sealed', False))" 2>/dev/null || echo "unknown")

if [ "$SEALED" = "True" ]; then
  echo "[auto-unseal] Vault is sealed — unsealing..."
  wget -qO- --post-data="{\"key\":\"$UNSEAL_KEY\"}" \
    --header="Content-Type: application/json" \
    "$VAULT_ADDR/v1/sys/unseal" >/dev/null 2>&1
  echo "[auto-unseal] ✓ Vault unsealed successfully"
elif [ "$SEALED" = "False" ]; then
  echo "[auto-unseal] Vault is already unsealed"
else
  # Not initialized
  echo "[auto-unseal] Vault is not initialized — run vault-init.sh"
fi
