# Foundry CI/CD

A minimal, self-hosted CI/CD system written in Rust. Replace GitHub Actions with something you control.

## Architecture

```
GitHub (webhook) → foundryd (server) → PostgreSQL (job queue) → foundry-agent (runner) → Docker
```

- **foundryd**: HTTP server that receives GitHub webhooks and manages the job queue
- **foundry-agent**: Polls for jobs and executes them in Docker containers
- **foundry-core**: Shared types and utilities

## Quick Start

### 1. Start PostgreSQL

```bash
docker compose -f docker/compose.yml up -d
```

### 2. Initialize the database

```bash
psql -h localhost -U postgres -d foundry -f migrations/001_init.sql
```

### 3. Configure environment

```bash
# Server
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/foundry
export GITHUB_WEBHOOK_SECRET=your-secret-here

# Agent
export FOUNDRY_SERVER_URL=http://localhost:8080
export FOUNDRY_AGENT_ID=mac-mini-agent
export FOUNDRY_DEFAULT_COMMAND="echo 'Hello from Foundry!'"
```

### 4. Build and run

```bash
# Build everything
cargo build --release

# Run the server (in one terminal)
cargo run --release -p foundryd

# Run the agent (in another terminal)
cargo run --release -p foundry-agent
```

### 5. Set up GitHub webhook

1. Go to your repo/org settings → Webhooks
2. Add webhook:
   - **Payload URL**: `https://your-server/webhook/github`
   - **Content type**: `application/json`
   - **Secret**: Same as `GITHUB_WEBHOOK_SECRET`
   - **Events**: Just the `push` event

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

| Variable                  | Description                  | Default                 |
| ------------------------- | ---------------------------- | ----------------------- |
| `FOUNDRY_SERVER_URL`      | URL of foundryd server       | `http://localhost:8080` |
| `FOUNDRY_AGENT_ID`        | Unique agent identifier      | Auto-generated          |
| `FOUNDRY_WORKSPACE_DIR`   | Directory for job workspaces | `/tmp/foundry`          |
| `FOUNDRY_POLL_INTERVAL`   | Seconds between job polls    | `5`                     |
| `FOUNDRY_DEFAULT_COMMAND` | Command to run in containers | `echo 'No command'`     |

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
