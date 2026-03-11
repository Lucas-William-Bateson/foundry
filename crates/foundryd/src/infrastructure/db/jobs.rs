//! Job CRUD — enqueue, claim, update status, rerun, get by ID, list jobs.

use anyhow::Result;
use sqlx::{SqlitePool, Row};
use uuid::Uuid;

use foundry_core::{ClaimedJob, RunnerRequirements, github::{PushEvent, TriggerType}};

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
    pub fn from_pr_event(event: &foundry_core::github::PullRequestEvent) -> Self {
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

pub async fn enqueue_job(
    pool: &SqlitePool,
    repo_id: i64,
    data: &PushEventData,
) -> Result<i64> {
    let trigger_type_str = data.trigger_type.to_string();
    let files_added_json = serde_json::to_string(&data.files_added)?;
    let files_modified_json = serde_json::to_string(&data.files_modified)?;
    let files_removed_json = serde_json::to_string(&data.files_removed)?;

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
            ?1, ?2, ?3, 'queued', ?4,
            ?5, ?6, ?7, ?8,
            ?9, ?10, ?11,
            ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?20,
            ?21, ?22, ?23,
            ?24, ?25,
            ?26, ?27, ?28, ?29,
            ?30
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
    .bind(&files_added_json)
    .bind(&files_modified_json)
    .bind(&files_removed_json)
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
    pool: &SqlitePool,
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
            ?1, ?2, ?3, 'queued', 'pull_request',
            ?4, ?5, ?6, ?7, ?8,
            ?9, ?10,
            ?11, ?12, ?13,
            ?14, ?15
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
pub async fn rerun_job(pool: &SqlitePool, job_id: i64) -> Result<Option<i64>> {
    // First, get the original job
    let original = sqlx::query(
        r#"
        SELECT 
            repo_id, git_sha, git_ref, trigger_type,
            pr_number, pr_title, pr_url, pr_author, pr_author_avatar,
            base_ref, base_sha, commit_message, commit_author
        FROM job
        WHERE id = ?1
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
            ?1, ?2, ?3, 'queued', ?4,
            ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13,
            ?14
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

/// Store individual commits from a push event
pub async fn store_commits(pool: &SqlitePool, job_id: i64, event: &PushEvent) -> Result<()> {
    for commit in &event.commits {
        let added_json = serde_json::to_string(&commit.added)?;
        let modified_json = serde_json::to_string(&commit.modified)?;
        let removed_json = serde_json::to_string(&commit.removed)?;

        sqlx::query(
            r#"
            INSERT INTO job_commit (
                job_id, sha, tree_id, message,
                author_name, author_email, author_username,
                committer_name, committer_email, committer_username,
                timestamp, url, added, modified, removed, distinct_commit
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
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
        .bind(&added_json)
        .bind(&modified_json)
        .bind(&removed_json)
        .bind(commit.distinct)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn claim_job(pool: &SqlitePool, agent_id: &str, runner_id: Option<Uuid>) -> Result<Option<ClaimedJob>> {
    let claim_token = Uuid::new_v4();
    let runner_id_str = runner_id.map(|u| u.to_string());

    let mut tx = pool.begin().await?;

    // Step 1: Find the best candidate job that matches the runner's capabilities
    let candidate: Option<(i64,)> = sqlx::query_as(
        r#"
        SELECT j.id
        FROM job j
        LEFT JOIN runner ON runner.id = ?1
        WHERE j.status = 'queued'
          AND (
            j.runner_requirements IS NULL
            OR (
              (json_extract(j.runner_requirements, '$.runner_name') IS NULL
               OR json_extract(j.runner_requirements, '$.runner_name') = runner.name)
              AND (
                json_extract(j.runner_requirements, '$.required_tags') IS NULL
                OR json_extract(j.runner_requirements, '$.required_tags') = '[]'
                OR (
                  runner.tags IS NOT NULL
                  AND NOT EXISTS (
                    SELECT 1 FROM json_each(json_extract(j.runner_requirements, '$.required_tags')) AS req
                    WHERE req.value NOT IN (SELECT value FROM json_each(COALESCE(runner.tags, '[]')))
                  )
                )
              )
              AND (json_extract(j.runner_requirements, '$.min_cpu') IS NULL
                   OR COALESCE(runner.cpu, 0) >= CAST(json_extract(j.runner_requirements, '$.min_cpu') AS INTEGER))
              AND (json_extract(j.runner_requirements, '$.min_memory_mb') IS NULL
                   OR COALESCE(runner.memory_mb, 0) >= CAST(json_extract(j.runner_requirements, '$.min_memory_mb') AS INTEGER))
              AND (json_extract(j.runner_requirements, '$.min_gpu') IS NULL
                   OR COALESCE(runner.gpu, 0) >= CAST(json_extract(j.runner_requirements, '$.min_gpu') AS INTEGER))
              AND (json_extract(j.runner_requirements, '$.arch') IS NULL
                   OR json_extract(j.runner_requirements, '$.arch') = runner.arch)
            )
          )
        ORDER BY j.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(&runner_id_str)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((job_id,)) = candidate else {
        return Ok(None);
    };

    // Step 2: Atomically claim the job (re-check status to prevent races)
    let claim_token_str = claim_token.to_string();
    let updated = sqlx::query(
        r#"
        UPDATE job
        SET status = 'running',
            started_at = datetime('now'),
            claimed_by = ?1,
            claim_token = ?2,
            runner_id = ?3
        WHERE id = ?4 AND status = 'queued'
        "#,
    )
    .bind(agent_id)
    .bind(&claim_token_str)
    .bind(&runner_id_str)
    .bind(job_id)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        // Job was claimed by another agent between our SELECT and UPDATE
        return Ok(None);
    }

    // Step 3: Fetch the claimed job with repo info
    let row = sqlx::query(
        r#"
        SELECT
            j.id,
            j.repo_id,
            j.git_sha,
            j.git_ref,
            j.claim_token,
            r.owner as repo_owner,
            r.name as repo_name,
            r.clone_url,
            r.default_image as image
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        WHERE j.id = ?1
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(row.map(|r| {
        let token_str: String = r.get("claim_token");
        ClaimedJob {
            id: r.get("id"),
            repo_id: r.get("repo_id"),
            repo_owner: r.get("repo_owner"),
            repo_name: r.get("repo_name"),
            clone_url: r.get("clone_url"),
            git_sha: r.get("git_sha"),
            git_ref: r.get("git_ref"),
            image: r.get("image"),
            claim_token: Uuid::parse_str(&token_str).unwrap_or(claim_token),
        }
    }))
}

pub async fn report_result(
    pool: &SqlitePool,
    job_id: i64,
    claim_token: Uuid,
    success: bool,
) -> Result<bool> {
    let status = if success { "success" } else { "failed" };

    let result = sqlx::query(
        r#"
        UPDATE job
        SET status = ?3, finished_at = datetime('now')
        WHERE id = ?1 AND claim_token = ?2 AND status = 'running'
        "#,
    )
    .bind(job_id)
    .bind(claim_token.to_string())
    .bind(status)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Verify that a claim token belongs to a running job for a given repo
pub async fn verify_job_token(
    pool: &SqlitePool,
    repo_id: i64,
    claim_token: Uuid,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM job 
            WHERE repo_id = ?1 AND claim_token = ?2 AND status = 'running'
        )
        "#,
    )
    .bind(repo_id)
    .bind(claim_token.to_string())
    .fetch_one(pool)
    .await?;

    Ok(exists)
}

pub async fn store_metrics(
    pool: &SqlitePool,
    job_id: i64,
    claim_token: Uuid,
    metrics: &serde_json::Value,
) -> Result<bool> {
    let metrics_str = serde_json::to_string(metrics)?;

    let result = sqlx::query(
        r#"
        UPDATE job
        SET metrics_json = ?3
        WHERE id = ?1 AND claim_token = ?2
        "#,
    )
    .bind(job_id)
    .bind(claim_token.to_string())
    .bind(&metrics_str)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_jobs(pool: &SqlitePool, limit: i64) -> Result<Vec<JobSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            j.id, 
            r.owner as repo_owner, 
            r.name as repo_name, 
            j.git_sha, 
            j.status,
            j.created_at,
            j.commit_message,
            j.commit_author,
            CAST((julianday(j.finished_at) - julianday(j.started_at)) * 86400 AS INTEGER) as duration_secs,
            j.trigger_type
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        ORDER BY j.created_at DESC
        LIMIT ?1
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

pub async fn get_job(pool: &SqlitePool, job_id: i64) -> Result<Option<JobDetail>> {
    let row = sqlx::query(
        r#"
        SELECT 
            j.id, 
            r.owner as repo_owner, 
            r.name as repo_name, 
            j.git_sha,
            j.git_ref,
            j.status,
            j.created_at,
            j.started_at,
            j.finished_at,
            j.commit_message,
            j.commit_author,
            j.commit_url,
            CAST((julianday(j.finished_at) - julianday(j.started_at)) * 86400 AS INTEGER) as duration_secs,
            j.trigger_type,
            j.pr_number,
            j.pr_title,
            j.pr_url,
            j.metrics_json as metrics
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        WHERE j.id = ?1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let metrics_str: Option<String> = r.get("metrics");
        JobDetail {
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
            metrics: metrics_str.and_then(|s| serde_json::from_str(&s).ok()),
        }
    }))
}

pub async fn get_repo_jobs(pool: &SqlitePool, repo_id: i64, limit: i64) -> Result<Vec<JobSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            j.id, r.owner as repo_owner, r.name as repo_name,
            j.git_sha, j.status,
            j.created_at,
            j.commit_message, j.commit_author,
            CAST((julianday(COALESCE(j.finished_at, datetime('now'))) - julianday(j.started_at)) * 86400 AS INTEGER) as duration_secs,
            j.trigger_type
        FROM job j
        JOIN repo r ON r.id = j.repo_id
        WHERE j.repo_id = ?1
        ORDER BY j.created_at DESC
        LIMIT ?2
        "#,
    )
    .bind(repo_id)
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

/// Set runner requirements on a queued job.
pub async fn set_job_runner_requirements(
    pool: &SqlitePool,
    job_id: i64,
    requirements: &RunnerRequirements,
) -> Result<()> {
    let json_str = serde_json::to_string(&serde_json::to_value(requirements)?)?;
    sqlx::query(
        r#"
        UPDATE job
        SET runner_requirements = ?2
        WHERE id = ?1
        "#,
    )
    .bind(job_id)
    .bind(&json_str)
    .execute(pool)
    .await?;

    Ok(())
}
