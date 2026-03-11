use anyhow::{Context, Result};
use secrecy::SecretString;
use std::fmt;

#[derive(Clone)]
pub struct Config {
    pub bind_addr: String,
    pub bind_port: u16,
    pub database_url: String,
    pub github_webhook_secret: String,
    pub agent_secret: Option<String>,
    pub tunnel: Option<TunnelConfig>,
    pub auth: Option<AuthConfig>,

    // Built-in worker config
    pub workspace_dir: String,
    pub default_command: String,
    pub poll_interval_secs: u64,
    pub github_app_id: Option<String>,
    pub github_installation_id: Option<String>,
    pub github_private_key: Option<String>,
    pub self_repo: Option<String>,
    pub self_deploy_script: Option<String>,
    pub vault_addr: Option<String>,
    pub vault_role_id: Option<String>,
    pub vault_bootstrap_token: Option<SecretString>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("bind_port", &self.bind_port)
            .field("database_url", &"[REDACTED]")
            .field("github_webhook_secret", &"[REDACTED]")
            .field("agent_secret", &self.agent_secret.as_ref().map(|_| "[REDACTED]"))
            .field("tunnel", &self.tunnel)
            .field("auth", &self.auth)
            .field("workspace_dir", &self.workspace_dir)
            .field("default_command", &self.default_command)
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("github_app_id", &self.github_app_id)
            .field("github_installation_id", &self.github_installation_id)
            .field("github_private_key", &self.github_private_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Clone)]
pub struct TunnelConfig {
    pub cf_account_id: String,
    pub cf_api_token: String,
    pub cf_zone_id: String,
    pub tunnel_name: String,
    pub domain: String,
}

impl fmt::Debug for TunnelConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TunnelConfig")
            .field("cf_account_id", &"[REDACTED]")
            .field("cf_api_token", &"[REDACTED]")
            .field("cf_zone_id", &"[REDACTED]")
            .field("tunnel_name", &self.tunnel_name)
            .field("domain", &self.domain)
            .finish()
    }
}

#[derive(Clone)]
pub struct AuthConfig {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub cookie_secret: String,
    pub redirect_url: String,
    pub allowed_emails: Vec<String>,
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field("issuer_url", &self.issuer_url)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("cookie_secret", &"[REDACTED]")
            .field("redirect_url", &self.redirect_url)
            .field("allowed_emails", &self.allowed_emails)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind_addr = std::env::var("FOUNDRY_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

        let bind_port = bind_addr
            .split(':')
            .last()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        let tunnel = if std::env::var("FOUNDRY_ENABLE_TUNNEL")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
        {
            Some(TunnelConfig {
                cf_account_id: std::env::var("CF_ACCOUNT_ID")
                    .context("CF_ACCOUNT_ID required when tunnel enabled")?,
                cf_api_token: std::env::var("CF_API_TOKEN")
                    .context("CF_API_TOKEN required when tunnel enabled")?,
                cf_zone_id: std::env::var("CF_ZONE_ID")
                    .context("CF_ZONE_ID required when tunnel enabled")?,
                tunnel_name: std::env::var("CF_TUNNEL_NAME")
                    .unwrap_or_else(|_| "foundry".to_string()),
                domain: std::env::var("CF_TUNNEL_DOMAIN")
                    .context("CF_TUNNEL_DOMAIN required when tunnel enabled")?,
            })
        } else {
            None
        };

        let auth = if std::env::var("FOUNDRY_AUTH_ENABLED")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
        {
            Some(AuthConfig {
                issuer_url: std::env::var("FOUNDRY_AUTH_ISSUER_URL")
                    .context("FOUNDRY_AUTH_ISSUER_URL required when auth enabled")?,
                client_id: std::env::var("FOUNDRY_AUTH_CLIENT_ID")
                    .context("FOUNDRY_AUTH_CLIENT_ID required when auth enabled")?,
                client_secret: std::env::var("FOUNDRY_AUTH_CLIENT_SECRET")
                    .context("FOUNDRY_AUTH_CLIENT_SECRET required when auth enabled")?,
                cookie_secret: std::env::var("FOUNDRY_AUTH_COOKIE_SECRET")
                    .context("FOUNDRY_AUTH_COOKIE_SECRET required when auth enabled")?,
                redirect_url: std::env::var("FOUNDRY_AUTH_REDIRECT_URL")
                    .context("FOUNDRY_AUTH_REDIRECT_URL required when auth enabled")?,
                allowed_emails: std::env::var("FOUNDRY_AUTH_ALLOWED_EMAILS")
                    .unwrap_or_default()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            })
        } else {
            None
        };

        let github_private_key = match std::env::var("GITHUB_APP_PRIVATE_KEY_PATH") {
            Ok(path) => Some(
                std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read GitHub App private key from {}", path))?,
            ),
            Err(_) => std::env::var("GITHUB_APP_PRIVATE_KEY").ok(),
        };

        Ok(Self {
            bind_addr,
            bind_port,
            // e.g. sqlite:///absolute/path/to/foundry.db?mode=rwc
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL must be set (e.g. sqlite:///path/to/foundry.db?mode=rwc)")?,
            github_webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET")
                .context("GITHUB_WEBHOOK_SECRET must be set")?,
            agent_secret: std::env::var("FOUNDRY_AGENT_SECRET").ok(),
            tunnel,
            auth,

            // Built-in worker config
            workspace_dir: std::env::var("FOUNDRY_WORKSPACE_DIR")
                .unwrap_or_else(|_| "/tmp/foundry".to_string()),
            default_command: std::env::var("FOUNDRY_DEFAULT_COMMAND")
                .unwrap_or_else(|_| "echo 'No command configured'".to_string()),
            poll_interval_secs: std::env::var("FOUNDRY_POLL_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            github_app_id: std::env::var("GITHUB_APP_ID").ok(),
            github_installation_id: std::env::var("GITHUB_INSTALLATION_ID").ok(),
            github_private_key,
            self_repo: std::env::var("FOUNDRY_SELF_REPO").ok(),
            self_deploy_script: std::env::var("FOUNDRY_SELF_DEPLOY_SCRIPT").ok(),
            vault_addr: std::env::var("VAULT_ADDR").ok(),
            vault_role_id: std::env::var("VAULT_ROLE_ID").ok(),
            vault_bootstrap_token: Self::load_vault_bootstrap_token(),
        })
    }

    pub fn has_github_app(&self) -> bool {
        self.github_app_id.is_some()
            && self.github_installation_id.is_some()
            && self.github_private_key.is_some()
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
