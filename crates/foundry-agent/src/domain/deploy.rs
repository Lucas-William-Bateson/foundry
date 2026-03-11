//! Deployment logic — container deployment, compose-based deploys, domain routing,
//! and Cloudflare integration.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::process::Command;

use foundry_core::{ClaimedJob, FoundryConfig};
use foundry_core::cloudflare::CloudflareClient;

use crate::types::config::Config;
use crate::infrastructure::docker::build_image;
use crate::api::server::ServerClient;

/// Allowed host path prefixes for volume mounts.
/// Only these directories (and their subdirectories) may be mounted.
const ALLOWED_VOLUME_PREFIXES: &[&str] = &[
    "/data/",
    "/opt/foundry/",
    "/tmp/foundry/",
    "/var/lib/foundry/",
];

/// Validate a volume mount specification.
/// Returns Ok(()) if safe, Err with reason if not.
fn validate_volume_mount(vol: &str) -> Result<()> {
    let host_part = vol.split(':').next().unwrap_or("");

    // Block empty host paths
    if host_part.is_empty() {
        anyhow::bail!("Empty host path in volume mount");
    }

    // Only allow named volumes (no path separator) or absolute paths in the allowlist
    if !host_part.contains('/') {
        // Named volume — always allowed
        return Ok(());
    }

    // Must be absolute path
    if !host_part.starts_with('/') {
        anyhow::bail!("Volume mount must use absolute path: {}", host_part);
    }

    // Canonicalize to resolve "..", symlinks, etc.
    // Use the lexical approach since the path may not exist yet on the host
    let normalized = lexical_clean(host_part);

    // Check for path traversal after normalization
    if normalized.contains("..") {
        anyhow::bail!("Path traversal detected in volume mount: {}", host_part);
    }

    // Must be under an allowed prefix
    let allowed = ALLOWED_VOLUME_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix) || normalized == prefix.trim_end_matches('/'));

    if !allowed {
        anyhow::bail!(
            "Volume mount host path not in allowlist: {}. Allowed prefixes: {:?}",
            normalized,
            ALLOWED_VOLUME_PREFIXES
        );
    }

    Ok(())
}

/// Lexical path cleaning: resolve `.` and `..` components without filesystem access.
fn lexical_clean(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            other => components.push(other),
        }
    }
    format!("/{}", components.join("/"))
}

/// Validate a deploy name: must be alphanumeric + hyphens only, max 63 chars.
/// This is used as a Docker container name and compose project name.
fn validate_deploy_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Deploy name cannot be empty");
    }
    if name.len() > 63 {
        anyhow::bail!("Deploy name too long (max 63 chars): {}", name);
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        anyhow::bail!(
            "Deploy name contains invalid characters (only alphanumeric, hyphens, underscores allowed): {}",
            name
        );
    }
    Ok(())
}

pub(crate) async fn run_deploy(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    _config: &Config,
    untrusted_repo_config: &FoundryConfig,
) -> Result<()> {
    let app_name = untrusted_repo_config.deploy.name.as_deref().unwrap_or(&job.repo_name);

    // Validate deploy name before using it as container/project name
    validate_deploy_name(app_name)?;

    client.log(job, &format!("🚀 Deploying {}", app_name)).await?;

    if let Some(compose_file) = &untrusted_repo_config.deploy.compose_file {
        client.log(job, &format!("Using compose file: {}", compose_file)).await?;

        let compose_path = repo_dir.join(compose_file);

        // Build the complete set of env vars: deploy metadata + vault secrets + Forgefile env
        let mut env_vars: HashMap<String, String> = HashMap::new();

        // Deploy metadata (lowest priority — can be overridden by vault/env)
        env_vars.insert("APP_NAME".into(), app_name.to_string());
        env_vars.insert("CONTAINER_NAME".into(), format!("foundry-{}", app_name));
        env_vars.insert("ENVIRONMENT".into(), "production".into());
        env_vars.insert("VERSION".into(), job.git_sha.clone());
        if let Some(port) = untrusted_repo_config.deploy.port {
            env_vars.insert("APP_PORT".into(), port.to_string());
            env_vars.insert("HOST_PORT".into(), port.to_string());
        }
        if let Some(domain) = &untrusted_repo_config.deploy.domain {
            env_vars.insert("APP_DOMAIN".into(), domain.clone());
        }

        // Vault secrets + Forgefile env (highest priority — overrides metadata defaults)
        for (key, value) in &untrusted_repo_config.env {
            env_vars.insert(key.clone(), value.clone());
        }

        if !untrusted_repo_config.env.is_empty() {
            client.log(job, &format!("🔐 Injecting {} env var(s) from Vault", untrusted_repo_config.env.len())).await?;
        }

        // Write env vars to .env and secrets.env so compose files that use
        // env_file or ${VAR} interpolation can access them
        let env_content: String = env_vars.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        // Write .env next to the compose file
        if let Some(compose_dir) = compose_path.parent() {
            let _ = tokio::fs::write(compose_dir.join(".env"), &env_content).await;
        }
        // Write secrets.env and .env in repo root for backward compat
        let _ = tokio::fs::write(repo_dir.join(".env"), &env_content).await;
        let _ = tokio::fs::write(repo_dir.join("secrets.env"), &env_content).await;

        // Stop existing project containers to free ports before re-deploying
        client.log(job, &format!("Stopping existing {} containers...", app_name)).await?;
        let _ = Command::new("docker")
            .args(["compose", "-p", app_name, "down", "--remove-orphans"])
            .current_dir(repo_dir)
            .output()
            .await;

        let mut args = vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.to_string_lossy().to_string(),
            "-p".to_string(),
            app_name.to_string(),
        ];

        args.extend(["up", "-d", "--build", "--force-recreate"].iter().map(|s| s.to_string()));

        let mut cmd = Command::new("docker");
        cmd.args(&args).current_dir(repo_dir);

        // Pass all env vars to the compose process
        for (key, value) in &env_vars {
            cmd.env(key, value);
        }

        let output = cmd
            .output()
            .await
            .context("Failed to run docker compose")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            client.log(job, &format!("Deploy failed: {}", stderr)).await?;
            anyhow::bail!("Docker compose failed");
        }
    } else {
        let image_tag = if untrusted_repo_config.build.dockerfile.is_some() {
            build_image(client, job, repo_dir, untrusted_repo_config).await?
        } else {
            untrusted_repo_config.build.image.clone()
        };

        let container_name = format!("foundry-{}", app_name);

        client.log(job, &format!("Stopping existing container: {}", container_name)).await?;
        let _ = Command::new("docker")
            .args(["stop", &container_name])
            .output()
            .await;
        let _ = Command::new("docker")
            .args(["rm", &container_name])
            .output()
            .await;

        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            container_name.clone(),
            "--restart".to_string(),
            "unless-stopped".to_string(),
        ];

        if let Some(port) = untrusted_repo_config.deploy.port {
            args.push("-p".to_string());
            args.push(format!("{}:{}", port, port));
        }

        // Add volume mounts (validated against allowlist)
        if let Some(volumes) = &untrusted_repo_config.deploy.volumes {
            for vol in volumes {
                validate_volume_mount(vol).map_err(|e| {
                    tracing::warn!("Blocked volume mount '{}': {}", vol, e);
                    e
                })?;
                args.push("-v".to_string());
                args.push(vol.clone());
            }
        }

        for (key, value) in &untrusted_repo_config.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(image_tag);

        if let Some(cmd) = &untrusted_repo_config.build.command {
            args.extend(cmd.split_whitespace().map(String::from));
        }

        client.log(job, &format!("Starting container: {}", container_name)).await?;

        let output = Command::new("docker")
            .args(&args)
            .current_dir(repo_dir)
            .output()
            .await
            .context("Failed to start container")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            client.log(job, &format!("Failed to start: {}", stderr)).await?;
            anyhow::bail!("Failed to start container");
        }
    }

    let domains = untrusted_repo_config.deploy.all_domains();
    if !domains.is_empty() {
        let port = untrusted_repo_config.deploy.port.unwrap_or(8080);
        client.log(job, &format!("🌐 Configuring {} domain route(s) -> port {}", domains.len(), port)).await?;
        
        for domain in domains {
            match setup_domain_route(domain, port).await {
                Ok(()) => {
                    client.log(job, &format!("✅ Domain configured: https://{}", domain)).await?;
                }
                Err(e) => {
                    client.log(job, &format!("⚠️ Failed to setup domain route for {}: {}", domain, e)).await?;
                    tracing::error!("Failed to setup domain route for {}: {}", domain, e);
                }
            }
        }
    }

    client.log(job, &format!("✅ {} deployed successfully", app_name)).await?;
    Ok(())
}

async fn setup_domain_route(domain: &str, port: u16) -> anyhow::Result<()> {
    if let Some(cf_client) = CloudflareClient::from_env()? {
        if let Some(existing_service) = cf_client.get_route(domain).await? {
            let new_service = format!("http://127.0.0.1:{}", port);
            if existing_service != new_service {
                tracing::info!(
                    "Domain {} is currently routed to {}, updating to {}",
                    domain, existing_service, new_service
                );
            }
        }

        let service = format!("http://127.0.0.1:{}", port);
        cf_client.add_route(domain, &service).await?;
        tracing::info!("Domain route configured: {} -> {}", domain, service);
    } else {
        tracing::warn!(
            "Cloudflare credentials not configured, skipping domain setup for {}",
            domain
        );
    }
    Ok(())
}
