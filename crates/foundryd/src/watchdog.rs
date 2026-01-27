//! Watchdog module for monitoring and restarting peer containers
//! 
//! This ensures that foundryd and the agent can restart each other
//! if one goes down.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, info, warn};

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const UNHEALTHY_THRESHOLD: u32 = 3;

/// Start the watchdog task that monitors the agent container
pub fn start_agent_watchdog() {
    tokio::spawn(async move {
        info!("🐕 Starting agent watchdog");
        let mut consecutive_failures = 0u32;
        
        loop {
            tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
            
            match check_container_health("foundry-agent-1").await {
                Ok(true) => {
                    if consecutive_failures > 0 {
                        info!("🐕 Agent container recovered");
                    }
                    consecutive_failures = 0;
                }
                Ok(false) => {
                    consecutive_failures += 1;
                    warn!(
                        "🐕 Agent container unhealthy ({}/{})",
                        consecutive_failures, UNHEALTHY_THRESHOLD
                    );
                    
                    if consecutive_failures >= UNHEALTHY_THRESHOLD {
                        error!("🐕 Agent container appears down, attempting restart...");
                        if let Err(e) = restart_container("foundry-agent-1").await {
                            error!("🐕 Failed to restart agent: {}", e);
                        } else {
                            info!("🐕 Agent container restart initiated");
                            consecutive_failures = 0;
                            // Wait a bit for container to come up
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                    }
                }
                Err(e) => {
                    warn!("🐕 Failed to check agent health: {}", e);
                }
            }
        }
    });
}

/// Check if a container is running and healthy
async fn check_container_health(container_name: &str) -> Result<bool> {
    // First check if container exists and is running
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()
        .await
        .context("Failed to run docker inspect")?;
    
    if !output.status.success() {
        // Container doesn't exist
        return Ok(false);
    }
    
    let running = String::from_utf8_lossy(&output.stdout).trim() == "true";
    if !running {
        return Ok(false);
    }
    
    // Check if container is healthy (if it has a healthcheck)
    let output = Command::new("docker")
        .args(["inspect", "-f", "{{.State.Health.Status}}", container_name])
        .output()
        .await
        .context("Failed to check health status")?;
    
    let health = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    // If no healthcheck configured, just check if running
    if health.is_empty() || health == "<no value>" {
        return Ok(running);
    }
    
    Ok(health == "healthy")
}

/// Restart a container
async fn restart_container(container_name: &str) -> Result<()> {
    // Try to start if stopped, or restart if running
    let output = Command::new("docker")
        .args(["start", container_name])
        .output()
        .await
        .context("Failed to start container")?;
    
    if !output.status.success() {
        // Try restart instead
        let output = Command::new("docker")
            .args(["restart", container_name])
            .output()
            .await
            .context("Failed to restart container")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker restart failed: {}", stderr);
        }
    }
    
    Ok(())
}
