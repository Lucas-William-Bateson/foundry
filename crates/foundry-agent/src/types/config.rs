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
    pub vault_role_id: Option<String>,
    pub vault_bootstrap_token: Option<SecretString>,
    pub vault_addr: Option<String>,
    pub agent_secret: Option<String>,
    pub runner_name: Option<String>,
    pub runner_tags: Vec<String>,
    pub runner_cpu: Option<i32>,
    pub runner_mem_mb: Option<i32>,
    pub runner_gpu: i32,
    pub runner_arch: String,
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
            runner_name: std::env::var("FOUNDRY_RUNNER_NAME").ok(),
            runner_tags: std::env::var("FOUNDRY_RUNNER_TAGS")
                .ok()
                .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                .unwrap_or_default(),
            runner_cpu: std::env::var("FOUNDRY_RUNNER_CPU")
                .ok()
                .and_then(|v| v.parse().ok()),
            runner_mem_mb: std::env::var("FOUNDRY_RUNNER_MEM")
                .ok()
                .and_then(|v| Self::parse_memory(&v)),
            runner_gpu: std::env::var("FOUNDRY_RUNNER_GPU")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            runner_arch: std::env::var("FOUNDRY_RUNNER_ARCH")
                .unwrap_or_else(|_| std::env::consts::ARCH.to_string()),
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

    fn parse_memory(s: &str) -> Option<i32> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (num_str, multiplier) = if let Some(n) = s.strip_suffix('G').or_else(|| s.strip_suffix('g')) {
            (n, 1024)
        } else if let Some(n) = s.strip_suffix('M').or_else(|| s.strip_suffix('m')) {
            (n, 1)
        } else if let Some(n) = s.strip_suffix('T').or_else(|| s.strip_suffix('t')) {
            (n, 1024 * 1024)
        } else {
            (s, 1)
        };
        num_str.trim().parse::<i32>().ok().map(|n| n * multiplier)
    }

    pub fn effective_runner_name(&self) -> String {
        self.runner_name
            .clone()
            .or_else(|| hostname::get().ok().and_then(|h| h.into_string().ok()))
            .unwrap_or_else(|| self.agent_id.clone())
    }

    fn load_vault_bootstrap_token() -> Option<SecretString> {
        if let Ok(token) = std::env::var("VAULT_BOOTSTRAP_TOKEN") {
            std::env::remove_var("VAULT_BOOTSTRAP_TOKEN");
            return Some(SecretString::from(token));
        }
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
