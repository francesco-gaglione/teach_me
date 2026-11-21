//! Port request -> Anthropic Messages API body (`llm-port.md` §8).

use domain::llm::{LlmMessage, LlmRequest, MessageRole, OutputFormat, Part, ToolChoice, ToolDef};
use serde_json::{Map, Value, json};

use super::RoleModel;

/// Provider key written on `ProviderBlock`s produced by this adapter.
pub const PROVIDER: &str = "anthropic";
/// The API accepts at most this many `cache_control` breakpoints per request.
const MAX_BREAKPOINTS: usize = 4;

/// Tool type of the server-side web search when none is configured. Newer
/// versions (`web_search_20260209`...) run code to filter results and need
/// other settings; see `llm-port.md` §12.1.
pub const DEFAULT_WEB_SEARCH_TOOL: &str = "web_search_20250305";

/// Tool type of the server-side web fetch when none is configured: the base
/// version, without dynamic filtering (no code execution); ADR 0009.
pub const DEFAULT_WEB_FETCH_TOOL: &str = "web_fetch_20250910";

pub struct BuildOptions {
    pub refusal_fallback: bool,
    /// `type` sent for [`ToolDef::ServerWebSearch`].
    pub web_search_tool: String,
    /// `type` sent for [`ToolDef::ServerWebFetch`].
    pub web_fetch_tool: String,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            refusal_fallback: false,
            web_search_tool: DEFAULT_WEB_SEARCH_TOOL.to_owned(),
            web_fetch_tool: DEFAULT_WEB_FETCH_TOOL.to_owned(),
        }
    }
}

pub fn build_body(req: &LlmRequest, role: &RoleModel, options: &BuildOptions) -> Value {
    let mut system_flags: Vec<bool> = req.system.iter().map(|b| b.cache_breakpoint).collect();
    let mut message_flags: Vec<bool> = req.messages.iter().map(|m| m.cache_breakpoint).collect();
    limit_breakpoints(&mut system_flags, &mut message_flags);

    let system: Vec<Value> = req
        .system
        .iter()
        .zip(&system_flags)
        .map(|(block, cache)| with_cache(json!({ "type": "text", "text": block.text }), *cache))
        .collect();

    let messages: Vec<Value> = req
        .messages
        .iter()
        .zip(&message_flags)
        .filter_map(|(m, cache)| message(m, *cache))
        .collect();

    let mut body = Map::new();
    body.insert("model".into(), json!(role.model));
    body.insert(
        "max_tokens".into(),
        json!(req.max_output_tokens.unwrap_or(role.max_output_tokens)),
    );
    body.insert("stream".into(), json!(true));
    if !system.is_empty() {
        body.insert("system".into(), Value::Array(system));
    }
    body.insert("messages".into(), Value::Array(messages));

    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .filter_map(|tool| tool_json(tool, options))
            .collect();
        body.insert("tools".into(), Value::Array(tools));
        body.insert(
            "tool_choice".into(),
            match req.tool_choice {
                ToolChoice::None => json!({ "type": "none" }),
                // `Auto` and anything added later: the safe default.
                _ => json!({ "type": "auto" }),
            },
        );
    }

    let mut output = Map::new();
    if let Some(effort) = &role.effort {
        output.insert("effort".into(), json!(effort));
    }
    if let OutputFormat::JsonSchema { schema, .. } = &req.output {
        output.insert(
            "format".into(),
            json!({ "type": "json_schema", "schema": schema }),
        );
    }
    if !output.is_empty() {
        body.insert("output_config".into(), Value::Object(output));
    }
    // `temperature`, `top_p`, `top_k` and `thinking` are deliberately never sent:
    // current models reject or default them (ADR 0004).

    if options.refusal_fallback {
        body.insert("fallbacks".into(), json!("default"));
    }
    Value::Object(body)
}

/// A port tool as the Messages API wants it; `None` for a tool this adapter
/// does not know (they are left out rather than sent wrong).
fn tool_json(tool: &ToolDef, options: &BuildOptions) -> Option<Value> {
    match tool {
        ToolDef::Function {
            name,
            description,
            input_schema,
        } => Some(json!({
            "name": name,
            "description": description,
            "input_schema": input_schema,
            // Large tool inputs stream as they are generated.
            "eager_input_streaming": true,
        })),
        ToolDef::ServerWebSearch {
            max_uses,
            allowed_domains,
            blocked_domains,
        } => {
            let mut tool = json!({
                "type": options.web_search_tool,
                "name": "web_search",
                "max_uses": max_uses,
            });
            // The API rejects both lists together; the allow list wins.
            if !allowed_domains.is_empty() {
                tool["allowed_domains"] = json!(allowed_domains);
            } else if !blocked_domains.is_empty() {
                tool["blocked_domains"] = json!(blocked_domains);
            }
            Some(tool)
        }
        ToolDef::ServerWebFetch {
            max_uses,
            allowed_domains,
            max_content_tokens,
        } => {
            let mut tool = json!({
                "type": options.web_fetch_tool,
                "name": "web_fetch",
                "max_uses": max_uses,
                "max_content_tokens": max_content_tokens,
            });
            if !allowed_domains.is_empty() {
                tool["allowed_domains"] = json!(allowed_domains);
            }
            // No `citations` field: with `output_config.format` the API
            // answers 400 if citations are enabled, and the lesson cites with
            // its own `[^key]` (ADR 0009).
            Some(tool)
        }
        _ => None,
    }
}

/// Keeps the breakpoints closest to the end of the request: the system
/// prefix is the most stable, the newest messages the most reusable, and the
/// oldest message breakpoints are the first to give way.
fn limit_breakpoints(system: &mut [bool], messages: &mut [bool]) {
    let mut total = system.iter().chain(messages.iter()).filter(|f| **f).count();
    for flag in messages.iter_mut().filter(|f| **f) {
        if total <= MAX_BREAKPOINTS {
            return;
        }
        *flag = false;
        total -= 1;
    }
    for flag in system.iter_mut().filter(|f| **f).rev() {
        if total <= MAX_BREAKPOINTS {
            return;
        }
        *flag = false;
        total -= 1;
    }
}

fn with_cache(mut block: Value, cache: bool) -> Value {
    if cache {
        block["cache_control"] = json!({ "type": "ephemeral" });
    }
    block
}

fn message(m: &LlmMessage, cache: bool) -> Option<Value> {
    let mut content: Vec<Value> = m.parts.iter().filter_map(part).collect();
    if content.is_empty() {
        return None;
    }
    if cache && let Some(last) = content.pop() {
        content.push(with_cache(last, true));
    }
    Some(json!({
        "role": match m.role { MessageRole::User => "user", MessageRole::Assistant => "assistant" },
        "content": content,
    }))
}

fn part(p: &Part) -> Option<Value> {
    match p {
        // The API refuses text blocks that are only whitespace; the user-visible
        // message keeps whatever the model wrote.
        Part::Text(text) if text.trim().is_empty() => None,
        Part::Text(text) => Some(json!({ "type": "text", "text": text })),
        Part::ToolUse { id, name, input } => {
            Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
        }
        Part::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => Some(json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content,
            "is_error": is_error,
        })),
        // Blocks from another provider mean nothing here.
        Part::ProviderBlock { provider, payload } if provider == PROVIDER => Some(payload.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::llm::{LlmRole, SystemBlock};

    fn role() -> RoleModel {
        RoleModel {
            model: "claude-sonnet-5-5".into(),
            effort: Some("low".into()),
            max_output_tokens: 4096,
        }
    }

    fn off() -> BuildOptions {
        BuildOptions::default()
    }

    fn request() -> LlmRequest {
        let mut r = LlmRequest::new(LlmRole::Tutor, "t");
        r.system = vec![
            SystemBlock {
                text: "A".into(),
                cache_breakpoint: true,
            },
            SystemBlock {
                text: "B".into(),
                cache_breakpoint: true,
            },
        ];
        let mut last = LlmMessage::text(MessageRole::User, "ciao");
        last.cache_breakpoint = true;
        r.messages = vec![last];
        r
    }

    #[test]
    fn plain_chat_request_shape() {
        let body = build_body(&request(), &role(), &off());
        assert_eq!(body["model"], "claude-sonnet-5-5");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], true);
        assert_eq!(body["output_config"], json!({ "effort": "low" }));
        assert_eq!(body["system"][0]["text"], "A");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["system"][1]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["text"], "ciao");
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );
        for absent in [
            "temperature",
            "top_p",
            "top_k",
            "thinking",
            "tools",
            "tool_choice",
            "fallbacks",
        ] {
            assert!(body.get(absent).is_none(), "{absent} must not be sent");
        }
    }

    #[test]
    fn request_override_of_max_tokens_wins() {
        let mut r = request();
        r.max_output_tokens = Some(99);
        assert_eq!(build_body(&r, &role(), &off())["max_tokens"], 99);
    }

    #[test]
    fn no_effort_and_no_format_means_no_output_config() {
        let mut role = role();
        role.effort = None;
        assert!(
            build_body(&request(), &role, &off())
                .get("output_config")
                .is_none()
        );
    }

    #[test]
    fn tools_and_structured_output() {
        let mut r = request();
        r.tools = vec![ToolDef::Function {
            name: "save".into(),
            description: "d".into(),
            input_schema: json!({ "type": "object" }),
        }];
        r.output = OutputFormat::JsonSchema {
            name: "x".into(),
            schema: json!({ "type": "object" }),
        };
        let body = build_body(&r, &role(), &off());
        assert_eq!(body["tools"][0]["name"], "save");
        assert_eq!(body["tools"][0]["eager_input_streaming"], true);
        assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
        assert_eq!(
            body["output_config"]["format"],
            json!({ "type": "json_schema", "schema": { "type": "object" } })
        );
        r.tool_choice = ToolChoice::None;
        assert_eq!(
            build_body(&r, &role(), &off())["tool_choice"],
            json!({ "type": "none" })
        );
    }

    #[test]
    fn server_web_fetch_is_a_typed_tool_without_citations() {
        let mut r = request();
        r.tools = vec![ToolDef::ServerWebFetch {
            max_uses: 2,
            allowed_domains: vec!["doc.rust-lang.org".into(), "docs.rs".into()],
            max_content_tokens: 8000,
        }];
        let body = build_body(&r, &role(), &off());
        assert_eq!(
            body["tools"][0],
            json!({
                "type": "web_fetch_20250910", "name": "web_fetch", "max_uses": 2,
                "allowed_domains": ["doc.rust-lang.org", "docs.rs"],
                "max_content_tokens": 8000
            })
        );
        // Citations on would make `output_config.format` a 400 (ADR 0009):
        // the field is not sent at all.
        assert!(body["tools"][0].get("citations").is_none());
        assert!(body["tools"][0].get("eager_input_streaming").is_none());
        let newer = BuildOptions {
            web_fetch_tool: "web_fetch_20260209".into(),
            ..BuildOptions::default()
        };
        assert_eq!(
            build_body(&r, &role(), &newer)["tools"][0]["type"],
            "web_fetch_20260209"
        );
    }

    #[test]
    fn a_request_without_server_tools_has_no_tools_field_and_streams() {
        // What the Generator sends with the web fetch switched off.
        let body = build_body(&request(), &role(), &off());
        assert!(body.get("tools").is_none() && body.get("tool_choice").is_none());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn server_web_search_is_a_typed_tool_with_a_use_ceiling() {
        let mut r = request();
        r.tools = vec![ToolDef::ServerWebSearch {
            max_uses: 4,
            allowed_domains: vec![],
            blocked_domains: vec!["spam.example".into()],
        }];
        let body = build_body(&r, &role(), &off());
        assert_eq!(
            body["tools"][0],
            json!({
                "type": "web_search_20250305", "name": "web_search", "max_uses": 4,
                "blocked_domains": ["spam.example"]
            })
        );
        let newer = BuildOptions {
            web_search_tool: "web_search_20260209".into(),
            ..BuildOptions::default()
        };
        assert_eq!(
            build_body(&r, &role(), &newer)["tools"][0]["type"],
            "web_search_20260209"
        );
    }

    #[test]
    fn a_paused_turn_is_continued_by_replaying_every_server_block() {
        let mut r = request();
        let blocks = [
            json!({ "type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": { "query": "q" } }),
            json!({ "type": "web_search_tool_result", "tool_use_id": "srv_1", "content": [] }),
        ];
        r.messages = vec![
            LlmMessage::text(MessageRole::User, "cerca"),
            LlmMessage {
                role: MessageRole::Assistant,
                parts: blocks
                    .iter()
                    .map(|payload| Part::ProviderBlock {
                        provider: PROVIDER.into(),
                        payload: payload.clone(),
                    })
                    .collect(),
                cache_breakpoint: false,
            },
        ];
        let body = build_body(&r, &role(), &off());
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"], json!(blocks));
    }

    #[test]
    fn tool_cycle_and_provider_blocks_round_trip() {
        let mut r = request();
        r.messages = vec![
            LlmMessage::text(MessageRole::User, "q"),
            LlmMessage {
                role: MessageRole::Assistant,
                parts: vec![
                    Part::ProviderBlock {
                        provider: PROVIDER.into(),
                        payload: json!({ "type": "thinking", "thinking": "", "signature": "sig" }),
                    },
                    Part::ProviderBlock {
                        provider: "openai".into(),
                        payload: json!({ "x": 1 }),
                    },
                    Part::ToolUse {
                        id: "t1".into(),
                        name: "save".into(),
                        input: json!({ "a": 1 }),
                    },
                ],
                cache_breakpoint: false,
            },
            LlmMessage {
                role: MessageRole::User,
                parts: vec![Part::ToolResult {
                    tool_use_id: "t1".into(),
                    content: "ok".into(),
                    is_error: false,
                }],
                cache_breakpoint: false,
            },
        ];
        let body = build_body(&r, &role(), &off());
        let assistant = &body["messages"][1]["content"];
        assert_eq!(
            assistant.as_array().unwrap().len(),
            2,
            "foreign block dropped"
        );
        assert_eq!(assistant[0]["signature"], "sig");
        assert_eq!(assistant[1]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "t1");
    }

    #[test]
    fn at_most_four_breakpoints_oldest_message_ones_dropped_first() {
        let mut r = request();
        r.messages = (0..4)
            .map(|i| {
                let mut m = LlmMessage::text(
                    if i % 2 == 0 {
                        MessageRole::User
                    } else {
                        MessageRole::Assistant
                    },
                    format!("m{i}"),
                );
                m.cache_breakpoint = true;
                m
            })
            .collect();
        let body = build_body(&r, &role(), &off());
        let marked = |v: &Value| v.get("cache_control").is_some();
        let sys = body["system"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| marked(b))
            .count();
        let msgs: Vec<bool> = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| marked(&m["content"][0]))
            .collect();
        assert_eq!(sys, 2);
        assert_eq!(msgs, [false, false, true, true]);
    }

    #[test]
    fn refusal_fallback_is_opt_in() {
        let body = build_body(
            &request(),
            &role(),
            &BuildOptions {
                refusal_fallback: true,
                ..BuildOptions::default()
            },
        );
        assert_eq!(body["fallbacks"], "default");
    }

    #[test]
    fn whitespace_only_text_parts_are_not_sent_and_empty_messages_vanish() {
        let mut req = LlmRequest::new(LlmRole::Tutor, "t");
        req.messages = vec![
            LlmMessage {
                role: MessageRole::User,
                parts: vec![Part::Text("ciao".into())],
                cache_breakpoint: false,
            },
            LlmMessage {
                role: MessageRole::Assistant,
                parts: vec![
                    Part::Text("  \n".into()),
                    Part::ToolUse {
                        id: "tu".into(),
                        name: "x".into(),
                        input: json!({}),
                    },
                ],
                cache_breakpoint: false,
            },
        ];
        let body = build_body(
            &req,
            &role(),
            &BuildOptions {
                refusal_fallback: false,
                ..BuildOptions::default()
            },
        );
        let content = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");

        req.messages[1].parts = vec![Part::Text(" ".into())];
        let body = build_body(
            &req,
            &role(),
            &BuildOptions {
                refusal_fallback: false,
                ..BuildOptions::default()
            },
        );
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }
}
