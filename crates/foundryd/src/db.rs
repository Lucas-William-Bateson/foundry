use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use foundry_core::ClaimedJob;

pub struct CommitInfo {
    pub message: Option<String>,
    pub author: Option<String>,
    pub url: Option<String>,
}

pub async fn enqueue_job(
    pool: &PgPool,
    repo_id: i64,
    git_sha: &str,
    git_ref: &str,
    commit_info: Option<CommitInfo>,
) -> Result<i64> {
    let (message, author, url) = commit_info
        .map(|c| (c.message, c.author, c.url))
        .unwrap_or((None, None, None));
    
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO job (repo_id, git_sha, git_ref, status, commit_message, commit_author, commit_url)
        VALUES ($1, $2, $3, 'queued', $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(git_sha)
    .bind(git_ref)
    .bind(message)
    .bind(author)
    .bind(url)
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

#[derive(Debug, serde::Serialize)]
pub struct JobSummary {
    pub id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub git_sha: String,
    pub status: String,
    pub created_at: String,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub duration_secs: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct JobDetail {
    pub id: i64,
    pub repo_owner: String,
    pub repo_name: String,
    pub git_sha: String,
    pub git_ref: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub commit_url: Option<String>,
    pub duration_secs: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct RepoSummary {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub build_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub last_build_at: Option<String>,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct DashboardStats {
    pub total_jobs: i64,
    pub jobs_today: i64,
    pub success_rate: f64,
    pub queued_count: i64,
    pub running_count: i64,
}

pub async fn get_dashboard_stats(pool: &PgPool) -> Result<DashboardStats> {
    let row = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_jobs,
            COUNT(*) FILTER (WHERE created_at > now() - interval '24 hours') as jobs_today,
            COALESCE(
                COUNT(*) FILTER (WHERE status = 'success')::float / 
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

pub async fn list_repos(pool: &PgPool) -> Result<Vec<RepoSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            id, owner, name, build_count, success_count, failure_count,
            to_char(last_build_at, 'YYYY-MM-DD HH24:MI:SS') as last_build_at
        FROM repo
        ORDER BY last_build_at DESC NULLS LAST
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| RepoSummary {
            id: r.get("id"),
            owner: r.get("owner"),
            name: r.get("name"),
            build_count: r.get("build_count"),
            success_count: r.get("success_count"),
            failure_count: r.get("failure_count"),
            last_build_at: r.get("last_build_at"),
        })
        .collect())
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
            to_char(j.created_at, 'YYYY-MM-DD HH24:MI:SS') as created_at,
            j.commit_message,
            j.commit_author,
            EXTRACT(EPOCH FROM (j.finished_at - j.started_at))::bigint as duration_secs
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
            commit_message: r.get("commit_message"),
            commit_author: r.get("commit_author"),
            duration_secs: r.get("duration_secs"),
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
            to_char(j.created_at, 'YYYY-MM-DD HH24:MI:SS') as created_at,
            to_char(j.started_at, 'YYYY-MM-DD HH24:MI:SS') as started_at,
            to_char(j.finished_at, 'YYYY-MM-DD HH24:MI:SS') as finished_at,
            j.commit_message,
            j.commit_author,
            j.commit_url,
            EXTRACT(EPOCH FROM (j.finished_at - j.started_at))::bigint as duration_secs
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
        started_at: r.get("started_at"),
        finished_at: r.get("finished_at"),
        commit_message: r.get("commit_message"),
        commit_author: r.get("commit_author"),
        commit_url: r.get("commit_url"),
        duration_secs: r.get("duration_secs"),
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
