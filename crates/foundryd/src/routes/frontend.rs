use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{self, DashboardStats, JobDetail, JobSummary, RepoSummary};
use crate::AppState;

const BASE_TEMPLATE: &str = include_str!("../../templates/base.html");
const INDEX_TEMPLATE: &str = include_str!("../../templates/index.html");
const JOB_TEMPLATE: &str = include_str!("../../templates/job.html");
const REPOS_TEMPLATE: &str = include_str!("../../templates/repos.html");

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        // HTML pages
        .route("/", get(index))
        .route("/job/{id}", get(job_detail))
        .route("/repos", get(repos))
        // JSON API
        .route("/api/stats", get(api_stats))
        .route("/api/jobs", get(api_jobs))
        .route("/api/job/{id}", get(api_job))
        .route("/api/repos", get(api_repos))
}

fn render_page(title: &str, content: &str, active_nav: &str) -> Html<String> {
    let html = BASE_TEMPLATE
        .replace("{{TITLE}}", title)
        .replace("{{NAV_DASHBOARD}}", if active_nav == "dashboard" { "active" } else { "" })
        .replace("{{NAV_REPOS}}", if active_nav == "repos" { "active" } else { "" })
        .replace("{{CONTENT}}", content);
    Html(html)
}

async fn index(State(_state): State<Arc<AppState>>) -> Html<String> {
    render_page("Dashboard", INDEX_TEMPLATE, "dashboard")
}

async fn job_detail(Path(id): Path<i64>) -> Html<String> {
    let content = JOB_TEMPLATE.replace("{{JOB_ID}}", &id.to_string());
    render_page(&format!("Build #{}", id), &content, "")
}

async fn repos() -> Html<String> {
    render_page("Repositories", REPOS_TEMPLATE, "repos")
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

