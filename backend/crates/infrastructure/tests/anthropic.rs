//! Anthropic adapter against recorded SSE/JSON fixtures served by a local
//! HTTP server. No test touches the network beyond 127.0.0.1.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use domain::llm::{
    LlmError, LlmEvent, LlmMessage, LlmProvider, LlmRequest, LlmRole, MessageRole, Part,
    StopReason, SystemBlock, ToolDef, Usage,
};
use infrastructure::anthropic::openrouter_model_id;
use infrastructure::{AnthropicConfig, AnthropicProvider, ApiAuth, Secret};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("fixtures/anthropic/", $name))
    };
}

// ---------- a tiny scripted HTTP server ----------

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<Vec<u8>>,
    /// Keep the connection open (silent) after the body.
    hang: bool,
}

impl Reply {
    fn sse(body: &str) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("content-type", "text/event-stream".into()),
                ("request-id", "req_011Test".into()),
            ],
            body: vec![body.as_bytes().to_vec()],
            hang: false,
        }
    }

    fn error(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type", "application/json".into())],
            body: vec![body.as_bytes().to_vec()],
            hang: false,
        }
    }

    fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_owned()));
        self
    }

    fn chunked(mut self, size: usize) -> Self {
        let all: Vec<u8> = self.body.concat();
        self.body = all.chunks(size).map(<[u8]>::to_vec).collect();
        self
    }

    fn hang(mut self) -> Self {
        self.hang = true;
        self
    }
}

#[derive(Debug, Clone)]
struct Captured {
    headers: Vec<(String, String)>,
    body: Value,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

struct Server {
    addr: SocketAddr,
    captured: Arc<Mutex<Vec<Captured>>>,
}

async fn serve(replies: Vec<Reply>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let queue = Arc::new(Mutex::new(VecDeque::from(replies)));
    let seen = captured.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let (seen, queue) = (seen.clone(), queue.clone());
            tokio::spawn(async move {
                let request = read_request(&mut socket).await;
                seen.lock().unwrap().push(request);
                let Some(reply) = queue.lock().unwrap().pop_front() else {
                    return;
                };
                let mut head = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", reply.status);
                for (n, v) in &reply.headers {
                    write!(head, "{n}: {v}\r\n").unwrap();
                }
                head.push_str("\r\n");
                let _ = socket.write_all(head.as_bytes()).await;
                for chunk in &reply.body {
                    let _ = socket.write_all(chunk).await;
                    let _ = socket.flush().await;
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                if reply.hang {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            });
        }
    });
    Server { addr, captured }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Captured {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let head_end = loop {
        let n = socket.read(&mut buf).await.unwrap();
        data.extend_from_slice(&buf[..n]);
        if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(n > 0, "client closed before sending headers");
    };
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let headers: Vec<(String, String)> = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_lowercase(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map_or(0, |(_, v)| v.parse().unwrap());
    while data.len() < head_end + length {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0, "client closed mid-body");
        data.extend_from_slice(&buf[..n]);
    }
    Captured {
        headers,
        body: serde_json::from_slice(&data[head_end..head_end + length]).unwrap(),
    }
}

// ---------- helpers ----------

const KEY: &str = "sk-ant-test-0123456789";

fn provider(server: &Server, tweak: impl FnOnce(&mut AnthropicConfig)) -> AnthropicProvider {
    let mut config = AnthropicConfig {
        api_key: Secret::new(KEY),
        base_url: format!("http://{}", server.addr),
        ..AnthropicConfig::default()
    };
    tweak(&mut config);
    AnthropicProvider::new(config).unwrap()
}

fn request() -> LlmRequest {
    let mut r = LlmRequest::new(LlmRole::Tutor, "test");
    r.system = vec![SystemBlock {
        text: "Sei il Tutor".into(),
        cache_breakpoint: true,
    }];
    r.messages = vec![LlmMessage::text(MessageRole::User, "Voglio imparare Rust")];
    r
}

async fn events(provider: &AnthropicProvider) -> (Vec<LlmEvent>, Option<LlmError>) {
    let mut stream = provider.stream(request()).await.unwrap();
    let mut out = Vec::new();
    while let Some(item) = stream.next().await {
        match item {
            Ok(e) => out.push(e),
            Err(e) => return (out, Some(e)),
        }
    }
    (out, None)
}

async fn open_error(replies: Vec<Reply>) -> LlmError {
    let server = serve(replies).await;
    match provider(&server, |_| {}).stream(request()).await {
        Ok(_) => panic!("expected the request to be refused"),
        Err(e) => e,
    }
}

fn text_of(events: &[LlmEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            LlmEvent::TextDelta { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

// ---------- streaming ----------

#[tokio::test]
async fn simple_text_stream_yields_deltas_usage_and_finish() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let (events, error) = events(&provider(&server, |_| {})).await;
    assert!(error.is_none());
    assert_eq!(
        events[0],
        LlmEvent::Started {
            model: "claude-sonnet-5-5".into(),
            request_id: Some("req_011Test".into())
        }
    );
    assert_eq!(text_of(&events), "Ciao! Perché vuoi imparare Rust?");
    let usage = Usage {
        input_tokens: 120,
        output_tokens: 14,
        thinking_tokens: 0,
        cache_read_tokens: 80,
        cache_write_tokens: 0,
        web_searches: 0,
        web_fetches: 0,
    };
    assert_eq!(
        events.last().unwrap(),
        &LlmEvent::Finished {
            stop_reason: StopReason::EndTurn,
            usage
        }
    );
}

#[tokio::test]
async fn the_stream_survives_arbitrary_network_chunking() {
    for size in [1, 7, 64] {
        let server = serve(vec![Reply::sse(fixture!("text_simple.sse")).chunked(size)]).await;
        let (events, error) = events(&provider(&server, |_| {})).await;
        assert!(error.is_none(), "chunk size {size}");
        assert_eq!(text_of(&events), "Ciao! Perché vuoi imparare Rust?");
    }
}

#[tokio::test]
async fn thinking_text_and_tool_use_are_translated() {
    let server = serve(vec![Reply::sse(fixture!("thinking_and_tool_use.sse"))]).await;
    let (events, error) = events(&provider(&server, |_| {})).await;
    assert!(error.is_none());

    let thinking = events
        .iter()
        .find_map(|e| match e {
            LlmEvent::ProviderBlock {
                block: 0,
                provider,
                payload,
            } => Some((provider.clone(), payload.clone())),
            _ => None,
        })
        .expect("thinking block");
    assert_eq!(thinking.0, "anthropic");
    assert_eq!(
        thinking.1,
        json!({"type":"thinking","thinking":"L'utente vuole salvare.","signature":"EqQBsig"})
    );
    // Thinking text never shows up as user-visible text.
    assert_eq!(text_of(&events), "Salvo.");

    assert!(events.contains(&LlmEvent::ToolCallStarted {
        block: 2,
        id: "toolu_01A".into(),
        name: "save_note".into()
    }));
    let fragments: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            LlmEvent::ToolCallInputDelta { partial_json, .. } => Some(partial_json.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fragments.concat(), r#"{"title": "Rust", "n": 3}"#);
    assert!(events.contains(&LlmEvent::ToolCallFinished {
        block: 2,
        id: "toolu_01A".into(),
        name: "save_note".into(),
        input: json!({"title": "Rust", "n": 3})
    }));
    assert!(matches!(
        events.last().unwrap(),
        LlmEvent::Finished {
            stop_reason: StopReason::ToolUse,
            ..
        }
    ));
}

#[tokio::test]
async fn complete_collects_blocks_in_order_for_the_default_impl() {
    let server = serve(vec![Reply::sse(fixture!("thinking_and_tool_use.sse"))]).await;
    let response = provider(&server, |_| {}).complete(request()).await.unwrap();
    assert_eq!(response.text(), "Salvo.");
    assert!(matches!(
        response.message.parts[0],
        Part::ProviderBlock { .. }
    ));
    assert!(matches!(response.message.parts[2], Part::ToolUse { .. }));
    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert_eq!(response.model, "claude-sonnet-5-5");
}

#[tokio::test]
async fn max_tokens_and_refusal_are_stop_reasons_not_errors() {
    for (file, expected) in [
        (fixture!("max_tokens.sse"), StopReason::MaxTokens),
        (fixture!("refusal.sse"), StopReason::Refusal),
    ] {
        let server = serve(vec![Reply::sse(file)]).await;
        let (events, error) = events(&provider(&server, |_| {})).await;
        assert!(error.is_none());
        assert!(matches!(
            events.last().unwrap(),
            LlmEvent::Finished { stop_reason, .. } if *stop_reason == expected
        ));
    }
}

#[tokio::test]
async fn error_event_midstream_keeps_earlier_text_and_ends_with_the_error() {
    let server = serve(vec![Reply::sse(fixture!("error_midstream.sse"))]).await;
    let (events, error) = events(&provider(&server, |_| {})).await;
    assert_eq!(text_of(&events), "Perché ");
    assert_eq!(error, Some(LlmError::Overloaded));
}

#[tokio::test]
async fn connection_closed_before_message_stop_is_unavailable() {
    let server = serve(vec![Reply::sse(fixture!("truncated_no_stop.sse"))]).await;
    let (events, error) = events(&provider(&server, |_| {})).await;
    assert_eq!(text_of(&events), "Perché ");
    assert_eq!(error, Some(LlmError::Unavailable));
}

#[tokio::test]
async fn unparsable_tool_input_is_a_non_object_call_and_the_cut_stream_still_fails() {
    let server = serve(vec![Reply::sse(fixture!("bad_tool_json.sse"))]).await;
    let (events, error) = events(&provider(&server, |_| {})).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, LlmEvent::ToolCallFinished { input, .. } if !input.is_object()))
    );
    // The fixture ends without `message_stop`.
    assert_eq!(error, Some(LlmError::Unavailable));
}

#[tokio::test]
async fn silence_on_an_open_stream_times_out() {
    // The first event arrives, then nothing.
    let head = fixture!("text_simple.sse")
        .split("event: ping")
        .next()
        .unwrap();
    let server = serve(vec![Reply::sse(head).hang()]).await;
    let provider = provider(&server, |c| {
        c.stream_idle_timeout = Duration::from_millis(150);
    });
    let (events, error) = events(&provider).await;
    assert!(matches!(events[0], LlmEvent::Started { .. }));
    assert_eq!(error, Some(LlmError::Timeout));
}

#[tokio::test]
async fn headers_that_never_arrive_time_out() {
    // Accepts the request and never answers.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let provider = AnthropicProvider::new(AnthropicConfig {
        api_key: Secret::new(KEY),
        base_url: format!("http://{addr}"),
        request_timeout: Duration::from_millis(150),
        ..AnthropicConfig::default()
    })
    .unwrap();
    assert!(matches!(
        provider.stream(request()).await,
        Err(LlmError::Timeout)
    ));
}

// ---------- refused requests ----------

#[tokio::test]
async fn http_errors_are_classified() {
    let limited = open_error(vec![
        Reply::error(429, fixture!("rate_limit_429.json")).header("retry-after", "7"),
    ])
    .await;
    assert_eq!(
        limited,
        LlmError::RateLimited {
            retry_after: Some(Duration::from_secs(7))
        }
    );
    let no_header = open_error(vec![Reply::error(429, fixture!("rate_limit_429.json"))]).await;
    assert_eq!(no_header, LlmError::RateLimited { retry_after: None });

    assert_eq!(
        open_error(vec![Reply::error(529, fixture!("overloaded_529.json"))]).await,
        LlmError::Overloaded
    );
    for status in [401, 403] {
        assert_eq!(
            open_error(vec![Reply::error(status, fixture!("auth_401.json"))]).await,
            LlmError::Authentication
        );
    }
    assert_eq!(
        open_error(vec![Reply::error(402, fixture!("auth_401.json"))]).await,
        LlmError::CreditExhausted
    );
    assert_eq!(
        open_error(vec![Reply::error(408, "{}")]).await,
        LlmError::Timeout
    );
    for status in [500, 502, 503, 504] {
        assert_eq!(
            open_error(vec![Reply::error(status, "{}")]).await,
            LlmError::Unavailable
        );
    }
    assert_eq!(
        open_error(vec![Reply::error(
            400,
            fixture!("prompt_too_long_400.json")
        )])
        .await,
        LlmError::ContextTooLong
    );
    assert_eq!(
        open_error(vec![Reply::error(413, "{}")]).await,
        LlmError::ContextTooLong
    );
    assert!(matches!(
        open_error(vec![Reply::error(400, fixture!("invalid_request_400.json"))]).await,
        LlmError::InvalidRequest(m) if m.contains("temperature")
    ));
    assert!(matches!(
        open_error(vec![Reply::error(404, "not json at all")]).await,
        LlmError::InvalidRequest(_)
    ));
}

#[tokio::test]
async fn connection_refused_is_a_network_error_without_the_key() {
    // Nothing listens on this port.
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let provider = AnthropicProvider::new(AnthropicConfig {
        api_key: Secret::new(KEY),
        base_url: format!("http://127.0.0.1:{port}"),
        ..AnthropicConfig::default()
    })
    .unwrap();
    let Err(error) = provider.stream(request()).await else {
        panic!("expected a network error");
    };
    assert!(matches!(error, LlmError::Network(_)), "{error:?}");
    assert!(!error.to_string().contains(KEY));
}

#[tokio::test]
async fn without_a_key_no_request_is_sent() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let provider = provider(&server, |c| c.api_key = None);
    assert!(matches!(
        provider.stream(request()).await,
        Err(LlmError::Authentication)
    ));
    assert!(server.captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_server_without_a_key_sends_the_key_of_the_request() {
    let server = serve(vec![
        Reply::sse(fixture!("text_simple.sse")),
        Reply::sse(fixture!("text_simple.sse")),
    ])
    .await;
    let provider = provider(&server, |c| c.api_key = None);
    let mut mine = request();
    mine.api_key = Secret::new("sk-of-the-user");
    let mut stream = provider.stream(mine).await.unwrap();
    while stream.next().await.is_some() {}
    let mut other = request();
    other.api_key = Secret::new("sk-of-someone-else");
    let mut stream = provider.stream(other).await.unwrap();
    while stream.next().await.is_some() {}

    let sent = server.captured.lock().unwrap().clone();
    assert_eq!(sent[0].header("x-api-key"), Some("sk-of-the-user"));
    assert_eq!(sent[1].header("x-api-key"), Some("sk-of-someone-else"));
}

#[tokio::test]
async fn a_key_that_cannot_be_a_header_is_a_rejected_key_not_a_panic() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let provider = provider(&server, |c| c.api_key = None);
    let mut req = request();
    req.api_key = Secret::new("sk-bad\nkey");
    assert!(matches!(
        provider.stream(req).await,
        Err(LlmError::Authentication)
    ));
    assert!(server.captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_fixed_key_of_the_adapter_wins_over_the_request_key() {
    // The eval runner has one credential for everything it does.
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let mut req = request();
    req.api_key = Secret::new("sk-placeholder");
    let mut stream = provider(&server, |_| {}).stream(req).await.unwrap();
    while stream.next().await.is_some() {}
    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(sent.header("x-api-key"), Some(KEY));
}

// ---------- what we send ----------

#[tokio::test]
async fn request_carries_the_documented_headers_and_body() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let mut req = request();
    req.messages[0].cache_breakpoint = true;
    req.tools = vec![ToolDef::Function {
        name: "save_note".into(),
        description: "Saves a note".into(),
        input_schema: json!({"type": "object"}),
    }];
    let mut stream = provider(&server, |_| {}).stream(req).await.unwrap();
    while stream.next().await.is_some() {}

    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(sent.header("x-api-key"), Some(KEY));
    assert_eq!(sent.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(sent.header("content-type"), Some("application/json"));
    assert_eq!(sent.header("anthropic-beta"), None);

    let body = &sent.body;
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 16000);
    assert_eq!(body["output_config"], json!({"effort": "low"}));
    assert_eq!(body["system"][0]["text"], "Sei il Tutor");
    assert_eq!(
        body["system"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );
    assert_eq!(
        body["messages"][0]["content"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );
    assert_eq!(body["tools"][0]["name"], "save_note");
    assert_eq!(body["tool_choice"], json!({"type": "auto"}));
    for forbidden in ["temperature", "top_p", "top_k", "thinking", "fallbacks"] {
        assert!(body.get(forbidden).is_none(), "{forbidden} was sent");
    }
}

#[tokio::test]
async fn model_comes_from_the_role_configuration() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let provider = provider(&server, |c| {
        let tutor = c.roles.get_mut(&LlmRole::Tutor).unwrap();
        tutor.model = "claude-haiku-5-5".into();
        tutor.effort = None;
        tutor.max_output_tokens = 100;
    });
    let mut stream = provider.stream(request()).await.unwrap();
    while stream.next().await.is_some() {}
    let body = server.captured.lock().unwrap()[0].body.clone();
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert_eq!(body["max_tokens"], 100);
    assert!(body.get("output_config").is_none());
}

#[tokio::test]
async fn refusal_fallback_sends_beta_header_and_field_only_when_enabled() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let mut stream = provider(&server, |c| c.refusal_fallback = true)
        .stream(request())
        .await
        .unwrap();
    while stream.next().await.is_some() {}
    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(
        sent.header("anthropic-beta"),
        Some("server-side-fallback-2026-07-01")
    );
    assert_eq!(sent.body["fallbacks"], "default");
}

#[tokio::test]
async fn bearer_auth_sends_authorization_and_no_x_api_key() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let provider = provider(&server, |c| c.auth = ApiAuth::Bearer);
    let (_, error) = events(&provider).await;
    assert!(error.is_none());
    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(
        sent.header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
    assert_eq!(sent.header("x-api-key"), None);
}

#[test]
fn openrouter_slugs_come_from_anthropic_ids() {
    assert_eq!(
        openrouter_model_id("claude-sonnet-5-5"),
        "anthropic/claude-sonnet-5.5"
    );
    assert_eq!(
        openrouter_model_id("claude-opus-5"),
        "anthropic/claude-opus-5"
    );
    assert_eq!(
        openrouter_model_id("anthropic/claude-opus-5"),
        "anthropic/claude-opus-5"
    );
}

#[tokio::test]
async fn base_url_with_trailing_slash_works() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let provider = provider(&server, |c| c.base_url.push('/'));
    let (events, error) = events(&provider).await;
    assert!(error.is_none() && !events.is_empty());
}

#[test]
fn config_debug_never_prints_the_key_and_defaults_cover_every_role() {
    let config = AnthropicConfig {
        api_key: Secret::new(KEY),
        ..AnthropicConfig::default()
    };
    assert!(!format!("{config:?}").contains(KEY));
    assert!(AnthropicProvider::new(config.clone()).is_ok());
    let mut missing = config;
    missing.roles.remove(&LlmRole::Grader);
    assert!(AnthropicProvider::new(missing).is_err());
}

#[tokio::test]
async fn capabilities_report_the_cap_actually_sent_for_the_role() {
    let server = serve(vec![]).await;
    let provider = provider(&server, |c| {
        c.roles.get_mut(&LlmRole::Grader).unwrap().max_output_tokens = 1234;
    });
    assert_eq!(
        provider.capabilities(LlmRole::Grader).max_output_tokens,
        1234
    );
    assert_eq!(
        provider.capabilities(LlmRole::Tutor).max_output_tokens,
        16_000
    );
}

#[tokio::test]
async fn the_generator_defaults_to_low_effort_and_a_32k_cap() {
    let server = serve(vec![]).await;
    let provider = provider(&server, |_| {});
    let caps = provider.capabilities(LlmRole::Generator);
    assert_eq!(caps.max_output_tokens, 32_000);
    assert_eq!(caps.effort.as_deref(), Some("low"));
    // The Planner keeps `medium`.
    assert_eq!(
        provider.capabilities(LlmRole::Planner).effort.as_deref(),
        Some("medium")
    );
}

#[tokio::test]
async fn redirects_are_not_followed_so_the_key_cannot_leave() {
    let target = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let redirect = Reply {
        status: 307,
        headers: vec![("location", format!("http://{}/v1/messages", target.addr))],
        body: vec![],
        hang: false,
    };
    let origin = serve(vec![redirect]).await;
    let result = provider(&origin, |_| {}).stream(request()).await;
    assert!(matches!(result, Err(LlmError::InvalidRequest(_))));
    assert!(target.captured.lock().unwrap().is_empty());
}

// ---------- server-side web search (llm-port.md §12) ----------

fn research_request() -> LlmRequest {
    let mut r = LlmRequest::new(LlmRole::Researcher, "test.research");
    r.system = vec![SystemBlock {
        text: "Sei il Researcher".into(),
        cache_breakpoint: false,
    }];
    r.messages = vec![LlmMessage::text(MessageRole::User, "Procedi")];
    r.tools = vec![ToolDef::ServerWebSearch {
        max_uses: 4,
        allowed_domains: vec![],
        blocked_domains: vec![],
    }];
    r
}

fn json_reply(body: &str) -> Reply {
    Reply {
        status: 200,
        headers: vec![
            ("content-type", "application/json".into()),
            ("request-id", "req_011Search".into()),
        ],
        body: vec![body.as_bytes().to_vec()],
        hang: false,
    }
}

#[tokio::test]
async fn a_search_request_is_not_streamed_and_its_answer_keeps_every_block() {
    let server = serve(vec![json_reply(fixture!("web_search_end_turn.json"))]).await;
    let response = provider(&server, |_| {})
        .complete(research_request())
        .await
        .unwrap();

    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(sent.body["stream"], false);
    assert_eq!(sent.header("accept"), Some("application/json"));
    assert_eq!(
        sent.body["tools"][0],
        json!({ "type": "web_search_20250305", "name": "web_search", "max_uses": 4 })
    );
    assert!(sent.body["tools"][0].get("eager_input_streaming").is_none());

    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert_eq!(response.request_id.as_deref(), Some("req_011Search"));
    assert_eq!(response.model, "anthropic/claude-haiku-5.5");
    // Extra fields of OpenRouter (cost, is_byok...) are ignored; the rest counts.
    assert_eq!(response.usage.input_tokens, 28389);
    assert_eq!(response.usage.output_tokens, 2169);
    assert_eq!(response.usage.thinking_tokens, 868);
    assert_eq!(response.usage.web_searches, 1);

    assert_eq!(response.server_tool_uses.len(), 1);
    assert_eq!(
        response.server_tool_uses[0].query(),
        Some("Rust ownership borrowing references")
    );
    assert_eq!(response.search_result_urls.len(), 3);
    assert!(response.search_result_urls.contains(
        &"https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html".to_owned()
    ));

    // thinking, server_tool_use, web_search_tool_result, thinking, text
    let kinds: Vec<&str> = response
        .message
        .parts
        .iter()
        .map(|p| match p {
            Part::ProviderBlock { payload, .. } => payload["type"].as_str().unwrap(),
            Part::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "thinking",
            "server_tool_use",
            "web_search_tool_result",
            "thinking",
            "text"
        ]
    );
    let text = response.text();
    let answer: Value = serde_json::from_str(&text).unwrap();
    assert!(answer["resources"].is_array());
}

#[tokio::test]
async fn a_paused_turn_is_continued_with_the_blocks_sent_back_unchanged() {
    let server = serve(vec![
        json_reply(fixture!("web_search_pause_turn.json")),
        json_reply(fixture!("web_search_end_turn.json")),
    ])
    .await;
    let provider = provider(&server, |_| {});
    let mut request = research_request();
    let paused = provider.complete(request.clone()).await.unwrap();
    assert_eq!(paused.stop_reason, StopReason::PauseTurn);
    assert_eq!(paused.usage.web_searches, 2);
    // A failed search is not an error: no URLs, the block is kept for the replay.
    assert_eq!(
        paused.search_result_urls,
        ["https://docs.rs/axum/latest/axum/"]
    );

    request.messages.push(paused.message.clone());
    provider.complete(request).await.unwrap();
    let sent = server.captured.lock().unwrap()[1].clone();
    let messages = sent.body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2, "no user message after the paused turn");
    assert_eq!(messages[1]["role"], "assistant");
    let original: Value = serde_json::from_str(fixture!("web_search_pause_turn.json")).unwrap();
    assert_eq!(messages[1]["content"], original["content"]);
}

#[tokio::test]
async fn a_search_request_has_its_own_longer_time_limit() {
    let server = serve(vec![json_reply("{}").hang()]).await;
    let slow = provider(&server, |c| {
        c.server_tool_timeout = Duration::from_millis(200);
        // The streaming limits do not apply to a search request.
        c.request_timeout = Duration::from_secs(60);
    });
    let started = std::time::Instant::now();
    let error = slow.complete(research_request()).await.unwrap_err();
    assert_eq!(error, LlmError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn search_request_errors_use_the_usual_mapping() {
    let server = serve(vec![
        Reply::error(429, fixture!("rate_limit_429.json")).header("retry-after", "7"),
    ])
    .await;
    let error = provider(&server, |_| {})
        .complete(research_request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        LlmError::RateLimited {
            retry_after: Some(Duration::from_secs(7))
        }
    );
    let server = serve(vec![json_reply("not json")]).await;
    assert!(matches!(
        provider(&server, |_| {}).complete(research_request()).await,
        Err(LlmError::MalformedResponse(_))
    ));
}

#[tokio::test]
async fn a_request_without_search_still_streams_through_complete() {
    let server = serve(vec![Reply::sse(fixture!("text_simple.sse"))]).await;
    let response = provider(&server, |_| {}).complete(request()).await.unwrap();
    assert_eq!(response.text(), "Ciao! Perché vuoi imparare Rust?");
    assert_eq!(server.captured.lock().unwrap()[0].body["stream"], true);
}

#[tokio::test]
async fn streamed_server_tools_become_events_and_replayable_blocks() {
    let server = serve(vec![Reply::sse(fixture!("web_search_stream.sse"))]).await;
    let response = domain::llm::collect_response(
        provider(&server, |_| {})
            .stream(research_request())
            .await
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.server_tool_uses[0].query(), Some("rust book"));
    assert_eq!(
        response.search_result_urls,
        ["https://doc.rust-lang.org/book/"]
    );
    assert_eq!(response.usage.web_searches, 1);
    let Part::ProviderBlock { payload, .. } = &response.message.parts[0] else {
        panic!("server_tool_use must be kept as a provider block");
    };
    assert_eq!(payload["input"], json!({ "query": "rust book" }));
    assert_eq!(response.text(), "Fatto.");
}

// ---------- server-side web fetch (llm-port.md §14, ADR 0009) ----------

fn fetch_request() -> LlmRequest {
    let mut r = LlmRequest::new(LlmRole::Generator, "test.fetch");
    r.system = vec![SystemBlock {
        text: "Sei il Generator".into(),
        cache_breakpoint: false,
    }];
    r.messages = vec![LlmMessage::text(MessageRole::User, "Procedi.")];
    r.tools = vec![ToolDef::ServerWebFetch {
        max_uses: 2,
        allowed_domains: vec!["doc.rust-lang.org".into()],
        max_content_tokens: 8000,
    }];
    r
}

#[tokio::test]
async fn a_fetch_request_is_not_streamed_and_its_answer_keeps_every_block() {
    let server = serve(vec![json_reply(fixture!("web_fetch_end_turn.json"))]).await;
    let response = provider(&server, |_| {})
        .complete(fetch_request())
        .await
        .unwrap();

    let sent = server.captured.lock().unwrap()[0].clone();
    assert_eq!(sent.body["stream"], false);
    assert_eq!(sent.body["tools"][0]["type"], "web_fetch_20250910");
    assert_eq!(sent.body["tools"][0]["name"], "web_fetch");
    assert!(sent.body["tools"][0].get("citations").is_none());
    // No beta header is sent for the tool (ADR 0009, open point 3).
    assert_eq!(sent.header("anthropic-beta"), None);

    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert_eq!(response.usage.web_fetches, 2);
    assert_eq!(response.usage.web_searches, 0);
    assert_eq!(response.server_tool_uses.len(), 2);
    assert_eq!(
        response.server_tool_uses[0].url(),
        Some("https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html".to_owned())
    );
    // A failed read is not an error of the call: it is listed for the logs and
    // the block stays in the message.
    assert_eq!(response.fetch_errors, ["url_not_allowed"]);

    let kinds: Vec<&str> = response
        .message
        .parts
        .iter()
        .map(|p| match p {
            Part::ProviderBlock { payload, .. } => payload["type"].as_str().unwrap(),
            Part::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "text",
            "server_tool_use",
            "web_fetch_tool_result",
            "server_tool_use",
            "web_fetch_tool_result",
            "text"
        ]
    );
    // The lesson is found by block type: the last text.
    assert_eq!(
        response.message.parts.last(),
        Some(&Part::Text("{\"title\": \"Ownership\"}".into()))
    );
}

#[tokio::test]
async fn a_paused_fetch_turn_is_sent_back_with_the_page_text_unchanged() {
    let server = serve(vec![
        json_reply(fixture!("web_fetch_pause_turn.json")),
        json_reply(fixture!("web_fetch_end_turn.json")),
    ])
    .await;
    let provider = provider(&server, |_| {});
    let mut request = fetch_request();
    let paused = provider.complete(request.clone()).await.unwrap();
    assert_eq!(paused.stop_reason, StopReason::PauseTurn);
    assert_eq!(paused.usage.web_fetches, 1);

    request.messages.push(paused.message.clone());
    provider.complete(request).await.unwrap();
    let sent = server.captured.lock().unwrap()[1].clone();
    let messages = sent.body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2, "no user message after the paused turn");
    let original: Value = serde_json::from_str(fixture!("web_fetch_pause_turn.json")).unwrap();
    assert_eq!(messages[1]["content"], original["content"]);
}

#[tokio::test]
async fn a_400_on_a_fetch_request_is_an_invalid_request_with_the_provider_text() {
    let server = serve(vec![Reply::error(
        400,
        fixture!("invalid_request_400.json"),
    )])
    .await;
    let error = provider(&server, |_| {})
        .complete(fetch_request())
        .await
        .unwrap_err();
    assert!(matches!(error, LlmError::InvalidRequest(_)), "{error:?}");
}

#[test]
fn the_fetch_capability_follows_the_configuration() {
    let server_addr = "http://127.0.0.1:1";
    let build = |on: bool| {
        let config = AnthropicConfig {
            base_url: server_addr.into(),
            server_web_fetch: on,
            ..AnthropicConfig::default()
        };
        AnthropicProvider::new(config).unwrap()
    };
    assert!(
        build(true)
            .capabilities(LlmRole::Generator)
            .server_web_fetch
    );
    assert!(
        !build(false)
            .capabilities(LlmRole::Generator)
            .server_web_fetch
    );
}

#[tokio::test]
async fn a_read_the_provider_does_not_report_is_counted_and_its_url_comes_from_the_result() {
    // OpenRouter shape (ADR 0009, real run of 2026-11-11): no
    // `web_fetch_requests` in the usage, and the request block has no url.
    let server = serve(vec![json_reply(fixture!("web_fetch_openrouter.json"))]).await;
    let response = provider(&server, |_| {})
        .complete(fetch_request())
        .await
        .unwrap();
    assert_eq!(response.usage.web_fetches, 1);
    assert_eq!(response.usage.web_searches, 0);
    assert_eq!(response.server_tool_uses[0].url(), None);
    let reads = response.web_fetch_reads();
    assert_eq!(reads.len(), 1);
    assert_eq!(
        reads[0].url.as_deref(),
        Some("https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html")
    );
}

#[tokio::test]
async fn a_402_for_exhausted_credit_is_reported_as_credit_exhausted_not_as_a_bad_key() {
    // OpenRouter: HTTP 402 with the reason in `error.message`.
    let body = r#"{"error":{"code":402,"message":"Insufficient credits. Add more using https://openrouter.ai/credits"}}"#;
    let server = serve(vec![Reply::error(402, body)]).await;
    let error = provider(&server, |_| {})
        .complete(fetch_request())
        .await
        .unwrap_err();
    assert_eq!(error, LlmError::CreditExhausted);
}
