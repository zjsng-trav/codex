use super::*;
use crate::chatwidget::ThreadUsageOutcome;
use codex_app_server_protocol::ThreadUsage;
use pretty_assertions::assert_eq;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

#[tokio::test]
async fn litellm_thread_cost_renders_without_enterprise_auth_and_hides_credits() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.config.model_provider_id = "litellm".to_string();
    chat.thread_id = Some(thread_id);
    chat.has_codex_backend_auth = false;
    chat.plan_type = None;
    chat.show_welcome_banner = false;
    chat.local_settings.tui.status_line = Some(vec![
        "estimated-thread-cost".to_string(),
        "thread-credits".to_string(),
    ]);
    chat.refresh_status_line();
    assert_eq!(status_line_text(&chat), None);
    let request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshThreadUsage {
            thread_id: requested,
            request_id,
        }) => {
            assert_eq!(requested, thread_id);
            request_id
        }
        event => panic!("expected LiteLLM thread cost request, got {event:?}"),
    };
    assert!(chat.finish_thread_usage_refresh(
        thread_id,
        request_id,
        Ok(ThreadUsageOutcome::Available(ThreadUsage {
            thread_id: thread_id.to_string(),
            estimated_usage_credits_micros: 0,
            estimated_usage_usd_micros: Some(1_250_000),
            groups: Vec::new(),
        })),
    ));
    assert_eq!(status_line_text(&chat), Some("~$1.25".to_string()));
    assert!(rx.try_recv().is_err());

    let width = 60;
    let mut terminal = Terminal::new(TestBackend::new(width, chat.desired_height(width)))
        .expect("create terminal");
    terminal
        .draw(|frame| chat.render(frame.area(), frame.buffer_mut()))
        .expect("render cost");
    assert_chatwidget_snapshot!(
        "litellm_thread_cost_footer",
        normalized_backend_snapshot(terminal.backend())
    );
}
