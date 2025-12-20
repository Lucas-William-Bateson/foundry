use anyhow::{Context, Result};
use uuid::Uuid;

#[derive(Clone)]
pub struct Config {
    pub agent_id: String,
    pub server_url: String,
    pub workspace_dir: String,
    pub poll_interval_secs: u64,
    pub default_command: String,
    pub github_app_id: Option<String>,
    pub github_installation_id: Option<String>,
    pub github_private_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let github_private_key = match std::env::var("GITHUB_APP_PRIVATE_KEY_PATH") {
            Ok(path) => Some(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read GitHub App private key from {}", path))?,
            ),
            Err(_) => std::env::var("GITHUB_APP_PRIVATE_KEY").ok(),
        };

        Ok(Self {
            agent_id: std::env::var("FOUNDRY_AGENT_ID")
                .unwrap_or_else(|_| format!("agent-{}", &Uuid::new_v4().to_string()[..8])),

            server_url: std::env::var("FOUNDRY_SERVER_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),

            workspace_dir: std::env::var("FOUNDRY_WORKSPACE_DIR")
                .unwrap_or_else(|_| "/tmp/foundry".to_string()),

            poll_interval_secs: std::env::var("FOUNDRY_POLL_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),

            default_command: std::env::var("FOUNDRY_DEFAULT_COMMAND")
                .unwrap_or_else(|_| "echo 'No command configured'".to_string()),

            github_app_id: std::env::var("GITHUB_APP_ID").ok(),
            github_installation_id: std::env::var("GITHUB_INSTALLATION_ID").ok(),
            github_private_key,
        })
    }

    pub fn has_github_app(&self) -> bool {
        self.github_app_id.is_some()
            && self.github_installation_id.is_some()
            && self.github_private_key.is_some()
    }
}
