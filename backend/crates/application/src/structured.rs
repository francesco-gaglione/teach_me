//! What the job handlers that ask a model for structured output have in
//! common: one logged call, the mapping of its failures to job errors, the
//! text to validate, and the repair request. The Researcher, the Planner, the
//! Generator and the Recorder differ in what they validate, not in how they
//! talk to the model.

use std::fmt::Write as _;

use async_trait::async_trait;
use domain::JobErrorCode;
use domain::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmRole, MessageRole, OutputFormat,
    Part, StopReason, SystemBlock, ToolDef, Usage,
};
use serde_json::Value;

/// A role's output schema: `{version, name, schema}` as shipped in
/// `backend/prompts/<role>/output_schema.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputSchema {
    pub name: String,
    pub schema: Value,
}

impl OutputSchema {
    pub fn parse(raw: &str, fallback_name: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(raw)?;
        Ok(Self {
            name: value["name"].as_str().unwrap_or(fallback_name).to_owned(),
            schema: value["schema"].clone(),
        })
    }

    #[must_use]
    pub fn format(&self) -> OutputFormat {
        OutputFormat::JsonSchema {
            name: self.name.clone(),
            schema: self.schema.clone(),
        }
    }
}

/// What a job consumed, summed over its calls.
#[derive(Debug, Default, Clone, Copy)]
pub struct Spent {
    pub calls: u32,
    pub usage: Usage,
    /// Page reads asked for by the model, failed ones included: the count the
    /// per-job read budget is spent against. The provider's `web_fetches`
    /// may leave failures out, so the larger of it and the blocks seen is kept
    /// for each answer.
    pub fetch_uses: u32,
}

impl Spent {
    pub fn add(&mut self, usage: Usage) {
        self.calls += 1;
        let u = &mut self.usage;
        u.input_tokens = u.input_tokens.saturating_add(usage.input_tokens);
        u.output_tokens = u.output_tokens.saturating_add(usage.output_tokens);
        u.thinking_tokens = u.thinking_tokens.saturating_add(usage.thinking_tokens);
        u.cache_read_tokens = u.cache_read_tokens.saturating_add(usage.cache_read_tokens);
        u.cache_write_tokens = u
            .cache_write_tokens
            .saturating_add(usage.cache_write_tokens);
        u.web_searches = u.web_searches.saturating_add(usage.web_searches);
        u.web_fetches = u.web_fetches.saturating_add(usage.web_fetches);
    }
}

#[must_use]
pub fn job_error_code(error: &LlmError) -> JobErrorCode {
    match error {
        LlmError::RateLimited { .. } => JobErrorCode::RateLimited,
        LlmError::Timeout => JobErrorCode::Timeout,
        LlmError::Authentication => JobErrorCode::LlmKeyRejected,
        LlmError::CreditExhausted => JobErrorCode::LlmCreditExhausted,
        LlmError::InvalidRequest(_) | LlmError::Unsupported(_) | LlmError::ContextTooLong => {
            JobErrorCode::Internal
        }
        _ => JobErrorCode::UpstreamFailed,
    }
}

/// One model call, logged, with its failure mapped to a job error.
pub async fn call(
    llm: &dyn LlmProvider,
    prompt_version: &str,
    request: &LlmRequest,
    spent: &mut Spent,
) -> Result<LlmResponse, JobErrorCode> {
    let label = request.trace_label;
    let mut response = match llm.complete(request.clone()).await {
        Ok(response) => response,
        Err(error) => {
            if error == LlmError::Authentication {
                tracing::warn!(
                    trace_label = label,
                    "LLM authentication failed: the user's key was rejected"
                );
            } else if error == LlmError::CreditExhausted {
                tracing::warn!(
                    trace_label = label,
                    "LLM credit exhausted: the provider account has no credit left"
                );
            } else if let (LlmError::InvalidRequest(message), true) = (
                &error,
                request.tools.iter().any(ToolDef::is_server_web_fetch),
            ) {
                // A 400 on a request that carries the web fetch tool is most
                // likely the tool (ADR 0009, open points 1-3). There is no
                // silent fallback: the job fails here, the operator decides
                // (`max_fetches = 0` switches the tool off).
                tracing::error!(
                    %message,
                    trace_label = label,
                    tool = "web_fetch",
                    "the provider rejected a request that carries the web fetch tool; \
                     no automatic fallback: set [llm.roles.generator] max_fetches = 0 to run without it"
                );
            } else {
                tracing::warn!(%error, trace_label = label, "LLM call failed");
            }
            return Err(job_error_code(&error));
        }
    };
    // The one rule for the number of reads: whichever of the provider's figure
    // and the blocks seen is larger (idempotent: adapters apply it too).
    response.reconcile_fetch_count();
    spent.add(response.usage);
    spent.fetch_uses = spent.fetch_uses.saturating_add(response.usage.web_fetches);
    tracing::info!(
        trace_label = label,
        model = response.model,
        prompt_version = prompt_version.trim(),
        input_tokens = response.usage.input_tokens,
        output_tokens = response.usage.output_tokens,
        cache_read_tokens = response.usage.cache_read_tokens,
        cache_write_tokens = response.usage.cache_write_tokens,
        web_searches = response.usage.web_searches,
        web_fetches = response.usage.web_fetches,
        fetch_errors = ?response.fetch_errors,
        stop_reason = ?response.stop_reason,
        request_id = response.request_id,
        "LLM call finished"
    );
    Ok(response)
}

/// Like [`call`], for roles whose output must be whole: an answer cut short
/// by the token limit, or refused, is a failure of the provider's side
/// (`upstream_failed`), not something a repair can fix (`llm-port.md` §13.3).
pub async fn call_complete(
    llm: &dyn LlmProvider,
    prompt_version: &str,
    request: &LlmRequest,
    spent: &mut Spent,
) -> Result<LlmResponse, JobErrorCode> {
    let response = call(llm, prompt_version, request, spent).await?;
    if response.stop_reason == StopReason::EndTurn {
        Ok(response)
    } else {
        warn_if_out_of_tokens(llm, request, &response);
        tracing::warn!(
            trace_label = request.trace_label,
            stop_reason = ?response.stop_reason,
            "the answer is incomplete or refused"
        );
        Err(JobErrorCode::UpstreamFailed)
    }
}

/// A call that ends because the token cap was reached fails the job with the
/// generic `upstream_failed`, and nothing else says why. This is the line that
/// does: the role, what was spent, the cap and the effort, and the two knobs
/// that fix it. No automatic retry: a larger cap costs money.
fn warn_if_out_of_tokens(llm: &dyn LlmProvider, request: &LlmRequest, response: &LlmResponse) {
    if response.stop_reason != StopReason::MaxTokens {
        return;
    }
    let caps = llm.capabilities(request.role);
    let role = request.role.as_str();
    tracing::warn!(
        trace_label = request.trace_label,
        role,
        output_tokens = response.usage.output_tokens,
        max_output_tokens = request.max_output_tokens.unwrap_or(caps.max_output_tokens),
        effort = caps.effort.as_deref().unwrap_or("default"),
        "the model ran out of output tokens before finishing (thinking counts): \
         lower `effort` or raise `max_output_tokens` in [llm.roles.{role}]"
    );
}

/// What a caller wants to do with each answer of a continued turn, as it
/// arrives (job progress, bookkeeping of what the model was shown).
#[async_trait]
pub trait TurnObserver: Send {
    async fn observe(&mut self, response: &LlmResponse);
}

/// For callers with nothing to report between calls.
pub struct NoObserver;

#[async_trait]
impl TurnObserver for NoObserver {
    async fn observe(&mut self, _response: &LlmResponse) {}
}

/// How a turn that runs server-side tools is continued.
#[derive(Debug, Clone, Copy)]
pub struct Continuations {
    /// How many times a paused turn is continued before giving up.
    pub max: u32,
    /// Treat a truncated answer as `upstream_failed` (see [`call_complete`]).
    pub strict: bool,
    /// When every tool budget is spent on a paused turn: `false` returns the
    /// paused answer as it is (the Researcher: what was found is in it);
    /// `true` keeps continuing, up to `max`, because the turn is half done
    /// and its text is not a result (the Generator). The tools stay in the
    /// request with the smallest `max_uses` (1: the provider's behaviour for
    /// 0 is not documented), and a read past the budget is answered by the
    /// provider with `max_uses_exceeded`, which the model sees.
    pub continue_when_spent: bool,
}

/// One model turn that may run server-side tools (web search, web fetch): it
/// calls the model until it stops for a reason other than `pause_turn`,
/// sending a paused answer back unchanged (all its blocks, tools included),
/// with the same tools and no user message after it (`llm-port.md` §12.2).
/// The only implementation of that loop: the Researcher and the Generator
/// both use it.
///
/// The provider counts a continuation as a new request, so `max_uses` does
/// not span the turn by itself: the budget of each tool is carried over here
/// (what is left, or no more of it). When every budget is spent the paused
/// answer is returned as it is: what was read is in it, and a caller that
/// needs more than that repairs it.
///
/// On return `request.messages` holds the paused answers pushed so far, so a
/// caller that goes on with the same conversation (a repair) has them.
pub async fn run_turn(
    llm: &dyn LlmProvider,
    prompt_version: &str,
    request: &mut LlmRequest,
    spent: &mut Spent,
    how: Continuations,
    observer: &mut dyn TurnObserver,
) -> Result<LlmResponse, JobErrorCode> {
    let start = spent.usage;
    let (mut seen_searches, mut seen_fetches) = (0u32, 0u32);
    let budget = |request: &LlmRequest, fetch: bool| {
        request.tools.iter().find_map(|t| match t {
            ToolDef::ServerWebSearch { max_uses, .. } if !fetch => Some(*max_uses),
            ToolDef::ServerWebFetch { max_uses, .. } if fetch => Some(*max_uses),
            _ => None,
        })
    };
    let (search_budget, fetch_budget) = (budget(request, false), budget(request, true));
    let has_server_tool = request.tools.iter().any(ToolDef::is_server_tool);
    let mut continuations = 0u32;
    loop {
        let response = call(llm, prompt_version, request, spent).await?;
        observer.observe(&response).await;
        let count = |name: &str| {
            u32::try_from(
                response
                    .server_tool_uses
                    .iter()
                    .filter(|c| c.name == name)
                    .count(),
            )
            .unwrap_or(u32::MAX)
        };
        seen_searches += count("web_search");
        seen_fetches += count("web_fetch");
        match response.stop_reason {
            // Without a server tool nothing can pause: that would be odd, not
            // something to continue.
            StopReason::PauseTurn if has_server_tool => {
                if continuations >= how.max {
                    tracing::warn!(trace_label = request.trace_label, "too many paused turns");
                    return Err(JobErrorCode::Timeout);
                }
                // What the provider reports wins over what was counted from
                // the blocks, unless it reports less (some do not report).
                let used_searches = spent
                    .usage
                    .web_searches
                    .saturating_sub(start.web_searches)
                    .max(seen_searches);
                let used_fetches = spent
                    .usage
                    .web_fetches
                    .saturating_sub(start.web_fetches)
                    .max(seen_fetches);
                let left_searches = search_budget.map(|b| b.saturating_sub(used_searches));
                let left_fetches = fetch_budget.map(|b| b.saturating_sub(used_fetches));
                if [left_searches, left_fetches]
                    .iter()
                    .flatten()
                    .all(|n| *n == 0)
                    && (search_budget.is_some() || fetch_budget.is_some())
                    && !how.continue_when_spent
                {
                    tracing::warn!(
                        trace_label = request.trace_label,
                        used_searches,
                        used_fetches,
                        "tool budget spent on a paused turn, using what there is"
                    );
                    return Ok(response);
                }
                for tool in &mut request.tools {
                    match tool {
                        ToolDef::ServerWebSearch { max_uses, .. } => {
                            if let Some(left) = left_searches {
                                *max_uses = next_limit(left, how.continue_when_spent, *max_uses);
                            }
                        }
                        ToolDef::ServerWebFetch { max_uses, .. } => {
                            if let Some(left) = left_fetches {
                                *max_uses = next_limit(left, how.continue_when_spent, *max_uses);
                            }
                        }
                        _ => {}
                    }
                }
                continuations += 1;
                request.messages.push(response.message);
            }
            StopReason::EndTurn => return Ok(response),
            StopReason::MaxTokens if !how.strict => return Ok(response),
            ref other => {
                warn_if_out_of_tokens(llm, request, &response);
                tracing::warn!(
                    trace_label = request.trace_label,
                    stop_reason = ?other,
                    "the answer is incomplete or refused"
                );
                return Err(JobErrorCode::UpstreamFailed);
            }
        }
    }
}

/// `max_uses` of the next request of a paused turn. A spent budget leaves the
/// old value (a caller that stops there never sends it) unless the turn goes
/// on regardless: then it is the smallest the port allows, never the old one,
/// which would hand the model a fresh budget.
fn next_limit(left: u32, continue_when_spent: bool, old: u32) -> u32 {
    match (left, continue_when_spent) {
        (0, true) => 1,
        (0, false) => old,
        (left, _) => left,
    }
}

/// The text the model wrote after its last tool activity: with citations the
/// answer may be split over several text blocks, and thinking or search
/// blocks may precede it.
#[must_use]
pub fn final_text(message: &LlmMessage) -> String {
    let mut tail: Vec<&str> = message
        .parts
        .iter()
        .rev()
        .map_while(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    tail.reverse();
    let tail = tail.concat();
    if !tail.trim().is_empty() {
        return tail;
    }
    // Nothing after the last tool block or thinking: the answer is looked for
    // by block type, not by position, in the last text that says something.
    message
        .parts
        .iter()
        .rev()
        .find_map(|p| match p {
            Part::Text(t) if !t.trim().is_empty() => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The JSON object inside the model's text: the whole text, or else from the
/// first `{` to its matching `}` (code fences and sentences around it are
/// ignored; braces inside strings do not count).
#[must_use]
pub fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && serde_json::from_str::<Value>(trimmed).is_ok() {
        return Some(trimmed);
    }
    let start = text.find('{')?;
    let mut depth = 0usize;
    let (mut in_string, mut escaped) = (false, false);
    for (i, c) in text[start..].char_indices() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parses the model's text into a JSON object, with a message fit for a repair.
pub fn parse_object(text: &str) -> Result<Value, Vec<String>> {
    let Some(json) = extract_json(text) else {
        return Err(vec![
            "Nella risposta non c'è nessun oggetto JSON.".to_owned(),
        ]);
    };
    let value: Value =
        serde_json::from_str(json).map_err(|e| vec![format!("Il JSON non è valido ({e}).")])?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(vec![
            "La radice del JSON deve essere un oggetto.".to_owned(),
        ])
    }
}

/// How one role asks for a repair: no tools, native structured output.
pub struct RepairSpec<'a> {
    pub role: LlmRole,
    pub trace_label: &'static str,
    /// The system blocks of the repair call: the role's own for the
    /// Researcher, the original ones (still cached) for the others.
    pub system: Vec<SystemBlock>,
    pub schema: &'a OutputSchema,
    pub prompt_version: &'a str,
    pub max_output_tokens: Option<u32>,
    /// Treat a truncated or refused answer as `upstream_failed` (see
    /// [`call_complete`]).
    pub strict: bool,
}

/// A numbered list of problems, one per line.
#[must_use]
pub fn bullet_problems(problems: &[String]) -> String {
    problems
        .iter()
        .map(|p| format!("- {p}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The user message of a repair when the role has no `repair.md` of its own.
#[must_use]
pub fn repair_message(text: &str, problems: &[String], extra: Option<&str>) -> String {
    let mut message = format!("Testo da correggere:\n{text}\n\nErrori da risolvere:\n");
    for problem in problems {
        let _ = writeln!(message, "- {problem}");
    }
    if let Some(extra) = extra {
        message.push('\n');
        message.push_str(extra);
    }
    message
}

/// One attempt to fix a rejected answer: the model gets `message` (which
/// carries the text to fix, as material, and the errors) and answers again.
pub async fn repair_once(
    llm: &dyn LlmProvider,
    spec: &RepairSpec<'_>,
    message: String,
    spent: &mut Spent,
) -> Result<String, JobErrorCode> {
    let mut request = LlmRequest::new(spec.role, spec.trace_label);
    request.system.clone_from(&spec.system);
    request.output = spec.schema.format();
    request.max_output_tokens = spec.max_output_tokens;
    request.messages = vec![LlmMessage::text(MessageRole::User, message)];
    let response = if spec.strict {
        call_complete(llm, spec.prompt_version, &request, spent).await?
    } else {
        call(llm, spec.prompt_version, &request, spent).await?
    };
    Ok(response.text())
}

/// Checks `value` against the subset of JSON Schema the output schemas use
/// (`type`, `required`, `properties`, `items`, `enum`, `additionalProperties:
/// false`). Native structured output already guarantees this; it is checked
/// again because a provider with emulated output guarantees nothing.
#[must_use]
pub fn check_schema(value: &Value, schema: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    walk(value, schema, "", &mut errors);
    errors
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn walk(value: &Value, schema: &Value, at: &str, errors: &mut Vec<String>) {
    let here = if at.is_empty() { "la radice" } else { at };
    let kinds: Vec<&str> = match schema.get("type") {
        Some(Value::String(k)) => vec![k.as_str()],
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !kinds.is_empty() && !kinds.iter().any(|k| type_matches(k, value)) {
        errors.push(format!("{here} deve essere di tipo {}.", kinds.join(" o ")));
        return;
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        let list: Vec<String> = allowed.iter().map(Value::to_string).collect();
        errors.push(format!("{here} deve essere uno tra {}.", list.join(", ")));
    }
    if let Some(object) = value.as_object() {
        for key in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !object.contains_key(key) {
                errors.push(format!("{here}: manca il campo `{key}`."));
            }
        }
        let props = schema.get("properties").and_then(Value::as_object);
        for (key, v) in object {
            match props.and_then(|p| p.get(key)) {
                Some(sub) => walk(v, sub, &format!("{at}/{key}"), errors),
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    errors.push(format!("{here}: il campo `{key}` non è previsto."));
                }
                None => {}
            }
        }
    }
    if let (Some(items), Some(sub)) = (value.as_array(), schema.get("items")) {
        for (i, v) in items.iter().enumerate() {
            walk(v, sub, &format!("{at}/{i}"), errors);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn a_read_the_provider_does_not_report_is_counted_in_the_log_figure_and_the_budget() {
        use domain::llm::{LlmEvent, LlmRole, StopReason};

        use crate::fakes::{FakeLlm, Step};
        let block = |payload| {
            Step::Event(LlmEvent::ProviderBlock {
                block: 0,
                provider: "anthropic".into(),
                payload,
            })
        };
        let llm = FakeLlm::with(vec![vec![
            block(json!({"type": "server_tool_use", "id": "a", "name": "web_fetch", "input": {}})),
            block(json!({"type": "web_fetch_tool_result", "tool_use_id": "a",
                         "content": {"type": "web_fetch_result", "url": "https://a.example/"}})),
            Step::Event(LlmEvent::Finished {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            }),
        ]]);
        let request = LlmRequest::new(LlmRole::Generator, "test");
        let mut spent = Spent::default();
        let response = call(llm.as_ref(), "v1", &request, &mut spent)
            .await
            .unwrap();
        assert_eq!(response.usage.web_fetches, 1);
        assert_eq!((spent.usage.web_fetches, spent.fetch_uses), (1, 1));
    }

    #[test]
    fn spent_sums_the_server_tool_counters_of_every_call() {
        let mut spent = Spent::default();
        for (searches, fetches) in [(1, 2), (0, 1)] {
            spent.add(Usage {
                web_searches: searches,
                web_fetches: fetches,
                ..Usage::default()
            });
        }
        assert_eq!(
            (
                spent.calls,
                spent.usage.web_searches,
                spent.usage.web_fetches
            ),
            (2, 1, 3)
        );
    }

    #[test]
    fn the_answer_is_found_by_block_type_even_when_a_tool_block_ends_the_message() {
        let tool = |t: &str| Part::ProviderBlock {
            provider: "p".into(),
            payload: serde_json::json!({ "type": t }),
        };
        let message = |parts: Vec<Part>| LlmMessage {
            role: MessageRole::Assistant,
            parts,
            cache_breakpoint: false,
        };
        // The usual shape: preamble, read, the document.
        let usual = message(vec![
            Part::Text("Leggo.".into()),
            tool("server_tool_use"),
            tool("web_fetch_tool_result"),
            Part::Text("{\"a\":".into()),
            Part::Text("1}".into()),
        ]);
        assert_eq!(final_text(&usual), "{\"a\":1}");
        // Thinking after the document: the document is still the answer.
        let trailing = message(vec![Part::Text("{\"a\":1}".into()), tool("thinking")]);
        assert_eq!(final_text(&trailing), "{\"a\":1}");
    }

    #[test]
    fn extracts_the_object_around_a_fence() {
        assert_eq!(
            extract_json("ecco:\n```json\n{\"a\": 1}\n```"),
            Some("{\"a\": 1}")
        );
        assert_eq!(extract_json("niente"), None);
        assert_eq!(
            extract_json(r#"Ecco {"a": "}{"} e poi {"b": 2}"#),
            Some(r#"{"a": "}{"}"#)
        );
        assert_eq!(extract_json("{\"a\": "), None);
        assert!(parse_object("[1]").is_err());
        assert!(parse_object("{\"a\": }").is_err());
        assert_eq!(parse_object("{\"a\": 1}").unwrap(), json!({"a": 1}));
    }

    #[test]
    fn the_schema_subset_reports_every_problem() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["a", "b", "c"],
            "properties": {
                "a": {"type": "string"},
                "b": {"type": "array", "items": {"type": "integer"}},
                "c": {"enum": ["x", "y"]},
                "d": {"type": ["string", "null"]}
            }
        });
        let ok = json!({"a": "s", "b": [1, 2], "c": "x", "d": null});
        assert_eq!(check_schema(&ok, &schema), Vec::<String>::new());
        let bad = json!({"a": 1, "b": [1, "z"], "c": "q", "e": true});
        let errors = check_schema(&bad, &schema);
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("/a")));
        assert!(errors.iter().any(|e| e.contains("/b/1")));
        assert!(errors.iter().any(|e| e.contains("/c")));
        assert!(errors.iter().any(|e| e.contains("`e` non è previsto")));
    }
}
