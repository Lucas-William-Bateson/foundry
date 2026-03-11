//! Built-in worker loop — polls for jobs and executes them in-process.
//!
//! This replaces the external foundry-agent binary. The worker claims jobs
//! directly from the database and logs results without HTTP round-trips.

use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn, error};

use crate::AppState;
use crate::infrastructure::github_app::{GitHubApp, CheckConclusion};
use crate::runtime::db_logger::DbLogger;

pub async fn run_worker(state: Arc<AppState>, github_app: Option<Arc<GitHubApp>>) {
    info!("Built-in worker started — polling for jobs");

    let logger = DbLogger::new(state.db.clone());
    let hostname = hostname::get()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    loop {
        match crate::infrastructure::db::jobs::claim_job(
            &state.db,
            &hostname,
            None, // no external runner_id for built-in worker
        ).await {
            Ok(Some(job)) => {
                info!(
                    "Claimed job {} for {}/{} @ {}",
                    job.id,
                    job.repo_owner,
                    job.repo_name,
                    &job.git_sha[..8.min(job.git_sha.len())]
                );

                // Create GitHub check run
                let check_run_id = if let Some(ref app) = github_app {
                    info!("Creating GitHub check run for {}/{}", job.repo_owner, job.repo_name);
                    match app.create_check_run(
                        &job.repo_owner,
                        &job.repo_name,
                        &job.git_sha,
                        "Foundry CI",
                    ).await {
                        Ok(id) => {
                            info!("Created check run with ID {}", id);
                            Some(id)
                        }
                        Err(e) => {
                            warn!("Failed to create check run: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

                // Execute the pipeline
                let (success, error_msg) = match crate::runtime::execution::execute_pipeline(
                    &logger,
                    &job,
                    &state.config,
                    github_app.as_deref(),
                ).await {
                    Ok(()) => {
                        info!("Job {} completed successfully", job.id);
                        (true, None)
                    }
                    Err(e) => {
                        error!("Job {} failed: {}", job.id, e);
                        let _ = logger.log(&job, &format!("ERROR: {}", e)).await;
                        (false, Some(e.to_string()))
                    }
                };

                // Complete GitHub check run
                if let Some(ref app) = github_app {
                    if let Some(check_id) = check_run_id {
                        let logs = match logger.get_logs(&job).await {
                            Ok(logs) => Some(logs),
                            Err(e) => {
                                warn!("Failed to fetch logs: {}", e);
                                None
                            }
                        };

                        let (conclusion, summary) = if success {
                            (CheckConclusion::Success, "Build completed successfully! ✅".to_string())
                        } else {
                            let summary = format!(
                                "Build failed ❌\n\n{}",
                                error_msg.as_deref().unwrap_or_default()
                            );
                            (CheckConclusion::Failure, summary)
                        };

                        if let Err(e) = app.complete_check_run(
                            &job.repo_owner,
                            &job.repo_name,
                            check_id,
                            conclusion,
                            &summary,
                            logs.as_deref(),
                        ).await {
                            warn!("Failed to complete check run: {}", e);
                        }
                    }
                }

                // Report result directly to DB
                if let Err(e) = logger.report_result(&job, success).await {
                    error!("Failed to report job completion: {}", e);
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_secs(
                    state.config.poll_interval_secs,
                )).await;
            }
            Err(e) => {
                warn!("Failed to claim job: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
