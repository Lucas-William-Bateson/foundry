use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use foundry_core::ClaimedJob;

pub async fn enqueue_job(
    pool: &PgPool,
    repo_id: i64,
    git_sha: &str,
    git_ref: &str,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO job (repo_id, git_sha, git_ref, status)
        VALUES ($1, $2, $3, 'queued')
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(git_sha)
    .bind(git_ref)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn upsert_repo(
    pool: &PgPool,
    owner: &str,
    name: &str,
    clone_url: &str,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO repo (owner, name, clone_url)
        VALUES ($1, $2, $3)
        ON CONFLICT (owner, name) DO UPDATE SET clone_url = $3
        RETURNING id
        "#,
    )
    .bind(owner)
    .bind(name)
    .bind(clone_url)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn claim_job(pool: &PgPool, agent_id: &str) -> Result<Option<ClaimedJob>> {
    let claim_token = Uuid::new_v4();

    let row = sqlx::query(
        r#"
        WITH claimed AS (
            UPDATE job
            SET status = 'running', 
                started_at = now(), 
                claimed_by = $1, 
                claim_token = $2
            WHERE id = (
                SELECT id FROM job
                WHERE status = 'queued'
                ORDER BY created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING id, repo_id, git_sha, git_ref, claim_token
        )
        SELECT 
            c.id,
            c.repo_id,
            c.git_sha,
            c.git_ref,
            c.claim_token,
            r.owner as repo_owner,
            r.name as repo_name,
            r.clone_url,
            r.default_image as image
        FROM claimed c
        JOIN repo r ON r.id = c.repo_id
        "#,
    )
    .bind(agent_id)
    .bind(claim_token)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ClaimedJob {
        id: r.get("id"),
        repo_id: r.get("repo_id"),
        repo_owner: r.get("repo_owner"),
        repo_name: r.get("repo_name"),
        clone_url: r.get("clone_url"),
        git_sha: r.get("git_sha"),
        git_ref: r.get("git_ref"),
        image: r.get("image"),
        claim_token: r.get("claim_token"),
    }))
}

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

pub async fn finish_job(
    pool: &PgPool,
    job_id: i64,
    claim_token: Uuid,
    success: bool,
) -> Result<bool> {
    let status = if success { "success" } else { "failed" };

    let result = sqlx::query(
        r#"
        UPDATE job
        SET status = $3::job_status, finished_at = now()
        WHERE id = $1 AND claim_token = $2 AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(claim_token)
    .bind(status)
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

#[derive(Debug)]
pub struct JobSummary {
    pub id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub git_sha: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug)]
pub struct JobDetail {
    pub id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub git_sha: String,
    pub git_ref: String,
    pub status: String,
    pub created_at: String,
}

pub async fn list_jobs(pool: &PgPool, limit: i64) -> Result<Vec<JobSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            j.id, 
            r.owner as repo_owner, 
            r.name as repo_name, 
            j.git_sha, 
            j.status::text,
            to_char(j.created_at, 'YYYY-MM-DD HH24:MI:SS') as created_at
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        ORDER BY j.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| JobSummary {
            id: r.get("id"),
            repo_owner: r.get("repo_owner"),
            repo_name: r.get("repo_name"),
            git_sha: r.get("git_sha"),
            status: r.get("status"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn get_job(pool: &PgPool, job_id: i64) -> Result<Option<JobDetail>> {
    let row = sqlx::query(
        r#"
        SELECT 
            j.id, 
            r.owner as repo_owner, 
            r.name as repo_name, 
            j.git_sha,
            j.git_ref,
            j.status::text,
            to_char(j.created_at, 'YYYY-MM-DD HH24:MI:SS') as created_at
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        WHERE j.id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| JobDetail {
        id: r.get("id"),
        repo_owner: r.get("repo_owner"),
        repo_name: r.get("repo_name"),
        git_sha: r.get("git_sha"),
        git_ref: r.get("git_ref"),
        status: r.get("status"),
        created_at: r.get("created_at"),
    }))
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
