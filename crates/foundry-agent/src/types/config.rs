use anyhow::{Context, Result};
use secrecy::SecretString;
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
    pub self_repo: Option<String>,
    pub self_deploy_script: Option<String>,
    /// Vault AppRole role_id (non-secret, stored in CI config)
    pub vault_role_id: Option<String>,
    /// Vault bootstrap token for generating per-job secret_ids.
    /// Wrapped in SecretString to prevent accidental logging.
    pub vault_bootstrap_token: Option<SecretString>,
    /// Vault address (e.g. http://vault:8200)
    pub vault_addr: Option<String>,
    /// Shared secret for authenticating with the server
    pub agent_secret: Option<String>,
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
            self_repo: std::env::var("FOUNDRY_SELF_REPO").ok(),
            self_deploy_script: std::env::var("FOUNDRY_SELF_DEPLOY_SCRIPT").ok(),
            vault_addr: std::env::var("VAULT_ADDR").ok(),
            vault_role_id: std::env::var("VAULT_ROLE_ID").ok(),
            vault_bootstrap_token: Self::load_vault_bootstrap_token(),
            agent_secret: std::env::var("FOUNDRY_AGENT_SECRET").ok(),
        })
    }

    pub fn has_github_app(&self) -> bool {
        self.github_app_id.is_some()
            && self.github_installation_id.is_some()
            && self.github_private_key.is_some()
    }

    pub fn has_vault(&self) -> bool {
        self.vault_addr.is_some() && self.vault_role_id.is_some()
    }

    /// Load the Vault bootstrap token from env var or file.
    /// The bootstrap token is used to generate per-job secret_ids.
    /// Immediately wrapped in SecretString; the raw env var is removed.
    fn load_vault_bootstrap_token() -> Option<SecretString> {
        // Try env var first
        if let Ok(token) = std::env::var("VAULT_BOOTSTRAP_TOKEN") {
            // Remove from process env — only our SecretString copy should exist
            std::env::remove_var("VAULT_BOOTSTRAP_TOKEN");
            return Some(SecretString::from(token));
        }
        // Try file (600 perms on host)
        if let Ok(path) = std::env::var("VAULT_BOOTSTRAP_TOKEN_FILE") {
            if let Ok(token) = std::fs::read_to_string(&path) {
                let token = token.trim().to_string();
                if !token.is_empty() {
                    return Some(SecretString::from(token));
                }
            }
        }
        None
    }
}
