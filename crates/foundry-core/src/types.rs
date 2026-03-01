use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type JobId = i64;
pub type RepositoryId = i64;
pub type ClaimToken = Uuid;
pub type AgentId = String;
pub type StageId = String;

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
