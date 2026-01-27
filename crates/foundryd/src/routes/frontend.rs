use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use crate::db::{self, DashboardStats, JobDetail, JobSummary, RepoDetail, RepoSummary, ScheduleSummary};
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
        .route("/api/repos", get(api_repos))
        .route("/api/repo/{id}", get(api_repo))
        .route("/api/repo/{id}/jobs", get(api_repo_jobs))
        .route("/api/schedules", get(api_schedules))
        .route("/api/schedule/{id}/toggle", post(api_toggle_schedule))
        .route("/api/schedule/{id}", delete(api_delete_schedule))
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

async fn api_repo(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match db::get_repo(&state.db, id).await {
        Ok(Some(repo)) => Json(serde_json::json!(repo)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Repo not found"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct RepoJobsQuery {
    limit: Option<i32>,
}

async fn api_repo_jobs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(query): Query<RepoJobsQuery>,
) -> Json<Vec<JobSummary>> {
    let limit = query.limit.unwrap_or(50) as i64;
    let jobs = db::get_repo_jobs(&state.db, id, limit).await.unwrap_or_default();
    Json(jobs)
}

async fn api_schedules(State(state): State<Arc<AppState>>) -> Json<Vec<ScheduleSummary>> {
    let schedules = db::list_schedules(&state.db).await.unwrap_or_default();
    Json(schedules)
}

#[derive(Deserialize)]
struct ToggleScheduleRequest {
    enabled: bool,
}

async fn api_toggle_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<ToggleScheduleRequest>,
) -> impl IntoResponse {
    match db::toggle_schedule(&state.db, id, req.enabled).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"ok": false, "error": "Schedule not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

async fn api_delete_schedule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match db::delete_schedule_by_id(&state.db, id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"ok": false, "error": "Schedule not found"}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}
