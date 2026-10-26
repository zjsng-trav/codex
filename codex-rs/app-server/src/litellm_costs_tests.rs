use super::*;
use codex_protocol::ResponseUsageMetadata;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;

fn response(response_id: &str, usage: Value) -> RawResponseCompletedEvent {
    RawResponseCompletedEvent {
        response_id: response_id.to_string(),
        token_usage: None,
        usage_metadata: Some(ResponseUsageMetadata {
            amount: None,
            metadata: Some(usage),
        }),
    }
}

#[tokio::test]
async fn litellm_costs_persist_and_deduplicate_per_thread() -> anyhow::Result<()> {
    let home = TempDir::new()?;
    let sqlite = SqliteConfig::from_sqlite_home(home.path().try_into()?);
    let thread = ThreadId::new();
    let other = ThreadId::new();
    assert_eq!(thread_usd_micros(&sqlite, thread).await?, None);
    for (thread, id, cost) in [
        (thread, "response-1", 0.125),
        (thread, "response-1", 0.125),
        (thread, "response-2", 1.125),
        (other, "response-1", 5.0),
    ] {
        record(&sqlite, thread, &response(id, json!({"cost": cost}))).await?;
    }
    assert_eq!(thread_usd_micros(&sqlite, thread).await?, Some(1_250_000));
    assert_eq!(thread_usd_micros(&sqlite, other).await?, Some(5_000_000));
    Ok(())
}

#[tokio::test]
async fn litellm_costs_distinguish_unknown_from_free_and_round_only_the_total() -> anyhow::Result<()>
{
    let home = TempDir::new()?;
    let sqlite = SqliteConfig::from_sqlite_home(home.path().try_into()?);
    let thread = ThreadId::new();
    for (index, usage) in [
        json!({}),
        json!({"cost": null}),
        json!({"cost": -1}),
        json!({"cost": "invalid"}),
        json!({"cost": 1e100}),
    ]
    .into_iter()
    .enumerate()
    {
        record(&sqlite, thread, &response(&index.to_string(), usage)).await?;
    }
    assert_eq!(thread_usd_micros(&sqlite, thread).await?, None);
    record(&sqlite, thread, &response("free", json!({"cost": 0}))).await?;
    assert_eq!(thread_usd_micros(&sqlite, thread).await?, Some(0));
    for id in ["tiny-1", "tiny-2", "tiny-3"] {
        record(&sqlite, thread, &response(id, json!({"cost": 0.0000002}))).await?;
    }
    assert_eq!(thread_usd_micros(&sqlite, thread).await?, Some(1));
    Ok(())
}
