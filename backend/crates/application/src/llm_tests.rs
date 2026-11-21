//! Tests of `domain::llm::collect_response`, here because they need an async
//! runtime and the domain crate has none.

use std::collections::VecDeque;

use async_trait::async_trait;
use domain::llm::{LlmError, LlmEvent, LlmEventStream, Part, StopReason, Usage, collect_response};

struct Scripted(VecDeque<Result<LlmEvent, LlmError>>);

#[async_trait]
impl LlmEventStream for Scripted {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        self.0.pop_front()
    }
}

fn scripted(events: Vec<Result<LlmEvent, LlmError>>) -> Box<dyn LlmEventStream> {
    Box::new(Scripted(events.into()))
}

fn usage(n: u32) -> Usage {
    Usage {
        input_tokens: n,
        ..Usage::default()
    }
}

#[tokio::test]
async fn collects_text_tool_calls_and_blocks_in_block_order() {
    let response = collect_response(scripted(vec![
        Ok(LlmEvent::Started {
            model: "m".into(),
            request_id: Some("req_1".into()),
        }),
        Ok(LlmEvent::ProviderBlock {
            block: 0,
            provider: "anthropic".into(),
            payload: serde_json::json!({"type": "thinking"}),
        }),
        Ok(LlmEvent::TextDelta {
            block: 1,
            text: "Ciao ".into(),
        }),
        Ok(LlmEvent::TextDelta {
            block: 1,
            text: "mondo".into(),
        }),
        Ok(LlmEvent::ToolCallStarted {
            block: 2,
            id: "t1".into(),
            name: "f".into(),
        }),
        Ok(LlmEvent::ToolCallFinished {
            block: 2,
            id: "t1".into(),
            name: "f".into(),
            input: serde_json::json!({"a": 1}),
        }),
        Ok(LlmEvent::Usage(usage(5))),
        Ok(LlmEvent::Finished {
            stop_reason: StopReason::ToolUse,
            usage: usage(7),
        }),
    ]))
    .await
    .unwrap();
    assert_eq!(response.text(), "Ciao mondo");
    assert_eq!(response.message.parts.len(), 3);
    assert!(matches!(
        response.message.parts[0],
        Part::ProviderBlock { .. }
    ));
    assert!(matches!(response.message.parts[2], Part::ToolUse { .. }));
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(response.usage, usage(7));
    assert_eq!(
        (response.model.as_str(), response.request_id.as_deref()),
        ("m", Some("req_1"))
    );
}

#[tokio::test]
async fn stream_errors_and_missing_finish_are_errors() {
    let err = collect_response(scripted(vec![
        Ok(LlmEvent::TextDelta {
            block: 0,
            text: "a".into(),
        }),
        Err(LlmError::Timeout),
    ]))
    .await
    .unwrap_err();
    assert_eq!(err, LlmError::Timeout);

    let err = collect_response(scripted(vec![Ok(LlmEvent::TextDelta {
        block: 0,
        text: "a".into(),
    })]))
    .await
    .unwrap_err();
    assert!(matches!(err, LlmError::MalformedResponse(_)));
}

#[tokio::test]
async fn server_tool_activity_is_reported_apart_from_the_replayable_blocks() {
    let response = collect_response(scripted(vec![
        Ok(LlmEvent::ServerToolUse {
            block: 0,
            name: "web_search".into(),
            input: serde_json::json!({ "query": "rust" }),
        }),
        Ok(LlmEvent::ProviderBlock {
            block: 0,
            provider: "anthropic".into(),
            payload: serde_json::json!({ "type": "server_tool_use" }),
        }),
        Ok(LlmEvent::ServerSearchResults {
            block: 1,
            urls: vec![
                "https://a.example.org".into(),
                "https://b.example.org".into(),
            ],
        }),
        Ok(LlmEvent::Finished {
            stop_reason: StopReason::PauseTurn,
            usage: Usage {
                web_searches: 1,
                ..Usage::default()
            },
        }),
    ]))
    .await
    .unwrap();
    assert_eq!(response.stop_reason, StopReason::PauseTurn);
    assert_eq!(response.server_tool_uses[0].query(), Some("rust"));
    assert_eq!(response.search_result_urls.len(), 2);
    assert_eq!(response.usage.web_searches, 1);
    // Only the replayable block is part of the message.
    assert_eq!(response.message.parts.len(), 1);
    assert!(matches!(
        response.message.parts[0],
        Part::ProviderBlock { .. }
    ));
}
