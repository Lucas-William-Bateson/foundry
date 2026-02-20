use anyhow::{Context, Result};
use std::fmt;

#[derive(Clone)]
pub struct Config {
    pub bind_addr: String,
    pub bind_port: u16,
    pub database_url: String,
    pub github_webhook_secret: String,
    pub tunnel: Option<TunnelConfig>,
    pub auth: Option<AuthConfig>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("bind_port", &self.bind_port)
            .field("database_url", &"[REDACTED]")
            .field("github_webhook_secret", &"[REDACTED]")
            .field("tunnel", &self.tunnel)
            .field("auth", &self.auth)
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

        Ok(Self {
            bind_addr,
            bind_port,
            database_url: std::env::var("DATABASE_URL")
                .context("DATABASE_URL must be set")?,
            github_webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET")
                .context("GITHUB_WEBHOOK_SECRET must be set")?,
            tunnel,
            auth,
        })
    }
}
