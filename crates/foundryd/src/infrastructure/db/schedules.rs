//! Scheduled job queries — list, toggle, create/delete scheduled jobs.

use anyhow::Result;
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScheduleSummary {
    pub id: i64,
    pub repo_id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub cron_expression: String,
    pub branch: String,
    pub timezone: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}

pub async fn list_schedules(pool: &PgPool) -> Result<Vec<ScheduleSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            s.id,
            s.repo_id,
            r.owner as repo_owner,
            r.name as repo_name,
            s.cron_expression,
            COALESCE(s.branch, 'main') as branch,
            COALESCE(s.timezone, 'UTC') as timezone,
            s.enabled,
            to_char(s.last_run_at, 'YYYY-MM-DD HH24:MI:SS') as last_run_at,
            to_char(s.next_run_at, 'YYYY-MM-DD HH24:MI:SS') as next_run_at
        FROM scheduled_job s
        JOIN repo r ON r.id = s.repo_id
        ORDER BY s.next_run_at ASC NULLS LAST
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ScheduleSummary {
            id: r.get("id"),
            repo_id: r.get("repo_id"),
            repo_owner: r.get("repo_owner"),
            repo_name: r.get("repo_name"),
            cron_expression: r.get("cron_expression"),
            branch: r.get("branch"),
            timezone: r.get("timezone"),
            enabled: r.get("enabled"),
            last_run_at: r.get("last_run_at"),
            next_run_at: r.get("next_run_at"),
        })
        .collect())
}

pub async fn toggle_schedule(pool: &PgPool, schedule_id: i64, enabled: bool) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE scheduled_job
        SET enabled = $2, updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(schedule_id)
    .bind(enabled)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_schedule_by_id(pool: &PgPool, schedule_id: i64) -> Result<bool> {
    let result = sqlx::query(
        r#"DELETE FROM scheduled_job WHERE id = $1"#,
    )
    .bind(schedule_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
