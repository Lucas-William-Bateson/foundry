#!/bin/bash
set -euo pipefail

REPO_URL="https://github.com/${FOUNDRY_REPO:-Lucas-William-Bateson/foundry}.git"
DEPLOY_DIR="/tmp/foundry-deploy"
PROJECT_NAME="foundry"

echo "=== Foundry Self-Deploy ==="
echo "Timestamp: $(date)"

# Back up before deploying — protects against data loss during upgrades
echo "Running pre-deploy backup..."
if [ -x "/app/scripts/backup.sh" ]; then
  bash /app/scripts/backup.sh --quiet 2>/dev/null \
    || echo "Warning: backup failed, continuing deploy anyway"
else
  echo "Warning: backup.sh not found, skipping pre-deploy backup"
fi

CLONE_URL="$REPO_URL"
if [ -n "${GITHUB_TOKEN:-}" ]; then
    CLONE_URL="https://x-access-token:${GITHUB_TOKEN}@github.com/${FOUNDRY_REPO:-Lucas-William-Bateson/foundry}.git"
fi

# Reuse existing clone if possible; fall back to fresh clone on failure
if [ -d "$DEPLOY_DIR/.git" ]; then
    echo "Updating existing clone..."
    cd "$DEPLOY_DIR"
    git remote set-url origin "$CLONE_URL"
    if ! git fetch --depth 1 origin main; then
        echo "Fetch failed, falling back to fresh clone..."
        cd /
        rm -rf "$DEPLOY_DIR"
        git clone --depth 1 "$CLONE_URL" "$DEPLOY_DIR"
        cd "$DEPLOY_DIR"
    else
        git reset --hard origin/main
        git clean -fdx --exclude=secrets.env --exclude=.env
    fi
else
    echo "Cloning fresh copy..."
    rm -rf "$DEPLOY_DIR"
    git clone --depth 1 "$CLONE_URL" "$DEPLOY_DIR"
    cd "$DEPLOY_DIR"
fi

export GITHUB_APP_PRIVATE_KEY_FILE="${HOST_PRIVATE_KEY_PATH:-/root/.config/foundry/github-app.pem}"

cp /app/secrets.env "$DEPLOY_DIR/secrets.env" 2>/dev/null || true

cp /app/.env "$DEPLOY_DIR/.env" 2>/dev/null || true

# Pull base images in parallel with build context preparation
echo "Pulling base images and rebuilding containers..."
docker compose -p "$PROJECT_NAME" pull --quiet 2>/dev/null &
PULL_PID=$!

# Build with layer caching (much faster when only code changed)
docker compose -p "$PROJECT_NAME" build
wait "$PULL_PID" 2>/dev/null || true

echo "Restarting services..."
docker compose -p "$PROJECT_NAME" up -d --force-recreate --no-deps postgres foundryd cloudflared

echo "Waiting for foundryd to be healthy..."
HEALTH_TIMEOUT=60
HEALTH_INTERVAL=2
ELAPSED=0
while [ "$ELAPSED" -lt "$HEALTH_TIMEOUT" ]; do
    if docker inspect --format='{{.State.Health.Status}}' "$(docker compose -p "$PROJECT_NAME" ps -q foundryd 2>/dev/null)" 2>/dev/null | grep -q "healthy"; then
        echo "foundryd is healthy after ${ELAPSED}s"
        break
    fi
    sleep "$HEALTH_INTERVAL"
    ELAPSED=$((ELAPSED + HEALTH_INTERVAL))
done
if [ "$ELAPSED" -ge "$HEALTH_TIMEOUT" ]; then
    echo "Warning: foundryd health check timed out after ${HEALTH_TIMEOUT}s, proceeding anyway..."
fi

echo "Scheduling agent restart..."
# Run agent restart in background and detach - the current agent container
# can't restart itself while this script is running inside it
nohup sh -c "sleep 2 && cd $DEPLOY_DIR && docker compose -p $PROJECT_NAME up -d --force-recreate --no-deps agent && docker image prune -f && rm -rf $DEPLOY_DIR" > /tmp/agent-restart.log 2>&1 &

echo "=== Deploy complete (agent will restart in background) ==="
