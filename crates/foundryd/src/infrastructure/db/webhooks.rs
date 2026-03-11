//! Webhook event storage — store and retrieve raw webhook events.

use anyhow::Result;
use sqlx::SqlitePool;

/// Store raw webhook event for debugging/replay
pub async fn store_webhook_event(
    pool: &SqlitePool,
    event_type: &str,
    delivery_id: Option<&str>,
    payload: &[u8],
    job_id: Option<i64>,
) -> Result<i64> {
    let payload_json: serde_json::Value = serde_json::from_slice(payload).unwrap_or(serde_json::Value::Null);
    
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO webhook_event (event_type, delivery_id, payload, job_id, processed)
        VALUES (?1, ?2, ?3, ?4, ?5)
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
