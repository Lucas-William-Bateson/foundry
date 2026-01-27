use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, sse::{Event, Sse}},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt as _;
use tower_http::services::{ServeDir, ServeFile};
use crate::db::{self, DashboardStats, JobDetail, JobSummary, RepoSummary, ScheduleSummary};
use crate::docker;
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
        // Docker management routes
        .route("/api/containers", get(api_list_containers))
        .route("/api/containers/{id}/logs", get(api_container_logs))
        .route("/api/containers/{id}/logs/stream", get(api_container_logs_stream))
        .route("/api/containers/{id}/restart", post(api_restart_container))
        .route("/api/containers/{id}/stop", post(api_stop_container))
        .route("/api/containers/{id}/start", post(api_start_container))
        .route("/api/projects", get(api_list_projects))
        .route("/api/projects/{name}/restart", post(api_restart_project))
        .route("/api/projects/{name}/stop", post(api_stop_project))
        .route("/api/projects/{name}/start", post(api_start_project))
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

// Docker Container API Endpoints

#[derive(Deserialize)]
struct ContainersQuery {
    project: Option<String>,
}

async fn api_list_containers(
    Query(query): Query<ContainersQuery>,
) -> impl IntoResponse {
    match docker::list_containers(query.project.as_deref()).await {
        Ok(containers) => (StatusCode::OK, Json(serde_json::json!(containers))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct LogsQuery {
    lines: Option<u32>,
}

async fn api_container_logs(
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    match docker::get_container_logs(&id, query.lines).await {
        Ok(logs) => (StatusCode::OK, Json(serde_json::json!(logs))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn api_container_logs_stream(
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> impl IntoResponse {
    match docker::stream_container_logs(&id, query.lines).await {
        Ok(rx) => {
            let stream = ReceiverStream::new(rx).map(|line| {
                Ok::<_, Infallible>(Event::default().data(line))
            });
            Sse::new(stream).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn api_restart_container(
    Path(id): Path<String>,
) -> impl IntoResponse {
    match docker::restart_container(&id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

async fn api_stop_container(
    Path(id): Path<String>,
) -> impl IntoResponse {
    match docker::stop_container(&id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

async fn api_start_container(
    Path(id): Path<String>,
) -> impl IntoResponse {
    match docker::start_container(&id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

// Docker Project API Endpoints

async fn api_list_projects() -> impl IntoResponse {
    match docker::list_projects().await {
        Ok(projects) => (StatusCode::OK, Json(serde_json::json!(projects))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

async fn api_restart_project(
    Path(name): Path<String>,
) -> impl IntoResponse {
    match docker::restart_project(&name).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

async fn api_stop_project(
    Path(name): Path<String>,
) -> impl IntoResponse {
    match docker::stop_project(&name).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}

async fn api_start_project(
    Path(name): Path<String>,
) -> impl IntoResponse {
    match docker::start_project(&name).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"ok": false, "error": e.to_string()}))),
    }
}
