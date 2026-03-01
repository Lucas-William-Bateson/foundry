//! Watchdog module for monitoring and restarting peer containers
//!
//! This ensures that foundryd and the agent can restart each other
//! if one goes down.

use std::time::Duration;
use foundry_core::watchdog::{WatchdogConfig, start_watchdog};

/// Start the watchdog task that monitors the agent container
pub fn start_agent_watchdog() {
    start_watchdog(WatchdogConfig {
        container_name: "foundry-agent-1".to_string(),
        display_name: "Agent".to_string(),
        check_interval: Duration::from_secs(10),
        unhealthy_threshold: 3,
        startup_delay: None,
        restart_delay: Duration::from_secs(30),
    });
}
