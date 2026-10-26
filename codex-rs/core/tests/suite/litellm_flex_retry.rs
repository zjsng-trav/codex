//! LiteLLM Flex recovery retains the requested tier and remains cancelable.

use anyhow::Result;
use codex_protocol::protocol::CodexErrorInfo;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::turn_input::TurnInputRequest;
use codex_protocol::user_input::UserInput;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_with_timeout;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::time::Duration;
use wiremock::ResponseTemplate;

#[test_case::test_case("http"; "http_capacity_error")]
#[test_case::test_case("error"; "sse_error")]
#[test_case::test_case("response.failed"; "sse_failed_response")]
#[tokio::test(flavor = "current_thread")]
async fn litellm_flex_recovers_without_changing_tier_or_input(transport: &str) -> Result<()> {
    skip_if_no_network!(Ok(()));
    let error = json!({"code": "flex_unavailable", "message": "No Flex capacity"});
    let failed = if transport == "http" {
        ResponseTemplate::new(429).set_body_json(json!({"error": error}))
    } else if transport == "error" {
        responses::sse_response(responses::sse(vec![
            json!({"type": "error", "error": error}),
        ]))
    } else {
        responses::sse_response(responses::sse_failed(
            "failed",
            "flex_unavailable",
            "No capacity",
        ))
    };
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_sequence(
        &server,
        vec![
            failed.clone(),
            failed,
            responses::sse_response(responses::sse(vec![
                responses::ev_response_created("recovered"),
                responses::ev_completed("recovered"),
            ])),
        ],
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider_id = "litellm".into();
            config.service_tier = Some("flex".into());
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(1);
        })
        .build_with_auto_env(&server)
        .await?;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "recover when Flex capacity returns".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    let mut retries = Vec::new();
    loop {
        // Flex backoff intentionally exceeds the ordinary event helper's ten-second timeout.
        match wait_for_event_with_timeout(&test.codex, |_| true, Duration::from_secs(25)).await {
            EventMsg::StreamError(error) => retries.push(error.message),
            EventMsg::Error(error) => panic!("unexpected terminal error: {error:?}"),
            EventMsg::TurnComplete(event) => {
                assert_eq!(event.error, None);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(
        retries,
        vec![
            "Flex capacity unavailable; retrying... attempt 1",
            "Flex capacity unavailable; retrying... attempt 2"
        ]
    );
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let input = requests[0].input();
    assert_eq!(
        requests
            .iter()
            .map(|r| (r.body_json()["service_tier"].clone(), r.input()))
            .collect::<Vec<_>>(),
        vec![
            (json!("flex"), input.clone()),
            (json!("flex"), input.clone()),
            (json!("flex"), input)
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn litellm_flex_retry_can_be_disabled() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_once(
        &server,
        responses::sse_response(responses::sse_failed(
            "failed",
            "flex_unavailable",
            "No capacity",
        )),
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider_id = "litellm".into();
            config.service_tier = Some("flex".into());
            config.model_provider.stream_max_retries = Some(0);
        })
        .build_with_auto_env(&server)
        .await?;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "surface the capacity error when automatic retries are disabled".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    let mut errors = Vec::new();
    loop {
        match wait_for_event(&test.codex, |_| true).await {
            EventMsg::StreamError(error) => panic!("retries are disabled: {error:?}"),
            EventMsg::Error(error) => errors.push(error.codex_error_info),
            EventMsg::TurnComplete(event) => {
                assert_eq!(
                    event.error.and_then(|e| e.codex_error_info),
                    Some(CodexErrorInfo::FlexUnavailable)
                );
                break;
            }
            _ => {}
        }
    }
    assert_eq!(errors, vec![Some(CodexErrorInfo::FlexUnavailable)]);
    assert_eq!(
        mock.single_request().body_json()["service_tier"],
        json!("flex")
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn litellm_flex_backoff_can_be_interrupted() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_once(
        &server,
        responses::sse_response(responses::sse_failed(
            "failed",
            "flex_unavailable",
            "No capacity",
        )),
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider_id = "litellm".into();
            config.service_tier = Some("flex".into());
        })
        .build_with_auto_env(&server)
        .await?;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "cancel while waiting for Flex capacity".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    wait_for_event(&test.codex, |e| matches!(e, EventMsg::StreamError(_))).await;
    test.codex.submit(Op::Interrupt).await?;
    wait_for_event(&test.codex, |e| matches!(e, EventMsg::TurnAborted(_))).await;
    test.codex.submit(Op::Shutdown).await?;
    wait_for_event(&test.codex, |e| matches!(e, EventMsg::ShutdownComplete)).await;
    assert_eq!(
        mock.single_request().body_json()["service_tier"],
        json!("flex")
    );
    Ok(())
}
