use anyhow::{Context, Result};
use reqwest::Client;
use tracing::debug;

use foundry_core::{
    ApiResponse, ClaimRequest, ClaimResponse, ClaimedJob, FinishRequest, LogRequest,
};

use crate::config::Config;

#[derive(Clone)]
pub struct ServerClient {
    client: Client,
    server_url: String,
    agent_id: String,
}

impl ServerClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            server_url: config.server_url.clone(),
            agent_id: config.agent_id.clone(),
        }
    }

    pub async fn claim_job(&self) -> Result<Option<ClaimedJob>> {
        let url = format!("{}/agent/claim", self.server_url);
        let req = ClaimRequest {
            agent_id: self.agent_id.clone(),
        };

        let resp: ClaimResponse = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("Failed to connect to server")?
            .json()
            .await
            .context("Failed to parse claim response")?;

        match resp {
            ClaimResponse::Claimed { job } => Ok(Some(job)),
            ClaimResponse::Empty => Ok(None),
        }
    }

    pub async fn log(&self, job: &ClaimedJob, line: &str) -> Result<()> {
        let url = format!("{}/agent/log", self.server_url);
        let req = LogRequest {
            job_id: job.id,
            claim_token: job.claim_token,
            line: line.to_string(),
        };

        debug!("[job {}] {}", job.id, line);

        let resp: ApiResponse = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Server rejected log: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn log_raw(&self, job_id: i64, claim_token: &uuid::Uuid, line: &str) -> Result<()> {
        let url = format!("{}/agent/log", self.server_url);
        let req = LogRequest {
            job_id,
            claim_token: *claim_token,
            line: line.to_string(),
        };

        debug!("[job {}] {}", job_id, line);

        let resp: ApiResponse = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Server rejected log: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn finish(&self, job: &ClaimedJob, success: bool) -> Result<()> {
        let url = format!("{}/agent/finish", self.server_url);
        let req = FinishRequest {
            job_id: job.id,
            claim_token: job.claim_token,
            success,
        };

        let resp: ApiResponse = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Server rejected finish: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn get_logs(&self, job: &ClaimedJob) -> Result<String> {
        let url = format!("{}/agent/logs/{}", self.server_url, job.id);

        let resp = self
            .client
            .get(&url)
            .query(&[("claim_token", job.claim_token.to_string())])
            .send()
            .await
            .context("Failed to fetch logs")?;

        if !resp.status().is_success() {
            anyhow::bail!("Server returned error: {}", resp.status());
        }

        resp.text().await.context("Failed to read logs response")
    }
}
