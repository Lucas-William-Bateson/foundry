#!/bin/bash
set -euo pipefail

REPO_URL="https://github.com/Lucas-William-Bateson/foundry.git"
DEPLOY_DIR="/tmp/foundry-deploy"

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

echo "Rebuilding containers..."
docker compose build

echo "Restarting services..."
docker compose up -d --force-recreate

echo "Cleaning up..."
docker image prune -f
rm -rf "$DEPLOY_DIR"

echo "=== Deploy complete ==="
docker compose ps
