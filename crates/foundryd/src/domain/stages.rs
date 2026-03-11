//! Stage orchestration — runs pipeline stages sequentially with condition evaluation
//! and per-stage metrics collection.
//!
//! Adapted from foundry-agent to use DbLogger for direct DB logging.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use foundry_core::{ClaimedJob, FoundryConfig};

use crate::config::Config;
use crate::infrastructure::docker_runner::{build_image, run_container};
use crate::runtime::execution::{JobMetrics, StageMetrics};
use crate::runtime::db_logger::DbLogger;

pub(crate) async fn run_stages(
    logger: &DbLogger,
    job: &ClaimedJob,
    repo_dir: &PathBuf,
    _config: &Config,
    untrusted_repo_config: &FoundryConfig,
    clone_duration_ms: u64,
) -> Result<()> {
    let job_start = Instant::now();
    let mut stage_metrics: Vec<StageMetrics> = vec![];
    let mut any_failed = false;
    
    let image = if untrusted_repo_config.build.dockerfile.is_some() {
        build_image(logger, job, repo_dir, untrusted_repo_config).await?
    } else {
        untrusted_repo_config.build.image.clone()
    };
    
    logger.log(job, &format!("📋 Running {} stages", untrusted_repo_config.stages.len())).await?;
    
    for (i, stage) in untrusted_repo_config.stages.iter().enumerate() {
        let stage_image = stage.image.as_ref().unwrap_or(&image);
        let stage_start = Instant::now();
        
        let should_run = match &stage.condition {
            Some(foundry_core::config::StageCondition::Always) => true,
            Some(foundry_core::config::StageCondition::OnFailure) => any_failed,
            Some(foundry_core::config::StageCondition::OnSuccess) | None => !any_failed,
            Some(foundry_core::config::StageCondition::OnPr) => job.git_ref.starts_with("refs/pull/"),
            Some(foundry_core::config::StageCondition::OnPush) => !job.git_ref.starts_with("refs/pull/"),
        };
        
        if !should_run {
            logger.log(job, &format!("⏭️  Stage {}: {} (skipped)", i + 1, stage.name)).await?;
            stage_metrics.push(StageMetrics {
                name: stage.name.clone(),
                status: "skipped".to_string(),
                duration_ms: 0,
                exit_code: None,
            });
            continue;
        }
        
        logger.log(job, &format!("▶️  Stage {}: {}", i + 1, stage.name)).await?;
        
        let mut stage_env = untrusted_repo_config.env.clone();
        stage_env.extend(stage.env.clone());
        
        let result = run_container(
            logger,
            job,
            repo_dir,
            stage_image,
            &stage.command,
            Some(&stage_env),
            stage.timeout,
        ).await;
        
        let duration_ms = stage_start.elapsed().as_millis() as u64;
        
        match result {
            Ok(true) => {
                logger.log(job, &format!("✅ Stage {} complete ({} ms)", stage.name, duration_ms)).await?;
                stage_metrics.push(StageMetrics {
                    name: stage.name.clone(),
                    status: "success".to_string(),
                    duration_ms,
                    exit_code: Some(0),
                });
            }
            Ok(false) | Err(_) => {
                logger.log(job, &format!("❌ Stage {} failed ({} ms)", stage.name, duration_ms)).await?;
                stage_metrics.push(StageMetrics {
                    name: stage.name.clone(),
                    status: "failed".to_string(),
                    duration_ms,
                    exit_code: Some(1),
                });
                
                if stage.failure_policy == foundry_core::FailurePolicy::Strict {
                    any_failed = true;
                    if stage.condition.is_none() || stage.condition == Some(foundry_core::config::StageCondition::OnSuccess) {
                        break;
                    }
                }
            }
        }
    }
    
    let total_duration_ms = job_start.elapsed().as_millis() as u64;
    let metrics = JobMetrics {
        clone_duration_ms,
        build_duration_ms: None,
        stages: stage_metrics,
        total_duration_ms,
    };
    
    logger.report_metrics(job, &metrics).await.ok();
    
    if any_failed {
        anyhow::bail!("Pipeline failed")
    }
    
    Ok(())
}
