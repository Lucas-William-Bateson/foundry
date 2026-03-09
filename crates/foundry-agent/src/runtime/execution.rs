//! Main job lifecycle — `execute_pipeline()` orchestrates workspace setup, git operations,
//! trusted_agent_config loading, and dispatches to stages, deploy, or single-container execution.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use foundry_core::{ClaimedJob, FoundryConfig, VaultClient};
use secrecy::ExposeSecret;

use crate::types::config::Config;
use crate::domain::deploy::run_deploy;
use crate::domain::forgefile_converter;
use crate::infrastructure::docker::{build_image, run_container};
use crate::infrastructure::github_app::GitHubApp;
use crate::api::server::ServerClient;
use crate::domain::stages::run_stages;

#[derive(Debug, Clone, serde::Serialize)]
pub struct JobMetrics {
    pub clone_duration_ms: u64,
    pub build_duration_ms: Option<u64>,
    pub stages: Vec<StageMetrics>,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StageMetrics {
    pub name: String,
    pub status: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
}

fn is_self_deploy(job: &ClaimedJob, trusted_agent_config: &Config) -> bool {
    if let Some(self_repo) = &trusted_agent_config.self_repo {
        job.clone_url.contains(self_repo)
    } else {
        false
    }
}

pub async fn execute_pipeline(
    client: &ServerClient,
    job: &ClaimedJob,
    trusted_agent_config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    let job_start = Instant::now();
    
    if is_self_deploy(job, trusted_agent_config) {
        return run_self_deploy(client, job, trusted_agent_config, github_app).await;
    }

    let workspace = PathBuf::from(&trusted_agent_config.workspace_dir).join(format!("job-{}", job.id));

    if workspace.exists() {
        debug!("Cleaning up existing workspace: {:?}", workspace);
        if let Err(e) = tokio::fs::remove_dir_all(&workspace).await {
            debug!("Failed to remove existing workspace: {}", e);
        }
    }

    tokio::fs::create_dir_all(&workspace)
        .await
        .context("Failed to create workspace directory")?;

    let repo_dir = workspace.join("repo");

    let clone_url = if let Some(app) = github_app {
        client.log(job, "Fetching GitHub App installation token").await?;
        let token = app.get_installation_token().await?;
        app.authenticated_clone_url(&job.clone_url, &token)
    } else {
        job.clone_url.clone()
    };

    // For scheduled jobs, git_sha starts with "RESOLVE:" - we clone by branch and resolve later
    let (clone_ref, is_scheduled) = if job.git_sha.starts_with("RESOLVE:") {
        let branch = job.git_sha.strip_prefix("RESOLVE:").unwrap_or("main");
        (branch.to_string(), true)
    } else {
        (job.git_sha.clone(), false)
    };

    let clone_start = Instant::now();
    client
        .log(
            job,
            &format!(
                "Cloning {} @ {}",
                job.clone_url,
                if is_scheduled { &job.git_ref } else { &clone_ref[..8.min(clone_ref.len())] }
            ),
        )
        .await?;

    clone_repo(&clone_url, &job.clone_url, &clone_ref, &repo_dir, is_scheduled).await?;
    let clone_duration_ms = clone_start.elapsed().as_millis() as u64;

    client.log(job, &format!("Clone complete ({} ms)", clone_duration_ms)).await?;

    // --- Forgefile-first pipeline ---
    let forgefile_path = repo_dir.join("Forgefile");
    if forgefile_path.exists() {
        let forgefile_source = tokio::fs::read_to_string(&forgefile_path)
            .await
            .context("Failed to read Forgefile")?;

        client.log(job, "Found Forgefile").await?;

        let forgefile = forgefile::parse(&forgefile_source).map_err(|errors| {
            let msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::anyhow!("Forgefile parse error: {}", msg)
        })?;

        if let Err(errors) = forgefile::validate(&forgefile) {
            let msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            anyhow::bail!("Forgefile validation failed: {}", msg);
        }

        let plan = forgefile_converter::convert(&forgefile, &job.git_ref);

        // Fetch secrets from Vault if configured
        let mut env_vars: std::collections::HashMap<String, String> = plan.env.clone();
        for secret_cfg in &plan.secrets {
            match fetch_forgefile_vault_secrets(client, job, trusted_agent_config, secret_cfg).await {
                Ok(secrets) => {
                    let alias_map = forgefile_converter::build_secret_alias_map(&[secret_cfg.clone()]);
                    let count = secrets.len();
                    for (key, value) in secrets {
                        // Apply alias mapping: if alias exists, inject under alias name
                        let env_key = alias_map
                            .iter()
                            .find(|(_, vault_key)| **vault_key == key)
                            .map(|(env_name, _)| env_name.clone())
                            .unwrap_or(key);
                        env_vars.insert(env_key, value.expose_secret().to_string());
                    }
                    client.log(job, &format!("🔐 Injected {} secret(s) from Vault", count)).await?;
                }
                Err(e) => {
                    client.log(job, &format!("⚠️  Vault secrets fetch failed: {}", e)).await?;
                    warn!("Vault secrets fetch failed for job {}: {}", job.id, e);
                }
            }
        }

        // Sync trigger configuration
        if let Err(e) = client.sync_triggers(job, &plan.triggers).await {
            client.log(job, &format!("⚠️  Failed to sync triggers: {}", e)).await?;
        } else {
            client.log(job, &format!("🎯 Triggers synced: branches={:?}", plan.triggers.branches)).await?;
        }

        // Build FoundryConfig from the execution plan for compatibility with existing code
        let untrusted_repo_config = FoundryConfig {
            stages: plan.stages,
            env: env_vars,
            deploy: plan.deploy.unwrap_or_default(),
            ..Default::default()
        };

        if untrusted_repo_config.deploy.is_enabled() {
            return run_deploy(client, job, &repo_dir, trusted_agent_config, &untrusted_repo_config).await;
        }

        if untrusted_repo_config.has_stages() {
            return run_stages(client, job, &repo_dir, trusted_agent_config, &untrusted_repo_config, clone_duration_ms).await;
        }

        // No stages and no deploy — nothing to do
        client.log(job, "⚠️  Forgefile matched no stages for this event").await?;
        return Ok(());
    }

    // --- Legacy foundry.toml fallback ---
    let mut untrusted_repo_config = FoundryConfig::load(&repo_dir);

    if let Some(ref mut untrusted_repo_config) = untrusted_repo_config {
        client.log(job, "Found foundry.toml (legacy)").await?;

        // Fetch secrets from Vault if configured
        if let Some(ref secrets_cfg) = untrusted_repo_config.secrets {
            if secrets_cfg.is_enabled() {
                match fetch_vault_secrets(client, job, trusted_agent_config, secrets_cfg).await {
                    Ok(secrets) => {
                        let count = secrets.len();
                        for (key, value) in secrets {
                            // Expose the SecretString value when injecting into the env map
                            untrusted_repo_config.env.insert(key, value.expose_secret().to_string());
                        }
                        client.log(job, &format!("🔐 Injected {} secret(s) from Vault", count)).await?;
                    }
                    Err(e) => {
                        client.log(job, &format!("⚠️  Vault secrets fetch failed: {}", e)).await?;
                        warn!("Vault secrets fetch failed for job {}: {}", job.id, e);
                    }
                }
            }
        }
        
        // Sync schedule configuration from foundry.toml to the server
        if let Err(e) = client.sync_schedule(job, untrusted_repo_config.schedule.as_ref()).await {
            client.log(job, &format!("⚠️  Failed to sync schedule: {}", e)).await?;
        } else if untrusted_repo_config.schedule.is_some() {
            let sched = untrusted_repo_config.schedule.as_ref().unwrap();
            client.log(job, &format!("📅 Schedule synced: {}", sched.cron)).await?;
        }
        
        // Sync trigger configuration
        if let Err(e) = client.sync_triggers(job, &untrusted_repo_config.triggers).await {
            client.log(job, &format!("⚠️  Failed to sync triggers: {}", e)).await?;
        } else {
            client.log(job, &format!("🎯 Triggers synced: branches={:?}", untrusted_repo_config.triggers.branches)).await?;
        }
        
        if untrusted_repo_config.deploy.is_enabled() {
            return run_deploy(client, job, &repo_dir, trusted_agent_config, untrusted_repo_config).await;
        }
        
        if untrusted_repo_config.has_stages() {
            return run_stages(client, job, &repo_dir, trusted_agent_config, untrusted_repo_config, clone_duration_ms).await;
        }
    }

    let build_start = Instant::now();
    let (image, command) = if let Some(ref untrusted_repo_config) = untrusted_repo_config {
        let img = if untrusted_repo_config.build.dockerfile.is_some() {
            build_image(client, job, &repo_dir, untrusted_repo_config).await?
        } else {
            untrusted_repo_config.build.image.clone()
        };
        let cmd = untrusted_repo_config.effective_command(&trusted_agent_config.default_command);
        (img, cmd)
    } else {
        (job.image.clone(), trusted_agent_config.default_command.clone())
    };
    let build_duration_ms = build_start.elapsed().as_millis() as u64;

    client
        .log(job, &format!("Running in container: {}", image))
        .await?;

    let env_vars = untrusted_repo_config.as_ref().map(|untrusted_repo_config| &untrusted_repo_config.env);
    let timeout_secs = untrusted_repo_config.as_ref().map(|untrusted_repo_config| untrusted_repo_config.build.timeout).unwrap_or(1800);
    
    client.log(job, &format!("Timeout: {} seconds", timeout_secs)).await?;
    
    let success = run_container(client, job, &repo_dir, &image, &command, env_vars, timeout_secs).await?;
    
    let total_duration_ms = job_start.elapsed().as_millis() as u64;
    let metrics = JobMetrics {
        clone_duration_ms,
        build_duration_ms: Some(build_duration_ms),
        stages: vec![],
        total_duration_ms,
    };
    
    client.report_metrics(job, &metrics).await.ok();

    if let Err(e) = tokio::fs::remove_dir_all(&workspace).await {
        debug!("Failed to cleanup workspace: {}", e);
    }

    if success {
        Ok(())
    } else {
        anyhow::bail!("Container exited with non-zero status")
    }
}

async fn run_self_deploy(
    client: &ServerClient,
    job: &ClaimedJob,
    trusted_agent_config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    info!("Self-deploy triggered for Foundry");
    client.log(job, "🔄 Self-deploy triggered").await?;

    let script = trusted_agent_config
        .self_deploy_script
        .as_deref()
        .unwrap_or("/app/scripts/deploy.sh");

    client.log(job, &format!("Running deploy script: {}", script)).await?;

    let github_token = if let Some(app) = github_app {
        match app.get_installation_token().await {
            Ok(token) => Some(token),
            Err(e) => {
                client.log(job, &format!("⚠️ Failed to get GitHub token: {}", e)).await?;
                None
            }
        }
    } else {
        None
    };

    let mut cmd = Command::new("bash");
    cmd.arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(token) = github_token {
        cmd.env("GITHUB_TOKEN", token);
    }

    let mut child = cmd.spawn().context("Failed to start deploy script")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let job_id = job.id;
    let client_clone = client.clone();
    let claim_token = job.claim_token.clone();

    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = client_clone.log_raw(job_id, &claim_token, &line).await;
        }
    });

    let client_clone2 = client.clone();
    let claim_token2 = job.claim_token.clone();

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = client_clone2.log_raw(job_id, &claim_token2, &format!("STDERR: {}", line)).await;
        }
    });

    let status = child.wait().await.context("Failed to wait for deploy script")?;

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    if status.success() {
        client.log(job, "✅ Self-deploy complete").await?;
        Ok(())
    } else {
        client.log(job, "❌ Self-deploy failed").await?;
        anyhow::bail!("Deploy script exited with non-zero status")
    }
}

async fn clone_repo(url: &str, safe_url: &str, sha_or_branch: &str, dest: &PathBuf, clone_by_branch: bool) -> Result<()> {
    let mut args = vec!["clone", "--depth", "50"];
    
    // If cloning by branch (scheduled jobs), specify the branch explicitly
    if clone_by_branch {
        args.push("-b");
        args.push(sha_or_branch);
    }
    
    args.push(url);
    
    let output = Command::new("git")
        .args(&args)
        .arg(dest)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await
        .context("Failed to run git clone")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let sanitized = sanitize_git_error(&stderr, url, safe_url);
        anyhow::bail!("git clone failed: {}", sanitized);
    }

    // For scheduled jobs, we're already on the right branch after clone
    // For regular jobs, checkout the specific SHA
    if !clone_by_branch {
        let output = Command::new("git")
            .args(["checkout", sha_or_branch])
            .current_dir(dest)
            .output()
            .await
            .context("Failed to run git checkout")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git checkout failed: {}", stderr);
        }
    }

    Ok(())
}

fn sanitize_git_error(stderr: &str, secret_url: &str, safe_url: &str) -> String {
    stderr.replace(secret_url, safe_url)
}

/// Fetch secrets from Vault for a CI job using Forgefile secrets config.
async fn fetch_forgefile_vault_secrets(
    client: &ServerClient,
    job: &ClaimedJob,
    config: &Config,
    secrets_cfg: &forgefile_converter::SecretsConfig,
) -> Result<std::collections::HashMap<String, secrecy::SecretString>> {
    let vault_addr = config
        .vault_addr
        .as_deref()
        .context("VAULT_ADDR not configured on agent")?;
    let role_id = config
        .vault_role_id
        .as_deref()
        .context("VAULT_ROLE_ID not configured on agent")?;
    let bootstrap_token = config
        .vault_bootstrap_token
        .as_ref()
        .context("VAULT_BOOTSTRAP_TOKEN not configured — needed to generate per-job secret_ids")?;

    let vault_client = VaultClient::new(vault_addr, role_id);

    match vault_client.health_check().await {
        Ok(true) => {}
        Ok(false) => {
            anyhow::bail!("Vault is not healthy (sealed or not initialised) — cannot fetch secrets");
        }
        Err(e) => {
            anyhow::bail!("Vault unreachable: {} — cannot fetch secrets", e);
        }
    }

    client
        .log(job, &format!("🔑 Fetching secrets from Vault path: {}", secrets_cfg.vault_path))
        .await?;

    let mut secrets = vault_client
        .fetch_ci_secrets(bootstrap_token, &secrets_cfg.vault_path)
        .await?;

    // Filter to requested keys if specified
    if !secrets_cfg.keys.is_empty() {
        let requested_keys: std::collections::HashSet<&str> = secrets_cfg
            .keys
            .iter()
            .map(|k| k.name.as_str())
            .collect();
        secrets.retain(|k, _| requested_keys.contains(k.as_str()));
    }

    Ok(secrets)
}

/// Fetch secrets from Vault for a CI job.
///
/// Flow:
/// 1. Generate a single-use secret_id using the bootstrap token
/// 2. Login via AppRole to get a short-lived client token
/// 3. Read secrets from the KV v2 path declared in foundry.toml
/// 4. Optionally filter to requested keys only
async fn fetch_vault_secrets(
    client: &ServerClient,
    job: &ClaimedJob,
    config: &Config,
    secrets_cfg: &foundry_core::SecretsConfig,
) -> Result<std::collections::HashMap<String, secrecy::SecretString>> {
    let vault_addr = config
        .vault_addr
        .as_deref()
        .context("VAULT_ADDR not configured on agent")?;
    let role_id = config
        .vault_role_id
        .as_deref()
        .context("VAULT_ROLE_ID not configured on agent")?;
    let bootstrap_token = config
        .vault_bootstrap_token
        .as_ref()
        .context("VAULT_BOOTSTRAP_TOKEN not configured — needed to generate per-job secret_ids")?;

    let vault_path = secrets_cfg
        .vault_path
        .as_deref()
        .context("secrets.vault_path not set in foundry.toml")?;

    let vault_client = VaultClient::new(vault_addr, role_id);

    // Pre-flight: check Vault is healthy before attempting secret fetch
    match vault_client.health_check().await {
        Ok(true) => {}
        Ok(false) => {
            anyhow::bail!("Vault is not healthy (sealed or not initialised) — cannot fetch secrets");
        }
        Err(e) => {
            anyhow::bail!("Vault unreachable: {} — cannot fetch secrets", e);
        }
    }

    client
        .log(job, &format!("🔑 Fetching secrets from Vault path: {}", vault_path))
        .await?;

    let mut secrets = vault_client
        .fetch_ci_secrets(bootstrap_token, vault_path)
        .await?;

    // Filter to requested keys if specified
    if let Some(ref keys) = secrets_cfg.keys {
        if !keys.is_empty() {
            secrets.retain(|k, _| keys.contains(k));
        }
    }

    Ok(secrets)
}
