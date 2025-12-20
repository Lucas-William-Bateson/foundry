mod config;
mod docker;
mod github_app;
mod server;

use std::time::Duration;

use anyhow::Result;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::github_app::GitHubApp;
use crate::server::ServerClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "foundry_agent=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    info!("Starting foundry-agent: {}", config.agent_id);
    info!("Server URL: {}", config.server_url);

    let github_app = if config.has_github_app() {
        info!("GitHub App authentication enabled");
        Some(GitHubApp::new(
            config.github_app_id.clone().unwrap(),
            config.github_installation_id.clone().unwrap(),
            config.github_private_key.as_ref().unwrap(),
        )?)
    } else {
        warn!("GitHub App not configured - private repos will fail to clone");
        None
    };

    let client = ServerClient::new(&config);

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

                let success =
                    match docker::run_job(&client, &job, &config, github_app.as_ref()).await {
                        Ok(()) => {
                            info!("Job {} completed successfully", job.id);
                            true
                        }
                        Err(e) => {
                            error!("Job {} failed: {}", job.id, e);
                            let _ = client.log(&job, &format!("ERROR: {}", e)).await;
                            false
                        }
                    };

                if let Err(e) = client.finish(&job, success).await {
                    error!("Failed to report job completion: {}", e);
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)).await;
            }
            Err(e) => {
                warn!("Failed to claim job: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
