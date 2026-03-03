//! Watchdog module for monitoring and restarting peer containers
//!
//! This ensures that foundryd and the agent can restart each other
//! if one goes down.

use std::time::Duration;
use foundry_core::watchdog::{WatchdogConfig, start_watchdog};

/// Start the watchdog task that monitors the agent container.
/// Container name can be overridden via FOUNDRY_AGENT_CONTAINER env var.
pub fn start_agent_watchdog() {
    let container_name = std::env::var("FOUNDRY_AGENT_CONTAINER")
        .unwrap_or_else(|_| "foundry-agent-1".to_string());
    start_watchdog(WatchdogConfig {
        container_name,
        display_name: "Agent".to_string(),
        check_interval: Duration::from_secs(10),
        unhealthy_threshold: 3,
        startup_delay: None,
        restart_delay: Duration::from_secs(30),
    });
}
