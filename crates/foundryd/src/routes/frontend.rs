use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info};

use crate::db::{self, DashboardStats, JobDetail, JobSummary, RepoSummary};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    // Look for frontend dist in multiple locations
    let static_dir = if std::path::Path::new("frontend/dist").exists() {
        std::path::Path::new("frontend/dist")
    } else if std::path::Path::new("/app/frontend/dist").exists() {
        std::path::Path::new("/app/frontend/dist")
    } else {
        // Fallback - will fail gracefully
        std::path::Path::new("frontend/dist")
    };

    tracing::info!("Serving frontend from: {:?}", static_dir);

    Router::new()
        // JSON API routes first (more specific)
        .route("/api/stats", get(api_stats))
        .route("/api/jobs", get(api_jobs))
        .route("/api/job/{id}", get(api_job))
        .route("/api/job/{id}/rerun", post(api_rerun_job))
        .route("/api/repos", get(api_repos))
        // Serve static files, fall back to index.html for SPA routing
        .nest_service("/assets", ServeDir::new(static_dir.join("assets")))
        .fallback_service(ServeFile::new(static_dir.join("index.html")))
}

// API Endpoints

#[derive(Deserialize)]
struct JobsQuery {
    limit: Option<i32>,
}

async fn api_stats(State(state): State<Arc<AppState>>) -> Json<DashboardStats> {
    let stats = db::get_dashboard_stats(&state.db).await.unwrap_or_default();
    Json(stats)
}

async fn api_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<JobsQuery>,
) -> Json<Vec<JobSummary>> {
    let limit = query.limit.unwrap_or(50) as i64;
    let jobs = db::list_jobs(&state.db, limit).await.unwrap_or_default();
    Json(jobs)
}

#[derive(Serialize)]
struct JobWithLogs {
    #[serde(flatten)]
    job: JobDetail,
    logs: Vec<LogEntry>,
}

#[derive(Serialize)]
struct LogEntry {
    timestamp: String,
    message: String,
    level: String,
}

async fn api_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Json<Option<JobWithLogs>> {
    let job = match db::get_job(&state.db, id).await {
        Ok(Some(job)) => job,
        _ => return Json(None),
    };

    let logs_text = db::get_job_logs(&state.db, id)
        .await
        .unwrap_or_default()
        .unwrap_or_default();
    
    // Parse logs - each line is a log entry
    let logs: Vec<LogEntry> = logs_text
        .lines()
        .map(|line| {
            // Try to extract timestamp if present (format: [timestamp] message)
            let (timestamp, message) = if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    (line[1..end].to_string(), line[end+1..].trim().to_string())
                } else {
                    (chrono::Utc::now().to_rfc3339(), line.to_string())
                }
            } else {
                (chrono::Utc::now().to_rfc3339(), line.to_string())
            };
            
            let level = if message.to_lowercase().contains("error") {
                "error"
            } else if message.to_lowercase().contains("warning") || message.to_lowercase().contains("warn") {
                "warning"
            } else {
                "info"
            }.to_string();
            
            LogEntry { timestamp, message, level }
        })
        .collect();

    Json(Some(JobWithLogs { job, logs }))
}

async fn api_repos(State(state): State<Arc<AppState>>) -> Json<Vec<RepoSummary>> {
    let repos = db::list_repos(&state.db).await.unwrap_or_default();
    Json(repos)
}

#[derive(Serialize)]
struct RerunResponse {
    ok: bool,
    job_id: Option<i64>,
    error: Option<String>,
}

async fn api_rerun_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match db::rerun_job(&state.db, id).await {
        Ok(Some(new_job_id)) => {
            info!("Rerun job {} created as job {}", id, new_job_id);
            (
                StatusCode::OK,
                Json(RerunResponse {
                    ok: true,
                    job_id: Some(new_job_id),
                    error: None,
                }),
            )
        }
        Ok(None) => {
            (
                StatusCode::NOT_FOUND,
                Json(RerunResponse {
                    ok: false,
                    job_id: None,
                    error: Some("Job not found".to_string()),
                }),
            )
        }
        Err(e) => {
            error!("Failed to rerun job {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RerunResponse {
                    ok: false,
                    job_id: None,
                    error: Some("Failed to rerun job".to_string()),
                }),
            )
        }
    }
}

