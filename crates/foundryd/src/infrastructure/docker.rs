//! Docker container management module
//! 
//! Provides functionality to list, inspect, and manage Docker containers
//! deployed by Foundry.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

/// Information about a Docker container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: String,
    pub ports: String,
    pub project: Option<String>,
}

/// Container logs response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerLogs {
    pub container_id: String,
    pub logs: Vec<String>,
}

/// List all running containers, optionally filtered by project name
pub async fn list_containers(project_filter: Option<&str>) -> Result<Vec<ContainerInfo>> {
    let format = r#"{{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}\t{{.State}}\t{{.CreatedAt}}\t{{.Ports}}\t{{index .Labels "com.docker.compose.project"}}"#;
    
    let output = Command::new("docker")
        .args(["ps", "-a", "--format", format])
        .output()
        .await
        .context("Failed to run docker ps")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker ps failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let project = if parts.len() > 7 && !parts[7].is_empty() {
                Some(parts[7].to_string())
            } else {
                None
            };

            // Apply project filter if specified
            if let Some(filter) = project_filter {
                if project.as_deref() != Some(filter) {
                    continue;
                }
            }

            containers.push(ContainerInfo {
                id: parts[0].to_string(),
                name: parts[1].to_string(),
                image: parts[2].to_string(),
                status: parts[3].to_string(),
                state: parts[4].to_string(),
                created: parts[5].to_string(),
                ports: parts[6].to_string(),
                project,
            });
        }
    }

    Ok(containers)
}

/// Get logs from a specific container
pub async fn get_container_logs(container_id: &str, lines: Option<u32>) -> Result<ContainerLogs> {
    let mut args = vec!["logs".to_string()];
    
    if let Some(n) = lines {
        args.push("--tail".to_string());
        args.push(n.to_string());
    }
    
    args.push("--timestamps".to_string());
    args.push(container_id.to_string());

    let output = Command::new("docker")
        .args(&args)
        .output()
        .await
        .context("Failed to get container logs")?;

    // Docker logs outputs to both stdout and stderr
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    let mut logs: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
    logs.extend(stderr.lines().map(|s| s.to_string()));
    
    // Sort by timestamp if possible
    logs.sort();

    Ok(ContainerLogs {
        container_id: container_id.to_string(),
        logs,
    })
}

/// Stream logs from a container (returns a channel for live updates)
pub async fn stream_container_logs(
    container_id: &str,
    lines: Option<u32>,
) -> Result<mpsc::Receiver<String>> {
    let (tx, rx) = mpsc::channel(100);
    
    let mut args = vec!["logs", "-f", "--timestamps"];
    
    let tail_str;
    if let Some(n) = lines {
        tail_str = n.to_string();
        args.push("--tail");
        args.push(&tail_str);
    }
    
    args.push(container_id);

    let mut child = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn docker logs")?;

    let stdout = child.stdout.take().expect("stdout not captured");
    let stderr = child.stderr.take().expect("stderr not captured");

    // Spawn task to read stdout
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_clone.send(line).await.is_err() {
                break;
            }
        }
    });

    // Spawn task to read stderr
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(line).await.is_err() {
                break;
            }
        }
    });

    Ok(rx)
}

/// Restart a specific container
pub async fn restart_container(container_id: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["restart", container_id])
        .output()
        .await
        .context("Failed to restart container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to restart container: {}", stderr);
    }

    Ok(())
}

/// Stop a specific container
pub async fn stop_container(container_id: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["stop", container_id])
        .output()
        .await
        .context("Failed to stop container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to stop container: {}", stderr);
    }

    Ok(())
}

/// Start a stopped container
pub async fn start_container(container_id: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["start", container_id])
        .output()
        .await
        .context("Failed to start container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to start container: {}", stderr);
    }

    Ok(())
}

/// Restart all containers in a docker-compose project
pub async fn restart_project(project_name: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["compose", "-p", project_name, "restart"])
        .output()
        .await
        .context("Failed to restart project")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to restart project: {}", stderr);
    }

    Ok(())
}

/// Stop all containers in a docker-compose project
pub async fn stop_project(project_name: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["compose", "-p", project_name, "stop"])
        .output()
        .await
        .context("Failed to stop project")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to stop project: {}", stderr);
    }

    Ok(())
}

/// Start all containers in a docker-compose project
pub async fn start_project(project_name: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["compose", "-p", project_name, "start"])
        .output()
        .await
        .context("Failed to start project")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to start project: {}", stderr);
    }

    Ok(())
}

/// Get a list of all docker-compose projects
pub async fn list_projects() -> Result<Vec<String>> {
    let output = Command::new("docker")
        .args(["compose", "ls", "--format", "json"])
        .output()
        .await
        .context("Failed to list projects")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to list projects: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    #[derive(Deserialize)]
    struct ProjectInfo {
        #[serde(rename = "Name")]
        name: String,
    }

    let projects: Vec<ProjectInfo> = serde_json::from_str(&stdout).unwrap_or_default();
    Ok(projects.into_iter().map(|p| p.name).collect())
}
