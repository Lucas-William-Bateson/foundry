use std::sync::Arc;
use std::str::FromStr;
use chrono::{DateTime, Utc};
use cron::Schedule;
use sqlx::PgPool;
use tracing::{info, error, debug};

pub async fn run_scheduler(pool: Arc<PgPool>) {
    info!("Starting scheduler");
    
    loop {
        if let Err(e) = check_and_run_scheduled_jobs(&pool).await {
            error!("Scheduler error: {}", e);
        }
        
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

async fn check_and_run_scheduled_jobs(pool: &PgPool) -> anyhow::Result<()> {
    let now = Utc::now();
    
    let due_jobs = sqlx::query_as::<_, ScheduledJobRow>(
        r#"
        SELECT id, repo_id, cron_expression, branch, timezone
        FROM scheduled_job
        WHERE enabled = TRUE AND (next_run_at IS NULL OR next_run_at <= $1)
        "#,
    )
    .bind(now)
    .fetch_all(pool)
    .await?;
    
    for scheduled in due_jobs {
        debug!("Processing scheduled job {} for repo {}", scheduled.id, scheduled.repo_id);
        
        if let Err(e) = enqueue_scheduled_job(pool, &scheduled).await {
            error!("Failed to enqueue scheduled job {}: {}", scheduled.id, e);
        }
        
        if let Ok(schedule) = Schedule::from_str(&scheduled.cron_expression) {
            if let Some(next) = schedule.upcoming(Utc).next() {
                sqlx::query(
                    r#"
                    UPDATE scheduled_job
                    SET last_run_at = $2, next_run_at = $3, updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(scheduled.id)
                .bind(now)
                .bind(next)
                .execute(pool)
                .await?;
            }
        }
    }
    
    Ok(())
}

async fn enqueue_scheduled_job(pool: &PgPool, scheduled: &ScheduledJobRow) -> anyhow::Result<()> {
    let repo = sqlx::query_as::<_, RepoInfo>(
        r#"SELECT owner, name, clone_url, default_branch FROM repo WHERE id = $1"#,
    )
    .bind(scheduled.repo_id)
    .fetch_optional(pool)
    .await?;
    
    let Some(repo) = repo else {
        return Err(anyhow::anyhow!("Repo not found"));
    };
    
    let branch = scheduled.branch.as_deref().unwrap_or(
        repo.default_branch.as_deref().unwrap_or("main")
    );
    
    let git_ref = format!("refs/heads/{}", branch);
    
    sqlx::query(
        r#"
        INSERT INTO job (
            repo_id, git_sha, git_ref, status, trigger_type,
            scheduled_job_id, commit_message
        )
        VALUES ($1, 'HEAD', $2, 'queued', 'manual', $3, $4)
        "#,
    )
    .bind(scheduled.repo_id)
    .bind(&git_ref)
    .bind(scheduled.id)
    .bind(format!("Scheduled build: {}", scheduled.cron_expression))
    .execute(pool)
    .await?;
    
    info!("Enqueued scheduled job for repo {} branch {}", repo.name, branch);
    
    Ok(())
}

pub async fn upsert_schedule(
    pool: &PgPool,
    repo_id: i64,
    cron_expression: &str,
    branch: Option<&str>,
    timezone: Option<&str>,
) -> anyhow::Result<i64> {
    let schedule = Schedule::from_str(cron_expression)
        .map_err(|e| anyhow::anyhow!("Invalid cron expression: {}", e))?;
    
    let next_run: Option<DateTime<Utc>> = schedule.upcoming(Utc).next();
    
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO scheduled_job (repo_id, cron_expression, branch, timezone, next_run_at)
        VALUES ($1, $2, COALESCE($3, 'main'), COALESCE($4, 'UTC'), $5)
        ON CONFLICT (repo_id, branch) DO UPDATE SET
            cron_expression = EXCLUDED.cron_expression,
            timezone = COALESCE(EXCLUDED.timezone, scheduled_job.timezone),
            next_run_at = EXCLUDED.next_run_at,
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(cron_expression)
    .bind(branch)
    .bind(timezone)
    .bind(next_run)
    .fetch_one(pool)
    .await?;
    
    Ok(row.0)
}

pub async fn delete_schedule(pool: &PgPool, repo_id: i64, branch: Option<&str>) -> anyhow::Result<bool> {
    let branch = branch.unwrap_or("main");
    
    let result = sqlx::query(
        r#"DELETE FROM scheduled_job WHERE repo_id = $1 AND branch = $2"#,
    )
    .bind(repo_id)
    .bind(branch)
    .execute(pool)
    .await?;
    
    Ok(result.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct ScheduledJobRow {
    id: i64,
    repo_id: i64,
    cron_expression: String,
    branch: Option<String>,
    #[allow(dead_code)]
    timezone: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RepoInfo {
    #[allow(dead_code)]
    owner: String,
    name: String,
    #[allow(dead_code)]
    clone_url: String,
    default_branch: Option<String>,
}
