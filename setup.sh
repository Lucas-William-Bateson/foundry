#!/bin/bash
set -e

echo "=== Foundry Setup ==="
echo ""

if ! command -v brew &> /dev/null; then
    echo "Error: Homebrew not found. Install it first: https://brew.sh"
    exit 1
fi

echo "[1/4] Installing dependencies..."
brew install cloudflared postgresql@16 || true

echo ""
echo "[2/4] Starting PostgreSQL..."
cd "$(dirname "$0")"
docker compose -f docker/compose.yml up -d

sleep 3

echo ""
echo "[3/4] Initializing database..."
PGPASSWORD=postgres psql -h localhost -U postgres -d foundry -f migrations/001_init.sql 2>/dev/null || echo "Schema already exists or applied"

echo ""
echo "[4/4] Building Foundry..."
cargo build --release

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo ""
echo "1. Create a GitHub App in your org:"
echo "   - Go to: https://github.com/organizations/YOUR_ORG/settings/apps/new"
echo "   - Name: Foundry CI"
echo "   - Homepage URL: http://localhost:8080"
echo "   - Webhook: Leave unchecked (we use org webhooks)"
echo "   - Permissions:"
echo "     - Repository > Contents: Read-only"
echo "     - Repository > Metadata: Read-only"
echo "   - Install the app on your org (all repos or select repos)"
echo "   - Download the private key (.pem file)"
echo "   - Note the App ID and Installation ID"
echo ""
echo "2. Create config/foundry.env with:"
echo "   DATABASE_URL=postgres://postgres:postgres@localhost:5432/foundry"
echo "   GITHUB_WEBHOOK_SECRET=<generate with: openssl rand -hex 32>"
echo "   FOUNDRY_ENABLE_TUNNEL=true"
echo "   GITHUB_APP_ID=<your app id>"
echo "   GITHUB_INSTALLATION_ID=<your installation id>"
echo "   GITHUB_APP_PRIVATE_KEY_PATH=./config/github-app.pem"
echo ""
echo "3. Start the server:"
echo "   source config/foundry.env && ./target/release/foundryd"
echo ""
echo "4. Copy the tunnel URL and create an org webhook:"
echo "   - Go to: https://github.com/organizations/YOUR_ORG/settings/hooks/new"
echo "   - Payload URL: <tunnel url>/webhook/github"
echo "   - Content type: application/json"
echo "   - Secret: <same as GITHUB_WEBHOOK_SECRET>"
echo "   - Events: Just the push event"
echo ""
echo "5. Start the agent (in another terminal):"
echo "   source config/foundry.env && ./target/release/foundry-agent"
