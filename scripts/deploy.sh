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

# Generate secrets.env from Proton Pass vault
if command -v pass-cli &>/dev/null && [ -f "$DEPLOY_DIR/secrets.env.template" ]; then
    echo "Generating secrets.env from Proton Pass vault..."
    pass-cli inject -i "$DEPLOY_DIR/secrets.env.template" -o "$DEPLOY_DIR/secrets.env" --force
else
    echo "pass-cli not found or no template — copying existing secrets.env..."
    cp /app/secrets.env "$DEPLOY_DIR/secrets.env"
fi

cp /app/.env "$DEPLOY_DIR/.env" 2>/dev/null || true

echo "Rebuilding containers (no cache)..."
docker compose -p "$PROJECT_NAME" build --no-cache

echo "Restarting services..."
docker compose -p "$PROJECT_NAME" up -d --force-recreate --no-deps postgres foundryd cloudflared

echo "Waiting for foundryd to be healthy..."
sleep 10

echo "Scheduling agent restart..."
# Run agent restart in background and detach - the current agent container
# can't restart itself while this script is running inside it
nohup sh -c "sleep 2 && cd $DEPLOY_DIR && docker compose -p $PROJECT_NAME up -d --force-recreate --no-deps agent && docker image prune -f && rm -rf $DEPLOY_DIR" > /tmp/agent-restart.log 2>&1 &

echo "=== Deploy complete (agent will restart in background) ==="
