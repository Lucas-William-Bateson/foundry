use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

use foundry_core::{ApiResponse, ClaimRequest, ClaimResponse, FinishRequest, LogRequest};

use crate::{db, AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agent/claim", post(claim_job))
        .route("/agent/log", post(append_log))
        .route("/agent/finish", post(finish_job))
        .route("/agent/logs/{job_id}", get(get_logs))
        .route("/agent/metrics", post(report_metrics))
}

async fn claim_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClaimRequest>,
) -> impl IntoResponse {
    match db::claim_job(&state.db, &req.agent_id).await {
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

async fn finish_job(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FinishRequest>,
) -> impl IntoResponse {
    let status_str = if req.success { "success" } else { "failed" };

    match db::finish_job(&state.db, req.job_id, req.claim_token, req.success).await {
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
