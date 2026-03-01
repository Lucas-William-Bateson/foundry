//! Job log operations — append log lines, retrieve logs for a job.

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn append_log(
    pool: &PgPool,
    job_id: i64,
    claim_token: Uuid,
    line: &str,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO job_log (job_id, line)
        SELECT $1, $3
        WHERE EXISTS (
            SELECT 1 FROM job 
            WHERE id = $1 AND claim_token = $2 AND status = 'running'
        )
        "#,
    )
    .bind(job_id)
    .bind(claim_token)
    .bind(line)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn get_logs(
    pool: &PgPool,
    job_id: i64,
    claim_token: Uuid,
) -> Result<Option<String>> {
    let job_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM job WHERE id = $1 AND claim_token = $2
        )
        "#,
    )
    .bind(job_id)
    .bind(claim_token)
    .fetch_one(pool)
    .await?;

    if !job_exists {
        return Ok(None);
    }

    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT line FROM job_log
        WHERE job_id = $1
        ORDER BY ts ASC
        "#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    let logs = rows
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Some(logs))
}

pub async fn get_job_logs(pool: &PgPool, job_id: i64) -> Result<Option<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT line FROM job_log
        WHERE job_id = $1
        ORDER BY ts ASC
        "#,
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    Ok(Some(rows.into_iter().map(|(line,)| line).collect::<Vec<_>>().join("\n")))
}
