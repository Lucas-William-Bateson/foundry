use std::sync::Arc;
use std::str::FromStr;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use sqlx::PgPool;
use tracing::{info, error, debug, warn};

/// How long a job can stay in 'running' before being considered stale.
const STALE_JOB_TIMEOUT_MINUTES: i64 = 120;

pub async fn run_scheduler(pool: Arc<PgPool>) {
    info!("Starting scheduler");

    loop {
        if let Err(e) = check_and_run_scheduled_jobs(&pool).await {
            error!("Scheduler error: {}", e);
        }

        if let Err(e) = reap_stale_jobs(&pool).await {
            error!("Stale job reaper error: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

async fn check_and_run_scheduled_jobs(pool: &PgPool) -> anyhow::Result<()> {
    let now = Utc::now();

    let due_jobs = sqlx::query_as::<_, ScheduledJobRow>(
        r#"
        SELECT id, repo_id, cron_expression, branch, COALESCE(timezone, 'UTC') as timezone
        FROM scheduled_job
        WHERE enabled = TRUE AND (next_run_at IS NULL OR next_run_at <= $1)
        "#,
    )
    .bind(now)
    .fetch_all(pool)
    .await?;

    for scheduled in due_jobs {
        debug!("Processing scheduled job {} for repo {}", scheduled.id, scheduled.repo_id);

        // Wrap enqueue + next_run update in a transaction to prevent
        // duplicate enqueues if the process crashes between the two operations.
        let mut tx = pool.begin().await?;

        if let Err(e) = enqueue_scheduled_job(&mut *tx, &scheduled).await {
            error!("Failed to enqueue scheduled job {}: {}", scheduled.id, e);
            tx.rollback().await.ok();
            continue;
        }

        if let Ok(schedule) = Schedule::from_str(&scheduled.cron_expression) {
            let next = compute_next_run(&schedule, &scheduled.timezone);
            if let Some(next) = next {
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
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
    }

    Ok(())
}

async fn enqueue_scheduled_job<'e, E>(executor: E, scheduled: &ScheduledJobRow) -> anyhow::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    // We need two queries, so accept a generic executor that works with both pool and transaction.
    // For the repo lookup, we embed it in a single INSERT ... SELECT to keep it in one statement.
    let branch = scheduled.branch.as_deref().unwrap_or("main");
    let git_ref = format!("refs/heads/{}", branch);

    // IMPLICIT CONTRACT: The "RESOLVE:{branch}" prefix is an implicit contract
    // with foundry-agent's execution.rs, which detects this prefix at clone time
    // and resolves it to the actual branch ref (instead of treating it as a SHA).
    let placeholder_sha = format!("RESOLVE:{}", branch);

    let result = sqlx::query(
        r#"
        INSERT INTO job (
            repo_id, git_sha, git_ref, status, trigger_type,
            scheduled_job_id, commit_message
        )
        SELECT $1, $2, $3, 'queued', 'scheduled', $4, $5
        WHERE EXISTS (SELECT 1 FROM repo WHERE id = $1)
        "#,
    )
    .bind(scheduled.repo_id)
    .bind(&placeholder_sha)
    .bind(&git_ref)
    .bind(scheduled.id)
    .bind(format!("Scheduled build: {}", scheduled.cron_expression))
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!("Repo {} not found", scheduled.repo_id));
    }

    info!("Enqueued scheduled job for repo {} branch {}", scheduled.repo_id, branch);

    Ok(())
}

/// Reap jobs stuck in 'running' state for longer than STALE_JOB_TIMEOUT_MINUTES.
/// These are typically caused by agent crashes or network partitions.
async fn reap_stale_jobs(pool: &PgPool) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE job
        SET status = 'failed',
            finished_at = NOW()
        WHERE status = 'running'
          AND started_at < NOW() - make_interval(mins => $1)
        "#,
    )
    .bind(STALE_JOB_TIMEOUT_MINUTES as i32)
    .execute(pool)
    .await?;

    let reaped = result.rows_affected();
    if reaped > 0 {
        warn!("Reaped {} stale running job(s) (older than {} minutes)", reaped, STALE_JOB_TIMEOUT_MINUTES);
    }

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

    // Validate timezone if provided
    let tz_str = timezone.unwrap_or("UTC");
    if tz_str.parse::<Tz>().is_err() {
        anyhow::bail!("Invalid timezone: {}", tz_str);
    }

    let next_run = compute_next_run(&schedule, tz_str);

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

/// Compute the next run time for a cron schedule, respecting the configured timezone.
/// The cron expression is evaluated in the given timezone, then converted to UTC
/// for storage in the database.
fn compute_next_run(schedule: &Schedule, timezone_str: &str) -> Option<DateTime<Utc>> {
    match timezone_str.parse::<Tz>() {
        Ok(tz) => {
            // Evaluate the cron in the user's timezone, then convert to UTC
            schedule
                .upcoming(tz)
                .next()
                .map(|dt| dt.with_timezone(&Utc))
        }
        Err(_) => {
            warn!("Invalid timezone '{}', falling back to UTC", timezone_str);
            schedule.upcoming(Utc).next()
        }
    }
}

#[derive(sqlx::FromRow)]
struct ScheduledJobRow {
    id: i64,
    repo_id: i64,
    cron_expression: String,
    branch: Option<String>,
    timezone: String,
}
