use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type JobId = i64;
pub type RepositoryId = i64;
pub type ClaimToken = Uuid;
pub type AgentId = String;
pub type StageId = String;

/// Runner requirements stored as JSONB on jobs.
/// Describes what capabilities a runner must have to execute a job.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunnerRequirements {
    /// Named runner reference (e.g., "fast" from `runner.fast`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_name: Option<String>,
    /// Required tags (all must match)
    #[serde(default)]
    pub required_tags: Vec<String>,
    /// Minimum CPU cores
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_cpu: Option<u32>,
    /// Minimum memory in MB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_memory_mb: Option<u32>,
    /// Minimum GPUs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_gpu: Option<u32>,
    /// Required architecture
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimedJob {
    pub id: JobId,
    pub repo_id: RepositoryId,
    pub repo_owner: String,
    pub repo_name: String,
    pub clone_url: String,
    pub git_sha: String,
    pub git_ref: String,
    pub image: String,
    pub claim_token: ClaimToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub agent_id: AgentId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub tags: Vec<String>,
    pub cpu: Option<i32>,
    pub memory_mb: Option<i32>,
    pub gpu: i32,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub runner_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub runner_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ClaimResponse {
    #[serde(rename = "claimed")]
    Claimed { job: ClaimedJob },
    #[serde(rename = "empty")]
    Empty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRequest {
    pub job_id: JobId,
    pub claim_token: ClaimToken,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishRequest {
    pub job_id: JobId,
    pub claim_token: ClaimToken,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncScheduleRequest {
    pub repo_id: RepositoryId,
    pub claim_token: ClaimToken,
    pub cron: Option<String>,
    pub branch: Option<String>,
    pub timezone: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTriggersRequest {
    pub repo_id: RepositoryId,
    pub claim_token: ClaimToken,
    pub branches: Vec<String>,
    pub pull_requests: bool,
    pub pr_target_branches: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ApiResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
        }
    }
}
