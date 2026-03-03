//! Watchdog module for monitoring and restarting foundryd
//!
//! This ensures the agent can restart foundryd if it goes down.

use std::time::Duration;
use foundry_core::watchdog::{WatchdogConfig, start_watchdog};

/// Start the watchdog task that monitors the foundryd container.
/// Container name can be overridden via FOUNDRY_SERVER_CONTAINER env var.
pub fn start_foundryd_watchdog() {
    let container_name = std::env::var("FOUNDRY_SERVER_CONTAINER")
        .unwrap_or_else(|_| "foundry-foundryd-1".to_string());
    start_watchdog(WatchdogConfig {
        container_name,
        display_name: "foundryd".to_string(),
        check_interval: Duration::from_secs(10),
        unhealthy_threshold: 3,
        startup_delay: Some(Duration::from_secs(30)),
        restart_delay: Duration::from_secs(30),
    });
}
