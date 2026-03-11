//! Repository management — get/create/update repos, repo settings, build triggers.

use anyhow::Result;
use sqlx::{SqlitePool, Row};

use foundry_core::github::PushEvent;

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

#[derive(Debug, serde::Serialize)]
pub struct RepoDetail {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub full_name: Option<String>,
    pub html_url: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    pub private: bool,
    pub build_count: i32,
    pub success_count: i32,
    pub failure_count: i32,
    pub last_build_at: Option<String>,
    pub created_at: String,
}

pub async fn upsert_repo(pool: &SqlitePool, data: &RepoData) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO repo (owner, name, clone_url, github_id, full_name, html_url, ssh_url, private, default_branch, language, description)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
            updated_at = datetime('now')
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

pub async fn list_repos(pool: &SqlitePool) -> Result<Vec<RepoSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT 
            id, owner, name, build_count, success_count, failure_count,
            last_build_at
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

pub async fn get_repo(pool: &SqlitePool, id: i64) -> Result<Option<RepoDetail>> {
    let row = sqlx::query(
        r#"
        SELECT 
            id, owner, name, full_name, html_url, description, language,
            default_branch, private, build_count, success_count, failure_count,
            last_build_at,
            created_at
        FROM repo
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| RepoDetail {
        id: r.get("id"),
        owner: r.get("owner"),
        name: r.get("name"),
        full_name: r.get("full_name"),
        html_url: r.get("html_url"),
        description: r.get("description"),
        language: r.get("language"),
        default_branch: r.get("default_branch"),
        private: r.get("private"),
        build_count: r.get("build_count"),
        success_count: r.get("success_count"),
        failure_count: r.get("failure_count"),
        last_build_at: r.get("last_build_at"),
        created_at: r.get("created_at"),
    }))
}

/// Check if a push to a branch should trigger a build based on repo config
pub async fn should_build_branch(pool: &SqlitePool, owner: &str, name: &str, branch: &str) -> Result<bool> {
    let row: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(triggers_branches, '["main","master"]') as branches
        FROM repo
        WHERE owner = ?1 AND name = ?2
        "#,
    )
    .bind(owner)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    let branches: Vec<String> = row
        .map(|(b,)| serde_json::from_str(&b).unwrap_or_default())
        .unwrap_or_else(|| vec!["main".to_string(), "master".to_string()]);
    
    Ok(branches.iter().any(|b| b == branch))
}

/// Check if a PR should trigger a build based on repo config
pub async fn should_build_pr(pool: &SqlitePool, owner: &str, name: &str, target_branch: &str) -> Result<bool> {
    let row: Option<(bool, Option<String>)> = sqlx::query_as(
        r#"
        SELECT 
            COALESCE(triggers_pull_requests, 1) as pr_enabled,
            triggers_pr_target_branches
        FROM repo
        WHERE owner = ?1 AND name = ?2
        "#,
    )
    .bind(owner)
    .bind(name)
    .fetch_optional(pool)
    .await?;

    match row {
        Some((pr_enabled, target_branches_json)) => {
            if !pr_enabled {
                return Ok(false);
            }
            // If specific target branches are configured, check against them
            if let Some(json_str) = target_branches_json {
                let targets: Vec<String> = serde_json::from_str(&json_str).unwrap_or_default();
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
    pool: &SqlitePool,
    repo_id: i64,
    branches: &[String],
    pull_requests: bool,
    pr_target_branches: Option<&[String]>,
    config_json: Option<&serde_json::Value>,
) -> Result<()> {
    let branches_json = serde_json::to_string(branches)?;
    let pr_targets_json = pr_target_branches.map(serde_json::to_string).transpose()?;
    let config_str = config_json.map(serde_json::to_string).transpose()?;

    sqlx::query(
        r#"
        UPDATE repo SET
            triggers_branches = ?2,
            triggers_pull_requests = ?3,
            triggers_pr_target_branches = ?4,
            config_json = COALESCE(?5, config_json),
            updated_at = datetime('now')
        WHERE id = ?1
        "#,
    )
    .bind(repo_id)
    .bind(&branches_json)
    .bind(pull_requests)
    .bind(&pr_targets_json)
    .bind(&config_str)
    .execute(pool)
    .await?;

    Ok(())
}
