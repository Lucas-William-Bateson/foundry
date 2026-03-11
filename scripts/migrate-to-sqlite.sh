#!/bin/bash
set -euo pipefail

# ─── Foundry: Migrate Vault + PostgreSQL → age-encrypted secrets + SQLite ────
#
# Run on the Mac Mini server to export the old Docker-based stack into the new
# local-file architecture.
#
# Prerequisites:
#   brew install age jq sqlite3
#   Docker containers foundry-vault-1 and foundry-postgres-1 must be running.
#   VAULT_TOKEN must be set (or VAULT_ROOT_TOKEN, or config/vault/init.json).

export PATH="/opt/homebrew/bin:/opt/homebrew/Cellar/docker/29.2.1/bin:$PATH"

FOUNDRY_DIR="${FOUNDRY_DIR:-$HOME/.foundry}"
SECRETS_FILE="${FOUNDRY_DIR}/secrets.age"
DB_FILE="${FOUNDRY_DIR}/foundry.db"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SQLITE_SCHEMA="${PROJECT_ROOT}/migrations_sqlite/001_init.sql"

PG_CONTAINER="foundry-postgres-1"
PG_USER="foundry"
PG_DB="foundry"

VAULT_CONTAINER="foundry-vault-1"
VAULT_ADDR="http://127.0.0.1:8200"
VAULT_PATHS=(
  "secret/foundry/prod"
  "secret/foundry/agent"
  "secret/budget/prod"
  "secret/portfolio/prod"
)

# ─── Colours ─────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()  { echo -e "${GREEN}✓${NC} $*"; }
warn()  { echo -e "${YELLOW}⚠${NC} $*"; }
fail()  { echo -e "${RED}✗${NC} $*" >&2; exit 1; }

# ─── Prerequisite checks ────────────────────────────────────────────────────

check_prereqs() {
  echo "Checking prerequisites…"

  command -v age  >/dev/null 2>&1 || fail "age not found — install with: brew install age"
  command -v jq   >/dev/null 2>&1 || fail "jq not found — install with: brew install jq"
  command -v sqlite3 >/dev/null 2>&1 || fail "sqlite3 not found"
  command -v docker  >/dev/null 2>&1 || fail "docker not found"

  docker info >/dev/null 2>&1 || fail "Docker daemon is not running"

  docker inspect "${VAULT_CONTAINER}" >/dev/null 2>&1 \
    || fail "Vault container '${VAULT_CONTAINER}' not found"
  docker inspect "${PG_CONTAINER}" >/dev/null 2>&1 \
    || fail "PostgreSQL container '${PG_CONTAINER}' not found"

  [ -f "${SQLITE_SCHEMA}" ] || fail "SQLite schema not found at ${SQLITE_SCHEMA}"

  info "All prerequisites satisfied"
  echo ""
}

# ─── Resolve Vault token ────────────────────────────────────────────────────

resolve_vault_token() {
  if [ -n "${VAULT_TOKEN:-}" ]; then
    return
  fi

  if [ -n "${VAULT_ROOT_TOKEN:-}" ]; then
    VAULT_TOKEN="${VAULT_ROOT_TOKEN}"
    return
  fi

  local init_json="${PROJECT_ROOT}/config/vault/init.json"
  if [ -f "${init_json}" ]; then
    VAULT_TOKEN="$(jq -r '.root_token' "${init_json}")"
    if [ -n "${VAULT_TOKEN}" ] && [ "${VAULT_TOKEN}" != "null" ]; then
      return
    fi
  fi

  read -rsp "Enter Vault root token: " VAULT_TOKEN
  echo ""
  [ -n "${VAULT_TOKEN}" ] || fail "Vault token is required"
}

# ─── 1. Export Vault secrets → age-encrypted file ────────────────────────────

export_vault_secrets() {
  echo "═══ Exporting Vault secrets ═══"

  resolve_vault_token
  export VAULT_TOKEN

  local combined="{}"
  local exported_paths=0

  for vault_path in "${VAULT_PATHS[@]}"; do
    # Strip the leading "secret/" to get the store path (e.g. "foundry/prod")
    local store_path="${vault_path#secret/}"

    echo -n "  Reading ${vault_path}… "
    local raw
    if ! raw=$(docker exec \
      -e "VAULT_TOKEN=${VAULT_TOKEN}" \
      "${VAULT_CONTAINER}" \
      vault kv get -address="${VAULT_ADDR}" -format=json "${vault_path}" 2>/dev/null); then
      warn "path not found or inaccessible — skipping"
      continue
    fi

    # Vault KV v2 response: {"data":{"data":{...}}}
    local inner
    inner=$(echo "${raw}" | jq -r '.data.data // empty')
    if [ -z "${inner}" ]; then
      warn "no data at path — skipping"
      continue
    fi

    # Merge into combined JSON: { "foundry/prod": { ... }, ... }
    combined=$(echo "${combined}" | jq --arg key "${store_path}" --argjson val "${inner}" '. + {($key): $val}')
    exported_paths=$((exported_paths + 1))
    info "ok"
  done

  if [ "${exported_paths}" -eq 0 ]; then
    warn "No secrets exported from Vault"
    return
  fi

  # Encrypt with age (passphrase mode)
  echo ""
  echo "Encrypting secrets → ${SECRETS_FILE}"

  if [ -f "${SECRETS_FILE}" ]; then
    local backup="${SECRETS_FILE}.bak.$(date +%Y%m%d%H%M%S)"
    cp "${SECRETS_FILE}" "${backup}"
    warn "Existing secrets file backed up to ${backup}"
  fi

  # Write JSON to a temp file so age can read content from it
  local secrets_tmpfile
  secrets_tmpfile=$(mktemp)
  echo "${combined}" > "${secrets_tmpfile}"

  if [ -n "${FOUNDRY_SECRETS_PASSPHRASE:-}" ]; then
    # Non-interactive: pipe passphrase via stdin, content from file arg
    printf '%s\n%s\n' "${FOUNDRY_SECRETS_PASSPHRASE}" "${FOUNDRY_SECRETS_PASSPHRASE}" \
      | age -p -o "${SECRETS_FILE}" "${secrets_tmpfile}"
  else
    echo "You will be prompted for a passphrase by age."
    age -p -o "${SECRETS_FILE}" "${secrets_tmpfile}"
  fi

  rm -f "${secrets_tmpfile}"

  chmod 600 "${SECRETS_FILE}"
  info "Secrets encrypted (${exported_paths} paths) → ${SECRETS_FILE}"
  echo ""

  STATS_SECRETS="${exported_paths}"
}

# ─── 2. Export PostgreSQL → SQLite ───────────────────────────────────────────

pg_query() {
  # Run a psql command against the PG container and return stdout
  docker exec "${PG_CONTAINER}" \
    psql -U "${PG_USER}" -d "${PG_DB}" -t -A "$@"
}

pg_csv() {
  # Export a query as CSV (no header) via COPY TO STDOUT
  docker exec "${PG_CONTAINER}" \
    psql -U "${PG_USER}" -d "${PG_DB}" -c "COPY ($1) TO STDOUT WITH (FORMAT csv, NULL '')"
}

# Convert a PostgreSQL TEXT[] literal like {main,master} to a JSON array ["main","master"]
pg_array_to_json() {
  local val="$1"
  if [ -z "${val}" ] || [ "${val}" = "NULL" ]; then
    echo ""
    return
  fi
  # {main,master} → ["main","master"]
  echo "${val}" | sed 's/^{/["/; s/}$/"]/' | sed 's/,/","/g'
}

# Convert PostgreSQL boolean t/f to SQLite 1/0
pg_bool_to_sqlite() {
  case "${1:-}" in
    t|true|TRUE)  echo "1" ;;
    f|false|FALSE) echo "0" ;;
    *) echo "${1:-}" ;;
  esac
}

export_postgres_to_sqlite() {
  echo "═══ Exporting PostgreSQL → SQLite ═══"

  if [ -f "${DB_FILE}" ]; then
    local backup="${DB_FILE}.bak.$(date +%Y%m%d%H%M%S)"
    cp "${DB_FILE}" "${backup}"
    warn "Existing database backed up to ${backup}"
    rm "${DB_FILE}"
  fi

  # Create fresh SQLite database from schema
  echo "  Creating SQLite database from schema…"
  sqlite3 "${DB_FILE}" < "${SQLITE_SCHEMA}"
  info "Schema applied"

  # ── Export repos ─────────────────────────────────────────────────────────
  echo "  Exporting repos…"
  local repo_count=0

  # Get repo data as pipe-delimited to avoid CSV quoting headaches with arrays
  local repo_sql="SELECT id, owner, name, clone_url, default_image, created_at,
    COALESCE(description,''), COALESCE(last_build_at::text,''),
    build_count, success_count, failure_count,
    COALESCE(github_id::text,''), COALESCE(full_name,''),
    COALESCE(html_url,''), COALESCE(ssh_url,''),
    COALESCE(private::text,'f'),
    COALESCE(default_branch,'main'), COALESCE(language,''),
    COALESCE(topics::text,''), COALESCE(updated_at::text,''),
    COALESCE(config_json::text,''), COALESCE(triggers_branches::text,''),
    COALESCE(triggers_pull_requests::text,'t'),
    COALESCE(triggers_pr_target_branches::text,'')
  FROM repo ORDER BY id"

  while IFS='|' read -r id owner name clone_url default_image created_at \
      description last_build_at build_count success_count failure_count \
      github_id full_name html_url ssh_url private \
      default_branch language topics updated_at \
      config_json triggers_branches triggers_pull_requests \
      triggers_pr_target_branches; do

    [ -z "${id}" ] && continue

    # Convert PG types
    private=$(pg_bool_to_sqlite "${private}")
    triggers_pull_requests=$(pg_bool_to_sqlite "${triggers_pull_requests}")
    topics=$(pg_array_to_json "${topics}")
    triggers_branches=$(pg_array_to_json "${triggers_branches}")
    triggers_pr_target_branches=$(pg_array_to_json "${triggers_pr_target_branches}")

    sqlite3 "${DB_FILE}" <<EOSQL
INSERT OR REPLACE INTO repo (
  id, owner, name, clone_url, default_image, created_at,
  description, last_build_at, build_count, success_count, failure_count,
  github_id, full_name, html_url, ssh_url, private,
  default_branch, language, topics, updated_at,
  config_json, triggers_branches, triggers_pull_requests, triggers_pr_target_branches
) VALUES (
  ${id},
  '$(echo "${owner}" | sed "s/'/''/g")',
  '$(echo "${name}" | sed "s/'/''/g")',
  '$(echo "${clone_url}" | sed "s/'/''/g")',
  '$(echo "${default_image}" | sed "s/'/''/g")',
  '${created_at}',
  $([ -n "${description}" ] && echo "'$(echo "${description}" | sed "s/'/''/g")'" || echo "NULL"),
  $([ -n "${last_build_at}" ] && echo "'${last_build_at}'" || echo "NULL"),
  ${build_count:-0}, ${success_count:-0}, ${failure_count:-0},
  $([ -n "${github_id}" ] && echo "${github_id}" || echo "NULL"),
  $([ -n "${full_name}" ] && echo "'$(echo "${full_name}" | sed "s/'/''/g")'" || echo "NULL"),
  $([ -n "${html_url}" ] && echo "'$(echo "${html_url}" | sed "s/'/''/g")'" || echo "NULL"),
  $([ -n "${ssh_url}" ] && echo "'$(echo "${ssh_url}" | sed "s/'/''/g")'" || echo "NULL"),
  ${private:-0},
  '$(echo "${default_branch}" | sed "s/'/''/g")',
  $([ -n "${language}" ] && echo "'$(echo "${language}" | sed "s/'/''/g")'" || echo "NULL"),
  $([ -n "${topics}" ] && echo "'${topics}'" || echo "NULL"),
  $([ -n "${updated_at}" ] && echo "'${updated_at}'" || echo "NULL"),
  $([ -n "${config_json}" ] && echo "'$(echo "${config_json}" | sed "s/'/''/g")'" || echo "NULL"),
  $([ -n "${triggers_branches}" ] && echo "'${triggers_branches}'" || echo "NULL"),
  ${triggers_pull_requests:-1},
  $([ -n "${triggers_pr_target_branches}" ] && echo "'${triggers_pr_target_branches}'" || echo "NULL")
);
EOSQL
    repo_count=$((repo_count + 1))
  done < <(pg_query -c "${repo_sql}")

  info "Exported ${repo_count} repos"

  # ── Export runners ───────────────────────────────────────────────────────
  echo "  Exporting runners…"
  local runner_count=0

  local runner_sql="SELECT id, name, COALESCE(tags::text,'{}'),
    COALESCE(cpu::text,''), COALESCE(memory_mb::text,''),
    COALESCE(gpu::text,'0'), arch, status,
    COALESCE(last_heartbeat::text,''),
    registered_at::text, updated_at::text
  FROM runner ORDER BY registered_at"

  while IFS='|' read -r id name tags cpu memory_mb gpu arch status \
      last_heartbeat registered_at updated_at; do

    [ -z "${id}" ] && continue

    tags=$(pg_array_to_json "${tags}")

    sqlite3 "${DB_FILE}" <<EOSQL
INSERT OR REPLACE INTO runner (
  id, name, tags, cpu, memory_mb, gpu, arch, status,
  last_heartbeat, registered_at, updated_at
) VALUES (
  '${id}',
  '$(echo "${name}" | sed "s/'/''/g")',
  '${tags:-[]}',
  $([ -n "${cpu}" ] && echo "${cpu}" || echo "NULL"),
  $([ -n "${memory_mb}" ] && echo "${memory_mb}" || echo "NULL"),
  ${gpu:-0},
  '${arch}',
  '${status}',
  $([ -n "${last_heartbeat}" ] && echo "'${last_heartbeat}'" || echo "NULL"),
  '${registered_at}',
  '${updated_at}'
);
EOSQL
    runner_count=$((runner_count + 1))
  done < <(pg_query -c "${runner_sql}")

  info "Exported ${runner_count} runners"

  # ── Export scheduled_jobs ────────────────────────────────────────────────
  echo "  Exporting scheduled jobs…"
  local sched_count=0

  local sched_sql="SELECT id, repo_id, cron_expression, branch,
    COALESCE(timezone,'UTC'), COALESCE(enabled::text,'t'),
    COALESCE(last_run_at::text,''), COALESCE(next_run_at::text,''),
    created_at::text, updated_at::text
  FROM scheduled_job ORDER BY id"

  while IFS='|' read -r id repo_id cron_expression branch timezone enabled \
      last_run_at next_run_at created_at updated_at; do

    [ -z "${id}" ] && continue
    enabled=$(pg_bool_to_sqlite "${enabled}")

    sqlite3 "${DB_FILE}" <<EOSQL
INSERT OR REPLACE INTO scheduled_job (
  id, repo_id, cron_expression, branch, timezone, enabled,
  last_run_at, next_run_at, created_at, updated_at
) VALUES (
  ${id}, ${repo_id},
  '$(echo "${cron_expression}" | sed "s/'/''/g")',
  '${branch}',
  '${timezone}',
  ${enabled:-1},
  $([ -n "${last_run_at}" ] && echo "'${last_run_at}'" || echo "NULL"),
  $([ -n "${next_run_at}" ] && echo "'${next_run_at}'" || echo "NULL"),
  '${created_at}',
  '${updated_at}'
);
EOSQL
    sched_count=$((sched_count + 1))
  done < <(pg_query -c "${sched_sql}")

  info "Exported ${sched_count} scheduled jobs"

  # ── Export jobs (last 100 per repo) ──────────────────────────────────────
  echo "  Exporting jobs (last 100 per repo)…"
  local job_count=0

  # Build the subquery to get latest 100 jobs per repo
  local job_sql="SELECT j.id, j.repo_id, j.git_sha, j.git_ref, j.status::text,
    j.created_at::text, COALESCE(j.started_at::text,''), COALESCE(j.finished_at::text,''),
    COALESCE(j.claimed_by,''), COALESCE(j.claim_token::text,''),
    COALESCE(j.commit_message,''), COALESCE(j.commit_author,''), COALESCE(j.commit_url,''),
    COALESCE(j.before_sha,''), COALESCE(j.compare_url,''),
    COALESCE(j.commits_count::text,''), COALESCE(j.distinct_commits_count::text,''),
    COALESCE(j.forced::text,'f'), COALESCE(j.deleted::text,'f'), COALESCE(j.created::text,'f'),
    COALESCE(j.pusher_name,''), COALESCE(j.pusher_email,''),
    COALESCE(j.sender_id::text,''), COALESCE(j.sender_login,''),
    COALESCE(j.sender_avatar_url,''), COALESCE(j.sender_type,''),
    COALESCE(j.commit_author_email,''), COALESCE(j.commit_timestamp,''), COALESCE(j.commit_tree_id,''),
    COALESCE(j.committer_name,''), COALESCE(j.committer_email,''), COALESCE(j.committer_username,''),
    COALESCE(j.files_added::text,''), COALESCE(j.files_modified::text,''), COALESCE(j.files_removed::text,''),
    COALESCE(j.installation_id::text,''), COALESCE(j.check_run_id::text,''), COALESCE(j.check_suite_id::text,''),
    COALESCE(j.docker_image,''), COALESCE(j.exit_code::text,''), COALESCE(j.error_message,''),
    COALESCE(j.agent_version,''), COALESCE(j.agent_hostname,''),
    COALESCE(j.trigger_type::text,'push'),
    COALESCE(j.pr_number::text,''), COALESCE(j.pr_title,''), COALESCE(j.pr_url,''),
    COALESCE(j.pr_author,''), COALESCE(j.pr_author_avatar,''),
    COALESCE(j.base_ref,''), COALESCE(j.base_sha,''),
    COALESCE(j.parent_job_id::text,''), COALESCE(j.timeout_secs::text,'1800'),
    COALESCE(j.timed_out::text,'f'),
    COALESCE(j.scheduled_job_id::text,''),
    COALESCE(j.stages_json::text,''), COALESCE(j.metrics_json::text,''),
    COALESCE(j.runner_id::text,''), COALESCE(j.runner_requirements::text,'')
  FROM job j
  INNER JOIN (
    SELECT id FROM job WHERE repo_id IN (SELECT id FROM repo)
    ORDER BY created_at DESC
    LIMIT 100 * (SELECT COUNT(*) FROM repo)
  ) sub ON j.id = sub.id
  ORDER BY j.id"

  # Use a temp file for the large job export to avoid pipe buffer issues
  local job_tmpfile
  job_tmpfile=$(mktemp)

  pg_query -c "${job_sql}" > "${job_tmpfile}"

  # Begin a transaction for performance
  sqlite3 "${DB_FILE}" "BEGIN TRANSACTION;"

  while IFS='|' read -r id repo_id git_sha git_ref status \
      created_at started_at finished_at claimed_by claim_token \
      commit_message commit_author commit_url \
      before_sha compare_url commits_count distinct_commits_count \
      forced deleted created_ \
      pusher_name pusher_email sender_id sender_login sender_avatar_url sender_type \
      commit_author_email commit_timestamp commit_tree_id \
      committer_name committer_email committer_username \
      files_added files_modified files_removed \
      installation_id check_run_id check_suite_id \
      docker_image exit_code error_message \
      agent_version agent_hostname \
      trigger_type \
      pr_number pr_title pr_url pr_author pr_author_avatar \
      base_ref base_sha parent_job_id timeout_secs timed_out \
      scheduled_job_id stages_json metrics_json runner_id runner_requirements; do

    [ -z "${id}" ] && continue

    # Convert types
    forced=$(pg_bool_to_sqlite "${forced}")
    deleted=$(pg_bool_to_sqlite "${deleted}")
    created_=$(pg_bool_to_sqlite "${created_}")
    timed_out=$(pg_bool_to_sqlite "${timed_out}")
    files_added=$(pg_array_to_json "${files_added}")
    files_modified=$(pg_array_to_json "${files_modified}")
    files_removed=$(pg_array_to_json "${files_removed}")

    # Escape single quotes in text fields
    esc() { echo "${1}" | sed "s/'/''/g"; }

    sqlite3 "${DB_FILE}" <<EOSQL
INSERT OR REPLACE INTO job (
  id, repo_id, git_sha, git_ref, status, created_at, started_at, finished_at,
  claimed_by, claim_token,
  commit_message, commit_author, commit_url,
  before_sha, compare_url, commits_count, distinct_commits_count,
  forced, deleted, created,
  pusher_name, pusher_email, sender_id, sender_login, sender_avatar_url, sender_type,
  commit_author_email, commit_timestamp, commit_tree_id,
  committer_name, committer_email, committer_username,
  files_added, files_modified, files_removed,
  installation_id, check_run_id, check_suite_id,
  docker_image, exit_code, error_message,
  agent_version, agent_hostname,
  trigger_type,
  pr_number, pr_title, pr_url, pr_author, pr_author_avatar,
  base_ref, base_sha, parent_job_id, timeout_secs, timed_out,
  scheduled_job_id, stages_json, metrics_json, runner_id, runner_requirements
) VALUES (
  ${id}, ${repo_id},
  '$(esc "${git_sha}")', '$(esc "${git_ref}")', '${status}',
  '${created_at}',
  $([ -n "${started_at}" ] && echo "'${started_at}'" || echo "NULL"),
  $([ -n "${finished_at}" ] && echo "'${finished_at}'" || echo "NULL"),
  $([ -n "${claimed_by}" ] && echo "'$(esc "${claimed_by}")'" || echo "NULL"),
  $([ -n "${claim_token}" ] && echo "'${claim_token}'" || echo "NULL"),
  $([ -n "${commit_message}" ] && echo "'$(esc "${commit_message}")'" || echo "NULL"),
  $([ -n "${commit_author}" ] && echo "'$(esc "${commit_author}")'" || echo "NULL"),
  $([ -n "${commit_url}" ] && echo "'$(esc "${commit_url}")'" || echo "NULL"),
  $([ -n "${before_sha}" ] && echo "'${before_sha}'" || echo "NULL"),
  $([ -n "${compare_url}" ] && echo "'$(esc "${compare_url}")'" || echo "NULL"),
  $([ -n "${commits_count}" ] && echo "${commits_count}" || echo "NULL"),
  $([ -n "${distinct_commits_count}" ] && echo "${distinct_commits_count}" || echo "NULL"),
  ${forced:-0}, ${deleted:-0}, ${created_:-0},
  $([ -n "${pusher_name}" ] && echo "'$(esc "${pusher_name}")'" || echo "NULL"),
  $([ -n "${pusher_email}" ] && echo "'$(esc "${pusher_email}")'" || echo "NULL"),
  $([ -n "${sender_id}" ] && echo "${sender_id}" || echo "NULL"),
  $([ -n "${sender_login}" ] && echo "'$(esc "${sender_login}")'" || echo "NULL"),
  $([ -n "${sender_avatar_url}" ] && echo "'$(esc "${sender_avatar_url}")'" || echo "NULL"),
  $([ -n "${sender_type}" ] && echo "'${sender_type}'" || echo "NULL"),
  $([ -n "${commit_author_email}" ] && echo "'$(esc "${commit_author_email}")'" || echo "NULL"),
  $([ -n "${commit_timestamp}" ] && echo "'${commit_timestamp}'" || echo "NULL"),
  $([ -n "${commit_tree_id}" ] && echo "'${commit_tree_id}'" || echo "NULL"),
  $([ -n "${committer_name}" ] && echo "'$(esc "${committer_name}")'" || echo "NULL"),
  $([ -n "${committer_email}" ] && echo "'$(esc "${committer_email}")'" || echo "NULL"),
  $([ -n "${committer_username}" ] && echo "'$(esc "${committer_username}")'" || echo "NULL"),
  $([ -n "${files_added}" ] && echo "'${files_added}'" || echo "NULL"),
  $([ -n "${files_modified}" ] && echo "'${files_modified}'" || echo "NULL"),
  $([ -n "${files_removed}" ] && echo "'${files_removed}'" || echo "NULL"),
  $([ -n "${installation_id}" ] && echo "${installation_id}" || echo "NULL"),
  $([ -n "${check_run_id}" ] && echo "${check_run_id}" || echo "NULL"),
  $([ -n "${check_suite_id}" ] && echo "${check_suite_id}" || echo "NULL"),
  $([ -n "${docker_image}" ] && echo "'$(esc "${docker_image}")'" || echo "NULL"),
  $([ -n "${exit_code}" ] && echo "${exit_code}" || echo "NULL"),
  $([ -n "${error_message}" ] && echo "'$(esc "${error_message}")'" || echo "NULL"),
  $([ -n "${agent_version}" ] && echo "'$(esc "${agent_version}")'" || echo "NULL"),
  $([ -n "${agent_hostname}" ] && echo "'$(esc "${agent_hostname}")'" || echo "NULL"),
  '${trigger_type:-push}',
  $([ -n "${pr_number}" ] && echo "${pr_number}" || echo "NULL"),
  $([ -n "${pr_title}" ] && echo "'$(esc "${pr_title}")'" || echo "NULL"),
  $([ -n "${pr_url}" ] && echo "'$(esc "${pr_url}")'" || echo "NULL"),
  $([ -n "${pr_author}" ] && echo "'$(esc "${pr_author}")'" || echo "NULL"),
  $([ -n "${pr_author_avatar}" ] && echo "'$(esc "${pr_author_avatar}")'" || echo "NULL"),
  $([ -n "${base_ref}" ] && echo "'$(esc "${base_ref}")'" || echo "NULL"),
  $([ -n "${base_sha}" ] && echo "'${base_sha}'" || echo "NULL"),
  $([ -n "${parent_job_id}" ] && echo "${parent_job_id}" || echo "NULL"),
  ${timeout_secs:-1800},
  ${timed_out:-0},
  $([ -n "${scheduled_job_id}" ] && echo "${scheduled_job_id}" || echo "NULL"),
  $([ -n "${stages_json}" ] && echo "'$(esc "${stages_json}")'" || echo "NULL"),
  $([ -n "${metrics_json}" ] && echo "'$(esc "${metrics_json}")'" || echo "NULL"),
  $([ -n "${runner_id}" ] && echo "'${runner_id}'" || echo "NULL"),
  $([ -n "${runner_requirements}" ] && echo "'$(esc "${runner_requirements}")'" || echo "NULL")
);
EOSQL
    job_count=$((job_count + 1))
  done < "${job_tmpfile}"

  sqlite3 "${DB_FILE}" "COMMIT;"
  rm -f "${job_tmpfile}"

  info "Exported ${job_count} jobs"

  # ── Export job_log (for the migrated jobs) ───────────────────────────────
  echo "  Exporting job logs…"
  local log_count=0

  # Only export logs for jobs that were migrated
  local log_sql="SELECT jl.id, jl.job_id, jl.ts::text, jl.line
    FROM job_log jl
    INNER JOIN (
      SELECT id FROM job ORDER BY created_at DESC
      LIMIT 100 * (SELECT COUNT(*) FROM repo)
    ) j ON jl.job_id = j.id
    ORDER BY jl.job_id, jl.ts"

  local log_tmpfile
  log_tmpfile=$(mktemp)

  pg_query -c "${log_sql}" > "${log_tmpfile}"

  sqlite3 "${DB_FILE}" "BEGIN TRANSACTION;"

  while IFS='|' read -r id job_id ts line; do
    [ -z "${id}" ] && continue

    # Escape the log line (can contain anything)
    local escaped_line
    escaped_line=$(echo "${line}" | sed "s/'/''/g")

    sqlite3 "${DB_FILE}" \
      "INSERT OR REPLACE INTO job_log (id, job_id, ts, line) VALUES (${id}, ${job_id}, '${ts}', '${escaped_line}');"
    log_count=$((log_count + 1))
  done < "${log_tmpfile}"

  sqlite3 "${DB_FILE}" "COMMIT;"
  rm -f "${log_tmpfile}"

  info "Exported ${log_count} log lines"

  STATS_REPOS="${repo_count}"
  STATS_JOBS="${job_count}"
  STATS_LOGS="${log_count}"
  STATS_RUNNERS="${runner_count}"
  STATS_SCHEDULES="${sched_count}"
}

# ─── Summary ─────────────────────────────────────────────────────────────────

print_summary() {
  echo ""
  echo "═══════════════════════════════════════════════"
  echo "  Migration complete"
  echo "═══════════════════════════════════════════════"
  echo ""
  echo "  Secrets:        ${STATS_SECRETS:-0} Vault paths → ${SECRETS_FILE}"
  echo "  Repos:          ${STATS_REPOS:-0}"
  echo "  Jobs:           ${STATS_JOBS:-0}"
  echo "  Job logs:       ${STATS_LOGS:-0} lines"
  echo "  Runners:        ${STATS_RUNNERS:-0}"
  echo "  Scheduled jobs: ${STATS_SCHEDULES:-0}"
  echo ""
  echo "  Database:       ${DB_FILE}"
  echo "  Secrets:        ${SECRETS_FILE}"
  echo ""
}

# ─── Main ────────────────────────────────────────────────────────────────────

main() {
  echo ""
  echo "╔══════════════════════════════════════════════╗"
  echo "║  Foundry: Migrate to SQLite + age            ║"
  echo "╚══════════════════════════════════════════════╝"
  echo ""

  check_prereqs

  # Create foundry config directory
  mkdir -p "${FOUNDRY_DIR}"
  chmod 700 "${FOUNDRY_DIR}"

  export_vault_secrets
  export_postgres_to_sqlite
  print_summary
}

main "$@"
