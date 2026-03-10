use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn register_runner(
    pool: &PgPool,
    name: &str,
    tags: &[String],
    cpu: Option<i32>,
    memory_mb: Option<i32>,
    gpu: i32,
    arch: &str,
) -> Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO runner (name, tags, cpu, memory_mb, gpu, arch, status, last_heartbeat, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'online', NOW(), NOW())
        ON CONFLICT (name) DO UPDATE SET
            tags = EXCLUDED.tags,
            cpu = EXCLUDED.cpu,
            memory_mb = EXCLUDED.memory_mb,
            gpu = EXCLUDED.gpu,
            arch = EXCLUDED.arch,
            status = 'online',
            last_heartbeat = NOW(),
            updated_at = NOW()
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(tags)
    .bind(cpu)
    .bind(memory_mb)
    .bind(gpu)
    .bind(arch)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn heartbeat_runner(pool: &PgPool, runner_id: Uuid) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE runner
        SET last_heartbeat = NOW(), status = 'online', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(runner_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
