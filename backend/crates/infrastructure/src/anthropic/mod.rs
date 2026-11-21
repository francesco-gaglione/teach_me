//! Anthropic Messages API adapter for the [`LlmProvider`] port.
//!
//! No official Rust SDK exists, so this speaks HTTP + SSE directly with
//! `reqwest`. Always streams (long outputs would otherwise hit HTTP timeouts)
//! and never retries: retry policy lives in the application layer (ADR 0004).

mod request;
mod sse;
mod translate;

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmResponse,
    LlmRole, StructuredOutput, ToolDef,
};
use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
use serde_json::Value;

use crate::secret::Secret;
use request::{BuildOptions, build_body};
pub use request::{DEFAULT_WEB_FETCH_TOOL, DEFAULT_WEB_SEARCH_TOOL};
use sse::SseParser;
use translate::{Translator, parse_message};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_API_VERSION: &str = "2026-10-21";
/// `OpenRouter` exposes the Anthropic Messages API at `<base>/v1/messages`.
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";

/// How the key travels. Anthropic reads `x-api-key`; `OpenRouter`'s
/// Anthropic-compatible endpoint treats `x-api-key` as a direct Anthropic
/// credential and wants `Authorization: Bearer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiAuth {
    #[default]
    XApiKey,
    Bearer,
}

/// Anthropic model id -> `OpenRouter` slug: `claude-sonnet-5-5` ->
/// `anthropic/claude-sonnet-5.5`, `claude-opus-5` -> `anthropic/claude-opus-5`.
/// Ids that already carry a vendor prefix are kept as they are.
#[must_use]
pub fn openrouter_model_id(model: &str) -> String {
    if model.contains('/') {
        return model.to_owned();
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    // Only a trailing `<major>-<minor>` pair becomes `<major>.<minor>`.
    match model.rsplit_once('-') {
        Some((head, minor))
            if digits(minor)
                && head
                    .rsplit_once('-')
                    .is_some_and(|(_, major)| digits(major)) =>
        {
            format!("anthropic/{head}.{minor}")
        }
        _ => format!("anthropic/{model}"),
    }
}

/// Model and parameters resolved for a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleModel {
    pub model: String,
    /// `low` | `medium` | `high` | `xhigh` | `max`; `None` = the API default.
    pub effort: Option<String>,
    /// Includes thinking tokens.
    pub max_output_tokens: u32,
}

/// Initial model per role (ADR 0004). Hypotheses to confirm with the evals of
/// each phase; the configuration file only holds overrides.
#[must_use]
pub fn default_role_models() -> HashMap<LlmRole, RoleModel> {
    let role = |model: &str, effort: &str, max_output_tokens| RoleModel {
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        max_output_tokens,
    };
    HashMap::from([
        // 16000 is the skill's floor for non-lowballed output: the cap includes
        // adaptive thinking, and a cap hit truncates the reply mid-sentence.
        // Replies are short at `low` effort, so a high cap costs nothing.
        (LlmRole::Tutor, role("claude-sonnet-5-5", "low", 16_000)),
        (
            LlmRole::Interviewer,
            role("claude-sonnet-5-5", "medium", 4096),
        ),
        (
            LlmRole::Researcher,
            role("claude-sonnet-5-5", "medium", 8192),
        ),
        // Five concepts with example and steps are ~2k-3k tokens of JSON plus
        // reasoning; 8k leaves room for both and for a repair.
        (LlmRole::Planner, role("claude-opus-5-5", "medium", 8_000)),
        // `low` (rich-lesson evals, `backend/evals/README.md`): at `medium` the
        // reasoning took 60-80% of the output and plans of 4-5 concepts
        // exhausted the cap before the lesson was written. 32k leaves room for
        // the ~8k tokens a lesson takes at `low`, and for a repair.
        (LlmRole::Generator, role("claude-opus-5-5", "low", 32_000)),
        (LlmRole::Grader, role("claude-haiku-5-5", "medium", 4096)),
        (LlmRole::Recorder, role("claude-sonnet-5-5", "medium", 8192)),
    ])
}

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// A fixed credential for every call: the eval runner's. When set it is
    /// used instead of [`LlmRequest::api_key`]. The server leaves it `None`
    /// (the key of its configuration becomes the owner's, see `users.md` §6),
    /// so each request must carry its owner's key, and one without fails with
    /// `Authentication` instead of running on somebody else's credit.
    pub api_key: Option<Secret>,
    pub auth: ApiAuth,
    pub base_url: String,
    pub api_version: String,
    pub connect_timeout: Duration,
    /// Time allowed for the response headers (non-streaming phase).
    pub request_timeout: Duration,
    /// Longest silence between two chunks of an open stream. The API sends
    /// `ping`s, so real silence means a dead connection.
    pub stream_idle_timeout: Duration,
    /// Time allowed for a whole request that runs server-side tools (web
    /// search). Those are not streamed (`llm-port.md` §12.6), so this covers
    /// the searches and the complete answer, not only the headers.
    pub server_tool_timeout: Duration,
    /// Tool `type` sent for the server-side web search (`llm-port.md` §12.1).
    pub web_search_tool: String,
    /// Tool `type` sent for the server-side web fetch (`llm-port.md` §14,
    /// ADR 0009).
    pub web_fetch_tool: String,
    /// Whether this provider is declared able to run the web fetch tool.
    /// Direct Anthropic: yes. Through `OpenRouter`: no until a real call proves
    /// that it forwards the tool (ADR 0009, "Da verificare" 1).
    pub server_web_fetch: bool,
    /// Server-side retry on safety refusals. Off until proven with a real key.
    pub refusal_fallback: bool,
    pub roles: HashMap<LlmRole, RoleModel>,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            auth: ApiAuth::default(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            api_version: DEFAULT_API_VERSION.to_owned(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(120),
            stream_idle_timeout: Duration::from_secs(60),
            server_tool_timeout: Duration::from_secs(300),
            web_search_tool: DEFAULT_WEB_SEARCH_TOOL.to_owned(),
            web_fetch_tool: DEFAULT_WEB_FETCH_TOOL.to_owned(),
            server_web_fetch: true,
            refusal_fallback: false,
            roles: default_role_models(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AnthropicBuildError {
    #[error("cannot build the HTTP client: {0}")]
    Client(String),
    #[error("role `{0}` has no model configured")]
    MissingRole(&'static str),
    #[error("the API key contains characters that are not valid in a header")]
    InvalidKey,
}

pub struct AnthropicProvider {
    http: reqwest::Client,
    config: AnthropicConfig,
    url: String,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Result<Self, AnthropicBuildError> {
        for role in LlmRole::ALL {
            if !config.roles.contains_key(&role) {
                return Err(AnthropicBuildError::MissingRole(role.as_str()));
            }
        }
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            // The key travels in a header; never follow a redirect with it.
            .redirect(reqwest::redirect::Policy::none())
            // No total timeout: a stream may legitimately run for minutes.
            .build()
            .map_err(|e| AnthropicBuildError::Client(e.without_url().to_string()))?;
        if let Some(key) = &config.api_key {
            auth_value(config.auth, key).map_err(|()| AnthropicBuildError::InvalidKey)?;
        }
        let url = format!("{}/v1/messages", config.base_url.trim_end_matches('/'));
        Ok(Self { http, config, url })
    }

    /// The header value for this request: the adapter's fixed key if it has
    /// one, else the request's own.
    /// A key that cannot be sent as a header (stray control characters) is a
    /// rejected key, the same as one the provider refuses.
    fn key_header(&self, req: &LlmRequest) -> Result<HeaderValue, LlmError> {
        let key = self
            .config
            .api_key
            .as_ref()
            .or(req.api_key.as_ref())
            .ok_or_else(|| {
                tracing::warn!("no API key for the LLM call (reported as key missing)");
                LlmError::Authentication
            })?;
        auth_value(self.config.auth, key).map_err(|()| {
            tracing::warn!("the API key cannot be sent as a header (reported as key rejected)");
            LlmError::Authentication
        })
    }

    fn role(&self, role: LlmRole) -> &RoleModel {
        // Invariant: `new` rejects configs that lack any role.
        &self.config.roles[&role]
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        LlmCapabilities {
            tools: true,
            parallel_tool_calls: true,
            forced_tool_choice: false,
            structured_output: StructuredOutput::Native,
            streaming_tool_input: true,
            prompt_caching: true,
            max_cache_breakpoints: 4,
            context_window_tokens: 1_000_000,
            // The cap this adapter actually sends for the role.
            max_output_tokens: self.role(role).max_output_tokens,
            effort: self.role(role).effort.clone(),
            server_web_search: true,
            server_web_fetch: self.config.server_web_fetch,
        }
    }

    #[tracing::instrument(name = "llm.stream", skip_all, fields(llm.role = req.role.as_str(), llm.trace_label = req.trace_label))]
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        let response = self.send(&req, true, self.config.request_timeout).await?;

        let request_id = request_id(&response);
        Ok(Box::new(AnthropicStream {
            response,
            parser: SseParser::default(),
            translator: Translator::new(request_id),
            queue: VecDeque::new(),
            idle: self.config.stream_idle_timeout,
            done: false,
        }))
    }

    /// Requests with server-side tools (search, fetch) are not streamed:
    /// through `OpenRouter` the results never reach the stream, and without
    /// them a paused turn cannot be continued (`llm-port.md` §12.6, §14.3).
    /// Everything else keeps the default (stream, then collect).
    #[tracing::instrument(name = "llm.complete", skip_all, fields(
        llm.role = req.role.as_str(),
        llm.trace_label = req.trace_label,
        llm.model = tracing::field::Empty,
        llm.input_tokens = tracing::field::Empty,
        llm.output_tokens = tracing::field::Empty,
    ))]
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let result = self.complete_inner(req).await;
        if let Ok(r) = &result {
            let span = tracing::Span::current();
            span.record("llm.model", r.model.as_str());
            span.record("llm.input_tokens", r.usage.input_tokens);
            span.record("llm.output_tokens", r.usage.output_tokens);
        }
        result
    }
}

impl AnthropicProvider {
    async fn complete_inner(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        if !req.tools.iter().any(ToolDef::is_server_tool) {
            return domain::llm::collect_response(self.stream(req).await?).await;
        }
        let limit = self.config.server_tool_timeout;
        let result = tokio::time::timeout(limit, async {
            let response = self.send(&req, false, limit).await?;
            let request_id = request_id(&response);
            let bytes = response.bytes().await.map_err(transport_error)?;
            let body: Value = serde_json::from_slice(&bytes)
                .map_err(|_| LlmError::MalformedResponse("unreadable response body".into()))?;
            parse_message(&body, request_id)
        })
        .await
        .map_err(|_| LlmError::Timeout)
        .and_then(|r| r);
        if result.as_ref().is_err_and(|e| *e == LlmError::Timeout) {
            // The user only sees a generic timeout: whoever reads the logs
            // needs to know which limit it was and where to change it.
            let tools: Vec<&str> = req
                .tools
                .iter()
                .filter(|t| t.is_server_tool())
                .map(|t| {
                    if t.is_server_web_fetch() {
                        "web_fetch"
                    } else {
                        "web_search"
                    }
                })
                .collect();
            tracing::warn!(
                trace_label = req.trace_label,
                ?tools,
                limit_secs = limit.as_secs(),
                "a request with server-side tools timed out; \
                 raise server_tool_timeout_secs in [llm.providers.anthropic] if the pages are slow"
            );
        }
        result
    }
}

impl AnthropicProvider {
    /// Sends the request and returns the response once it is known to be a
    /// success; error statuses are mapped here. `wait` bounds the time to the
    /// response headers.
    async fn send(
        &self,
        req: &LlmRequest,
        stream: bool,
        wait: Duration,
    ) -> Result<reqwest::Response, LlmError> {
        let key = self.key_header(req)?;
        let mut body = build_body(
            req,
            self.role(req.role),
            &BuildOptions {
                refusal_fallback: self.config.refusal_fallback,
                web_search_tool: self.config.web_search_tool.clone(),
                web_fetch_tool: self.config.web_fetch_tool.clone(),
            },
        );
        body["stream"] = Value::Bool(stream);
        let mut call = self
            .http
            .post(&self.url)
            .header(
                match self.config.auth {
                    ApiAuth::XApiKey => "x-api-key",
                    ApiAuth::Bearer => "authorization",
                },
                key,
            )
            .header("anthropic-version", &self.config.api_version)
            .header(
                reqwest::header::ACCEPT,
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        if self.config.refusal_fallback {
            call = call.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }

        let response = tokio::time::timeout(wait, call.send())
            .await
            .map_err(|_| LlmError::Timeout)?
            .map_err(transport_error)?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let text = tokio::time::timeout(Duration::from_secs(10), response.text())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            return Err(http_error(status.as_u16(), &headers, &text));
        }
        Ok(response)
    }
}

fn auth_value(auth: ApiAuth, key: &Secret) -> Result<HeaderValue, ()> {
    let raw = match auth {
        ApiAuth::XApiKey => key.expose().to_owned(),
        ApiAuth::Bearer => format!("Bearer {}", key.expose()),
    };
    let mut value = HeaderValue::from_str(&raw).map_err(|_| ())?;
    value.set_sensitive(true);
    Ok(value)
}

fn request_id(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

fn transport_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        // `without_url` keeps the message free of anything but the cause.
        LlmError::Network(error.without_url().to_string())
    }
}

/// Maps a non-2xx response (`llm-port.md` §5, §8).
fn http_error(status: u16, headers: &HeaderMap, body: &str) -> LlmError {
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let message: String = parsed["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect();
    match status {
        429 => LlmError::RateLimited {
            retry_after: headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        },
        408 => LlmError::Timeout,
        529 => LlmError::Overloaded,
        402 => {
            // The key is valid, the account is out of credit. The provider's
            // own words go in the log (never the key, which is not in the
            // response).
            tracing::warn!(
                http_status = status,
                provider_message = %message,
                "the LLM provider reports the credit exhausted (reported as credit exhausted)"
            );
            LlmError::CreditExhausted
        }
        401 | 403 => {
            tracing::warn!(
                http_status = status,
                provider_message = %message,
                "the LLM provider refused the credentials or the account (reported as key rejected)"
            );
            LlmError::Authentication
        }
        413 => LlmError::ContextTooLong,
        400 if is_context_too_long(&message) => LlmError::ContextTooLong,
        400 | 404 | 422 => LlmError::InvalidRequest(message),
        500..=599 => LlmError::Unavailable,
        _ => LlmError::InvalidRequest(format!("unexpected status {status}: {message}")),
    }
}

fn is_context_too_long(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("prompt is too long") || (m.contains("too long") && m.contains("token"))
}

struct AnthropicStream {
    response: reqwest::Response,
    parser: SseParser,
    translator: Translator,
    queue: VecDeque<LlmEvent>,
    idle: Duration,
    /// Set after `Finished` or an error: nothing more will be produced.
    done: bool,
}

#[async_trait]
impl LlmEventStream for AnthropicStream {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        loop {
            if let Some(event) = self.queue.pop_front() {
                return Some(Ok(event));
            }
            if self.done {
                return None;
            }
            // Drain every complete SSE event before reading more bytes.
            if let Some(sse) = self.parser.next_event() {
                match self.translator.handle(&sse.event, &sse.data) {
                    Ok(events) => self.queue.extend(events),
                    Err(e) => return Some(self.fail(e)),
                }
                if self.translator.finished() && self.queue.is_empty() {
                    self.done = true;
                }
                continue;
            }
            if self.translator.finished() {
                self.done = true;
                continue;
            }
            match tokio::time::timeout(self.idle, self.response.chunk()).await {
                Err(_) => return Some(self.fail(LlmError::Timeout)),
                // The connection broke on an open stream, or EOF before `message_stop`.
                Ok(Err(_) | Ok(None)) => return Some(self.fail(LlmError::Unavailable)),
                Ok(Ok(Some(bytes))) => {
                    if let Err(e) = self.parser.push(&bytes) {
                        return Some(self.fail(e));
                    }
                }
            }
        }
    }
}

impl AnthropicStream {
    fn fail(&mut self, error: LlmError) -> Result<LlmEvent, LlmError> {
        self.done = true;
        self.queue.clear();
        Err(error)
    }
}
