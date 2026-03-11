use anyhow::Result;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn register_runner(
    pool: &SqlitePool,
    name: &str,
    tags: &[String],
    cpu: Option<i32>,
    memory_mb: Option<i32>,
    gpu: i32,
    arch: &str,
) -> Result<Uuid> {
    let new_id = Uuid::new_v4().to_string();
    let tags_json = serde_json::to_string(tags)?;

    let row: (String,) = sqlx::query_as(
        r#"
        INSERT INTO runner (id, name, tags, cpu, memory_mb, gpu, arch, status, last_heartbeat, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'online', datetime('now'), datetime('now'))
        ON CONFLICT (name) DO UPDATE SET
            tags = EXCLUDED.tags,
            cpu = EXCLUDED.cpu,
            memory_mb = EXCLUDED.memory_mb,
            gpu = EXCLUDED.gpu,
            arch = EXCLUDED.arch,
            status = 'online',
            last_heartbeat = datetime('now'),
            updated_at = datetime('now')
        RETURNING id
        "#,
    )
    .bind(&new_id)
    .bind(name)
    .bind(&tags_json)
    .bind(cpu)
    .bind(memory_mb)
    .bind(gpu)
    .bind(arch)
    .fetch_one(pool)
    .await?;

    Ok(Uuid::parse_str(&row.0)?)
}

pub async fn heartbeat_runner(pool: &SqlitePool, runner_id: Uuid) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE runner
        SET last_heartbeat = datetime('now'), status = 'online', updated_at = datetime('now')
        WHERE id = ?1
        "#,
    )
    .bind(runner_id.to_string())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
