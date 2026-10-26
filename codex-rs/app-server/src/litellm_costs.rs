//! Durable LiteLLM response costs, separate from the shared Codex state schema.
//!
//! Only provider-reported USD costs are recorded. A response is charged once per
//! thread, including after app-server restarts; missing costs remain unknown.

use codex_protocol::ThreadId;
use codex_protocol::protocol::RawResponseCompletedEvent;
use codex_state::SqliteConfig;
use sqlx::SqlitePool;

const DATABASE: &str = "litellm-costs.sqlite";

pub(crate) async fn record(
    sqlite: &SqliteConfig,
    thread_id: ThreadId,
    event: &RawResponseCompletedEvent,
) -> anyhow::Result<()> {
    let Some(cost) = event
        .usage_metadata
        .as_ref()
        .and_then(|usage| usage.metadata.as_ref())
        .and_then(|usage| usage.get("cost"))
        .and_then(serde_json::Value::as_f64)
        .filter(|cost| usd_micros(*cost).is_some())
    else {
        return Ok(());
    };
    let pool = connect(sqlite).await?;
    sqlx::query(
        "INSERT INTO response_costs(thread_id, response_id, usd) VALUES (?, ?, ?)
         ON CONFLICT(thread_id, response_id) DO NOTHING",
    )
    .bind(thread_id.to_string())
    .bind(&event.response_id)
    .bind(cost)
    .execute(&pool)
    .await?;
    pool.close().await;
    Ok(())
}

pub(crate) async fn thread_usd_micros(
    sqlite: &SqliteConfig,
    thread_id: ThreadId,
) -> anyhow::Result<Option<i64>> {
    if !tokio::fs::try_exists(sqlite.home().join(DATABASE)).await? {
        return Ok(None);
    }
    let pool = connect(sqlite).await?;
    let usd: Option<f64> =
        sqlx::query_scalar("SELECT SUM(usd) FROM response_costs WHERE thread_id = ?")
            .bind(thread_id.to_string())
            .fetch_one(&pool)
            .await?;
    pool.close().await;
    Ok(usd.and_then(usd_micros))
}

fn usd_micros(usd: f64) -> Option<i64> {
    let micros = (usd * 1_000_000.0).round();
    (usd >= 0.0 && micros.is_finite() && micros < i64::MAX as f64).then_some(micros as i64)
}

async fn connect(sqlite: &SqliteConfig) -> anyhow::Result<SqlitePool> {
    tokio::fs::create_dir_all(sqlite.home()).await?;
    let pool = sqlite
        .open_read_write_pool(&sqlite.home().join(DATABASE))
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS response_costs (
            thread_id TEXT NOT NULL,
            response_id TEXT NOT NULL,
            usd REAL NOT NULL CHECK (usd >= 0),
            PRIMARY KEY (thread_id, response_id)
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

#[cfg(test)]
#[path = "litellm_costs_tests.rs"]
mod tests;
