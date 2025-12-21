use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use foundry_core::{github::PushEvent, verify_github_signature, ApiResponse};

use crate::{db::{self, PushEventData, RepoData}, AppState};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/webhook/github", post(github_webhook))
}

async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let signature = match headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
    {
        Some(sig) => sig,
        None => {
            warn!("Webhook request missing signature header");
            return (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse::error("Missing signature")),
            );
        }
    };

    if !verify_github_signature(&state.config.github_webhook_secret, &body, signature) {
        warn!("Webhook signature verification failed");
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::error("Invalid signature")),
        );
    }

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    
    let delivery_id = headers
        .get("x-github-delivery")
        .and_then(|v| v.to_str().ok());

    info!("Received GitHub webhook: {} (delivery: {:?})", event_type, delivery_id);

    // Store all webhook events for debugging/replay (do this early)
    if let Err(e) = db::store_webhook_event(&state.db, event_type, delivery_id, &body, None).await {
        warn!("Failed to store webhook event: {}", e);
    }

    if event_type != "push" {
        info!("Ignoring non-push event: {}", event_type);
        return (StatusCode::OK, Json(ApiResponse::ok()));
    }

    let push: PushEvent = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to parse push event: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error("Invalid payload")),
            );
        }
    };

    // Skip deleted branches
    if push.deleted {
        info!("Ignoring branch deletion event");
        return (StatusCode::OK, Json(ApiResponse::ok()));
    }

    let ref_name = push.git_ref.strip_prefix("refs/heads/").unwrap_or(&push.git_ref);
    if ref_name != "main" && ref_name != "master" {
        info!("Ignoring push to non-default branch: {}", ref_name);
        return (StatusCode::OK, Json(ApiResponse::ok()));
    }

    // Extract comprehensive data from push event
    let repo_data = RepoData::from_push_event(&push);
    let push_data = PushEventData::from_push_event(&push);

    let repo = &push.repository;
    match db::upsert_repo(&state.db, &repo_data).await {
        Ok(repo_id) => {
            match db::enqueue_job(&state.db, repo_id, &push_data).await {
                Ok(job_id) => {
                    info!(
                        "Enqueued job {} for {}/{} @ {} (commits: {}, forced: {})",
                        job_id, 
                        repo.owner.login, 
                        repo.name, 
                        &push.after[..8.min(push.after.len())],
                        push.commits.len(),
                        push.forced
                    );
                    
                    // Store individual commits
                    if let Err(e) = db::store_commits(&state.db, job_id, &push).await {
                        warn!("Failed to store commits for job {}: {}", job_id, e);
                    }
                    
                    (StatusCode::OK, Json(ApiResponse::ok()))
                }
                Err(e) => {
                    error!("Failed to enqueue job: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse::error("Failed to enqueue job")),
                    )
                }
            }
        }
        Err(e) => {
            error!("Failed to upsert repo: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error("Failed to process repo")),
            )
        }
    }
}
