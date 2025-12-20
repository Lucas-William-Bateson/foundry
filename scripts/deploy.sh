#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Foundry Self-Deploy ==="
echo "Timestamp: $(date)"

echo "Pulling latest changes..."
git pull origin main

echo "Rebuilding containers..."
docker compose build

echo "Restarting services..."
docker compose up -d --force-recreate

echo "Cleaning up old images..."
docker image prune -f

echo "=== Deploy complete ==="
docker compose ps
