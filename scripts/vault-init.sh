#!/bin/bash
# =============================================================================
# vault-init.sh — First-time Vault initialisation and AppRole bootstrap
#
# Run this ONCE after Vault starts for the first time:
#   ./scripts/vault-init.sh
#
# It will:
#   1. Initialise Vault (1 key share, threshold 1 — fine for single-node)
#   2. Unseal Vault
#   3. Enable KV v2 secrets engine at secret/
#   4. Create a read-only CI policy
#   5. Enable AppRole auth
#   6. Create the ci-role with single-use, short-lived tokens
#   7. Output the role_id and an initial secret_id
#
# The unseal key and root token are written to config/vault/init.json.
# Keep this file safe (600 perms) — it's your recovery key.
# =============================================================================
set -euo pipefail

VAULT_ADDR="${VAULT_ADDR:-http://127.0.0.1:8200}"
INIT_OUTPUT="$(dirname "$0")/../config/vault/init.json"
ROLE_OUTPUT="$(dirname "$0")/../config/vault/approle.env"

export VAULT_ADDR

# ---- helpers ----------------------------------------------------------------
vault_cmd() {
  docker compose exec -T vault vault "$@"
}

wait_for_vault() {
  echo "Waiting for Vault to be reachable..."
  local attempts=0
  until vault_cmd status -format=json 2>/dev/null | grep -q '"initialized"' 2>/dev/null \
    || curl -sf "${VAULT_ADDR}/v1/sys/health" >/dev/null 2>&1; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 30 ]; then
      echo "ERROR: Vault not reachable after 30 attempts"
      exit 1
    fi
    sleep 2
  done
  echo "Vault is reachable."
}

# ---- main -------------------------------------------------------------------
echo "=== Vault Initialisation ==="
echo "VAULT_ADDR: ${VAULT_ADDR}"
echo ""

wait_for_vault

# Check if already initialised
INIT_STATUS=$(curl -sf "${VAULT_ADDR}/v1/sys/health" 2>/dev/null || true)
if echo "$INIT_STATUS" | grep -q '"initialized":true'; then
  echo "Vault is already initialised."
  
  # Check if sealed
  if echo "$INIT_STATUS" | grep -q '"sealed":true'; then
    echo "Vault is sealed — unsealing..."
    if [ -f "$INIT_OUTPUT" ]; then
      UNSEAL_KEY=$(jq -r '.unseal_keys_b64[0]' "$INIT_OUTPUT")
      vault_cmd operator unseal "$UNSEAL_KEY" >/dev/null
      echo "Unsealed."
    else
      echo "ERROR: No init.json found at $INIT_OUTPUT — cannot unseal."
      echo "You need to manually unseal with your unseal key."
      exit 1
    fi
  else
    echo "Vault is already unsealed."
  fi
else
  echo "[1/7] Initialising Vault (1 key share, threshold 1)..."
  INIT_RESULT=$(vault_cmd operator init -key-shares=1 -key-threshold=1 -format=json)
  
  mkdir -p "$(dirname "$INIT_OUTPUT")"
  echo "$INIT_RESULT" > "$INIT_OUTPUT"
  chmod 600 "$INIT_OUTPUT"
  echo "  Init data saved to: $INIT_OUTPUT"
  
  UNSEAL_KEY=$(echo "$INIT_RESULT" | jq -r '.unseal_keys_b64[0]')
  ROOT_TOKEN=$(echo "$INIT_RESULT" | jq -r '.root_token')
  
  echo "[2/7] Unsealing Vault..."
  vault_cmd operator unseal "$UNSEAL_KEY" >/dev/null
  echo "  Unsealed."
fi

# Load root token
if [ -z "${ROOT_TOKEN:-}" ]; then
  if [ -f "$INIT_OUTPUT" ]; then
    ROOT_TOKEN=$(jq -r '.root_token' "$INIT_OUTPUT")
  else
    echo "ERROR: Cannot determine root token."
    exit 1
  fi
fi

export VAULT_TOKEN="$ROOT_TOKEN"

echo "[3/7] Enabling KV v2 secrets engine at secret/..."
vault_cmd secrets enable -path=secret kv-v2 2>/dev/null \
  && echo "  Enabled." \
  || echo "  Already enabled (skipping)."

echo "[4/9] Writing CI policy (read-only, exact paths only)..."
vault_cmd policy write ci-policy - <<'POLICY'
# ci-policy — allows the CI agent to read secrets under secret/data/
# No list capability, no wildcard writes.
path "secret/data/+/+" {
  capabilities = ["read"]
}
POLICY
echo "  Policy written."

echo "[5/9] Enabling AppRole auth..."
vault_cmd auth enable approle 2>/dev/null \
  && echo "  Enabled." \
  || echo "  Already enabled (skipping)."

echo "[6/9] Creating ci-role (short TTLs, single-use secret_id)..."
vault_cmd write auth/approle/role/ci-role \
  token_policies="ci-policy" \
  token_ttl=10m \
  token_max_ttl=10m \
  secret_id_ttl=5m \
  secret_id_num_uses=1
echo "  Role created (token_ttl=10m, secret_id_ttl=5m, single-use)."

echo "[7/9] Writing bootstrap policy (narrow scope: secret-id generation only)..."
vault_cmd policy write bootstrap-policy - <<'POLICY'
# bootstrap-policy — can ONLY generate secret_ids for the ci-role.
# If this token leaks, blast radius is limited to secret-id generation.
path "auth/approle/role/ci-role/secret-id" {
  capabilities = ["update"]
}
POLICY
echo "  Bootstrap policy written."

echo "[8/9] Creating scoped bootstrap token (non-expiring, narrow policy)..."
BOOTSTRAP_TOKEN=$(vault_cmd token create \
  -policy="bootstrap-policy" \
  -ttl="0" \
  -display-name="ci-bootstrap" \
  -format=json | jq -r '.auth.client_token')
echo "  Bootstrap token created."

echo "[9/9] Fetching role_id..."
ROLE_ID=$(vault_cmd read -format=json auth/approle/role/ci-role/role-id | jq -r '.data.role_id')

mkdir -p "$(dirname "$ROLE_OUTPUT")"
cat > "$ROLE_OUTPUT" <<EOF
# Generated by vault-init.sh on $(date -u +"%Y-%m-%dT%H:%M:%SZ")
# role_id is non-secret — safe to store in CI config
VAULT_ROLE_ID=${ROLE_ID}
# Bootstrap token — scoped to secret-id generation ONLY (not root)
# Store in a file with 600 perms or inject via Proton Pass
VAULT_BOOTSTRAP_TOKEN=${BOOTSTRAP_TOKEN}
EOF
chmod 600 "$ROLE_OUTPUT"

echo ""
echo "=== Vault Initialisation Complete ==="
echo ""
echo "  Role ID:          ${ROLE_ID}"
echo "  Bootstrap token:  (scoped to secret-id generation only)"
echo ""
echo "  Init data:    ${INIT_OUTPUT}"
echo "  AppRole creds: ${ROLE_OUTPUT}"
echo ""
echo "Next steps:"
echo "  1. Store VAULT_ROLE_ID in your secrets.env (non-secret)"
echo "  2. Store VAULT_BOOTSTRAP_TOKEN in secrets.env or a 600-perm file"
echo "  3. Write project secrets:  vault kv put secret/myapp/prod DB_URL=... API_KEY=..."
echo "  4. The agent will auto-generate single-use secret_ids per job"
echo ""
