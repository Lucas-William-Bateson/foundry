//! Main job lifecycle — `execute_pipeline()` orchestrates workspace setup, git operations,
//! config loading, and dispatches to stages, deploy, or single-container execution.
//!
//! Adapted from foundry-agent to use DbLogger for direct DB logging.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use foundry_core::{ClaimedJob, FoundryConfig, VaultClient};
use secrecy::ExposeSecret;

use crate::config::Config;
use crate::domain::deploy::run_deploy;
use crate::domain::forgefile_converter;
use crate::infrastructure::docker_runner::{build_image, run_container};
use crate::infrastructure::github_app::GitHubApp;
use crate::runtime::db_logger::DbLogger;
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

fn is_self_deploy(job: &ClaimedJob, config: &Config) -> bool {
    if let Some(self_repo) = &config.self_repo {
        job.clone_url.contains(self_repo)
    } else {
        false
    }
}

pub async fn execute_pipeline(
    logger: &DbLogger,
    job: &ClaimedJob,
    config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    let job_start = Instant::now();
    
    if is_self_deploy(job, config) {
        return run_self_deploy(logger, job, config, github_app).await;
    }

    let workspace = PathBuf::from(&config.workspace_dir).join(format!("job-{}", job.id));

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
        logger.log(job, "Fetching GitHub App installation token").await?;
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
    logger
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

    logger.log(job, &format!("Clone complete ({} ms)", clone_duration_ms)).await?;

    // --- Forgefile-first pipeline ---
    let forgefile_path = repo_dir.join("Forgefile");
    if forgefile_path.exists() {
        let forgefile_source = tokio::fs::read_to_string(&forgefile_path)
            .await
            .context("Failed to read Forgefile")?;

        logger.log(job, "Found Forgefile").await?;

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

        // Fetch secrets from configured backend (Vault or Store)
        let mut env_vars: std::collections::HashMap<String, String> = plan.env.clone();
        for secret_cfg in &plan.secrets {
            match &secret_cfg.backend {
                forgefile_converter::SecretsBackend::Vault => {
                    match fetch_forgefile_vault_secrets(logger, job, config, secret_cfg).await {
                        Ok(secrets) => {
                            let alias_map = forgefile_converter::build_secret_alias_map(&[secret_cfg.clone()]);
                            let count = secrets.len();
                            for (key, value) in secrets {
                                let env_key = alias_map
                                    .iter()
                                    .find(|(_, vault_key)| **vault_key == key)
                                    .map(|(env_name, _)| env_name.clone())
                                    .unwrap_or(key);
                                env_vars.insert(env_key, value.expose_secret().to_string());
                            }
                            logger.log(job, &format!("🔐 Injected {} secret(s) from Vault", count)).await?;
                        }
                        Err(e) => {
                            logger.log(job, &format!("⚠️  Vault secrets fetch failed: {}", e)).await?;
                            warn!("Vault secrets fetch failed for job {}: {}", job.id, e);
                        }
                    }
                }
                forgefile_converter::SecretsBackend::Store => {
                    match fetch_forgefile_store_secrets(logger, job, secret_cfg).await {
                        Ok(secrets) => {
                            let alias_map = forgefile_converter::build_secret_alias_map(&[secret_cfg.clone()]);
                            let count = secrets.len();
                            for (key, value) in secrets {
                                let env_key = alias_map
                                    .iter()
                                    .find(|(_, store_key)| **store_key == key)
                                    .map(|(env_name, _)| env_name.clone())
                                    .unwrap_or(key);
                                env_vars.insert(env_key, value);
                            }
                            logger.log(job, &format!("🔐 Injected {} secret(s) from secrets store", count)).await?;
                        }
                        Err(e) => {
                            logger.log(job, &format!("⚠️  Secrets store fetch failed: {}", e)).await?;
                            warn!("Secrets store fetch failed for job {}: {}", job.id, e);
                        }
                    }
                }
            }
        }

        // Sync trigger configuration
        if let Err(e) = logger.sync_triggers(job, &plan.triggers).await {
            logger.log(job, &format!("⚠️  Failed to sync triggers: {}", e)).await?;
        } else {
            logger.log(job, &format!("🎯 Triggers synced: branches={:?}", plan.triggers.branches)).await?;
        }

        // Build FoundryConfig from the execution plan for compatibility with existing code
        let untrusted_repo_config = FoundryConfig {
            stages: plan.stages,
            env: env_vars,
            deploy: plan.deploy.unwrap_or_default(),
            ..Default::default()
        };

        if untrusted_repo_config.deploy.is_enabled() {
            return run_deploy(logger, job, &repo_dir, config, &untrusted_repo_config).await;
        }

        if untrusted_repo_config.has_stages() {
            return run_stages(logger, job, &repo_dir, config, &untrusted_repo_config, clone_duration_ms).await;
        }

        // No stages and no deploy — nothing to do
        logger.log(job, "⚠️  Forgefile matched no stages for this event").await?;
        return Ok(());
    }

    // --- Legacy foundry.toml fallback ---
    let mut untrusted_repo_config = FoundryConfig::load(&repo_dir);

    if let Some(ref mut untrusted_repo_config) = untrusted_repo_config {
        logger.log(job, "Found foundry.toml (legacy)").await?;

        // Fetch secrets from Vault if configured
        if let Some(ref secrets_cfg) = untrusted_repo_config.secrets {
            if secrets_cfg.is_enabled() {
                match fetch_vault_secrets(logger, job, config, secrets_cfg).await {
                    Ok(secrets) => {
                        let count = secrets.len();
                        for (key, value) in secrets {
                            untrusted_repo_config.env.insert(key, value.expose_secret().to_string());
                        }
                        logger.log(job, &format!("🔐 Injected {} secret(s) from Vault", count)).await?;
                    }
                    Err(e) => {
                        logger.log(job, &format!("⚠️  Vault secrets fetch failed: {}", e)).await?;
                        warn!("Vault secrets fetch failed for job {}: {}", job.id, e);
                    }
                }
            }
        }
        
        // Sync schedule configuration from foundry.toml to the DB
        if let Err(e) = logger.sync_schedule(job, untrusted_repo_config.schedule.as_ref()).await {
            logger.log(job, &format!("⚠️  Failed to sync schedule: {}", e)).await?;
        } else if untrusted_repo_config.schedule.is_some() {
            let sched = untrusted_repo_config.schedule.as_ref().unwrap();
            logger.log(job, &format!("📅 Schedule synced: {}", sched.cron)).await?;
        }
        
        // Sync trigger configuration
        if let Err(e) = logger.sync_triggers(job, &untrusted_repo_config.triggers).await {
            logger.log(job, &format!("⚠️  Failed to sync triggers: {}", e)).await?;
        } else {
            logger.log(job, &format!("🎯 Triggers synced: branches={:?}", untrusted_repo_config.triggers.branches)).await?;
        }
        
        if untrusted_repo_config.deploy.is_enabled() {
            return run_deploy(logger, job, &repo_dir, config, untrusted_repo_config).await;
        }
        
        if untrusted_repo_config.has_stages() {
            return run_stages(logger, job, &repo_dir, config, untrusted_repo_config, clone_duration_ms).await;
        }
    }

    let build_start = Instant::now();
    let (image, command) = if let Some(ref untrusted_repo_config) = untrusted_repo_config {
        let img = if untrusted_repo_config.build.dockerfile.is_some() {
            build_image(logger, job, &repo_dir, untrusted_repo_config).await?
        } else {
            untrusted_repo_config.build.image.clone()
        };
        let cmd = untrusted_repo_config.effective_command(&config.default_command);
        (img, cmd)
    } else {
        (job.image.clone(), config.default_command.clone())
    };
    let build_duration_ms = build_start.elapsed().as_millis() as u64;

    logger
        .log(job, &format!("Running in container: {}", image))
        .await?;

    let env_vars = untrusted_repo_config.as_ref().map(|c| &c.env);
    let timeout_secs = untrusted_repo_config.as_ref().map(|c| c.build.timeout).unwrap_or(1800);
    
    logger.log(job, &format!("Timeout: {} seconds", timeout_secs)).await?;
    
    let success = run_container(logger, job, &repo_dir, &image, &command, env_vars, timeout_secs).await?;
    
    let total_duration_ms = job_start.elapsed().as_millis() as u64;
    let metrics = JobMetrics {
        clone_duration_ms,
        build_duration_ms: Some(build_duration_ms),
        stages: vec![],
        total_duration_ms,
    };
    
    logger.report_metrics(job, &metrics).await.ok();

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
    logger: &DbLogger,
    job: &ClaimedJob,
    config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    info!("Self-deploy triggered for Foundry");
    logger.log(job, "🔄 Self-deploy triggered").await?;

    let script = config
        .self_deploy_script
        .as_deref()
        .unwrap_or("/app/scripts/deploy.sh");

    logger.log(job, &format!("Running deploy script: {}", script)).await?;

    let github_token = if let Some(app) = github_app {
        match app.get_installation_token().await {
            Ok(token) => Some(token),
            Err(e) => {
                logger.log(job, &format!("⚠️ Failed to get GitHub token: {}", e)).await?;
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
    let logger_clone = logger.clone();
    let claim_token = job.claim_token;

    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = logger_clone.log_raw(job_id, &claim_token, &line).await;
        }
    });

    let logger_clone2 = logger.clone();
    let claim_token2 = job.claim_token;

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = logger_clone2.log_raw(job_id, &claim_token2, &format!("STDERR: {}", line)).await;
        }
    });

    let status = child.wait().await.context("Failed to wait for deploy script")?;

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    if status.success() {
        logger.log(job, "✅ Self-deploy complete").await?;
        Ok(())
    } else {
        logger.log(job, "❌ Self-deploy failed").await?;
        anyhow::bail!("Deploy script exited with non-zero status")
    }
}

async fn clone_repo(url: &str, safe_url: &str, sha_or_branch: &str, dest: &PathBuf, clone_by_branch: bool) -> Result<()> {
    let mut args = vec!["clone", "--depth", "50"];
    
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
    logger: &DbLogger,
    job: &ClaimedJob,
    config: &Config,
    secrets_cfg: &forgefile_converter::SecretsConfig,
) -> Result<std::collections::HashMap<String, secrecy::SecretString>> {
    let vault_addr = config
        .vault_addr
        .as_deref()
        .context("VAULT_ADDR not configured")?;
    let role_id = config
        .vault_role_id
        .as_deref()
        .context("VAULT_ROLE_ID not configured")?;
    let bootstrap_token = config
        .vault_bootstrap_token
        .as_ref()
        .context("VAULT_BOOTSTRAP_TOKEN not configured")?;

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

    logger
        .log(job, &format!("🔑 Fetching secrets from Vault path: {}", secrets_cfg.path))
        .await?;

    let mut secrets = vault_client
        .fetch_ci_secrets(bootstrap_token, &secrets_cfg.path)
        .await?;

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

/// Fetch secrets from the local encrypted secrets store for a CI job.
async fn fetch_forgefile_store_secrets(
    _logger: &DbLogger,
    _job: &ClaimedJob,
    secrets_cfg: &forgefile_converter::SecretsConfig,
) -> Result<std::collections::HashMap<String, String>> {
    let store_path = std::env::var("FOUNDRY_SECRETS_PATH")
        .context("FOUNDRY_SECRETS_PATH not set — required for secrets from store()")?;
    let passphrase = std::env::var("FOUNDRY_SECRETS_PASSPHRASE")
        .context("FOUNDRY_SECRETS_PASSPHRASE not set — required for secrets from store()")?;

    let store = foundry_core::SecretsStore::load(std::path::Path::new(&store_path), &passphrase)
        .context("failed to load secrets store")?;

    let secret_map = store
        .get_secrets(&secrets_cfg.path)
        .cloned()
        .unwrap_or_default();

    let result: std::collections::HashMap<String, String> = if secrets_cfg.keys.is_empty() {
        secret_map
    } else {
        let requested_keys: std::collections::HashSet<&str> = secrets_cfg
            .keys
            .iter()
            .map(|k| k.name.as_str())
            .collect();
        secret_map
            .into_iter()
            .filter(|(k, _)| requested_keys.contains(k.as_str()))
            .collect()
    };

    Ok(result)
}

/// Fetch secrets from Vault for a CI job (legacy foundry.toml).
async fn fetch_vault_secrets(
    logger: &DbLogger,
    job: &ClaimedJob,
    config: &Config,
    secrets_cfg: &foundry_core::SecretsConfig,
) -> Result<std::collections::HashMap<String, secrecy::SecretString>> {
    let vault_addr = config
        .vault_addr
        .as_deref()
        .context("VAULT_ADDR not configured")?;
    let role_id = config
        .vault_role_id
        .as_deref()
        .context("VAULT_ROLE_ID not configured")?;
    let bootstrap_token = config
        .vault_bootstrap_token
        .as_ref()
        .context("VAULT_BOOTSTRAP_TOKEN not configured")?;

    let vault_path = secrets_cfg
        .vault_path
        .as_deref()
        .context("secrets.vault_path not set in foundry.toml")?;

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

    logger
        .log(job, &format!("🔑 Fetching secrets from Vault path: {}", vault_path))
        .await?;

    let mut secrets = vault_client
        .fetch_ci_secrets(bootstrap_token, vault_path)
        .await?;

    if let Some(ref keys) = secrets_cfg.keys {
        if !keys.is_empty() {
            secrets.retain(|k, _| keys.contains(k));
        }
    }

    Ok(secrets)
}
