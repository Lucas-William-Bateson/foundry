//! Statistics and dashboard queries — job counts, success rates, recent activity.

use anyhow::Result;
use sqlx::{SqlitePool, Row};

#[derive(Debug, Default, serde::Serialize)]
pub struct DashboardStats {
    pub total_jobs: i64,
    pub jobs_today: i64,
    pub success_rate: f64,
    pub queued_count: i64,
    pub running_count: i64,
}

pub async fn get_dashboard_stats(pool: &SqlitePool) -> Result<DashboardStats> {
    let row = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_jobs,
            COUNT(*) FILTER (WHERE created_at > datetime('now', '-24 hours')) as jobs_today,
            COALESCE(
                CAST(COUNT(*) FILTER (WHERE status = 'success') AS REAL) / 
                NULLIF(COUNT(*) FILTER (WHERE status IN ('success', 'failed')), 0) * 100,
                0
            ) as success_rate,
            COUNT(*) FILTER (WHERE status = 'queued') as queued_count,
            COUNT(*) FILTER (WHERE status = 'running') as running_count
        FROM job
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(DashboardStats {
        total_jobs: row.get("total_jobs"),
        jobs_today: row.get("jobs_today"),
        success_rate: row.get("success_rate"),
        queued_count: row.get("queued_count"),
        running_count: row.get("running_count"),
    })
}
