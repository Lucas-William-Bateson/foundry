//! Shared watchdog module for monitoring and restarting peer containers.
//!
//! Both `foundryd` and `foundry-agent` use this to watch each other's containers,
//! restarting the peer if it becomes unhealthy.

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, info, warn};

/// Configuration for a watchdog instance.
pub struct WatchdogConfig {
    /// Docker container name to monitor (e.g. `"foundry-agent-1"`).
    pub container_name: String,
    /// Human-readable name used in log messages (e.g. `"Agent"` or `"foundryd"`).
    pub display_name: String,
    /// How often to poll container health.
    pub check_interval: Duration,
    /// Number of consecutive failures before triggering a restart.
    pub unhealthy_threshold: u32,
    /// Optional delay before the watchdog loop begins (e.g. to let things settle on startup).
    pub startup_delay: Option<Duration>,
    /// How long to wait after a restart for the container to come up.
    pub restart_delay: Duration,
}

/// Run the watchdog loop. This spawns a tokio task that periodically checks
/// the configured container's health and restarts it after consecutive failures.
pub fn start_watchdog(config: WatchdogConfig) {
    tokio::spawn(async move {
        info!("🐕 Starting {} watchdog", config.display_name);

        if let Some(delay) = config.startup_delay {
            tokio::time::sleep(delay).await;
        }

        let mut consecutive_failures = 0u32;

        loop {
            tokio::time::sleep(config.check_interval).await;

            match check_container_health(&config.container_name).await {
                Ok(true) => {
                    if consecutive_failures > 0 {
                        info!("🐕 {} container recovered", config.display_name);
                    }
                    consecutive_failures = 0;
                }
                Ok(false) => {
                    consecutive_failures += 1;
                    warn!(
                        "🐕 {} container unhealthy ({}/{})",
                        config.display_name, consecutive_failures, config.unhealthy_threshold
                    );

                    if consecutive_failures >= config.unhealthy_threshold {
                        error!(
                            "🐕 {} container appears down, attempting restart...",
                            config.display_name
                        );
                        if let Err(e) = restart_container(&config.container_name).await {
                            error!("🐕 Failed to restart {}: {}", config.display_name, e);
                        } else {
                            info!("🐕 {} container restart initiated", config.display_name);
                            consecutive_failures = 0;
                            // Wait a bit for container to come up
                            tokio::time::sleep(config.restart_delay).await;
                        }
                    }
                }
                Err(e) => {
                    warn!("🐕 Failed to check {} health: {}", config.display_name, e);
                }
            }
        }
    });
}

/// Check if a container is running and healthy.
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

/// Restart a container (tries `docker start`, then `docker restart` on failure).
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
