#!/bin/bash
# =============================================================================
# restore.sh — Restore Foundry from a backup
#
# Restores PostgreSQL, Vault, and environment files from a backup directory
# created by backup.sh.
#
# Usage:
#   ./scripts/restore.sh ~/foundry-backups/latest     # restore from latest
#   ./scripts/restore.sh ~/foundry-backups/20260310-183000  # specific backup
#
# This script will:
#   1. Restore environment files (.env, secrets.env)
#   2. Restore Vault credentials (init.json, approle.env)
#   3. Restore vault-seed.sh
#   4. Re-initialize Vault if needed (init + unseal + seed)
#   5. Restore PostgreSQL from dump
# =============================================================================
set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <backup-directory>"
  echo ""
  echo "Available backups:"
  ls -1d ~/foundry-backups/2* 2>/dev/null | sort -r | head -10
  exit 1
fi

BACKUP_DIR="$(cd "$1" && pwd)"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ ! -f "$BACKUP_DIR/backup-info.txt" ]; then
  echo "ERROR: $BACKUP_DIR does not look like a valid Foundry backup"
  exit 1
fi

cd "$PROJECT_DIR"

echo "=== Foundry Restore ==="
echo "Backup: $BACKUP_DIR"
echo ""
cat "$BACKUP_DIR/backup-info.txt" | head -6
echo ""

read -p "This will overwrite current config. Continue? [y/N] " -r
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

# --- 1. Restore environment files ---
echo ""
echo "[1/5] Restoring environment files..."
for f in .env secrets.env; do
  if [ -f "$BACKUP_DIR/env/$f" ]; then
    cp "$BACKUP_DIR/env/$f" "$PROJECT_DIR/$f"
    chmod 600 "$PROJECT_DIR/$f"
    echo "  ✓ $f"
  fi
done

# --- 2. Restore Vault credentials ---
echo "[2/5] Restoring Vault credentials..."
mkdir -p config/vault
for f in init.json approle.env; do
  if [ -f "$BACKUP_DIR/vault/$f" ]; then
    cp "$BACKUP_DIR/vault/$f" "config/vault/$f"
    chmod 600 "config/vault/$f"
    echo "  ✓ config/vault/$f"
  fi
done

# --- 3. Restore vault-seed.sh ---
echo "[3/5] Restoring vault-seed.sh..."
if [ -f "$BACKUP_DIR/vault/vault-seed.sh" ]; then
  cp "$BACKUP_DIR/vault/vault-seed.sh" scripts/vault-seed.sh
  chmod 700 scripts/vault-seed.sh
  echo "  ✓ scripts/vault-seed.sh"
else
  echo "  ⚠ vault-seed.sh not in backup — secrets may need manual re-entry"
fi

# --- 4. Re-initialize Vault if needed ---
echo "[4/5] Checking Vault..."
if docker compose ps vault 2>/dev/null | grep -q "running"; then
  VAULT_INIT=$(docker compose exec -T vault vault status -format=json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('initialized', False))" 2>/dev/null || echo "False")
  VAULT_SEALED=$(docker compose exec -T vault vault status -format=json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('sealed', True))" 2>/dev/null || echo "True")

  if [ "$VAULT_INIT" = "False" ]; then
    echo "  Vault not initialized — running vault-init.sh..."
    bash scripts/vault-init.sh
    echo "  Seeding Vault secrets..."
    bash scripts/vault-seed.sh
  elif [ "$VAULT_SEALED" = "True" ]; then
    echo "  Vault is sealed — unsealing..."
    bash scripts/vault-unseal.sh
    echo "  Re-seeding Vault secrets..."
    bash scripts/vault-seed.sh
  else
    echo "  ✓ Vault is initialized and unsealed"
    echo "  Re-seeding Vault secrets (in case data was lost)..."
    bash scripts/vault-seed.sh
  fi
else
  echo "  ⚠ Vault container not running — start services first, then run:"
  echo "    ./scripts/vault-init.sh && ./scripts/vault-seed.sh"
fi

# --- 5. Restore PostgreSQL ---
echo "[5/5] Restoring PostgreSQL..."
if [ -f "$BACKUP_DIR/postgres.dump" ]; then
  if docker compose ps postgres 2>/dev/null | grep -q "running"; then
    # Drop and recreate to get a clean restore
    docker compose exec -T postgres dropdb -U foundry --if-exists foundry 2>/dev/null || true
    docker compose exec -T postgres createdb -U foundry foundry 2>/dev/null || true
    docker compose exec -T postgres pg_restore -U foundry -d foundry --no-owner --no-acl < "$BACKUP_DIR/postgres.dump" 2>/dev/null
    echo "  ✓ PostgreSQL restored"
  else
    echo "  ⚠ PostgreSQL not running — restore manually:"
    echo "    docker compose exec -T postgres pg_restore -U foundry -d foundry < $BACKUP_DIR/postgres.dump"
  fi
else
  echo "  ⚠ No postgres.dump in backup — database not restored"
fi

echo ""
echo "=== Restore Complete ==="
echo ""
echo "Next steps:"
echo "  1. Restart services:  docker compose up -d --pull never"
echo "  2. Verify health:     docker compose ps"
echo "  3. Check logs:        docker compose logs --tail 20"
