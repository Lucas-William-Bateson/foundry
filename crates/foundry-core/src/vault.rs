use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zeroize::Zeroize;

#[derive(Serialize, Zeroize)]
#[zeroize(drop)]
struct AppRoleLogin {
    role_id: String,
    secret_id: String,
}

#[derive(Deserialize)]
struct AuthResponse {
    auth: AuthData,
}

#[derive(Deserialize)]
struct AuthData {
    client_token: String,
}

#[derive(Deserialize)]
struct SecretResponse {
    data: SecretDataWrapper,
}

#[derive(Deserialize)]
struct SecretDataWrapper {
    data: HashMap<String, String>,
}

pub struct VaultClient {
    addr: String,
    role_id: String,
    http: Client,
    token: Option<SecretString>,
}

impl Clone for VaultClient {
    fn clone(&self) -> Self {
        Self {
            addr: self.addr.clone(),
            role_id: self.role_id.clone(),
            http: self.http.clone(),
            token: self
                .token
                .as_ref()
                .map(|t| SecretString::from(t.expose_secret().to_string())),
        }
    }
}

impl VaultClient {
    pub fn from_env() -> Result<Option<Self>> {
        let addr = match std::env::var("VAULT_ADDR") {
            Ok(a) => a.trim_end_matches('/').to_string(),
            Err(_) => return Ok(None),
        };

        let role_id = std::env::var("VAULT_ROLE_ID").unwrap_or_default();

        let token: Option<SecretString> =
            std::env::var("VAULT_TOKEN").ok().map(SecretString::from);

        if role_id.is_empty() && token.is_none() {
            warn!("VAULT_ADDR is set but neither VAULT_ROLE_ID nor VAULT_TOKEN provided — Vault disabled");
            return Ok(None);
        }

        Ok(Some(Self {
            addr,
            role_id,
            http: Client::new(),
            token,
        }))
    }

    pub fn new(addr: &str, role_id: &str) -> Self {
        Self {
            addr: addr.trim_end_matches('/').to_string(),
            role_id: role_id.to_string(),
            http: Client::new(),
            token: None,
        }
    }

    pub async fn login(&self, secret_id: &SecretString) -> Result<SecretString> {
        debug!("Authenticating with Vault AppRole");

        let login = AppRoleLogin {
            role_id: self.role_id.clone(),
            secret_id: secret_id.expose_secret().to_string(),
        };

        let res = self
            .http
            .post(format!("{}/v1/auth/approle/login", self.addr))
            .json(&login)
            .send()
            .await
            .context("Failed to connect to Vault for AppRole login")?;

        if !res.status().is_success() {
            let status = res.status();
            anyhow::bail!("Vault AppRole login failed ({})", status);
        }

        let auth: AuthResponse = res
            .json()
            .await
            .context("Failed to parse Vault login response")?;

        let token = SecretString::from(auth.auth.client_token);

        info!("Vault AppRole login successful");
        Ok(token)
    }

    pub async fn generate_secret_id(
        &self,
        bootstrap_token: &SecretString,
    ) -> Result<SecretString> {
        debug!("Generating fresh Vault secret_id");

        let res = self
            .http
            .post(format!(
                "{}/v1/auth/approle/role/ci-role/secret-id",
                self.addr
            ))
            .header("X-Vault-Token", bootstrap_token.expose_secret())
            .send()
            .await
            .context("Failed to generate Vault secret_id")?;

        if !res.status().is_success() {
            let status = res.status();
            anyhow::bail!("Failed to generate Vault secret_id ({})", status);
        }

        #[derive(Deserialize)]
        struct SecretIdResponse {
            data: SecretIdData,
        }
        #[derive(Deserialize)]
        struct SecretIdData {
            secret_id: String,
        }

        let resp: SecretIdResponse = res
            .json()
            .await
            .context("Failed to parse secret_id response")?;

        let secret_id = SecretString::from(resp.data.secret_id);
        debug!("Generated fresh secret_id");
        Ok(secret_id)
    }

    pub async fn get_secrets(
        &self,
        path: &str,
        token: Option<&SecretString>,
    ) -> Result<HashMap<String, SecretString>> {
        let vault_token: SecretString = match token {
            Some(t) => SecretString::from(t.expose_secret().to_string()),
            None => self
                .token
                .as_ref()
                .map(|t| SecretString::from(t.expose_secret().to_string()))
                .context("No token available — call login() first or set VAULT_TOKEN")?,
        };

        let url = format!("{}/v1/secret/data/{}", self.addr, path);
        debug!("Reading Vault secret at: {}", url);

        let res = self
            .http
            .get(&url)
            .header("X-Vault-Token", vault_token.expose_secret())
            .send()
            .await
            .context("Failed to connect to Vault for secret read")?;

        if res.status().as_u16() == 404 {
            debug!("No secret found at path: {}", path);
            return Ok(HashMap::new());
        }

        if !res.status().is_success() {
            let status = res.status();
            anyhow::bail!("Vault secret read failed ({})", status);
        }

        let secret: SecretResponse = res
            .json()
            .await
            .context("Failed to parse Vault secret response")?;

        let wrapped: HashMap<String, SecretString> = secret
            .data
            .data
            .into_iter()
            .map(|(k, v)| (k, SecretString::from(v)))
            .collect();

        info!(
            "Read {} secret(s) from Vault path: {}",
            wrapped.len(),
            path
        );
        Ok(wrapped)
    }

    pub async fn fetch_ci_secrets(
        &self,
        bootstrap_token: &SecretString,
        secret_path: &str,
    ) -> Result<HashMap<String, SecretString>> {
        let secret_id = self.generate_secret_id(bootstrap_token).await?;
        let token = self.login(&secret_id).await?;
        self.get_secrets(secret_path, Some(&token)).await
    }

    pub async fn health_check(&self) -> Result<bool> {
        let res = self
            .http
            .get(format!("{}/v1/sys/health", self.addr))
            .send()
            .await;

        match res {
            Ok(r) => Ok(r.status().as_u16() == 200),
            Err(e) => {
                debug!("Vault health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_trailing_slash_stripped() {
        let client = VaultClient::new("http://localhost:8200/", "test-role");
        assert_eq!(client.addr, "http://localhost:8200");
    }

    #[test]
    fn test_from_env_returns_none_without_addr() {
        // SAFETY: This test must not run in parallel with other tests that
        // read these env vars. `cargo test` runs tests in the same process,
        // so env mutation is inherently racy. We accept this for a unit test
        // that verifies the "no config" path.
        unsafe {
            std::env::remove_var("VAULT_ADDR");
            std::env::remove_var("VAULT_ROLE_ID");
            std::env::remove_var("VAULT_TOKEN");
        }

        let result = VaultClient::from_env().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_secret_string_not_in_debug() {
        let secret = SecretString::from("super-secret-token".to_string());
        let debug_output = format!("{:?}", secret);
        assert!(
            !debug_output.contains("super-secret-token"),
            "SecretString must not expose value in Debug"
        );
    }
}
