use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

use crate::config::AuthConfig;

use super::{AuthenticatedUser, OidcProvider};

#[derive(Debug, Deserialize)]
struct WorkOsAuthResponse {
    #[allow(dead_code)]
    access_token: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
    user: WorkOsUser,
}

#[derive(Debug, Deserialize)]
struct WorkOsUser {
    id: String,
    email: String,
    first_name: Option<String>,
    last_name: Option<String>,
}

pub struct WorkOsProvider {
    client_id: String,
    client_secret: String,
    redirect_url: String,
    authorization_endpoint: String,
    token_endpoint: String,
    http_client: Client,
}

impl WorkOsProvider {
    pub fn new(config: &AuthConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        info!("WorkOS auth initialised (client_id={})", config.client_id);

        Ok(Self {
            client_id: config.client_id.clone(),
            client_secret: config.client_secret.clone(),
            redirect_url: config.redirect_url.clone(),
            authorization_endpoint: "https://api.workos.com/user_management/authorize".to_string(),
            token_endpoint: "https://api.workos.com/user_management/authenticate".to_string(),
            http_client,
        })
    }
}

#[async_trait]
impl OidcProvider for WorkOsProvider {
    fn auth_url(&self, state: &str) -> Result<String> {
        Ok(format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&provider=authkit",
            self.authorization_endpoint,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(&self.redirect_url),
            urlencoding::encode(state),
        ))
    }

    async fn exchange_code(&self, code: &str) -> Result<AuthenticatedUser> {
        let body = serde_json::json!({
            "client_id": self.client_id,
            "client_secret": self.client_secret,
            "code": code,
            "grant_type": "authorization_code"
        });

        let response = self
            .http_client
            .post(&self.token_endpoint)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Token exchange failed: {}", error_text));
        }

        let workos_response: WorkOsAuthResponse = response.json().await?;

        let name = workos_response
            .user
            .first_name
            .map(|f| {
                format!(
                    "{} {}",
                    f,
                    workos_response.user.last_name.unwrap_or_default()
                )
            })
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty());

        Ok(AuthenticatedUser {
            id: workos_response.user.id,
            email: workos_response.user.email,
            name,
        })
    }
}
