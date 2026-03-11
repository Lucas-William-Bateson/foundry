//! Direct database logger — replaces the agent's HTTP-based ServerClient for logging.
//!
//! The built-in worker uses this to write logs and metrics directly to SQLite
//! instead of going through the HTTP API.

use anyhow::Result;
use sqlx::SqlitePool;
use tracing::debug;
use uuid::Uuid;

use foundry_core::ClaimedJob;

/// A logger that writes job logs directly to the database.
#[derive(Clone)]
pub struct DbLogger {
    pool: SqlitePool,
}

impl DbLogger {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn log(&self, job: &ClaimedJob, line: &str) -> Result<()> {
        debug!("[job {}] {}", job.id, line);
        self.log_raw(job.id, &job.claim_token, line).await
    }

    pub async fn log_raw(&self, job_id: i64, claim_token: &Uuid, line: &str) -> Result<()> {
        crate::infrastructure::db::logs::append_log(&self.pool, job_id, *claim_token, line).await?;
        Ok(())
    }

    pub async fn report_metrics(&self, job: &ClaimedJob, metrics: &crate::runtime::execution::JobMetrics) -> Result<()> {
        let metrics_value = serde_json::to_value(metrics).unwrap_or_default();
        crate::infrastructure::db::jobs::store_metrics(&self.pool, job.id, job.claim_token, &metrics_value).await?;
        Ok(())
    }

    pub async fn sync_schedule(
        &self,
        job: &ClaimedJob,
        schedule: Option<&foundry_core::ScheduleConfig>,
    ) -> Result<()> {
        if let Some(sched) = schedule {
            if sched.enabled {
                crate::domain::scheduler::upsert_schedule(
                    &self.pool,
                    job.repo_id,
                    &sched.cron,
                    sched.branch.as_deref(),
                    sched.timezone.as_deref(),
                ).await?;
            } else {
                crate::domain::scheduler::delete_schedule(
                    &self.pool,
                    job.repo_id,
                    sched.branch.as_deref(),
                ).await?;
            }
        } else {
            crate::domain::scheduler::delete_schedule(&self.pool, job.repo_id, None).await?;
        }
        Ok(())
    }

    pub async fn sync_triggers(
        &self,
        job: &ClaimedJob,
        triggers: &foundry_core::config::TriggersConfig,
    ) -> Result<()> {
        crate::infrastructure::db::repos::sync_repo_triggers(
            &self.pool,
            job.repo_id,
            &triggers.branches,
            triggers.pull_requests,
            triggers.pr_target_branches.as_deref(),
            None,
        ).await
    }

    pub async fn get_logs(&self, job: &ClaimedJob) -> Result<String> {
        let logs = crate::infrastructure::db::logs::get_job_logs(&self.pool, job.id).await?;
        Ok(logs.unwrap_or_default())
    }

    pub async fn report_result(&self, job: &ClaimedJob, success: bool) -> Result<()> {
        crate::infrastructure::db::jobs::report_result(&self.pool, job.id, job.claim_token, success).await?;
        Ok(())
    }
}
