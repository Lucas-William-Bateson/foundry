# Foundry CI/CD

A minimal, self-hosted CI/CD system written in Rust. Replace GitHub Actions with something you control. And don't pay for runners you self host!!!

## Architecture

```
GitHub (webhook) → foundryd (server) → PostgreSQL (job queue) → foundry-agent (runner) → Docker
```

- **foundryd**: HTTP server that receives GitHub webhooks and manages the job queue
- **foundry-agent**: Polls for jobs and executes them in Docker containers
- **foundry-core**: Shared types and utilities
- **forgefile**: Parser for the Forgefile DSL

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

### 4. Add a Forgefile to your repo

Create a file named `Forgefile` (no extension) in your repository root. See the [Forgefile DSL](#forgefile-dsl) section below for the full reference.

## Self-Deployment

Foundry can deploy itself! When you push to the Foundry repo, it will:

1. Detect the push is to its own repo
2. Run the deploy script instead of a Docker container
3. Pull, rebuild, and restart all services

To enable, set in docker-compose.yml:

```
FOUNDRY_SELF_REPO=your-org/foundry
```

---

## Forgefile DSL

Foundry uses a custom DSL called **Forgefile** to define CI/CD pipelines. Place a file named `Forgefile` in your repository root.

### Minimal Example

```forgefile
on push("main") {
  stage test {
    run "cargo test"
  }
}
```

That's a complete CI pipeline. Push to main → run tests.

### Full Example

```forgefile
runner "default" {
  image = "node:20-slim"
}

secrets from vault("myapp/prod") {
  DB_URL
  API_KEY as MY_API_KEY
}

on push("main", "develop"), pr("main") {
  stage install on runner.default {
    run "npm ci"
    timeout 5m
  }

  stage lint on runner.default {
    needs install
    run "npm run lint"
    allow_failure
  }

  stage test on runner.default {
    needs install
    run "npm test"
    timeout 10m
  }

  stage build on runner.default {
    needs test
    run "npm run build"
    artifacts "dist/"
  }

  stage deploy on runner.default {
    needs build
    condition on_push
    run "scripts/deploy.sh"
    deploy {
      name = "my-app"
      domain = "myapp.l3s.me"
      port = 3000
      compose_file = "docker-compose.yml"
    }
  }
}

on schedule("0 0 * * *", tz: "UTC") {
  stage nightly on runner.default {
    run "npm run test:integration"
    timeout 30m
  }
}
```

---

### Runners

Runners define **where** stages execute. Agents self-register with capabilities on startup, and stages are routed to matching runners.

#### Defining runners

```forgefile
runner "fast" {
  tags  = ["ssd", "x86"]
  cpu   = 4
  mem   = "8G"
  image = "rust:1.87-slim"
}

runner "gpu" {
  tags  = ["nvidia", "cuda"]
  gpu   = 1
  image = "nvidia/cuda:12.6-devel"
  arch  = "x86_64"
}
```

| Field   | Type       | Description                       |
|---------|------------|-----------------------------------|
| `tags`  | `[string]` | Capability tags for matching      |
| `cpu`   | `int`      | Minimum CPU cores                 |
| `mem`   | `string`   | Minimum memory (`"4G"`, `"512M"`) |
| `gpu`   | `int`      | Minimum GPUs                      |
| `arch`  | `string`   | CPU architecture (`x86_64`, `aarch64`) |
| `image` | `string`   | Default Docker image              |

#### Referencing runners in stages

```forgefile
stage test on runner.fast {
  run "cargo test"
}
```

Stages without `on runner.X` can be claimed by any agent.

#### Agent registration

Agents announce their capabilities at startup via environment variables:

```bash
FOUNDRY_RUNNER_NAME=worker-1
FOUNDRY_RUNNER_TAGS=ssd,x86
FOUNDRY_RUNNER_CPU=8
FOUNDRY_RUNNER_MEM=16G
FOUNDRY_RUNNER_GPU=0
FOUNDRY_RUNNER_ARCH=x86_64
```

The server matches jobs to runners automatically — if no runner matches, the job queues until one is available.

---

### Triggers

Trigger blocks define **when** stages run. Multiple triggers can be combined with commas.

#### Push

```forgefile
on push("main", "release/*") {
  stage build { run "make" }
}
```

Branch names support glob patterns (`*` wildcard).

#### Pull Requests

```forgefile
on pr("main") {
  stage test { run "cargo test" }
}
```

Stages inside `pr()` blocks run on pull request open and synchronize events.

#### Schedule

```forgefile
on schedule("0 3 * * *", tz: "Europe/Berlin") {
  stage nightly {
    run "cargo bench"
  }
}
```

Uses standard cron syntax. The `tz` parameter is optional (defaults to UTC).

#### Combined triggers

```forgefile
on push("main"), pr("main") {
  # These stages run on both push to main AND PRs targeting main
  stage test { run "cargo test" }
}
```

---

### Stages

Stages are the individual steps of your pipeline.

```forgefile
stage test on runner.fast {
  needs lint                           # dependency (run after lint)
  env DATABASE_URL = "postgres://..."  # environment variable
  run "cargo test --workspace"         # command to execute
  run "cargo test --doc"               # multiple run commands execute sequentially
  artifacts "target/test-results/**"   # files to store after execution
  output result_path = "test.json"     # outputs passed to downstream stages
  timeout 10m                          # kill after duration
  retry 2                              # retry up to N times on failure
  allow_failure                        # pipeline continues if this stage fails
  condition on_success                 # when to run (see conditions below)
}
```

| Field           | Type       | Description                                           |
|-----------------|------------|-------------------------------------------------------|
| `on runner.X`   | reference  | Route to a specific runner                            |
| `needs`         | identifier | Dependency — run after the named stage completes      |
| `env`           | key=value  | Environment variable (supports `${interpolation}`)    |
| `run`           | string     | Shell command to execute                              |
| `artifacts`     | string     | Glob pattern for files to store                       |
| `output`        | key=value  | Named output accessible by downstream stages          |
| `timeout`       | duration   | Max execution time (`30s`, `10m`, `1h`, `2d`)         |
| `retry`         | int        | Number of retry attempts on failure                   |
| `allow_failure` | flag       | Don't fail the pipeline if this stage fails           |
| `condition`     | keyword    | Execution condition (see below)                       |
| `services`      | `[name]`   | Sidecar services to start (see [Services](#services)) |
| `deploy`        | block      | Deployment metadata (see [Deploy](#deploy))           |

#### Multiline commands

Use triple-quoted strings for multiline commands:

```forgefile
stage test {
  run """
    cargo test --workspace
    cargo test --doc
    cargo clippy -- -D warnings
  """
}
```

#### Dependencies (needs)

Stages form a DAG via `needs`. Foundry executes stages in topological order.

```forgefile
stage lint  { run "cargo clippy" }
stage test  { needs lint; run "cargo test" }
stage build { needs test; run "cargo build --release" }
```

#### Conditions

Control when a stage executes:

| Condition    | Runs when                                 |
|--------------|-------------------------------------------|
| `on_success` | All dependencies succeeded (default)      |
| `on_failure` | Any dependency failed                     |
| `on_push`    | Event is a push (not a PR)                |
| `on_pr`      | Event is a pull request                   |
| `always`     | Always runs regardless of prior failures  |

```forgefile
stage deploy {
  needs build
  condition on_push
  run "scripts/deploy.sh"
}

stage cleanup {
  condition always
  run "scripts/cleanup.sh"
}
```

#### String interpolation

Strings support `${...}` interpolation for variables and stage outputs:

```forgefile
stage build {
  run "cargo build --release"
  output binary = "target/release/myapp"
}

stage containerize {
  needs build
  run "docker build --build-arg BIN=${build.binary} -t myapp:${branch} ."
}
```

Built-in variables: `branch`, `sha`, `repo`, `author`, `event`, `pr_number`, `timestamp`, `arch`, `os`.

---

### Secrets

Secrets are fetched from HashiCorp Vault at runtime and injected as environment variables.

```forgefile
secrets from vault("myapp/prod") {
  DATABASE_URL
  API_KEY
  GITHUB_TOKEN as GH_TOKEN    # rename: fetches GITHUB_TOKEN, injects as GH_TOKEN
}
```

- Keys listed without `as` are injected with their original name
- `KEY as ALIAS` renames the secret when injecting
- All secrets are available to every stage in the pipeline
- Secrets are **never** exposed in logs

#### Vault setup

```bash
# Store secrets
vault kv put secret/myapp/prod \
  DATABASE_URL="postgres://..." \
  API_KEY="abc123" \
  GITHUB_TOKEN="ghp_..."
```

The agent authenticates via AppRole with single-use `secret_id` tokens — no long-lived credentials.

---

### Services

Services are sidecar containers that run alongside stages — databases, caches, message queues.

```forgefile
service postgres {
  image = "postgres:17"
  env POSTGRES_PASSWORD = "test"
  env POSTGRES_DB = "test_db"
  health "pg_isready -U postgres"
  expose 5432
}

service redis {
  image = "redis:7-alpine"
  health "redis-cli ping"
  expose 6379
}
```

| Field    | Type      | Description                        |
|----------|-----------|------------------------------------|
| `image`  | `string`  | Docker image (required)            |
| `env`    | key=value | Environment variables              |
| `health` | `string`  | Health check command               |
| `expose` | `int`     | Port to expose to the stage        |

Reference services in stages:

```forgefile
stage integration_test {
  services [postgres, redis]
  env DATABASE_URL = "postgres://postgres:test@postgres:5432/test_db"
  run "cargo test --features integration"
}
```

Services start before the stage, are health-checked, and torn down after.

---

### Matrix

Matrix builds fan out a stage across variable combinations.

```forgefile
matrix build(target: ["x86_64", "aarch64"], profile: ["debug", "release"]) on runner.fast {
  needs test
  run "cargo build --target ${target}-unknown-linux-gnu --${profile}"
  artifacts "target/${target}-unknown-linux-gnu/${profile}/myapp"
}
```

This expands to 4 parallel stages (2 targets × 2 profiles).

Wait for all matrix expansions:

```forgefile
stage deploy {
  needs build(*)    # waits for ALL matrix combinations to complete
  run "scripts/deploy.sh"
}
```

---

### Deploy

The `deploy` block inside a stage enables deployment mode.

```forgefile
stage deploy on runner.default {
  needs build
  condition on_push
  run "scripts/deploy.sh"
  deploy {
    name = "my-app"
    domain = "myapp.l3s.me"
    port = 3000
    compose_file = "docker-compose.yml"
  }
}
```

| Field          | Type     | Description                                      |
|----------------|----------|--------------------------------------------------|
| `name`         | `string` | Container/project name (required)                |
| `domain`       | `string` | Auto-configured via Cloudflare tunnel + DNS      |
| `port`         | `int`    | Port to expose                                   |
| `compose_file` | `string` | Path to docker-compose.yml for complex deploys   |

**Deploy modes:**

- No `deploy` block → CI mode (run commands, exit)
- `deploy` with `name` → Persistent container (`--restart unless-stopped`)
- `deploy` with `compose_file` → `docker compose up -d --build`
- `domain` specified → Automatic Cloudflare tunnel route + DNS CNAME

---

### Duration Literals

Human-readable durations anywhere a timeout is expected:

| Literal | Meaning    |
|---------|------------|
| `30s`   | 30 seconds |
| `10m`   | 10 minutes |
| `1h`    | 1 hour     |
| `2d`    | 2 days     |

---

### Comments

Line comments start with `#`:

```forgefile
# This is a comment
stage test {
  run "cargo test"  # inline comment
}
```

---

### Complete Reference

Here's every construct in one place:

```forgefile
# Runner definitions — where stages execute
runner "name" {
  tags  = ["tag1", "tag2"]       # capability tags
  cpu   = 4                       # minimum CPU cores
  mem   = "8G"                    # minimum memory
  gpu   = 1                       # minimum GPUs
  arch  = "x86_64"                # required architecture
  image = "rust:1.87-slim"        # default Docker image
}

# Secrets — fetched from Vault at runtime
secrets from vault("app/env") {
  SECRET_KEY                      # injected as SECRET_KEY
  LONG_NAME as SHORT              # injected as SHORT
}

# Services — sidecar containers
service name {
  image = "postgres:17"           # Docker image
  env KEY = "value"               # environment variable
  health "pg_isready"             # health check command
  expose 5432                     # exposed port
}

# Trigger blocks — when to run
on push("main", "release/*"), pr("main") {

  # Stages — individual pipeline steps
  stage name on runner.name {
    needs other_stage              # dependency
    services [svc1, svc2]          # sidecar services
    env KEY = "value"              # environment variable
    env INTERP = "${branch}-build" # string interpolation
    run "command"                  # shell command
    run """                        # multiline command
      line 1
      line 2
    """
    artifacts "path/glob/**"       # files to store
    output key = "value"           # output for downstream stages
    timeout 10m                    # max execution time
    retry 2                        # retry count
    allow_failure                  # don't fail pipeline
    condition on_push              # execution condition
    deploy {                       # deployment metadata
      name = "app"
      domain = "app.example.com"
      port = 8080
      compose_file = "docker-compose.yml"
    }
  }

  # Matrix — fan-out across combinations
  matrix name(var: ["a", "b"]) on runner.name {
    needs other_stage
    run "build --variant ${var}"
  }
}

# Scheduled pipelines
on schedule("0 3 * * *", tz: "UTC") {
  stage name { run "command" }
}
```

---

## Vault Secrets Management

Foundry includes a built-in HashiCorp Vault integration for injecting secrets into CI jobs. Secrets are fetched at runtime via AppRole authentication — no secrets touch disk or source control.

### How it works

1. Vault runs as a Docker service alongside foundry
2. Secrets are stored in Vault's KV v2 engine (e.g. `secret/myapp/prod`)
3. Projects declare their Vault path in their `Forgefile`
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

### Security model

| Component         | Where it lives                 | Sensitivity |
| ----------------- | ------------------------------ | ----------- |
| `role_id`         | `secrets.env` / CI config      | Non-secret  |
| `bootstrap_token` | File with `600` perms on host  | Secret      |
| `secret_id`       | Generated per job, single-use  | Ephemeral   |
| Client token      | In-memory only, 15 min TTL     | Ephemeral   |
| Unseal key        | `config/vault/init.json` (600) | Critical    |

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
| `FOUNDRY_RUNNER_NAME`        | Runner name for registration    | Hostname                |
| `FOUNDRY_RUNNER_TAGS`        | Comma-separated capability tags | (none)                  |
| `FOUNDRY_RUNNER_CPU`         | Available CPU cores             | (none)                  |
| `FOUNDRY_RUNNER_MEM`         | Available memory (e.g. `16G`)   | (none)                  |
| `FOUNDRY_RUNNER_GPU`         | Available GPUs                  | `0`                     |
| `FOUNDRY_RUNNER_ARCH`        | CPU architecture                | Auto-detected           |
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
- Vault secrets use single-use AppRole tokens — never touch disk
- Runner registration uses heartbeats — stale runners auto-marked offline
