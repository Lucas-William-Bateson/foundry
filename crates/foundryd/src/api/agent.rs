use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info, warn};

use foundry_core::{ApiResponse, ClaimRequest, ClaimResponse, FinishRequest, LogRequest, SyncScheduleRequest, SyncTriggersRequest, RegisterRequest, RegisterResponse, HeartbeatRequest};

use crate::{infrastructure::db, domain::scheduler, AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agent/claim", post(claim_job))
        .route("/agent/log", post(append_log))
        .route("/agent/finish", post(report_result))
        .route("/agent/logs/{job_id}", get(get_logs))
        .route("/agent/metrics", post(report_metrics))
        .route("/agent/schedule", post(sync_schedule))
        .route("/agent/triggers", post(sync_triggers))
        .route("/agent/register", post(register_agent))
        .route("/agent/heartbeat", post(heartbeat))
}

/// Middleware that validates the agent bearer token against FOUNDRY_AGENT_SECRET.
/// If no secret is configured, all requests are allowed (backward-compatible).
pub async fn require_agent_auth(
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    if let Some(ref expected) = state.config.agent_secret {
        let auth_header = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok());

        let provided = auth_header.and_then(|h| h.strip_prefix("Bearer "));

        match provided {
            Some(token) if token == expected => {}
            Some(_) => {
                warn!("Agent request with invalid bearer token");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(ApiResponse::error("Invalid agent token")),
                )
                    .into_response();
            }
            None => {
                warn!("Agent request missing Authorization header");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(ApiResponse::error("Agent authentication required")),
                )
                    .into_response();
            }
        }
    }

    next.run(request).await
}

async fn claim_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClaimRequest>,
) -> impl IntoResponse {
    match db::claim_job(&state.db, &req.agent_id, req.runner_id).await {
        Ok(Some(job)) => {
            info!("Agent {} claimed job {}", req.agent_id, job.id);
            (StatusCode::OK, Json(ClaimResponse::Claimed { job }))
        }
        Ok(None) => (StatusCode::OK, Json(ClaimResponse::Empty)),
        Err(e) => {
            error!("Failed to claim job: {}", e);
            (StatusCode::OK, Json(ClaimResponse::Empty))
        }
    }
}

async fn append_log(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LogRequest>,
) -> impl IntoResponse {
    match db::append_log(&state.db, req.job_id, req.claim_token, &req.line).await {
        Ok(true) => (StatusCode::OK, Json(ApiResponse::ok())),
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Invalid job or token")),
        ),
        Err(e) => {
            error!("Failed to append log: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error("Database error")),
            )
        }
    }
}

async fn report_result(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FinishRequest>,
) -> impl IntoResponse {
    let status_str = if req.success { "success" } else { "failed" };

    match db::report_result(&state.db, req.job_id, req.claim_token, req.success).await {
        Ok(true) => {
            info!("Job {} finished with status: {}", req.job_id, status_str);
            (StatusCode::OK, Json(ApiResponse::ok()))
        }
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Invalid job or token")),
        ),
        Err(e) => {
            error!("Failed to finish job: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error("Database error")),
            )
        }
    }
}

#[derive(Deserialize)]
struct GetLogsQuery {
    claim_token: uuid::Uuid,
}

async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<i64>,
    Query(query): Query<GetLogsQuery>,
) -> impl IntoResponse {
    match db::get_logs(&state.db, job_id, query.claim_token).await {
        Ok(Some(logs)) => (StatusCode::OK, logs),
        Ok(None) => (StatusCode::FORBIDDEN, "Invalid job or token".to_string()),
        Err(e) => {
            error!("Failed to get logs: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
        }
    }
}

#[derive(Deserialize)]
struct MetricsRequest {
    job_id: i64,
    claim_token: uuid::Uuid,
    metrics: serde_json::Value,
}

async fn report_metrics(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MetricsRequest>,
) -> impl IntoResponse {
    match db::store_metrics(&state.db, req.job_id, req.claim_token, &req.metrics).await {
        Ok(true) => (StatusCode::OK, Json(ApiResponse::ok())),
        Ok(false) => (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Invalid job or token")),
        ),
        Err(e) => {
            error!("Failed to store metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error("Database error")),
            )
        }
    }
}

async fn sync_schedule(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyncScheduleRequest>,
) -> impl IntoResponse {
    // Verify the claim token belongs to a running job for this repo
    let valid = db::verify_job_token(&state.db, req.repo_id, req.claim_token).await;
    
    if !matches!(valid, Ok(true)) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Invalid repo or token")),
        );
    }
    
    if !req.enabled || req.cron.is_none() {
        // Delete existing schedule
        match scheduler::delete_schedule(&state.db, req.repo_id, req.branch.as_deref()).await {
            Ok(_) => {
                info!("Deleted schedule for repo {}", req.repo_id);
                (StatusCode::OK, Json(ApiResponse::ok()))
            }
            Err(e) => {
                error!("Failed to delete schedule: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error("Failed to delete schedule")),
                )
            }
        }
    } else {
        // Upsert schedule
        let cron = req.cron.as_ref().unwrap();
        match scheduler::upsert_schedule(
            &state.db,
            req.repo_id,
            cron,
            req.branch.as_deref(),
            req.timezone.as_deref(),
        ).await {
            Ok(id) => {
                info!("Upserted schedule {} for repo {} with cron: {}", id, req.repo_id, cron);
                (StatusCode::OK, Json(ApiResponse::ok()))
            }
            Err(e) => {
                error!("Failed to upsert schedule: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::error(&format!("Failed to create schedule: {}", e))),
                )
            }
        }
    }
}

async fn sync_triggers(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SyncTriggersRequest>,
) -> impl IntoResponse {
    // Verify the claim token belongs to a running job for this repo
    let valid = db::verify_job_token(&state.db, req.repo_id, req.claim_token).await;
    
    if !matches!(valid, Ok(true)) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Invalid repo or token")),
        );
    }
    
    match db::sync_repo_triggers(
        &state.db,
        req.repo_id,
        &req.branches,
        req.pull_requests,
        req.pr_target_branches.as_deref(),
        None,
    ).await {
        Ok(()) => {
            info!(
                "Synced triggers for repo {}: branches={:?}, PRs={}",
                req.repo_id, req.branches, req.pull_requests
            );
            (StatusCode::OK, Json(ApiResponse::ok()))
        }
        Err(e) => {
            error!("Failed to sync triggers: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(&format!("Failed to sync triggers: {}", e))),
            )
        }
    }
}

async fn register_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    match db::register_runner(
        &state.db,
        &req.name,
        &req.tags,
        req.cpu,
        req.memory_mb,
        req.gpu,
        &req.arch,
    )
    .await
    {
        Ok(runner_id) => {
            info!("Runner '{}' registered with id {}", req.name, runner_id);
            (StatusCode::OK, Json(serde_json::json!(RegisterResponse { runner_id })))
        }
        Err(e) => {
            error!("Failed to register runner: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!(ApiResponse::error(&format!("Failed to register: {}", e)))),
            )
        }
    }
}

async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    match db::heartbeat_runner(&state.db, req.runner_id).await {
        Ok(true) => (StatusCode::OK, Json(ApiResponse::ok())),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Unknown runner")),
        ),
        Err(e) => {
            error!("Failed to update heartbeat: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error("Database error")),
            )
        }
    }
}
