use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use foundry_core::{ClaimedJob, github::{PushEvent, PullRequestEvent, TriggerType}};

/// Comprehensive push event data for storage
#[derive(Debug)]
pub struct PushEventData {
    // Basic info
    pub git_sha: String,
    pub git_ref: String,
    pub before_sha: Option<String>,
    pub compare_url: Option<String>,
    
    // Commit info
    pub commit_message: Option<String>,
    pub commit_author: Option<String>,
    pub commit_author_email: Option<String>,
    pub commit_url: Option<String>,
    pub commit_timestamp: Option<String>,
    pub commit_tree_id: Option<String>,
    
    // Committer (can differ from author)
    pub committer_name: Option<String>,
    pub committer_email: Option<String>,
    pub committer_username: Option<String>,
    
    // Files changed
    pub files_added: Vec<String>,
    pub files_modified: Vec<String>,
    pub files_removed: Vec<String>,
    
    // Push metadata
    pub forced: bool,
    pub deleted: bool,
    pub created: bool,
    pub commits_count: i32,
    pub distinct_commits_count: i32,
    
    // Pusher info
    pub pusher_name: Option<String>,
    pub pusher_email: Option<String>,
    
    // Sender (GitHub user)
    pub sender_id: Option<i64>,
    pub sender_login: Option<String>,
    pub sender_avatar_url: Option<String>,
    pub sender_type: Option<String>,
    
    // Installation
    pub installation_id: Option<i64>,
    
    // Trigger type
    pub trigger_type: TriggerType,
}

impl PushEventData {
    pub fn from_push_event(event: &PushEvent) -> Self {
        let head = event.head_commit.as_ref();
        let distinct_count = event.commits.iter().filter(|c| c.distinct).count() as i32;
        
        Self {
            git_sha: event.after.clone(),
            git_ref: event.git_ref.clone(),
            before_sha: Some(event.before.clone()),
            compare_url: Some(event.compare.clone()),
            
            commit_message: head.map(|c| c.message.lines().next().unwrap_or(&c.message).to_string()),
            commit_author: head.and_then(|c| c.author.username.clone().or_else(|| Some(c.author.name.clone()))),
            commit_author_email: head.map(|c| c.author.email.clone()),
            commit_url: head.map(|c| c.url.clone()),
            commit_timestamp: head.map(|c| c.timestamp.clone()),
            commit_tree_id: head.map(|c| c.tree_id.clone()),
            
            committer_name: head.map(|c| c.committer.name.clone()),
            committer_email: head.map(|c| c.committer.email.clone()),
            committer_username: head.and_then(|c| c.committer.username.clone()),
            
            files_added: head.map(|c| c.added.clone()).unwrap_or_default(),
            files_modified: head.map(|c| c.modified.clone()).unwrap_or_default(),
            files_removed: head.map(|c| c.removed.clone()).unwrap_or_default(),
            
            forced: event.forced,
            deleted: event.deleted,
            created: event.created,
            commits_count: event.commits.len() as i32,
            distinct_commits_count: distinct_count,
            
            pusher_name: Some(event.pusher.name.clone()),
            pusher_email: event.pusher.email.clone(),
            
            sender_id: event.sender.as_ref().map(|s| s.id),
            sender_login: event.sender.as_ref().map(|s| s.login.clone()),
            sender_avatar_url: event.sender.as_ref().and_then(|s| s.avatar_url.clone()),
            sender_type: event.sender.as_ref().and_then(|s| s.sender_type.clone()),
            
            installation_id: event.installation.as_ref().map(|i| i.id),
            
            trigger_type: TriggerType::Push,
        }
    }
}

/// Pull request event data for storage
#[derive(Debug)]
pub struct PullRequestEventData {
    pub git_sha: String,
    pub git_ref: String,
    pub pr_number: i64,
    pub pr_title: String,
    pub pr_body: Option<String>,
    pub pr_url: String,
    pub pr_author: String,
    pub pr_author_avatar: Option<String>,
    pub base_ref: String,
    pub base_sha: String,
    pub sender_id: Option<i64>,
    pub sender_login: Option<String>,
    pub sender_avatar_url: Option<String>,
    pub installation_id: Option<i64>,
}

impl PullRequestEventData {
    pub fn from_pr_event(event: &PullRequestEvent) -> Self {
        let pr = &event.pull_request;
        Self {
            git_sha: pr.head.sha.clone(),
            git_ref: format!("refs/pull/{}/head", pr.number),
            pr_number: pr.number,
            pr_title: pr.title.clone(),
            pr_body: pr.body.clone(),
            pr_url: pr.html_url.clone(),
            pr_author: pr.user.login.clone(),
            pr_author_avatar: pr.user.avatar_url.clone(),
            base_ref: pr.base.git_ref.clone(),
            base_sha: pr.base.sha.clone(),
            sender_id: event.sender.as_ref().map(|s| s.id),
            sender_login: event.sender.as_ref().map(|s| s.login.clone()),
            sender_avatar_url: event.sender.as_ref().and_then(|s| s.avatar_url.clone()),
            installation_id: event.installation.as_ref().map(|i| i.id),
        }
    }
}

/// Repository data for upsert
#[derive(Debug)]
pub struct RepoData {
    pub owner: String,
    pub name: String,
    pub clone_url: String,
    pub github_id: Option<i64>,
    pub full_name: Option<String>,
    pub html_url: Option<String>,
    pub ssh_url: Option<String>,
    pub private: bool,
    pub default_branch: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
}

impl RepoData {
    pub fn from_push_event(event: &PushEvent) -> Self {
        let repo = &event.repository;
        Self {
            owner: repo.owner.login.clone(),
            name: repo.name.clone(),
            clone_url: repo.clone_url.clone(),
            github_id: Some(repo.id),
            full_name: Some(repo.full_name.clone()),
            html_url: Some(repo.html_url.clone()),
            ssh_url: Some(repo.ssh_url.clone()),
            private: repo.private,
            default_branch: Some(repo.default_branch.clone()),
            language: repo.language.clone(),
            description: repo.description.clone(),
        }
    }
}

pub async fn enqueue_job(
    pool: &PgPool,
    repo_id: i64,
    data: &PushEventData,
) -> Result<i64> {
    let trigger_type_str = data.trigger_type.to_string();
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO job (
            repo_id, git_sha, git_ref, status, trigger_type,
            before_sha, compare_url, commits_count, distinct_commits_count,
            forced, deleted, created,
            commit_message, commit_author, commit_author_email, commit_url, commit_timestamp, commit_tree_id,
            committer_name, committer_email, committer_username,
            files_added, files_modified, files_removed,
            pusher_name, pusher_email,
            sender_id, sender_login, sender_avatar_url, sender_type,
            installation_id
        )
        VALUES (
            $1, $2, $3, 'queued', $4::trigger_type,
            $5, $6, $7, $8,
            $9, $10, $11,
            $12, $13, $14, $15, $16, $17,
            $18, $19, $20,
            $21, $22, $23,
            $24, $25,
            $26, $27, $28, $29,
            $30
        )
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(&data.git_sha)
    .bind(&data.git_ref)
    .bind(&trigger_type_str)
    .bind(&data.before_sha)
    .bind(&data.compare_url)
    .bind(data.commits_count)
    .bind(data.distinct_commits_count)
    .bind(data.forced)
    .bind(data.deleted)
    .bind(data.created)
    .bind(&data.commit_message)
    .bind(&data.commit_author)
    .bind(&data.commit_author_email)
    .bind(&data.commit_url)
    .bind(&data.commit_timestamp)
    .bind(&data.commit_tree_id)
    .bind(&data.committer_name)
    .bind(&data.committer_email)
    .bind(&data.committer_username)
    .bind(&data.files_added)
    .bind(&data.files_modified)
    .bind(&data.files_removed)
    .bind(&data.pusher_name)
    .bind(&data.pusher_email)
    .bind(data.sender_id)
    .bind(&data.sender_login)
    .bind(&data.sender_avatar_url)
    .bind(&data.sender_type)
    .bind(data.installation_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Enqueue a job for a pull request event
pub async fn enqueue_pr_job(
    pool: &PgPool,
    repo_id: i64,
    data: &PullRequestEventData,
) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO job (
            repo_id, git_sha, git_ref, status, trigger_type,
            pr_number, pr_title, pr_url, pr_author, pr_author_avatar,
            base_ref, base_sha,
            sender_id, sender_login, sender_avatar_url,
            installation_id, commit_message
        )
        VALUES (
            $1, $2, $3, 'queued', 'pull_request',
            $4, $5, $6, $7, $8,
            $9, $10,
            $11, $12, $13,
            $14, $15
        )
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(&data.git_sha)
    .bind(&data.git_ref)
    .bind(data.pr_number)
    .bind(&data.pr_title)
    .bind(&data.pr_url)
    .bind(&data.pr_author)
    .bind(&data.pr_author_avatar)
    .bind(&data.base_ref)
    .bind(&data.base_sha)
    .bind(data.sender_id)
    .bind(&data.sender_login)
    .bind(&data.sender_avatar_url)
    .bind(data.installation_id)
    .bind(&data.pr_title) // Use PR title as commit message for display
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Re-run a job by creating a new job with the same parameters
pub async fn rerun_job(pool: &PgPool, job_id: i64) -> Result<Option<i64>> {
    // First, get the original job
    let original = sqlx::query(
        r#"
        SELECT 
            repo_id, git_sha, git_ref, trigger_type::text,
            pr_number, pr_title, pr_url, pr_author, pr_author_avatar,
            base_ref, base_sha, commit_message, commit_author
        FROM job
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;

    let Some(original) = original else {
        return Ok(None);
    };

    let trigger_type: String = original.get("trigger_type");
    
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO job (
            repo_id, git_sha, git_ref, status, trigger_type,
            pr_number, pr_title, pr_url, pr_author, pr_author_avatar,
            base_ref, base_sha, commit_message, commit_author,
            parent_job_id
        )
        VALUES (
            $1, $2, $3, 'queued', $4::trigger_type,
            $5, $6, $7, $8, $9,
            $10, $11, $12, $13,
            $14
        )
        RETURNING id
        "#,
    )
    .bind(original.get::<i64, _>("repo_id"))
    .bind(original.get::<String, _>("git_sha"))
    .bind(original.get::<String, _>("git_ref"))
    .bind(&trigger_type)
    .bind(original.get::<Option<i64>, _>("pr_number"))
    .bind(original.get::<Option<String>, _>("pr_title"))
    .bind(original.get::<Option<String>, _>("pr_url"))
    .bind(original.get::<Option<String>, _>("pr_author"))
    .bind(original.get::<Option<String>, _>("pr_author_avatar"))
    .bind(original.get::<Option<String>, _>("base_ref"))
    .bind(original.get::<Option<String>, _>("base_sha"))
    .bind(original.get::<Option<String>, _>("commit_message"))
    .bind(original.get::<Option<String>, _>("commit_author"))
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(row.0))
}

pub async fn upsert_repo(pool: &PgPool, data: &RepoData) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO repo (owner, name, clone_url, github_id, full_name, html_url, ssh_url, private, default_branch, language, description)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (owner, name) DO UPDATE SET 
            clone_url = EXCLUDED.clone_url,
            github_id = COALESCE(EXCLUDED.github_id, repo.github_id),
            full_name = COALESCE(EXCLUDED.full_name, repo.full_name),
            html_url = COALESCE(EXCLUDED.html_url, repo.html_url),
            ssh_url = COALESCE(EXCLUDED.ssh_url, repo.ssh_url),
            private = EXCLUDED.private,
            default_branch = COALESCE(EXCLUDED.default_branch, repo.default_branch),
            language = COALESCE(EXCLUDED.language, repo.language),
            description = COALESCE(EXCLUDED.description, repo.description),
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(&data.owner)
    .bind(&data.name)
    .bind(&data.clone_url)
    .bind(data.github_id)
    .bind(&data.full_name)
    .bind(&data.html_url)
    .bind(&data.ssh_url)
    .bind(data.private)
    .bind(&data.default_branch)
    .bind(&data.language)
    .bind(&data.description)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Store individual commits from a push event
pub async fn store_commits(pool: &PgPool, job_id: i64, event: &PushEvent) -> Result<()> {
    for commit in &event.commits {
        sqlx::query(
            r#"
            INSERT INTO job_commit (
                job_id, sha, tree_id, message,
                author_name, author_email, author_username,
                committer_name, committer_email, committer_username,
                timestamp, url, added, modified, removed, distinct_commit
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT (job_id, sha) DO NOTHING
            "#,
        )
        .bind(job_id)
        .bind(&commit.id)
        .bind(&commit.tree_id)
        .bind(&commit.message)
        .bind(&commit.author.name)
        .bind(&commit.author.email)
        .bind(&commit.author.username)
        .bind(&commit.committer.name)
        .bind(&commit.committer.email)
        .bind(&commit.committer.username)
        .bind(&commit.timestamp)
        .bind(&commit.url)
        .bind(&commit.added)
        .bind(&commit.modified)
        .bind(&commit.removed)
        .bind(commit.distinct)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Store raw webhook event for debugging/replay
pub async fn store_webhook_event(
    pool: &PgPool,
    event_type: &str,
    delivery_id: Option<&str>,
    payload: &[u8],
    job_id: Option<i64>,
) -> Result<i64> {
    let payload_json: serde_json::Value = serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
    
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO webhook_event (event_type, delivery_id, payload, job_id, processed)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(event_type)
    .bind(delivery_id)
    .bind(payload_json)
    .bind(job_id)
    .bind(job_id.is_some())
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

/// Verify that a claim token belongs to a running job for a given repo
pub async fn verify_job_token(
    pool: &PgPool,
    repo_id: i64,
    claim_token: Uuid,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM job 
            WHERE repo_id = $1 AND claim_token = $2 AND status = 'running'
        )
        "#,
    )
    .bind(repo_id)
    .bind(claim_token)
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

pub async fn store_metrics(
    pool: &PgPool,
    job_id: i64,
    claim_token: Uuid,
    metrics: &serde_json::Value,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE job
        SET metrics_json = $3
        WHERE id = $1 AND claim_token = $2
        "#,
    )
    .bind(job_id)
    .bind(claim_token)
    .bind(metrics)
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
    pub trigger_type: Option<String>,
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
    pub trigger_type: Option<String>,
    pub pr_number: Option<i64>,
    pub pr_title: Option<String>,
    pub pr_url: Option<String>,
    pub metrics: Option<serde_json::Value>,
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
            EXTRACT(EPOCH FROM (j.finished_at - j.started_at))::bigint as duration_secs,
            j.trigger_type::text as trigger_type
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
            trigger_type: r.get("trigger_type"),
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
            EXTRACT(EPOCH FROM (j.finished_at - j.started_at))::bigint as duration_secs,
            j.trigger_type::text as trigger_type,
            j.pr_number,
            j.pr_title,
            j.pr_url,
            j.metrics_json as metrics
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
        trigger_type: r.get("trigger_type"),
        pr_number: r.get("pr_number"),
        pr_title: r.get("pr_title"),
        pr_url: r.get("pr_url"),
        metrics: r.get("metrics"),
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

/// Check if a push to a branch should trigger a build based on repo config
pub async fn should_build_branch(pool: &PgPool, owner: &str, name: &str, branch: &str) -> Result<bool> {
    let row: Option<(Vec<String>,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(triggers_branches, ARRAY['main', 'master']) as branches
        FROM repo
        WHERE owner = $1 AND name = $2
        "#,
    )
    .bind(owner)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    // If repo doesn't exist yet, use defaults
    let branches = row.map(|(b,)| b).unwrap_or_else(|| vec!["main".to_string(), "master".to_string()]);
    
    Ok(branches.iter().any(|b| b == branch))
}

/// Check if a PR should trigger a build based on repo config
pub async fn should_build_pr(pool: &PgPool, owner: &str, name: &str, target_branch: &str) -> Result<bool> {
    let row: Option<(bool, Option<Vec<String>>)> = sqlx::query_as(
        r#"
        SELECT 
            COALESCE(triggers_pull_requests, TRUE) as pr_enabled,
            triggers_pr_target_branches
        FROM repo
        WHERE owner = $1 AND name = $2
        "#,
    )
    .bind(owner)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((pr_enabled, target_branches)) => {
            if !pr_enabled {
                return Ok(false);
            }
            // If specific target branches are configured, check against them
            if let Some(targets) = target_branches {
                Ok(targets.iter().any(|b| b == target_branch))
            } else {
                Ok(true) // No filter, build all PRs
            }
        }
        None => Ok(true), // Repo not in DB yet, default to building
    }
}

/// Sync the foundry config triggers to the repo table
pub async fn sync_repo_triggers(
    pool: &PgPool,
    repo_id: i64,
    branches: &[String],
    pull_requests: bool,
    pr_target_branches: Option<&[String]>,
    config_json: Option<&serde_json::Value>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE repo SET
            triggers_branches = $2,
            triggers_pull_requests = $3,
            triggers_pr_target_branches = $4,
            config_json = COALESCE($5, config_json),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(repo_id)
    .bind(branches)
    .bind(pull_requests)
    .bind(pr_target_branches)
    .bind(config_json)
    .execute(pool)
    .await?;

    Ok(())
}
