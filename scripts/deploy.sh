#!/bin/bash
set -euo pipefail

REPO_URL="https://github.com/Lucas-William-Bateson/foundry.git"
DEPLOY_DIR="/tmp/foundry-deploy"
PROJECT_NAME="foundry"

echo "=== Foundry Self-Deploy ==="
echo "Timestamp: $(date)"

echo "Cloning fresh copy..."
rm -rf "$DEPLOY_DIR"

if [ -n "${GITHUB_TOKEN:-}" ]; then
    AUTH_URL="https://x-access-token:${GITHUB_TOKEN}@github.com/Lucas-William-Bateson/foundry.git"
    git clone --depth 1 "$AUTH_URL" "$DEPLOY_DIR"
else
    git clone --depth 1 "$REPO_URL" "$DEPLOY_DIR"
fi

cd "$DEPLOY_DIR"

export GITHUB_APP_PRIVATE_KEY_FILE="${HOST_PRIVATE_KEY_PATH:-/root/.config/foundry/github-app.pem}"

cp /app/secrets.env "$DEPLOY_DIR/secrets.env"
cp /app/.env "$DEPLOY_DIR/.env" 2>/dev/null || true

echo "Rebuilding containers..."
docker compose -p "$PROJECT_NAME" build

echo "Restarting services (excluding agent)..."
docker compose -p "$PROJECT_NAME" up -d --force-recreate --no-deps postgres foundryd cloudflared

echo "Waiting for foundryd to be healthy..."
sleep 10

echo "Cleaning up..."
docker image prune -f
rm -rf "$DEPLOY_DIR"

echo "=== Deploy complete ==="
echo "NOTE: Agent was NOT restarted. It will use new code on next container restart."
echo "To manually restart agent: docker compose -p foundry up -d --force-recreate agent"
