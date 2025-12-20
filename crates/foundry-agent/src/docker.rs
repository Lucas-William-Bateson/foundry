use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::debug;

use foundry_core::ClaimedJob;

use crate::config::Config;
use crate::github_app::GitHubApp;
use crate::server::ServerClient;

pub async fn run_job(
    client: &ServerClient,
    job: &ClaimedJob,
    config: &Config,
    github_app: Option<&GitHubApp>,
) -> Result<()> {
    let workspace = PathBuf::from(&config.workspace_dir).join(format!("job-{}", job.id));

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

    client
        .log(
            job,
            &format!(
                "Cloning {} @ {}",
                job.clone_url,
                &job.git_sha[..8.min(job.git_sha.len())]
            ),
        )
        .await?;

    clone_repo(&clone_url, &job.clone_url, &job.git_sha, &repo_dir).await?;

    client.log(job, "Clone complete").await?;

    client
        .log(job, &format!("Running in container: {}", job.image))
        .await?;

    let success = run_container(client, job, &repo_dir, config).await?;

    if let Err(e) = tokio::fs::remove_dir_all(&workspace).await {
        debug!("Failed to cleanup workspace: {}", e);
    }

    if success {
        Ok(())
    } else {
        anyhow::bail!("Container exited with non-zero status")
    }
}

async fn clone_repo(url: &str, safe_url: &str, sha: &str, dest: &PathBuf) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", "--depth", "50", url])
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

    let output = Command::new("git")
        .args(["checkout", sha])
        .current_dir(dest)
        .output()
        .await
        .context("Failed to run git checkout")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git checkout failed: {}", stderr);
    }

    Ok(())
}

fn sanitize_git_error(stderr: &str, secret_url: &str, safe_url: &str) -> String {
    stderr.replace(secret_url, safe_url)
}

async fn run_container(
    client: &ServerClient,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    config: &Config,
) -> Result<bool> {
    let command = &config.default_command;

    let mut child = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("{}:/work", repo_dir.display()),
            "-w",
            "/work",
            &job.image,
            "bash",
            "-lc",
            command,
        ])
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

    let status = child.wait().await.context("Failed to wait for container")?;

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
