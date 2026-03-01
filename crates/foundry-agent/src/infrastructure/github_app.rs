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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitStatus {
    Pending,
    Success,
    Failure,
    Error,
}

impl CommitStatus {
    fn as_str(&self) -> &'static str {
        match self {
            CommitStatus::Pending => "pending",
            CommitStatus::Success => "success",
            CommitStatus::Failure => "failure",
            CommitStatus::Error => "error",
        }
    }
}

#[derive(Serialize)]
struct CreateStatusRequest<'a> {
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    context: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Queued,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckConclusion {
    Success,
    Failure,
    Cancelled,
    TimedOut,
}

impl CheckConclusion {
    fn as_str(&self) -> &'static str {
        match self {
            CheckConclusion::Success => "success",
            CheckConclusion::Failure => "failure",
            CheckConclusion::Cancelled => "cancelled",
            CheckConclusion::TimedOut => "timed_out",
        }
    }
}

#[derive(Serialize)]
struct CheckRunOutput<'a> {
    title: &'a str,
    summary: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
}

#[derive(Serialize)]
struct CreateCheckRunRequest<'a> {
    name: &'a str,
    head_sha: &'a str,
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conclusion: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<CheckRunOutput<'a>>,
}

#[derive(Serialize)]
struct UpdateCheckRunRequest<'a> {
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conclusion: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<CheckRunOutput<'a>>,
}

#[derive(Deserialize)]
pub struct CheckRun {
    pub id: i64,
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

    pub async fn create_commit_status(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        status: CommitStatus,
        description: Option<&str>,
        target_url: Option<&str>,
    ) -> Result<()> {
        let token = self.get_installation_token().await?;

        let url = format!(
            "https://api.github.com/repos/{}/{}/statuses/{}",
            owner, repo, sha
        );

        let body = CreateStatusRequest {
            state: status.as_str(),
            target_url,
            description,
            context: "foundry",
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "foundry-agent")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&body)
            .send()
            .await
            .context("Failed to create commit status")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {}: {}", status, body);
        }

        Ok(())
    }

    pub async fn create_check_run(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        name: &str,
    ) -> Result<i64> {
        let token = self.get_installation_token().await?;

        let url = format!(
            "https://api.github.com/repos/{}/{}/check-runs",
            owner, repo
        );

        let body = CreateCheckRunRequest {
            name,
            head_sha: sha,
            status: "in_progress",
            conclusion: None,
            output: Some(CheckRunOutput {
                title: "Build in progress",
                summary: "Foundry is building your project...",
                text: None,
            }),
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "foundry-agent")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&body)
            .send()
            .await
            .context("Failed to create check run")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {}: {}", status, body);
        }

        let check_run: CheckRun = resp.json().await.context("Failed to parse check run response")?;
        Ok(check_run.id)
    }

    pub async fn complete_check_run(
        &self,
        owner: &str,
        repo: &str,
        check_run_id: i64,
        conclusion: CheckConclusion,
        summary: &str,
        logs: Option<&str>,
    ) -> Result<()> {
        let token = self.get_installation_token().await?;

        let url = format!(
            "https://api.github.com/repos/{}/{}/check-runs/{}",
            owner, repo, check_run_id
        );

        let title = match conclusion {
            CheckConclusion::Success => "Build succeeded",
            CheckConclusion::Failure => "Build failed",
            CheckConclusion::Cancelled => "Build cancelled",
            CheckConclusion::TimedOut => "Build timed out",
        };

        let truncated_logs = logs.map(|l| {
            if l.len() > 60000 {
                &l[l.len() - 60000..]
            } else {
                l
            }
        });

        let body = UpdateCheckRunRequest {
            status: "completed",
            conclusion: Some(conclusion.as_str()),
            output: Some(CheckRunOutput {
                title,
                summary,
                text: truncated_logs,
            }),
        };

        let resp = self
            .client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "foundry-agent")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&body)
            .send()
            .await
            .context("Failed to update check run")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {}: {}", status, body);
        }

        Ok(())
    }
}
