use anyhow::{Context, Result};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct GitHubApp {
    app_id: String,
    installation_id: String,
    private_key: EncodingKey,
    client: Client,
}

#[derive(Serialize)]
struct Claims {
    iat: u64,
    exp: u64,
    iss: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

impl GitHubApp {
    pub fn new(app_id: String, installation_id: String, private_key_pem: &str) -> Result<Self> {
        let private_key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
            .context("Failed to parse GitHub App private key")?;

        Ok(Self {
            app_id,
            installation_id,
            private_key,
            client: Client::new(),
        })
    }

    fn generate_jwt(&self) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            iat: now - 60,
            exp: now + (10 * 60),
            iss: self.app_id.clone(),
        };

        let header = Header::new(Algorithm::RS256);
        encode(&header, &claims, &self.private_key).context("Failed to encode JWT")
    }

    pub async fn get_installation_token(&self) -> Result<String> {
        let jwt = self.generate_jwt()?;

        let url = format!(
            "https://api.github.com/app/installations/{}/access_tokens",
            self.installation_id
        );

        let resp: TokenResponse = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", jwt))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "foundry-agent")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .context("Failed to request installation token")?
            .json()
            .await
            .context("Failed to parse token response")?;

        Ok(resp.token)
    }

    pub fn authenticated_clone_url(&self, clone_url: &str, token: &str) -> String {
        clone_url.replace("https://", &format!("https://x-access-token:{}@", token))
    }
}
