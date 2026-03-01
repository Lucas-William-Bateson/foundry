pub mod api;
pub mod domain;
pub mod infrastructure;
pub mod runtime;
pub mod types;








use std::time::Duration;

use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::types::config::Config;
use crate::infrastructure::github_app::{CheckConclusion, GitHubApp};
use crate::api::server::ServerClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "foundry_agent=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let trusted_agent_config = Config::from_env()?;
    info!("Starting foundry-agent: {}", trusted_agent_config.agent_id);
    info!("Server URL: {}", trusted_agent_config.server_url);

    let github_app = if trusted_agent_config.has_github_app() {
        info!("GitHub App authentication enabled");
        Some(GitHubApp::new(
            trusted_agent_config.github_app_id.clone().unwrap(),
            trusted_agent_config.github_installation_id.clone().unwrap(),
            trusted_agent_config.github_private_key.as_ref().unwrap(),
        )?)
    } else {
        warn!("GitHub App not configured - private repos will fail to clone");
        None
    };

    let client = ServerClient::new(&trusted_agent_config);

    // Start the foundryd watchdog
    runtime::watchdog::start_foundryd_watchdog();

    loop {
        match client.claim_job().await {
            Ok(Some(job)) => {
                info!(
                    "Claimed job {} for {}/{} @ {}",
                    job.id,
                    job.repo_owner,
                    job.repo_name,
                    &job.git_sha[..8.min(job.git_sha.len())]
                );

                let check_run_id = if let Some(ref app) = github_app {
                    info!("Creating GitHub check run for {}/{}", job.repo_owner, job.repo_name);
                    match app
                        .create_check_run(
                            &job.repo_owner,
                            &job.repo_name,
                            &job.git_sha,
                            "Foundry CI",
                        )
                        .await
                    {
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

                let (success, error_msg) =
                    match runtime::execution::execute_pipeline(&client, &job, &trusted_agent_config, github_app.as_ref()).await {
                        Ok(()) => {
                            info!("Job {} completed successfully", job.id);
                            (true, None)
                        }
                        Err(e) => {
                            error!("Job {} failed: {}", job.id, e);
                            let _ = client.log(&job, &format!("ERROR: {}", e)).await;
                            (false, Some(e.to_string()))
                        }
                    };

                if let Some(ref app) = github_app {
                    if let Some(check_id) = check_run_id {
                        let logs = match client.get_logs(&job).await {
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
                                error_msg.unwrap_or_default()
                            );
                            (CheckConclusion::Failure, summary)
                        };

                        if let Err(e) = app
                            .complete_check_run(
                                &job.repo_owner,
                                &job.repo_name,
                                check_id,
                                conclusion,
                                &summary,
                                logs.as_deref(),
                            )
                            .await
                        {
                            warn!("Failed to complete check run: {}", e);
                        }
                    }
                }

                if let Err(e) = client.report_result(&job, success).await {
                    error!("Failed to report job completion: {}", e);
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_secs(trusted_agent_config.poll_interval_secs)).await;
            }
            Err(e) => {
                warn!("Failed to claim job: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
