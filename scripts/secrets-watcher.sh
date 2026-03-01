#!/bin/bash
# secrets-watcher.sh — Host-side file watcher for Proton Pass secrets injection
#
# Watches the foundry workspace for .secrets-inject-request signal files.
# When found, runs pass-cli inject on the corresponding secrets.env.template.
#
# Install as a launchd service:
#   cp scripts/com.foundry.secrets-watcher.plist ~/Library/LaunchAgents/
#   launchctl load ~/Library/LaunchAgents/com.foundry.secrets-watcher.plist

set -euo pipefail

WORKSPACE_DIR="${FOUNDRY_WORKSPACE_DIR:-/Users/lucas/foundry-workspace}"
POLL_INTERVAL="${POLL_INTERVAL:-1}"
PASS_CLI="${PASS_CLI:-$(which pass-cli 2>/dev/null || echo /opt/homebrew/bin/pass-cli)}"

log() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') [secrets-watcher] $*"
}

if [ ! -x "$PASS_CLI" ]; then
    log "ERROR: pass-cli not found at $PASS_CLI"
    exit 1
fi

log "Starting secrets watcher on $WORKSPACE_DIR"
log "Using pass-cli: $PASS_CLI"

while true; do
    # Find all signal files
    for signal_file in "$WORKSPACE_DIR"/job-*/repo/.secrets-inject-request; do
        [ -f "$signal_file" ] || continue

        repo_dir="$(dirname "$signal_file")"
        job_dir="$(dirname "$repo_dir")"
        template="$repo_dir/secrets.env.template"
        output="$repo_dir/secrets.env"

        if [ ! -f "$template" ]; then
            log "WARN: No template found at $template — removing signal"
            rm -f "$signal_file"
            continue
        fi

        log "Injecting secrets for $(basename "$job_dir")"

        if PROTON_PASS_KEY_PROVIDER="${PROTON_PASS_KEY_PROVIDER:-}" PROTON_PASS_ENCRYPTION_KEY="${PROTON_PASS_ENCRYPTION_KEY:-}" "$PASS_CLI" inject --in-file "$template" --out-file "$output" --force 2>&1; then
            log "Secrets injected successfully → $output"
        else
            log "ERROR: pass-cli inject failed for $template"
        fi

        # Remove signal file to notify the agent
        rm -f "$signal_file"
    done

    sleep "$POLL_INTERVAL"
done
