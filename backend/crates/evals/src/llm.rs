//! Provider decorators of the runner: one records every call (usage, latency,
//! cost) and enforces the cost budget; another answers the Planner with the
//! suite's fixed plan, so the Generator suite measures only the Generator.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmMessage, LlmProvider, LlmRequest,
    LlmResponse, LlmRole, MessageRole, Part, StopReason, ToolDef, Usage,
};
use serde::Serialize;
use serde_json::Value;

use crate::pricing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// The model under evaluation.
    Subject,
    /// The judge.
    Judge,
}

/// One model call as it happened.
#[derive(Debug, Clone)]
pub struct CallRecord {
    pub source: Source,
    pub role: LlmRole,
    pub label: String,
    pub model: String,
    pub request: LlmRequest,
    /// All text parts of the answer joined.
    pub text: String,
    pub tool_calls: Vec<(String, Value)>,
    pub stop_reason: String,
    pub usage: Usage,
    pub latency: Duration,
    pub search_urls: Vec<String>,
    /// The provider blocks of the answer in order (tool use, tool results,
    /// thinking): what the server-side tools did. Only this call's answer,
    /// not the history the request carried.
    pub blocks: Vec<Value>,
    pub error: Option<String>,
    pub cost_usd: f64,
    pub price_assumed: bool,
}

/// Everything recorded, plus the money spent. Shared by the subject and the
/// judge decorators so one budget covers both.
pub struct Ledger {
    inner: Mutex<Inner>,
    max_cost_usd: Option<f64>,
}

#[derive(Default)]
struct Inner {
    calls: Vec<CallRecord>,
    spent: f64,
    /// Estimates of the calls in flight.
    reserved: f64,
    /// Calls refused because their estimate did not fit the budget.
    refused: usize,
}

impl Ledger {
    #[must_use]
    pub fn new(max_cost_usd: Option<f64>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::default(),
            max_cost_usd,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned lock only means another thread panicked mid-push; the
        // data is still a valid list of calls.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn spent(&self) -> f64 {
        self.lock().spent
    }

    pub fn max_cost(&self) -> Option<f64> {
        self.max_cost_usd
    }

    #[must_use]
    pub fn exhausted(&self) -> bool {
        self.max_cost_usd.is_some_and(|max| self.spent() >= max)
    }

    /// Position to pass to [`Ledger::since`] later.
    pub fn mark(&self) -> usize {
        self.lock().calls.len()
    }

    pub fn since(&self, mark: usize) -> Vec<CallRecord> {
        self.lock().calls.get(mark..).unwrap_or_default().to_vec()
    }

    /// Books `estimate` for a call about to start, atomically: it is refused
    /// when what is spent, what is booked for calls in flight and this
    /// estimate would pass the budget. So the budget is exceeded at most by
    /// the gap between an estimate and the real cost of a call.
    pub fn reserve(&self, estimate: f64) -> Option<f64> {
        let mut inner = self.lock();
        if self
            .max_cost_usd
            .is_some_and(|max| inner.spent + inner.reserved + estimate > max)
        {
            inner.refused += 1;
            return None;
        }
        inner.reserved += estimate;
        Some(estimate)
    }

    /// Gives a booking back (a call that never produced a record).
    pub fn release(&self, reserved: f64) {
        let mut inner = self.lock();
        inner.reserved = (inner.reserved - reserved).max(0.0);
    }

    /// Calls refused by the budget so far: a run with one is not a result
    /// (the use case reports the refusal as an internal error).
    pub fn refused(&self) -> usize {
        self.lock().refused
    }

    /// Money booked for calls in flight.
    pub fn reserved(&self) -> f64 {
        self.lock().reserved
    }

    /// Replaces the booking with the real cost.
    fn settle(&self, reserved: f64, record: CallRecord) {
        let mut inner = self.lock();
        inner.reserved = (inner.reserved - reserved).max(0.0);
        inner.spent += record.cost_usd;
        inner.calls.push(record);
    }
}

pub const BUDGET_MESSAGE: &str = "eval cost budget exhausted";

/// Records every call that goes through `inner`.
pub struct RecordingLlm {
    inner: Arc<dyn LlmProvider>,
    ledger: Arc<Ledger>,
    source: Source,
    /// Model per role, to price a call before it is made (unknown: the
    /// most expensive one).
    models: HashMap<LlmRole, String>,
}

impl RecordingLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>, ledger: Arc<Ledger>, source: Source) -> Self {
        Self {
            inner,
            ledger,
            source,
            models: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_models(mut self, models: HashMap<LlmRole, String>) -> Self {
        self.models = models;
        self
    }

    /// What the call may cost: the prompt at ~3 characters per token, the
    /// answer at the request's limit (capped), searches at their price.
    fn estimate(&self, req: &LlmRequest) -> f64 {
        let chars: usize = req
            .system
            .iter()
            .map(|b| b.text.len())
            .chain(
                req.messages
                    .iter()
                    .flat_map(|m| m.parts.iter())
                    .map(|p| match p {
                        Part::Text(t) => t.len(),
                        Part::ToolUse { input, .. } => input.to_string().len(),
                        Part::ToolResult { content, .. } => content.len(),
                        _ => 0,
                    }),
            )
            .sum();
        // Pages read by the provider enter the context, up to the tool's cap
        // each; the repairs read them again.
        let pages: u32 = req
            .tools
            .iter()
            .map(|t| match t {
                ToolDef::ServerWebFetch {
                    max_uses,
                    max_content_tokens,
                    ..
                } => max_uses.saturating_mul(*max_content_tokens),
                _ => 0,
            })
            .sum();
        let input = u32::try_from(chars / 3)
            .unwrap_or(u32::MAX)
            .saturating_add(pages);
        let output = req
            .max_output_tokens
            // No cap on the request: the role's own, as the provider sends it.
            // Never lowered: the booking must cover the worst case the call
            // can cost, it is the guard on the owner's credit.
            .unwrap_or_else(|| self.inner.capabilities(req.role).max_output_tokens);
        let searches = if req.tools.iter().any(ToolDef::is_server_web_search) {
            4
        } else {
            0
        };
        let model = self
            .models
            .get(&req.role)
            .map_or("claude-opus-5-5", String::as_str);
        pricing::typical_cost(model, input, output, searches)
    }

    /// Books the call's estimate, or refuses the call.
    fn reserve(&self, req: &LlmRequest) -> Result<f64, LlmError> {
        self.ledger
            .reserve(self.estimate(req))
            .ok_or_else(|| LlmError::Unsupported(BUDGET_MESSAGE.to_owned()))
    }
}

fn texts(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|p| {
            if let Part::Text(t) = p {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[allow(clippy::too_many_arguments)]
fn record(
    ledger: &Ledger,
    reserved: f64,
    source: Source,
    req: LlmRequest,
    model: String,
    text: String,
    tool_calls: Vec<(String, Value)>,
    stop: &StopReason,
    usage: Usage,
    latency: Duration,
    search_urls: Vec<String>,
    blocks: Vec<Value>,
    error: Option<String>,
) {
    // Same rule as the use cases: reads seen in the blocks count even when the
    // provider's usage does not report them.
    let mut usage = usage;
    let seen = u32::try_from(domain::llm::web_fetch_reads(&blocks).len()).unwrap_or(u32::MAX);
    usage.web_fetches = usage.web_fetches.max(seen);
    let cost = pricing::cost(&model, &usage);
    ledger.settle(
        reserved,
        CallRecord {
            source,
            role: req.role,
            label: req.trace_label.to_owned(),
            model,
            request: req,
            text,
            tool_calls,
            stop_reason: format!("{stop:?}"),
            usage,
            latency,
            search_urls,
            blocks,
            error,
            cost_usd: cost.usd,
            price_assumed: cost.price_assumed,
        },
    );
}

#[async_trait]
impl LlmProvider for RecordingLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        let reserved = self.reserve(&req)?;
        let start = Instant::now();
        match self.inner.stream(req.clone()).await {
            Ok(stream) => Ok(Box::new(RecordingStream {
                inner: stream,
                ledger: self.ledger.clone(),
                reserved,
                source: self.source,
                request: Some(req),
                start,
                model: String::new(),
                text: String::new(),
                tool_calls: Vec::new(),
                search_urls: Vec::new(),
                blocks: Vec::new(),
                usage: Usage::default(),
            })),
            Err(e) => {
                record(
                    &self.ledger,
                    reserved,
                    self.source,
                    req,
                    String::new(),
                    String::new(),
                    Vec::new(),
                    &StopReason::Other("error".into()),
                    Usage::default(),
                    start.elapsed(),
                    Vec::new(),
                    Vec::new(),
                    Some(e.to_string()),
                );
                Err(e)
            }
        }
    }

    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let reserved = self.reserve(&req)?;
        let start = Instant::now();
        let result = self.inner.complete(req.clone()).await;
        let latency = start.elapsed();
        match &result {
            Ok(r) => {
                let calls = r
                    .message
                    .parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::ToolUse { name, input, .. } => Some((name.clone(), input.clone())),
                        _ => None,
                    })
                    .collect();
                record(
                    &self.ledger,
                    reserved,
                    self.source,
                    req,
                    r.model.clone(),
                    texts(&r.message.parts),
                    calls,
                    &r.stop_reason,
                    r.usage,
                    latency,
                    r.search_result_urls.clone(),
                    r.message
                        .parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::ProviderBlock { payload, .. } => Some(payload.clone()),
                            _ => None,
                        })
                        .collect(),
                    None,
                );
            }
            Err(e) => record(
                &self.ledger,
                reserved,
                self.source,
                req,
                String::new(),
                String::new(),
                Vec::new(),
                &StopReason::Other("error".into()),
                Usage::default(),
                latency,
                Vec::new(),
                Vec::new(),
                Some(e.to_string()),
            ),
        }
        result
    }
}

struct RecordingStream {
    inner: Box<dyn LlmEventStream>,
    ledger: Arc<Ledger>,
    reserved: f64,
    source: Source,
    request: Option<LlmRequest>,
    start: Instant,
    model: String,
    text: String,
    tool_calls: Vec<(String, Value)>,
    search_urls: Vec<String>,
    blocks: Vec<Value>,
    usage: Usage,
}

impl RecordingStream {
    fn finish(&mut self, stop: &StopReason, error: Option<String>) {
        if let Some(req) = self.request.take() {
            record(
                &self.ledger,
                self.reserved,
                self.source,
                req,
                std::mem::take(&mut self.model),
                std::mem::take(&mut self.text),
                std::mem::take(&mut self.tool_calls),
                stop,
                self.usage,
                self.start.elapsed(),
                std::mem::take(&mut self.search_urls),
                std::mem::take(&mut self.blocks),
                error,
            );
        }
    }
}

/// A stream dropped before it finished never settles: give its booking back.
impl Drop for RecordingStream {
    fn drop(&mut self) {
        if self.request.is_some() {
            self.ledger.release(self.reserved);
        }
    }
}

#[async_trait]
impl LlmEventStream for RecordingStream {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        let item = self.inner.next().await;
        match &item {
            Some(Ok(event)) => match event {
                LlmEvent::Started { model, .. } => self.model.clone_from(model),
                LlmEvent::TextDelta { text, .. } => self.text.push_str(text),
                LlmEvent::ToolCallFinished { name, input, .. } => {
                    self.tool_calls.push((name.clone(), input.clone()));
                }
                LlmEvent::ServerSearchResults { urls, .. } => {
                    self.search_urls.extend(urls.iter().cloned());
                }
                LlmEvent::ProviderBlock { payload, .. } => self.blocks.push(payload.clone()),
                LlmEvent::Usage(u) => self.usage = *u,
                LlmEvent::Finished { stop_reason, usage } => {
                    self.usage = *usage;
                    let stop = stop_reason.clone();
                    self.finish(&stop, None);
                }
                _ => {}
            },
            Some(Err(e)) => {
                let message = e.to_string();
                self.finish(&StopReason::Other("error".into()), Some(message));
            }
            None => self.finish(&StopReason::Other("closed".into()), None),
        }
        item
    }
}

// ---------------------------------------------------------------- injected pages

/// A page the Generator is made to find already read.
#[derive(Debug, Clone, PartialEq)]
pub struct InjectedPage {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// The "page already read" mode of a `fixture_pages` case: the web fetch tool
/// runs on the provider's servers, which cannot read a page nobody publishes,
/// so the conversation carries the page as if the model had read it. Armed
/// for one case at a time; a plain pass-through otherwise.
pub struct InjectingLlm {
    inner: Arc<dyn LlmProvider>,
    pages: Mutex<Vec<InjectedPage>>,
}

/// What the user says after the simulated read.
pub const CONTINUE_MESSAGE: &str = "Continua.";
const RETRIEVED_AT: &str = "2026-01-01T00:00:00Z";

/// The provider blocks of the simulated reads, in order.
#[must_use]
pub fn injected_blocks(pages: &[InjectedPage]) -> Vec<Value> {
    pages
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            let id = format!("srvtoolu_evalfixture{:02}", i + 1);
            [
                serde_json::json!({"type": "server_tool_use", "id": id, "name": "web_fetch",
                    "input": {"url": p.url}}),
                serde_json::json!({"type": "web_fetch_tool_result", "tool_use_id": id,
                    "content": {"type": "web_fetch_result", "url": p.url,
                        "retrieved_at": RETRIEVED_AT,
                        "content": {"type": "document",
                            "source": {"type": "text", "media_type": "text/plain", "data": p.text},
                            "title": p.title}}}),
            ]
        })
        .collect()
}

impl InjectingLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self {
            inner,
            pages: Mutex::new(Vec::new()),
        }
    }

    /// Arms (or, with an empty list, disarms) the injection.
    pub fn set_pages(&self, pages: Vec<InjectedPage>) {
        *self
            .pages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = pages;
    }

    fn armed(&self) -> Vec<InjectedPage> {
        self.pages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The use case builds each Generator request from its own history, which
    /// has no read in it: the simulated one goes right after the kickoff in
    /// every call (write, repair, continuation), so the conversation is
    /// always kickoff, read, "Continua.", then whatever followed. The fetch
    /// tool is dropped so the model does not read again.
    fn inject(&self, mut req: LlmRequest) -> LlmRequest {
        let pages = self.armed();
        if pages.is_empty() || req.role != LlmRole::Generator || req.messages.is_empty() {
            return req;
        }
        req.tools
            .retain(|t| !matches!(t, ToolDef::ServerWebFetch { .. }));
        let parts = injected_blocks(&pages)
            .into_iter()
            .map(|payload| Part::ProviderBlock {
                provider: "anthropic".to_owned(),
                payload,
            })
            .collect();
        let at = 1.min(req.messages.len());
        req.messages.splice(
            at..at,
            [
                LlmMessage {
                    role: MessageRole::Assistant,
                    parts,
                    cache_breakpoint: false,
                },
                LlmMessage::text(MessageRole::User, CONTINUE_MESSAGE),
            ],
        );
        req
    }
}

#[async_trait]
impl LlmProvider for InjectingLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        self.inner.stream(self.inject(req)).await
    }

    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        self.inner.complete(self.inject(req)).await
    }
}

// ---------------------------------------------------------------- planner

/// Answers every `Planner` request with a fixed plan and passes the rest on.
pub struct PlannedLlm {
    inner: Arc<dyn LlmProvider>,
    plan: Mutex<Option<String>>,
    /// Planner suites measure the Planner only: the Generator is refused, so
    /// it costs nothing and the job ends after the plan.
    refuse_generator: Mutex<bool>,
}

impl PlannedLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self {
            inner,
            plan: Mutex::new(None),
            refuse_generator: Mutex::new(false),
        }
    }

    pub fn set_refuse_generator(&self, refuse: bool) {
        *self
            .refuse_generator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = refuse;
    }

    fn refuses(&self, role: LlmRole) -> bool {
        role == LlmRole::Generator
            && *self
                .refuse_generator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_plan(&self, plan: Option<String>) {
        *self
            .plan
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = plan;
    }

    fn fixed(&self) -> Option<String> {
        self.plan
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Not retryable: the job ends at once.
const REFUSED_GENERATOR: LlmError = LlmError::Unsupported(String::new());

struct Replay(VecDeque<LlmEvent>);

#[async_trait]
impl LlmEventStream for Replay {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        self.0.pop_front().map(Ok)
    }
}

#[async_trait]
impl LlmProvider for PlannedLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        if self.refuses(req.role) {
            return Err(REFUSED_GENERATOR);
        }
        if req.role == LlmRole::Planner
            && let Some(plan) = self.fixed()
        {
            return Ok(Box::new(Replay(VecDeque::from([
                LlmEvent::Started {
                    model: "eval-fixed-plan".into(),
                    request_id: None,
                },
                LlmEvent::TextDelta {
                    block: 0,
                    text: plan,
                },
                LlmEvent::Finished {
                    stop_reason: StopReason::EndTurn,
                    usage: Usage::default(),
                },
            ]))));
        }
        self.inner.stream(req).await
    }

    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        if self.refuses(req.role) {
            return Err(REFUSED_GENERATOR);
        }
        if req.role == LlmRole::Planner && self.fixed().is_some() {
            return domain::llm::collect_response(self.stream(req).await?).await;
        }
        self.inner.complete(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_booking_counts_until_it_is_released() {
        let ledger = Ledger::new(Some(1.0));
        assert!(ledger.reserve(0.6).is_some());
        assert!(
            ledger.reserve(0.6).is_none(),
            "the first booking is still in flight"
        );
        ledger.release(0.6);
        assert!(ledger.reserve(0.6).is_some());
        assert!(Ledger::new(None).reserve(1e9).is_some());
    }

    #[test]
    fn bookings_from_many_threads_never_pass_the_budget() {
        let ledger = Ledger::new(Some(1.0));
        let granted: usize = std::thread::scope(|s| {
            let handles: Vec<_> = (0..32)
                .map(|_| s.spawn(|| ledger.reserve(0.1).is_some()))
                .collect();
            handles
                .into_iter()
                .map(|h| usize::from(h.join().unwrap()))
                .sum()
        });
        assert_eq!(granted, 10);
        assert!((ledger.reserved() - 1.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod injection_tests {
    use super::*;

    struct Echo;

    #[async_trait]
    impl LlmProvider for Echo {
        fn capabilities(&self, _: LlmRole) -> LlmCapabilities {
            unreachable!()
        }
        async fn stream(&self, _: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
            unreachable!()
        }
        async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, LlmError> {
            unreachable!()
        }
    }

    fn page() -> InjectedPage {
        InjectedPage {
            url: "https://g.invalid/p".into(),
            title: "T".into(),
            text: "testo".into(),
        }
    }

    fn fetch_tool() -> ToolDef {
        ToolDef::ServerWebFetch {
            max_uses: 3,
            max_content_tokens: 100,
            allowed_domains: Vec::new(),
        }
    }

    fn request(role: LlmRole, label: &'static str) -> LlmRequest {
        let mut r = LlmRequest::new(role, label);
        r.messages = vec![LlmMessage::text(MessageRole::User, "Procedi.")];
        r.tools = vec![fetch_tool()];
        r
    }

    #[test]
    fn the_read_follows_the_kickoff_and_the_tool_goes() {
        let llm = InjectingLlm::new(Arc::new(Echo));
        llm.set_pages(vec![page()]);
        let out = llm.inject(request(LlmRole::Generator, "generator.write"));
        assert_eq!(out.tools.len(), 0);
        let roles: Vec<_> = out.messages.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [MessageRole::User, MessageRole::Assistant, MessageRole::User]
        );
        let blocks: Vec<&Value> = out.messages[1]
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::ProviderBlock { provider, payload } if provider == "anthropic" => {
                    Some(payload)
                }
                _ => None,
            })
            .collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "server_tool_use");
        assert_eq!(blocks[0]["input"]["url"], "https://g.invalid/p");
        assert_eq!(blocks[1]["tool_use_id"], blocks[0]["id"]);
        assert_eq!(blocks[1]["content"]["content"]["source"]["data"], "testo");
        assert_eq!(blocks[1]["content"]["content"]["title"], "T");
        assert!(matches!(&out.messages[2].parts[0], Part::Text(t) if t == CONTINUE_MESSAGE));
    }

    #[test]
    fn a_repair_gets_the_read_once_before_its_own_history() {
        let llm = InjectingLlm::new(Arc::new(Echo));
        llm.set_pages(vec![page()]);
        let mut req = request(LlmRole::Generator, "generator.repair");
        req.messages
            .push(LlmMessage::text(MessageRole::Assistant, "{}"));
        req.messages
            .push(LlmMessage::text(MessageRole::User, "correggi"));
        let out = llm.inject(req);
        let roles: Vec<_> = out.messages.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            [
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User,
                MessageRole::Assistant,
                MessageRole::User
            ]
        );
        let reads = out
            .messages
            .iter()
            .flat_map(|m| &m.parts)
            .filter(|p| matches!(p, Part::ProviderBlock { .. }))
            .count();
        assert_eq!(reads, 2);
    }

    #[test]
    fn nothing_changes_when_disarmed_or_for_other_roles() {
        let llm = InjectingLlm::new(Arc::new(Echo));
        let plain = request(LlmRole::Generator, "generator.write");
        assert_eq!(llm.inject(plain.clone()).messages.len(), 1);
        assert_eq!(llm.inject(plain).tools.len(), 1);
        llm.set_pages(vec![page()]);
        let other = llm.inject(request(LlmRole::Planner, "planner"));
        assert_eq!(other.messages.len(), 1);
        assert_eq!(other.tools.len(), 1);
    }
}
