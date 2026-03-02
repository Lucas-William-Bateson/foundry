# Foundry CI/CD

A minimal, self-hosted CI/CD system written in Rust. Replace GitHub Actions with something you control. And don't pay for runners you self host!!!

## Architecture

```
GitHub (webhook) → foundryd (server) → PostgreSQL (job queue) → foundry-agent (runner) → Docker
```

- **foundryd**: HTTP server that receives GitHub webhooks and manages the job queue
- **foundry-agent**: Polls for jobs and executes them in Docker containers
- **foundry-core**: Shared types and utilities

## Quick Start (Docker)

### 1. Create secrets.env

```bash
cp config/foundry.env.example secrets.env
vim secrets.env  # Add your values
```

### 2. Start services

```bash
docker compose up -d
```

### 3. Set up GitHub webhook

1. Go to your GitHub org settings → Webhooks
2. Add webhook:
   - **Payload URL**: `https://your-domain/webhook/github`
   - **Content type**: `application/json`
   - **Secret**: Same as `GITHUB_WEBHOOK_SECRET` in secrets.env
   - **Events**: Select `push` and `pull_request` events

## Self-Deployment

Foundry can deploy itself! When you push to the Foundry repo, it will:

1. Detect the push is to its own repo
2. Run the deploy script instead of a Docker container
3. Pull, rebuild, and restart all services

To enable, set in docker-compose.yml:

```
FOUNDRY_SELF_REPO=your-org/foundry
```

## Deploying Apps with foundry.toml

Add a `foundry.toml` to your repo to configure builds and deployments:

```toml
[build]
dockerfile = "Dockerfile"    # Build from Dockerfile
# image = "node:20-alpine"   # Or use pre-built image
command = "npm test"         # CI command (when no [deploy] section)
timeout = 1800               # Build timeout in seconds (default: 30 min)

[triggers]
branches = ["main", "master"]  # Branches to build on push
pull_requests = true           # Build pull requests (default: true)
# pr_target_branches = ["main"] # Only build PRs targeting these branches

[schedule]
cron = "0 0 * * *"           # Run daily at midnight
branch = "main"              # Branch to build
enabled = true               # Toggle schedule on/off
timezone = "UTC"             # Timezone for cron expression

[deploy]
name = "my-app"              # Container/project name (triggers deploy mode)
domain = "myapp.l3s.me"      # Your app's domain (auto-configured via Cloudflare)
port = 3000                  # Port to expose
# compose_file = "docker-compose.yml"  # For complex deployments

[env]
NODE_ENV = "production"
```

**Modes:**

- No `[deploy]` section: Runs `build.command` in a container, then exits (CI mode)
- `[deploy]` with `name`: Builds image, runs persistent container with `--restart unless-stopped`
- `[deploy]` with `compose_file`: Runs `docker compose up -d --build`

**Triggers:**

- **Push builds**: Triggered when pushing to branches listed in `triggers.branches`
- **Pull request builds**: Triggered on PR open/sync if `triggers.pull_requests = true`
- **Re-runs**: Any completed build can be re-run from the dashboard
- **Scheduled builds**: Triggered by cron expression in `[schedule]` section

**Scheduled Builds:**

The `[schedule]` section allows you to run builds on a cron schedule. The schedule is synced from your `foundry.toml` to the server on each build, so you can update it by pushing changes. Schedules can be viewed, toggled, and deleted from the dashboard.

## Vault Secrets Management

Foundry includes a built-in HashiCorp Vault integration for injecting secrets into CI jobs. Secrets are fetched at runtime via AppRole authentication — no secrets touch disk or source control.

### How it works

1. Vault runs as a Docker service alongside foundry
2. Secrets are stored in Vault's KV v2 engine (e.g. `secret/myapp/prod`)
3. Projects declare their Vault path in `foundry.toml`
4. Before each job, the agent generates a **single-use** `secret_id`, authenticates via AppRole, fetches secrets, and injects them as environment variables

### Setup

```bash
# 1. Start all services (including Vault)
docker compose up -d

# 2. Initialise Vault (first time only)
./scripts/vault-init.sh

# 3. Store secrets
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=$(jq -r '.root_token' config/vault/init.json)
vault kv put secret/myapp/prod DB_URL="postgres://..." API_KEY="abc123"

# 4. Add role_id and bootstrap token to secrets.env
# (vault-init.sh outputs these — see config/vault/approle.env)
```

After a restart, Vault will be sealed. Unseal with:

```bash
./scripts/vault-unseal.sh
```

### Per-project configuration

Add a `[secrets]` section to your repo's `foundry.toml`:

```toml
[secrets]
vault_path = "myapp/prod"         # KV v2 path (reads secret/data/myapp/prod)
# keys = ["DB_URL", "API_KEY"]    # Optional: only inject specific keys
```

All key-value pairs at that path are injected as environment variables before stages, deploy, or container execution.

### Security model

| Component         | Where it lives                 | Sensitivity |
| ----------------- | ------------------------------ | ----------- |
| `role_id`         | `secrets.env` / CI config      | Non-secret  |
| `bootstrap_token` | File with `600` perms on host  | Secret      |
| `secret_id`       | Generated per job, single-use  | Ephemeral   |
| Client token      | In-memory only, 15 min TTL     | Ephemeral   |
| Unseal key        | `config/vault/init.json` (600) | Critical    |

**Timeouts:**

Builds automatically timeout after `build.timeout` seconds (default: 1800 = 30 minutes). Timed out builds are marked as failed.

**Automatic Domain Routing:**

When you specify a `domain` in `foundry.toml`, Foundry will automatically:

1. Add a route to your shared Cloudflare tunnel
2. Create/update the DNS CNAME record
3. Your app is instantly accessible at that domain!

## Development (without Docker)

### 1. Start PostgreSQL

```bash
docker compose up -d postgres
```

### 2. Initialize the database

```bash
psql -h localhost -U foundry -d foundry -f migrations/001_init.sql
```

### 3. Run locally

```bash
# Server
cargo run -p foundryd

# Agent (another terminal)
cargo run -p foundry-agent
```

## Environment Variables

### Server (foundryd)

| Variable                | Description                           | Default                      |
| ----------------------- | ------------------------------------- | ---------------------------- |
| `DATABASE_URL`          | PostgreSQL connection string          | (required)                   |
| `GITHUB_WEBHOOK_SECRET` | Secret for webhook verification       | (required)                   |
| `FOUNDRY_BIND_ADDR`     | Address to bind server                | `0.0.0.0:8080`               |
| `FOUNDRY_ENABLE_TUNNEL` | Enable Cloudflare tunnel              | `false`                      |
| `CF_ACCOUNT_ID`         | Cloudflare account ID                 | (required if tunnel enabled) |
| `CF_API_TOKEN`          | Cloudflare API token                  | (required if tunnel enabled) |
| `CF_ZONE_ID`            | Cloudflare zone ID                    | (required if tunnel enabled) |
| `CF_TUNNEL_NAME`        | Name for the tunnel                   | `foundry`                    |
| `CF_TUNNEL_DOMAIN`      | Domain to route (e.g. ci.example.com) | (required if tunnel enabled) |

### Agent (foundry-agent)

| Variable                     | Description                     | Default                 |
| ---------------------------- | ------------------------------- | ----------------------- |
| `FOUNDRY_SERVER_URL`         | URL of foundryd server          | `http://localhost:8080` |
| `FOUNDRY_AGENT_ID`           | Unique agent identifier         | Auto-generated          |
| `FOUNDRY_WORKSPACE_DIR`      | Directory for job workspaces    | `/tmp/foundry`          |
| `FOUNDRY_POLL_INTERVAL`      | Seconds between job polls       | `5`                     |
| `FOUNDRY_DEFAULT_COMMAND`    | Command to run in containers    | `echo 'No command'`     |
| `VAULT_ADDR`                 | Vault server address            | (optional)              |
| `VAULT_ROLE_ID`              | AppRole role ID (non-secret)    | (optional)              |
| `VAULT_BOOTSTRAP_TOKEN`      | Token for generating secret_ids | (optional)              |
| `VAULT_BOOTSTRAP_TOKEN_FILE` | File containing bootstrap token | (optional)              |

## Exposing to the Internet

### Cloudflare Tunnel (Recommended)

Foundry has built-in Cloudflare tunnel support via the API. This creates a persistent tunnel with your custom domain.

1. **Get your Cloudflare credentials**:
   - Account ID: Dashboard → right sidebar → "Account ID"
   - Zone ID: Dashboard → your domain → right sidebar → "Zone ID"
   - API Token: Profile → API Tokens → Create Token
     - Use "Edit Cloudflare Tunnel" template
     - Also add DNS:Edit permission for your zone

2. **Install cloudflared**:

   ```bash
   brew install cloudflared
   ```

3. **Configure environment**:

   ```bash
   export FOUNDRY_ENABLE_TUNNEL=true
   export CF_ACCOUNT_ID=your_account_id
   export CF_API_TOKEN=your_api_token
   export CF_ZONE_ID=your_zone_id
   export CF_TUNNEL_NAME=foundry
   export CF_TUNNEL_DOMAIN=ci.yourdomain.com
   ```

4. When you start foundryd, it will:
   - Create or reuse a tunnel named "foundry"
   - Configure routing from your domain to localhost
   - Set up DNS CNAME record automatically
   - Start cloudflared with the tunnel token

### Manual Options

If you prefer not to use the built-in tunnel:

1. **cloudflared**: `cloudflared tunnel --url http://localhost:8080`
2. **ngrok**: `ngrok http 8080`
3. **Tailscale Funnel**: If you use Tailscale

## Security

- Webhook signatures are **always** verified before processing
- Jobs are claimed atomically using `FOR UPDATE SKIP LOCKED`
- Claim tokens prevent unauthorized job status updates

## Roadmap

- [ ] Read `.foundry.yml` from repos for job configuration
- [ ] Post GitHub Checks for build status
- [ ] Artifact storage (MinIO/S3)
- [ ] Web UI for viewing jobs and logs
- [ ] Multiple job steps
- [ ] Caching between builds
