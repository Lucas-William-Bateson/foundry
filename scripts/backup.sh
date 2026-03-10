#!/bin/bash
# =============================================================================
# backup.sh — Automated backup of Foundry's critical data
#
# Backs up PostgreSQL, Vault config/keys, and env files to the host filesystem
# OUTSIDE the Docker/Colima VM so they survive VM deletion.
#
# Usage:
#   ./scripts/backup.sh              # full backup
#   ./scripts/backup.sh --quiet      # suppress non-error output (for cron)
#
# Backups are stored in: ~/foundry-backups/
# Retention: last 30 backups kept, older ones pruned automatically.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BACKUP_ROOT="${FOUNDRY_BACKUP_DIR:-$HOME/foundry-backups}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="$BACKUP_ROOT/$TIMESTAMP"
MAX_BACKUPS=30
QUIET="${1:-}"

log() {
  [ "$QUIET" = "--quiet" ] && return
  echo "$@"
}

err() {
  echo "ERROR: $*" >&2
}

cd "$PROJECT_DIR"

# Ensure Docker is reachable
if ! docker info >/dev/null 2>&1; then
  err "Docker is not running. Cannot back up."
  exit 1
fi

mkdir -p "$BACKUP_DIR"

FAILED=0

# --- 1. PostgreSQL dump ---
log "[1/4] Backing up PostgreSQL..."
if docker compose exec -T postgres pg_dump -U foundry -Fc foundry > "$BACKUP_DIR/postgres.dump" 2>/dev/null; then
  SIZE=$(du -h "$BACKUP_DIR/postgres.dump" | cut -f1)
  log "  ✓ postgres.dump ($SIZE)"
else
  err "PostgreSQL backup failed"
  rm -f "$BACKUP_DIR/postgres.dump"
  FAILED=$((FAILED + 1))
fi

# --- 2. Vault config and keys ---
log "[2/4] Backing up Vault credentials..."
mkdir -p "$BACKUP_DIR/vault"
for f in config/vault/init.json config/vault/approle.env config/vault/vault.hcl; do
  if [ -f "$f" ]; then
    cp "$f" "$BACKUP_DIR/vault/"
    chmod 600 "$BACKUP_DIR/vault/$(basename "$f")" 2>/dev/null || true
    log "  ✓ $f"
  else
    log "  - $f (not found, skipping)"
  fi
done

# --- 3. Environment files ---
log "[3/4] Backing up environment files..."
mkdir -p "$BACKUP_DIR/env"
for f in .env secrets.env; do
  if [ -f "$f" ]; then
    cp "$f" "$BACKUP_DIR/env/"
    chmod 600 "$BACKUP_DIR/env/$f"
    log "  ✓ $f"
  fi
done

# Also back up vault-seed.sh (contains hardcoded secrets, not in git)
if [ -f scripts/vault-seed.sh ]; then
  cp scripts/vault-seed.sh "$BACKUP_DIR/vault/"
  chmod 600 "$BACKUP_DIR/vault/vault-seed.sh"
  log "  ✓ scripts/vault-seed.sh"
fi

# --- 4. Vault status snapshot (for diagnostics) ---
log "[4/4] Recording Vault status..."
if docker compose exec -T vault vault status -format=json > "$BACKUP_DIR/vault/status.json" 2>/dev/null; then
  log "  ✓ vault status recorded"
else
  log "  - vault status unavailable (may be sealed)"
fi

# --- Metadata ---
cat > "$BACKUP_DIR/backup-info.txt" <<EOF
Foundry Backup
==============
Timestamp:  $TIMESTAMP
Host:       $(hostname)
Directory:  $BACKUP_DIR
Git commit: $(git -C "$PROJECT_DIR" rev-parse --short HEAD 2>/dev/null || echo "unknown")
Git branch: $(git -C "$PROJECT_DIR" branch --show-current 2>/dev/null || echo "unknown")

Contents:
  postgres.dump      — PostgreSQL custom-format dump
  vault/init.json    — Vault unseal keys and root token
  vault/approle.env  — AppRole credentials (role_id, bootstrap_token)
  vault/vault.hcl    — Vault server configuration
  vault/vault-seed.sh— Secret seeding script
  vault/status.json  — Vault status at backup time
  env/.env           — Docker Compose environment variables
  env/secrets.env    — Application secrets

Restore with:
  ./scripts/restore.sh $BACKUP_DIR
EOF

# --- Prune old backups ---
BACKUP_COUNT=$(find "$BACKUP_ROOT" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')
if [ "$BACKUP_COUNT" -gt "$MAX_BACKUPS" ]; then
  PRUNE_COUNT=$((BACKUP_COUNT - MAX_BACKUPS))
  log ""
  log "Pruning $PRUNE_COUNT old backup(s)..."
  find "$BACKUP_ROOT" -mindepth 1 -maxdepth 1 -type d | sort | head -n "$PRUNE_COUNT" | while read -r old; do
    rm -rf "$old"
    log "  ✗ removed $(basename "$old")"
  done
fi

# --- Summary ---
TOTAL_SIZE=$(du -sh "$BACKUP_DIR" | cut -f1)
log ""
if [ "$FAILED" -eq 0 ]; then
  log "✅ Backup complete: $BACKUP_DIR ($TOTAL_SIZE)"
else
  err "⚠️  Backup finished with $FAILED error(s): $BACKUP_DIR ($TOTAL_SIZE)"
  exit 1
fi

# Write a latest symlink for easy access
ln -sfn "$BACKUP_DIR" "$BACKUP_ROOT/latest"
