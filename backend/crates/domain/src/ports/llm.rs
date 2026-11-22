//! Port towards language models (ADR 0004; design in `backend/prompts/llm-port.md`).
//!
//! One request, one assistant message back: the tool loop lives in the
//! application layer. Nothing here is specific to a provider; what is
//! provider-specific travels as an opaque [`Part::ProviderBlock`].

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::secret::Secret;

/// Agent roles (`docs/mvp.md` §4). The model and its parameters are resolved
/// from the role by configuration; requests never name a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LlmRole {
    Tutor,
    Interviewer,
    Researcher,
    Planner,
    Generator,
    Grader,
    Recorder,
}

impl LlmRole {
    pub const ALL: [Self; 7] = [
        Self::Tutor,
        Self::Interviewer,
        Self::Researcher,
        Self::Planner,
        Self::Generator,
        Self::Grader,
        Self::Recorder,
    ];

    /// Key used in configuration and logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tutor => "tutor",
            Self::Interviewer => "interviewer",
            Self::Researcher => "researcher",
            Self::Planner => "planner",
            Self::Generator => "generator",
            Self::Grader => "grader",
            Self::Recorder => "recorder",
        }
    }
}

// ---------- request ----------

#[derive(Debug, Clone, PartialEq)]
pub struct LlmRequest {
    pub role: LlmRole,
    /// In order, most stable first (cacheable prefix).
    pub system: Vec<SystemBlock>,
    /// Alternating user/assistant. The last one is from the user, except when
    /// continuing a turn the provider paused ([`StopReason::PauseTurn`]): then
    /// it is the assistant message to continue, sent back unchanged.
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolDef>,
    pub tool_choice: ToolChoice,
    pub output: OutputFormat,
    /// `None` = the role's configured default.
    pub max_output_tokens: Option<u32>,
    /// For logs and metrics only (e.g. `tutor.chat`). Never user content.
    pub trace_label: &'static str,
    /// The key to call the provider with, resolved by the use case from the
    /// owner of what the request is about (ADR 0008 §6). `None`: the
    /// adapter's own key, if it has one (the eval runner); a server built for
    /// users has none, so the call fails with [`LlmError::Authentication`].
    pub api_key: Option<Secret>,
}

impl LlmRequest {
    /// Text request with no tools; fill `system` and `messages` afterwards.
    #[must_use]
    pub fn new(role: LlmRole, trace_label: &'static str) -> Self {
        Self {
            role,
            system: Vec::new(),
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            output: OutputFormat::Text,
            max_output_tokens: None,
            trace_label,
            api_key: None,
        }
    }

    /// The same request, to be sent with `key`.
    #[must_use]
    pub fn with_key(mut self, key: Secret) -> Self {
        self.api_key = Some(key);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemBlock {
    pub text: String,
    /// Hint: "the prefix up to here is stable, cache it".
    pub cache_breakpoint: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub parts: Vec<Part>,
    /// Hint: cache breakpoint after this message.
    pub cache_breakpoint: bool,
}

impl LlmMessage {
    #[must_use]
    pub fn text(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            parts: vec![Part::Text(text.into())],
            cache_breakpoint: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Part {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// A block to send back to the provider uninterpreted (e.g. signed
    /// thinking). `provider` is the `kind` of the adapter that produced it;
    /// any other adapter drops it.
    ProviderBlock {
        provider: String,
        payload: Value,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolDef {
    Function {
        name: String,
        description: String,
        input_schema: Value,
    },
    /// Web search executed by the provider: the backend runs nothing, the
    /// results come back inside the assistant message (`llm-port.md` §12).
    ServerWebSearch {
        /// Ceiling per turn, also across `pause_turn` continuations.
        max_uses: u32,
        /// Bare domains (no scheme); exclusive with `blocked_domains`.
        allowed_domains: Vec<String>,
        blocked_domains: Vec<String>,
    },
    /// Page reading executed by the provider (`llm-port.md` §14, ADR 0009):
    /// the provider downloads the pages and the text comes back inside the
    /// assistant message. It only reads URLs that appeared in the
    /// conversation.
    ServerWebFetch {
        /// Ceiling of reads per request; a continuation of a paused turn is a
        /// new request, so the caller carries the budget over.
        max_uses: u32,
        /// Bare hosts (no scheme, port or path), lower case, ASCII (punycode
        /// for international names). Subdomains are included by the provider.
        allowed_domains: Vec<String>,
        /// Longest text taken from one page, in tokens.
        max_content_tokens: u32,
    },
}

impl ToolDef {
    #[must_use]
    pub fn is_server_web_search(&self) -> bool {
        matches!(self, Self::ServerWebSearch { .. })
    }

    #[must_use]
    pub fn is_server_web_fetch(&self) -> bool {
        matches!(self, Self::ServerWebFetch { .. })
    }

    /// A tool the provider runs by itself: the request is not streamed and a
    /// turn may pause (`llm-port.md` §12.6).
    #[must_use]
    pub fn is_server_tool(&self) -> bool {
        self.is_server_web_search() || self.is_server_web_fetch()
    }
}

/// No forced tool in v1: current Claude models reject it (ADR 0004).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolChoice {
    Auto,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Text,
    /// JSON conforming to the schema, natively or emulated by capability.
    JsonSchema {
        name: String,
        schema: Value,
    },
}

// ---------- response ----------

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    pub message: LlmMessage,
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// The model that actually answered (for logs).
    pub model: String,
    pub request_id: Option<String>,
    /// Searches the provider ran for this turn, in order (progress and logs;
    /// never sent back).
    pub server_tool_uses: Vec<ServerToolCall>,
    /// Every URL the provider's web searches returned in this turn.
    pub search_result_urls: Vec<String>,
    /// Error codes of the page reads that failed in this turn
    /// (`url_not_accessible`...). Not a failure of the call: the model sees
    /// them and goes on; they are for the logs.
    pub fetch_errors: Vec<String>,
}

/// A tool the provider ran by itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerToolCall {
    pub name: String,
    pub input: Value,
}

impl ServerToolCall {
    /// The search query, if this was a web search.
    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.input["query"].as_str()
    }

    /// The address read, if this was a web fetch. Same rule as
    /// [`web_fetch_reads`]: see [`input_url`].
    #[must_use]
    pub fn url(&self) -> Option<String> {
        input_url(&self.input)
    }
}

/// The address in the `input` of a `server_tool_use` block. Through
/// `OpenRouter` the shape of `input` is not known for sure: it may be the
/// usual object, a JSON string holding it, or empty. Anything else is `None`.
#[must_use]
pub fn input_url(input: &Value) -> Option<String> {
    let from_object = |v: &Value| v["url"].as_str().map(str::to_owned);
    let url = match input {
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .and_then(|v| from_object(&v))
            .or_else(|| s.trim().starts_with("http").then(|| s.trim().to_owned())),
        other => from_object(other),
    };
    url.filter(|u| !u.trim().is_empty())
}

/// One page read the provider ran, as seen in the blocks of a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebFetchRead {
    /// The address, `None` when neither the request block nor the result
    /// block carries a usable one.
    pub url: Option<String>,
}

/// The page reads in provider blocks (`server_tool_use` named `web_fetch`,
/// `web_fetch_tool_result`), in order. The single rule that counts reads and
/// names their addresses: the address comes from the request block and, when
/// that has none, from the `url` of the matching result; a result without a
/// request block is a read too.
pub fn web_fetch_reads<'a>(blocks: impl IntoIterator<Item = &'a Value>) -> Vec<WebFetchRead> {
    struct Result {
        id: Option<String>,
        url: Option<String>,
        matched: bool,
    }
    let mut uses: Vec<(Option<String>, Option<String>)> = Vec::new();
    let mut results: Vec<Result> = Vec::new();
    for block in blocks {
        let id = block["id"]
            .as_str()
            .or_else(|| block["tool_use_id"].as_str())
            .map(str::to_owned);
        match block["type"].as_str() {
            Some("server_tool_use") if block["name"] == "web_fetch" => {
                uses.push((id, input_url(&block["input"])));
            }
            Some("web_fetch_tool_result") => results.push(Result {
                id,
                url: block["content"]["url"]
                    .as_str()
                    .filter(|u| !u.trim().is_empty())
                    .map(str::to_owned),
                matched: false,
            }),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (id, own) in uses {
        let hit = results
            .iter_mut()
            .find(|r| !r.matched && (r.id.is_none() || id.is_none() || r.id == id));
        let from_result = hit.map(|r| {
            r.matched = true;
            r.url.clone()
        });
        out.push(WebFetchRead {
            url: own.or(from_result.flatten()),
        });
    }
    out.extend(
        results
            .into_iter()
            .filter(|r| !r.matched)
            .map(|r| WebFetchRead { url: r.url }),
    );
    out
}

impl LlmResponse {
    /// The page reads of this turn (see [`web_fetch_reads`]). Falls back to
    /// the `server_tool_uses` list when the parts carry no blocks.
    #[must_use]
    pub fn web_fetch_reads(&self) -> Vec<WebFetchRead> {
        let reads = web_fetch_reads(self.message.parts.iter().filter_map(|p| match p {
            Part::ProviderBlock { payload, .. } => Some(payload),
            _ => None,
        }));
        if !reads.is_empty() {
            return reads;
        }
        self.server_tool_uses
            .iter()
            .filter(|c| c.name == "web_fetch")
            .map(|c| WebFetchRead { url: c.url() })
            .collect()
    }

    /// Makes `usage.web_fetches` the number of reads this turn ran: the
    /// provider's figure or the reads seen in the blocks, whichever is
    /// larger. Some providers (`OpenRouter`) run the reads but do not report
    /// them. Call once, where a response is built; the figure is idempotent.
    /// Searches are left as reported.
    pub fn reconcile_fetch_count(&mut self) {
        let seen = self.web_fetch_reads().len().max(
            self.server_tool_uses
                .iter()
                .filter(|c| c.name == "web_fetch")
                .count(),
        );
        let seen = u32::try_from(seen).unwrap_or(u32::MAX);
        self.usage.web_fetches = self.usage.web_fetches.max(seen);
    }

    /// All text parts concatenated.
    #[must_use]
    pub fn text(&self) -> String {
        self.message
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Refusal,
    PauseTurn,
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// Non-cached input.
    pub input_tokens: u32,
    /// Includes thinking.
    pub output_tokens: u32,
    /// Reasoning tokens, already counted in `output_tokens` (0 if the
    /// provider does not report them).
    pub thinking_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    /// Server-side web searches run (`usage.server_tool_use`).
    pub web_searches: u32,
    /// Server-side page reads run (`usage.server_tool_use`).
    pub web_fetches: u32,
}

// ---------- streaming ----------

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LlmEvent {
    Started {
        model: String,
        request_id: Option<String>,
    },
    TextDelta {
        block: u32,
        text: String,
    },
    ToolCallStarted {
        block: u32,
        id: String,
        name: String,
    },
    /// Fragment of the tool input JSON; may be incomplete. For UI and logs only.
    ToolCallInputDelta {
        block: u32,
        partial_json: String,
    },
    /// Final input, already parsed. An unparsable input is reported as
    /// [`LlmError::MalformedResponse`] instead.
    ToolCallFinished {
        block: u32,
        id: String,
        name: String,
        input: Value,
    },
    ProviderBlock {
        block: u32,
        provider: String,
        payload: Value,
    },
    /// The provider ran a tool by itself (input already read, e.g.
    /// `{"query": "..."}`). For progress; never sent back (the replayable
    /// block travels as a [`LlmEvent::ProviderBlock`]).
    ServerToolUse {
        block: u32,
        name: String,
        input: Value,
    },
    /// URLs returned by a server-side web search, in the order received.
    ServerSearchResults {
        block: u32,
        urls: Vec<String>,
    },
    /// A server-side page read failed (provider error code, e.g.
    /// `url_not_accessible`). For logs; the model sees the error itself.
    ServerFetchError {
        block: u32,
        error_code: String,
    },
    /// Cumulative usage so far; may arrive more than once.
    Usage(Usage),
    Finished {
        stop_reason: StopReason,
        usage: Usage,
    },
}

impl LlmEvent {
    /// Whether the event carries content a user may already have seen: text or
    /// a tool call. Provider blocks (thinking) are not shown, so a stream that
    /// only produced those can still be retried safely.
    #[must_use]
    pub fn is_content(&self) -> bool {
        matches!(
            self,
            Self::TextDelta { .. }
                | Self::ToolCallStarted { .. }
                | Self::ToolCallInputDelta { .. }
                | Self::ToolCallFinished { .. }
        )
    }
}

/// Pull-style stream of events. Dropping it cancels the request.
#[async_trait]
pub trait LlmEventStream: Send {
    /// `None` after `Finished`. An `Err` ends the stream.
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>>;
}

// ---------- capabilities ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuredOutput {
    Native,
    Emulated,
    None,
}

/// Plain feature flags mirror the design document one to one.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmCapabilities {
    pub tools: bool,
    pub parallel_tool_calls: bool,
    pub forced_tool_choice: bool,
    pub structured_output: StructuredOutput,
    pub streaming_tool_input: bool,
    pub prompt_caching: bool,
    pub max_cache_breakpoints: u8,
    pub context_window_tokens: u32,
    pub max_output_tokens: u32,
    /// Reasoning effort the role runs with (`low`, `medium`...), for logs: a
    /// truncated answer is fixed by lowering it or raising the cap.
    pub effort: Option<String>,
    /// The provider can run web searches by itself ([`ToolDef::ServerWebSearch`]).
    pub server_web_search: bool,
    /// The provider can read web pages by itself ([`ToolDef::ServerWebFetch`]).
    pub server_web_fetch: bool,
}

// ---------- errors ----------

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LlmError {
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    /// 529 or equivalent.
    #[error("provider overloaded")]
    Overloaded,
    /// 5xx, or a connection dropped on an open stream.
    #[error("provider unavailable")]
    Unavailable,
    /// 401, 403.
    #[error("authentication failed")]
    Authentication,
    /// 402, or a billing error on an open stream: the key is fine but the
    /// account has no credit. Not retryable: waiting does not refill it.
    #[error("provider credit exhausted")]
    CreditExhausted,
    /// 400/404/422: our bug or a wrong configuration. May hold provider text:
    /// log it, never show it.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("context too long")]
    ContextTooLong,
    #[error("network: {0}")]
    Network(String),
    #[error("timeout")]
    Timeout,
    /// Unreadable SSE/JSON, or an unparsable tool input.
    #[error("malformed response: {0}")]
    MalformedResponse(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl LlmError {
    /// What may be logged about this error. Provider text and transport
    /// errors can echo the prompt, the model output or a URL with a key in
    /// its query, so only the `invalid request` text (needed to diagnose a
    /// wrong configuration) survives, cut short; the rest keeps its length.
    #[must_use]
    pub fn log_summary(&self) -> String {
        const MAX_PROVIDER_TEXT: usize = 200;
        match self {
            Self::InvalidRequest(text) => {
                let cut: String = text.chars().take(MAX_PROVIDER_TEXT).collect();
                let more = if cut.len() < text.len() { "..." } else { "" };
                format!("invalid request: {cut}{more}")
            }
            Self::Network(text) => {
                format!("network error ({} chars withheld)", text.chars().count())
            }
            Self::MalformedResponse(text) => {
                format!(
                    "malformed response ({} chars withheld)",
                    text.chars().count()
                )
            }
            Self::Unsupported(text) => {
                format!("unsupported ({} chars withheld)", text.chars().count())
            }
            other => other.to_string(),
        }
    }

    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. }
                | Self::Overloaded
                | Self::Unavailable
                | Self::Network(_)
                | Self::Timeout
        )
    }
}

// ---------- the port ----------

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Capabilities of the model configured for the role. Synchronous: the
    /// adapter keeps them in memory.
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities;

    /// `Err` before the first event if the request is refused (auth, 400, 429...);
    /// failures on an open stream come out of `next`.
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError>;

    /// Default: `stream` collected into one response. Adapters may override.
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        collect_response(self.stream(req).await?).await
    }
}

/// Drains a stream into a complete response.
pub async fn collect_response(
    mut stream: Box<dyn LlmEventStream>,
) -> Result<LlmResponse, LlmError> {
    let mut blocks: Vec<(u32, Part)> = Vec::new();
    let mut model = String::new();
    let mut request_id = None;
    let mut usage = Usage::default();
    let mut finished = None;
    let mut server_tool_uses = Vec::new();
    let mut search_result_urls = Vec::new();
    let mut fetch_errors = Vec::new();

    while let Some(event) = stream.next().await {
        match event? {
            LlmEvent::Started {
                model: m,
                request_id: r,
            } => {
                model = m;
                request_id = r;
            }
            LlmEvent::TextDelta { block, text } => {
                match blocks.iter_mut().find(|(b, _)| *b == block) {
                    Some((_, Part::Text(t))) => t.push_str(&text),
                    _ => blocks.push((block, Part::Text(text))),
                }
            }
            LlmEvent::ToolCallFinished {
                block,
                id,
                name,
                input,
            } => blocks.push((block, Part::ToolUse { id, name, input })),
            LlmEvent::ProviderBlock {
                block,
                provider,
                payload,
            } => blocks.push((block, Part::ProviderBlock { provider, payload })),
            LlmEvent::ServerToolUse { name, input, .. } => {
                server_tool_uses.push(ServerToolCall { name, input });
            }
            LlmEvent::ServerSearchResults { urls, .. } => search_result_urls.extend(urls),
            LlmEvent::ServerFetchError { error_code, .. } => fetch_errors.push(error_code),
            LlmEvent::Usage(u) => usage = u,
            LlmEvent::Finished {
                stop_reason,
                usage: u,
            } => {
                usage = u;
                finished = Some(stop_reason);
            }
            LlmEvent::ToolCallStarted { .. } | LlmEvent::ToolCallInputDelta { .. } => {}
        }
    }
    let stop_reason = finished
        .ok_or_else(|| LlmError::MalformedResponse("stream ended without a stop reason".into()))?;
    blocks.sort_by_key(|(b, _)| *b);
    let mut response = LlmResponse {
        message: LlmMessage {
            role: MessageRole::Assistant,
            parts: blocks.into_iter().map(|(_, p)| p).collect(),
            cache_breakpoint: false,
        },
        stop_reason,
        usage,
        model,
        request_id,
        server_tool_uses,
        search_result_urls,
        fetch_errors,
    };
    response.reconcile_fetch_count();
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn log_summary_withholds_provider_and_transport_text() {
        let long = "x".repeat(500);
        let invalid = LlmError::InvalidRequest(format!("{long}TAIL")).log_summary();
        assert!(!invalid.contains("TAIL") && invalid.ends_with("..."));
        for e in [
            LlmError::Network("https://h/?key=SECRET".into()),
            LlmError::MalformedResponse("SECRET".into()),
            LlmError::Unsupported("SECRET".into()),
        ] {
            assert!(!e.log_summary().contains("SECRET"));
        }
        assert_eq!(LlmError::Timeout.log_summary(), "timeout");
    }

    fn response_with(blocks: Vec<Value>, reported: u32) -> LlmResponse {
        LlmResponse {
            message: LlmMessage {
                role: MessageRole::Assistant,
                parts: blocks
                    .into_iter()
                    .map(|payload| Part::ProviderBlock {
                        provider: "anthropic".into(),
                        payload,
                    })
                    .collect(),
                cache_breakpoint: false,
            },
            stop_reason: StopReason::EndTurn,
            usage: Usage {
                web_fetches: reported,
                web_searches: 4,
                ..Usage::default()
            },
            model: "m".into(),
            request_id: None,
            server_tool_uses: vec![],
            search_result_urls: vec![],
            fetch_errors: vec![],
        }
    }

    fn page(id: &str, url: &str) -> Value {
        json!({"type": "web_fetch_tool_result", "tool_use_id": id,
               "content": {"type": "web_fetch_result", "url": url}})
    }

    #[test]
    fn the_url_comes_from_the_request_block_or_else_from_its_result() {
        let blocks = [
            json!({"type": "server_tool_use", "id": "a", "name": "web_fetch", "input": {}}),
            page("a", "https://x.example/1"),
            json!({"type": "server_tool_use", "id": "b", "name": "web_fetch",
                   "input": "{\"url\": \"https://x.example/2\"}"}),
            json!({"type": "server_tool_use", "id": "c", "name": "web_fetch", "input": null}),
            json!({"type": "web_fetch_tool_result", "tool_use_id": "c",
                   "content": {"type": "web_fetch_tool_result_error", "error_code": "x"}}),
        ];
        let urls: Vec<Option<String>> = web_fetch_reads(&blocks)
            .into_iter()
            .map(|r| r.url)
            .collect();
        assert_eq!(
            urls,
            [
                Some("https://x.example/1".to_owned()),
                Some("https://x.example/2".to_owned()),
                None
            ]
        );
    }

    #[test]
    fn a_result_without_a_request_block_is_still_a_read() {
        let reads = web_fetch_reads(&[page("z", "https://x.example/9")]);
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].url.as_deref(), Some("https://x.example/9"));
    }

    #[test]
    fn the_read_count_is_the_larger_of_the_reported_and_the_seen_and_searches_are_untouched() {
        let blocks = vec![
            json!({"type": "server_tool_use", "id": "a", "name": "web_fetch", "input": {}}),
            page("a", "https://x.example/1"),
        ];
        let mut r = response_with(blocks.clone(), 0);
        r.reconcile_fetch_count();
        assert_eq!((r.usage.web_fetches, r.usage.web_searches), (1, 4));
        let mut r = response_with(blocks, 3);
        r.reconcile_fetch_count();
        assert_eq!(r.usage.web_fetches, 3, "a larger reported figure wins");
    }
    #[test]
    fn only_transient_errors_are_retryable() {
        assert!(LlmError::RateLimited { retry_after: None }.is_retryable());
        assert!(LlmError::Overloaded.is_retryable());
        assert!(LlmError::Network("x".into()).is_retryable());
        assert!(!LlmError::Authentication.is_retryable());
        assert!(!LlmError::CreditExhausted.is_retryable());
        assert!(!LlmError::ContextTooLong.is_retryable());
        assert!(!LlmError::InvalidRequest("x".into()).is_retryable());
        assert!(!LlmError::MalformedResponse("x".into()).is_retryable());
    }

    #[test]
    fn content_events_are_the_ones_a_user_may_have_seen() {
        assert!(
            LlmEvent::TextDelta {
                block: 0,
                text: String::new()
            }
            .is_content()
        );
        assert!(!LlmEvent::Usage(Usage::default()).is_content());
        assert!(
            !LlmEvent::ProviderBlock {
                block: 0,
                provider: "anthropic".into(),
                payload: serde_json::json!({ "type": "thinking" }),
            }
            .is_content()
        );
        assert!(
            !LlmEvent::Started {
                model: String::new(),
                request_id: None
            }
            .is_content()
        );
    }
}
