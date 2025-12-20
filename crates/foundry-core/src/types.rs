use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub id: i64,
    pub repo_id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub clone_url: String,
    pub git_sha: String,
    pub git_ref: String,
    pub image: String,
    pub claim_token: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub agent_id: String,
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
    pub job_id: i64,
    pub claim_token: Uuid,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinishRequest {
    pub job_id: i64,
    pub claim_token: Uuid,
    pub success: bool,
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
