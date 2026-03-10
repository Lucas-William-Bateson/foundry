use anyhow::{Context, Result};
use reqwest::{Client, RequestBuilder};
use tracing::debug;

use foundry_core::{
    ApiResponse, ClaimRequest, ClaimResponse, ClaimedJob, FinishRequest, LogRequest,
    SyncScheduleRequest, SyncTriggersRequest, RegisterRequest, RegisterResponse, HeartbeatRequest,
};

use crate::types::config::Config;

#[derive(Clone)]
pub struct ServerClient {
    client: Client,
    server_url: String,
    agent_id: String,
    agent_secret: Option<String>,
    runner_id: std::sync::Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
}

impl ServerClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            server_url: config.server_url.clone(),
            agent_id: config.agent_id.clone(),
            agent_secret: config.agent_secret.clone(),
            runner_id: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    fn auth(&self, req: RequestBuilder) -> RequestBuilder {
        if let Some(ref secret) = self.agent_secret {
            req.bearer_auth(secret)
        } else {
            req
        }
    }

    pub async fn claim_job(&self) -> Result<Option<ClaimedJob>> {
        let url = format!("{}/agent/claim", self.server_url);
        let runner_id = *self.runner_id.lock().await;
        let req = ClaimRequest {
            agent_id: self.agent_id.clone(),
            runner_id,
        };

        let response = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await
            .context("Failed to connect to server")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read claim response body")?;

        let resp: ClaimResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse claim response (HTTP {}): {}",
                status,
                &body[..body.len().min(500)]
            )
        })?;

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
            .auth(self.client.post(&url).json(&req))
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
            .auth(self.client.post(&url).json(&req))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Server rejected log: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn report_result(&self, job: &ClaimedJob, success: bool) -> Result<()> {
        let url = format!("{}/agent/finish", self.server_url);
        let req = FinishRequest {
            job_id: job.id,
            claim_token: job.claim_token,
            success,
        };

        let resp: ApiResponse = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Server rejected finish: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn report_metrics(&self, job: &ClaimedJob, metrics: &crate::runtime::execution::JobMetrics) -> Result<()> {
        let url = format!("{}/agent/metrics", self.server_url);
        
        #[derive(serde::Serialize)]
        struct MetricsRequest {
            job_id: i64,
            claim_token: uuid::Uuid,
            metrics: serde_json::Value,
        }
        
        let req = MetricsRequest {
            job_id: job.id,
            claim_token: job.claim_token,
            metrics: serde_json::to_value(metrics).unwrap_or_default(),
        };

        let resp = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await;

        if let Err(e) = resp {
            debug!("Failed to report metrics: {}", e);
        }

        Ok(())
    }

    pub async fn get_logs(&self, job: &ClaimedJob) -> Result<String> {
        let url = format!("{}/agent/logs/{}", self.server_url, job.id);

        let resp = self
            .auth(
                self.client
                    .get(&url)
                    .query(&[("claim_token", job.claim_token.to_string())]),
            )
            .send()
            .await
            .context("Failed to fetch logs")?;

        if !resp.status().is_success() {
            anyhow::bail!("Server returned error: {}", resp.status());
        }

        resp.text().await.context("Failed to read logs response")
    }

    pub async fn sync_schedule(
        &self,
        job: &ClaimedJob,
        schedule: Option<&foundry_core::ScheduleConfig>,
    ) -> Result<()> {
        let url = format!("{}/agent/schedule", self.server_url);
        
        let req = SyncScheduleRequest {
            repo_id: job.repo_id,
            claim_token: job.claim_token,
            cron: schedule.map(|s| s.cron.clone()),
            branch: schedule.and_then(|s| s.branch.clone()),
            timezone: schedule.and_then(|s| s.timezone.clone()),
            enabled: schedule.map(|s| s.enabled).unwrap_or(false),
        };

        let resp: ApiResponse = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Failed to sync schedule: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn sync_triggers(
        &self,
        job: &ClaimedJob,
        triggers: &foundry_core::config::TriggersConfig,
    ) -> Result<()> {
        let url = format!("{}/agent/triggers", self.server_url);

        let req = SyncTriggersRequest {
            repo_id: job.repo_id,
            claim_token: job.claim_token,
            branches: triggers.branches.clone(),
            pull_requests: triggers.pull_requests,
            pr_target_branches: triggers.pr_target_branches.clone(),
        };

        let resp: ApiResponse = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Failed to sync triggers: {:?}", resp.error);
        }

        Ok(())
    }

    pub async fn register(&self, config: &Config) -> Result<uuid::Uuid> {
        let url = format!("{}/agent/register", self.server_url);
        let req = RegisterRequest {
            name: config.effective_runner_name(),
            tags: config.runner_tags.clone(),
            cpu: config.runner_cpu,
            memory_mb: config.runner_mem_mb,
            gpu: config.runner_gpu,
            arch: config.runner_arch.clone(),
        };

        let response = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await
            .context("Failed to register with server")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read register response body")?;

        if !status.is_success() {
            let error_msg = serde_json::from_str::<ApiResponse>(&body)
                .ok()
                .and_then(|r| r.error)
                .unwrap_or_else(|| body[..body.len().min(500)].to_string());
            anyhow::bail!("Registration failed (HTTP {}): {}", status, error_msg);
        }

        let resp: RegisterResponse = serde_json::from_str(&body).with_context(|| {
            format!(
                "Failed to parse register response (HTTP {}): {}",
                status,
                &body[..body.len().min(500)]
            )
        })?;

        *self.runner_id.lock().await = Some(resp.runner_id);
        Ok(resp.runner_id)
    }

    pub async fn heartbeat(&self) -> Result<()> {
        let runner_id = *self.runner_id.lock().await;
        let Some(runner_id) = runner_id else {
            return Ok(());
        };

        let url = format!("{}/agent/heartbeat", self.server_url);
        let req = HeartbeatRequest { runner_id };

        let resp: ApiResponse = self
            .auth(self.client.post(&url).json(&req))
            .send()
            .await?
            .json()
            .await?;

        if !resp.ok {
            anyhow::bail!("Heartbeat rejected: {:?}", resp.error);
        }

        Ok(())
    }
}
