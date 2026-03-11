//! Docker primitives for the built-in worker — container execution and image building.
//!
//! Adapted from foundry-agent's docker module to use DbLogger for direct DB logging.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use foundry_core::{ClaimedJob, FoundryConfig};

use crate::runtime::db_logger::DbLogger;

pub(crate) async fn build_image(
    logger: &DbLogger,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    fc: &FoundryConfig,
) -> Result<String> {
    let dockerfile = fc.build.dockerfile.as_deref().unwrap_or("Dockerfile");
    let context = fc.build.context.as_deref().unwrap_or(".");
    let image_tag = format!("foundry-{}-{}:latest", job.repo_name, job.id);

    logger.log(job, &format!("Building image from {}", dockerfile)).await?;

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
        logger.log(job, &format!("Build failed: {}", stderr)).await?;
        anyhow::bail!("Docker build failed");
    }

    logger.log(job, "Image built successfully").await?;
    Ok(image_tag)
}

pub(crate) async fn run_container(
    logger: &DbLogger,
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

    // Stream stdout to DB in real-time
    let logger_stdout = logger.clone();
    let job_id = job.id;
    let claim_token = job.claim_token;
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = logger_stdout.log_raw(job_id, &claim_token, &line).await;
        }
    });

    // Stream stderr to DB in real-time
    let logger_stderr = logger.clone();
    let job_id = job.id;
    let claim_token = job.claim_token;
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = logger_stderr
                .log_raw(job_id, &claim_token, &format!("STDERR: {}", line))
                .await;
        }
    });

    let timeout_duration = std::time::Duration::from_secs(timeout_secs);
    let wait_result = tokio::time::timeout(timeout_duration, child.wait()).await;

    let status = match wait_result {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Failed to wait for container: {}", e));
        }
        Err(_) => {
            logger.log(job, &format!("⏰ Build timed out after {} seconds", timeout_secs)).await?;

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
                    if let Err(e) = Command::new("docker")
                        .args(["kill", container_id.trim()])
                        .output()
                        .await
                    {
                        tracing::warn!("Failed to kill container {}: {}", container_id.trim(), e);
                    }
                }
            }

            return Err(anyhow::anyhow!("Build timed out after {} seconds", timeout_secs));
        }
    };

    // Wait for streaming tasks to finish flushing
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    Ok(status.success())
}
