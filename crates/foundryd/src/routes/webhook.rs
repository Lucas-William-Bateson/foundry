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

use crate::{db, AppState};

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

    info!("Received GitHub webhook: {}", event_type);

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

    let ref_name = push.git_ref.strip_prefix("refs/heads/").unwrap_or(&push.git_ref);
    if ref_name != "main" && ref_name != "master" {
        info!("Ignoring push to non-default branch: {}", ref_name);
        return (StatusCode::OK, Json(ApiResponse::ok()));
    }

    let repo = &push.repository;
    match db::upsert_repo(
        &state.db,
        &repo.owner.login,
        &repo.name,
        &repo.clone_url,
    )
    .await
    {
        Ok(repo_id) => {
            match db::enqueue_job(&state.db, repo_id, &push.after, &push.git_ref).await {
                Ok(job_id) => {
                    info!(
                        "Enqueued job {} for {}/{} @ {}",
                        job_id, repo.owner.login, repo.name, &push.after[..8]
                    );
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
