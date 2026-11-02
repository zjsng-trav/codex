use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::AccountTokenUsageSummary;
use codex_app_server_protocol::GetAccountTokenUsageResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadUsage;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(/*secs*/ 60);

#[tokio::test]
async fn litellm_thread_cost_uses_reported_usage_and_survives_server_restart() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mut completed = responses::ev_completed("resp-cost");
    completed["response"]["usage"] = json!({
        "input_tokens": 10,
        "output_tokens": 5,
        "total_tokens": 15,
        "cost": 1.25,
    });
    let mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-cost"),
            responses::ev_assistant_message("msg-cost", "Done"),
            completed,
        ]),
    )
    .await;
    let home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .with_model_provider("litellm")
        .with_provider_config("requires_openai_auth = false\nsupports_websockets = false")
        .write(home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized_with_timeout(TIMEOUT)
        .await?;
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams::default())
        .await?;
    let started: ThreadStartResponse = timeout(TIMEOUT, app.read_response(request)).await??;
    let thread_id = started.thread.id;
    let request = app
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: "Hi".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let _: TurnStartResponse = timeout(TIMEOUT, app.read_response(request)).await??;
    timeout(
        TIMEOUT,
        app.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let _request = mock.single_request();

    let expected = GetAccountTokenUsageResponse {
        summary: AccountTokenUsageSummary {
            lifetime_tokens: None,
            peak_daily_tokens: None,
            longest_running_turn_sec: None,
            current_streak_days: None,
            longest_streak_days: None,
        },
        daily_usage_buckets: None,
        thread_usage: Some(ThreadUsage {
            thread_id: thread_id.clone(),
            estimated_usage_credits_micros: 0,
            estimated_usage_usd_micros: Some(1_250_000),
            native_usage_usd_micros: None,
            groups: Vec::new(),
        }),
    };
    let request = app
        .send_raw_request("account/usage/read", Some(json!({"threadId":thread_id})))
        .await?;
    let result: GetAccountTokenUsageResponse =
        timeout(TIMEOUT, app.read_response(request)).await??;
    assert_eq!(result, expected);
    app.shutdown_gracefully().await?;

    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized_with_timeout(TIMEOUT)
        .await?;
    let request = app
        .send_raw_request("account/usage/read", Some(json!({"threadId":thread_id})))
        .await?;
    let result: GetAccountTokenUsageResponse =
        timeout(TIMEOUT, app.read_response(request)).await??;
    assert_eq!(result, expected);
    app.shutdown_gracefully().await?;
    Ok(())
}
