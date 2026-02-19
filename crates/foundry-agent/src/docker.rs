use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info};

use foundry_core::{ClaimedJob, FoundryConfig};
use foundry_core::cloudflare::CloudflareClient;

use crate::config::Config;
use crate::github_app::GitHubApp;
use crate::server::ServerClient;

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

pub async fn run_job(
    client: &ServerClient,
    job: &ClaimedJob,
    config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    let job_start = Instant::now();
    
    if is_self_deploy(job, config) {
        return run_self_deploy(client, job, config, github_app).await;
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

    let foundry_config = FoundryConfig::load(&repo_dir);

    if let Some(ref fc) = foundry_config {
        client.log(job, "Found foundry.toml").await?;
        
        // Sync schedule configuration from foundry.toml to the server
        if let Err(e) = client.sync_schedule(job, fc.schedule.as_ref()).await {
            client.log(job, &format!("⚠️  Failed to sync schedule: {}", e)).await?;
        } else if fc.schedule.is_some() {
            let sched = fc.schedule.as_ref().unwrap();
            client.log(job, &format!("📅 Schedule synced: {}", sched.cron)).await?;
        }
        
        // Sync trigger configuration
        if let Err(e) = client.sync_triggers(job, &fc.triggers).await {
            client.log(job, &format!("⚠️  Failed to sync triggers: {}", e)).await?;
        } else {
            client.log(job, &format!("🎯 Triggers synced: branches={:?}", fc.triggers.branches)).await?;
        }
        
        if fc.deploy.is_enabled() {
            return run_deploy(client, job, &repo_dir, config, fc).await;
        }
        
        if fc.has_stages() {
            return run_stages(client, job, &repo_dir, config, fc, clone_duration_ms).await;
        }
    }

    let build_start = Instant::now();
    let (image, command) = if let Some(ref fc) = foundry_config {
        let img = if fc.build.dockerfile.is_some() {
            build_image(client, job, &repo_dir, fc).await?
        } else {
            fc.build.image.clone()
        };
        let cmd = fc.effective_command(&config.default_command);
        (img, cmd)
    } else {
        (job.image.clone(), config.default_command.clone())
    };
    let build_duration_ms = build_start.elapsed().as_millis() as u64;

    client
        .log(job, &format!("Running in container: {}", image))
        .await?;

    let env_vars = foundry_config.as_ref().map(|fc| &fc.env);
    let timeout_secs = foundry_config.as_ref().map(|fc| fc.build.timeout).unwrap_or(1800);
    
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

async fn run_stages(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    config: &Config,
    fc: &FoundryConfig,
    clone_duration_ms: u64,
) -> Result<()> {
    let job_start = Instant::now();
    let mut stage_metrics: Vec<StageMetrics> = vec![];
    let mut any_failed = false;
    
    let image = if fc.build.dockerfile.is_some() {
        build_image(client, job, repo_dir, fc).await?
    } else {
        fc.build.image.clone()
    };
    
    client.log(job, &format!("📋 Running {} stages", fc.stages.len())).await?;
    
    for (i, stage) in fc.stages.iter().enumerate() {
        let stage_image = stage.image.as_ref().unwrap_or(&image);
        let stage_start = Instant::now();
        
        let should_run = match &stage.condition {
            Some(foundry_core::config::StageCondition::Always) => true,
            Some(foundry_core::config::StageCondition::OnFailure) => any_failed,
            Some(foundry_core::config::StageCondition::OnSuccess) | None => !any_failed,
            Some(foundry_core::config::StageCondition::OnPr) => job.git_ref.starts_with("refs/pull/"),
            Some(foundry_core::config::StageCondition::OnPush) => !job.git_ref.starts_with("refs/pull/"),
        };
        
        if !should_run {
            client.log(job, &format!("⏭️  Stage {}: {} (skipped)", i + 1, stage.name)).await?;
            stage_metrics.push(StageMetrics {
                name: stage.name.clone(),
                status: "skipped".to_string(),
                duration_ms: 0,
                exit_code: None,
            });
            continue;
        }
        
        client.log(job, &format!("▶️  Stage {}: {}", i + 1, stage.name)).await?;
        
        let mut stage_env = fc.env.clone();
        stage_env.extend(stage.env.clone());
        
        let result = run_container(
            client,
            job,
            repo_dir,
            stage_image,
            &stage.command,
            Some(&stage_env),
            stage.timeout,
        ).await;
        
        let duration_ms = stage_start.elapsed().as_millis() as u64;
        
        match result {
            Ok(true) => {
                client.log(job, &format!("✅ Stage {} complete ({} ms)", stage.name, duration_ms)).await?;
                stage_metrics.push(StageMetrics {
                    name: stage.name.clone(),
                    status: "success".to_string(),
                    duration_ms,
                    exit_code: Some(0),
                });
            }
            Ok(false) | Err(_) => {
                client.log(job, &format!("❌ Stage {} failed ({} ms)", stage.name, duration_ms)).await?;
                stage_metrics.push(StageMetrics {
                    name: stage.name.clone(),
                    status: "failed".to_string(),
                    duration_ms,
                    exit_code: Some(1),
                });
                
                if !stage.allow_failure {
                    any_failed = true;
                    if stage.condition.is_none() || stage.condition == Some(foundry_core::config::StageCondition::OnSuccess) {
                        break;
                    }
                }
            }
        }
    }
    
    let total_duration_ms = job_start.elapsed().as_millis() as u64;
    let metrics = JobMetrics {
        clone_duration_ms,
        build_duration_ms: None,
        stages: stage_metrics,
        total_duration_ms,
    };
    
    client.report_metrics(job, &metrics).await.ok();
    
    if any_failed {
        anyhow::bail!("Pipeline failed")
    }
    
    Ok(())
}

async fn run_self_deploy(
    client: &ServerClient,
    job: &ClaimedJob,
    config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    info!("Self-deploy triggered for Foundry");
    client.log(job, "🔄 Self-deploy triggered").await?;

    let script = config
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

async fn build_image(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    fc: &FoundryConfig,
) -> Result<String> {
    let dockerfile = fc.build.dockerfile.as_deref().unwrap_or("Dockerfile");
    let context = fc.build.context.as_deref().unwrap_or(".");
    let image_tag = format!("foundry-{}-{}:latest", job.repo_name, job.id);

    client.log(job, &format!("Building image from {}", dockerfile)).await?;

    let context_path = repo_dir.join(context);

    let output = Command::new("docker")
        .args([
            "build",
            "-t", &image_tag,
            "-f", &repo_dir.join(dockerfile).to_string_lossy(),
            &context_path.to_string_lossy(),
        ])
        .current_dir(repo_dir)
        .output()
        .await
        .context("Failed to run docker build")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        client.log(job, &format!("Build failed: {}", stderr)).await?;
        anyhow::bail!("Docker build failed");
    }

    client.log(job, "Image built successfully").await?;
    Ok(image_tag)
}

async fn run_deploy(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    _config: &Config,
    fc: &FoundryConfig,
) -> Result<()> {
    let app_name = fc.deploy.name.as_deref().unwrap_or(&job.repo_name);

    client.log(job, &format!("🚀 Deploying {}", app_name)).await?;

    if let Some(compose_file) = &fc.deploy.compose_file {
        client.log(job, &format!("Using compose file: {}", compose_file)).await?;

        let compose_path = repo_dir.join(compose_file);

        let mut args = vec![
            "compose".to_string(),
            "-f".to_string(),
            compose_path.to_string_lossy().to_string(),
            "-p".to_string(),
            app_name.to_string(),
        ];

        // Add env file if specified (absolute path on host)
        if let Some(env_file) = &fc.deploy.env_file {
            client.log(job, &format!("Using env file: {}", env_file)).await?;
            args.push("--env-file".to_string());
            args.push(env_file.clone());
        }

        args.extend(["up", "-d", "--build", "--force-recreate"].iter().map(|s| s.to_string()));

        // Inject secrets from Proton Pass if template exists
        let template_path = repo_dir.join("secrets.env.template");
        if template_path.exists() {
            client.log(job, "Injecting secrets from Proton Pass...").await?;
            let inject_output = Command::new("pass-cli")
                .args([
                    "inject",
                    "--in-file", "secrets.env.template",
                    "--out-file", "secrets.env",
                    "--force",
                ])
                .current_dir(repo_dir)
                .output()
                .await
                .context("Failed to run pass-cli inject (is pass-cli installed?)")?;

            if !inject_output.status.success() {
                let stderr = String::from_utf8_lossy(&inject_output.stderr);
                anyhow::bail!("pass-cli inject failed: {}", stderr);
            }
            client.log(job, "Secrets injected successfully").await?;
        }

        let output = Command::new("docker")
            .args(&args)
            .current_dir(repo_dir)
            .output()
            .await
            .context("Failed to run docker compose")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            client.log(job, &format!("Deploy failed: {}", stderr)).await?;
            anyhow::bail!("Docker compose failed");
        }
    } else {
        let image_tag = if fc.build.dockerfile.is_some() {
            build_image(client, job, repo_dir, fc).await?
        } else {
            fc.build.image.clone()
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

        if let Some(port) = fc.deploy.port {
            args.push("-p".to_string());
            args.push(format!("{}:{}", port, port));
        }

        // Add volume mounts (validated)
        if let Some(volumes) = &fc.deploy.volumes {
            for vol in volumes {
                // Validate volume spec: block host paths that could compromise the host
                let host_part = vol.split(':').next().unwrap_or("");
                let blocked = [
                    "/var/run/docker.sock",
                    "/etc",
                    "/root",
                    "/home",
                    "/proc",
                    "/sys",
                    "/dev",
                    "/boot",
                    "/var/run",
                ];
                let is_blocked = blocked.iter().any(|b| host_part == *b || host_part.starts_with(&format!("{}/", b)));
                if is_blocked {
                    tracing::warn!("Blocked dangerous volume mount: {}", vol);
                    return Err(anyhow::anyhow!("Volume mount not allowed: {}", host_part));
                }
                args.push("-v".to_string());
                args.push(vol.clone());
            }
        }

        for (key, value) in &fc.env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }

        args.push(image_tag);

        if let Some(cmd) = &fc.build.command {
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

    let domains = fc.deploy.all_domains();
    if !domains.is_empty() {
        let port = fc.deploy.port.unwrap_or(8080);
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

async fn run_container(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    image: &str,
    command: &str,
    env_vars: Option<&std::collections::HashMap<String, String>>,
    timeout_secs: u64,
) -> Result<bool> {
    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        "-v".to_string(),
        format!("{}:/work", repo_dir.display()),
        "-w".to_string(),
        "/work".to_string(),
    ];

    if let Some(env) = env_vars {
        for (key, value) in env {
            args.push("-e".to_string());
            args.push(format!("{}={}", key, value));
        }
    }

    args.push(image.to_string());
    args.push("bash".to_string());
    args.push("-lc".to_string());
    args.push(command.to_string());

    let mut child = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start docker container")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            lines.push(line);
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut lines = Vec::new();
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            lines.push(format!("STDERR: {}", line));
        }
        lines
    });

    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

    let status = match wait_result {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Failed to wait for container: {}", e));
        }
        Err(_) => {
            client.log(job, &format!("⏰ Build timed out after {} seconds", timeout_secs)).await?;
            
            if let Err(e) = child.kill().await {
                tracing::warn!("Failed to kill timed out process: {}", e);
            }
            
            let container_list = Command::new("docker")
                .args(["ps", "-q", "--filter", &format!("label=foundry.job_id={}", job.id)])
                .output()
                .await;
            
            if let Ok(output) = container_list {
                let container_ids = String::from_utf8_lossy(&output.stdout);
                for container_id in container_ids.lines() {
                    let _ = Command::new("docker")
                        .args(["kill", container_id.trim()])
                        .output()
                        .await;
                }
            }
            
            return Err(anyhow::anyhow!("Build timed out after {} seconds", timeout_secs));
        }
    };

    if let Ok(stdout_lines) = stdout_handle.await {
        for line in stdout_lines {
            let _ = client.log(job, &line).await;
        }
    }

    if let Ok(stderr_lines) = stderr_handle.await {
        for line in stderr_lines {
            let _ = client.log(job, &line).await;
        }
    }

    Ok(status.success())
}
