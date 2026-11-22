//! Anthropic streaming events -> port events (`llm-port.md` §8).

use std::collections::HashMap;

use domain::llm::{
    LlmError, LlmEvent, LlmMessage, LlmResponse, MessageRole, Part, ServerToolCall, StopReason,
    Usage,
};
use serde_json::{Value, json};

use super::request::PROVIDER;

enum Block {
    Text,
    Thinking {
        thinking: String,
        signature: String,
    },
    ToolUse {
        id: String,
        name: String,
        json: String,
    },
    /// A tool the provider runs itself; `start` is the block as it opened and
    /// `json` the input streamed after it.
    ServerToolUse {
        start: Value,
        json: String,
    },
    /// Anything we do not interpret (redacted thinking, server tools): sent
    /// back to the provider verbatim.
    Opaque(Value),
}

#[derive(Default)]
pub struct Translator {
    blocks: HashMap<u32, Block>,
    usage: Usage,
    stop_reason: Option<StopReason>,
    request_id: Option<String>,
    finished: bool,
}

impl Translator {
    #[must_use]
    pub fn new(request_id: Option<String>) -> Self {
        Self {
            request_id,
            ..Self::default()
        }
    }

    /// Whether `message_stop` was seen; EOF before that is a dropped connection.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn handle(&mut self, name: &str, data: &str) -> Result<Vec<LlmEvent>, LlmError> {
        let json: Value = serde_json::from_str(data)
            .map_err(|_| LlmError::MalformedResponse(format!("unreadable `{name}` event")))?;
        // The `type` inside the data is authoritative; the SSE name is a hint.
        let kind = json["type"].as_str().unwrap_or(name);
        match kind {
            "message_start" => Ok(self.message_start(&json)),
            "content_block_start" => self.block_start(&json),
            "content_block_delta" => self.block_delta(&json),
            "content_block_stop" => self.block_stop(&json),
            "message_delta" => Ok(self.message_delta(&json)),
            "message_stop" => self.message_stop(),
            "error" => Err(map_stream_error(&json["error"])),
            // `ping` and event types added later.
            _ => Ok(Vec::new()),
        }
    }

    fn message_start(&mut self, json: &Value) -> Vec<LlmEvent> {
        let message = &json["message"];
        self.merge_usage(&message["usage"]);
        vec![
            LlmEvent::Started {
                model: message["model"].as_str().unwrap_or_default().to_owned(),
                request_id: self.request_id.clone(),
            },
            LlmEvent::Usage(self.usage),
        ]
    }

    fn block_start(&mut self, json: &Value) -> Result<Vec<LlmEvent>, LlmError> {
        let index = block_index(json)?;
        let block = &json["content_block"];
        let mut events = Vec::new();
        let state = match block["type"].as_str() {
            Some("text") => {
                if let Some(text) = block["text"].as_str().filter(|t| !t.is_empty()) {
                    events.push(LlmEvent::TextDelta {
                        block: index,
                        text: text.to_owned(),
                    });
                }
                Block::Text
            }
            Some("thinking") => Block::Thinking {
                thinking: block["thinking"].as_str().unwrap_or_default().to_owned(),
                signature: block["signature"].as_str().unwrap_or_default().to_owned(),
            },
            Some("tool_use") => {
                let id = block["id"].as_str().unwrap_or_default().to_owned();
                let name = block["name"].as_str().unwrap_or_default().to_owned();
                events.push(LlmEvent::ToolCallStarted {
                    block: index,
                    id: id.clone(),
                    name: name.clone(),
                });
                Block::ToolUse {
                    id,
                    name,
                    json: String::new(),
                }
            }
            Some("server_tool_use") => Block::ServerToolUse {
                start: block.clone(),
                json: String::new(),
            },
            _ => Block::Opaque(block.clone()),
        };
        self.blocks.insert(index, state);
        Ok(events)
    }

    fn block_delta(&mut self, json: &Value) -> Result<Vec<LlmEvent>, LlmError> {
        let index = block_index(json)?;
        let delta = &json["delta"];
        let Some(block) = self.blocks.get_mut(&index) else {
            return Err(LlmError::MalformedResponse(
                "delta for a block that was never started".into(),
            ));
        };
        let mut events = Vec::new();
        match (delta["type"].as_str(), block) {
            (Some("text_delta"), Block::Text) => {
                if let Some(text) = delta["text"].as_str().filter(|t| !t.is_empty()) {
                    events.push(LlmEvent::TextDelta {
                        block: index,
                        text: text.to_owned(),
                    });
                }
            }
            (Some("input_json_delta"), Block::ToolUse { json, .. }) => {
                let partial = delta["partial_json"].as_str().unwrap_or_default();
                json.push_str(partial);
                events.push(LlmEvent::ToolCallInputDelta {
                    block: index,
                    partial_json: partial.to_owned(),
                });
            }
            (Some("input_json_delta"), Block::ServerToolUse { json, .. }) => {
                json.push_str(delta["partial_json"].as_str().unwrap_or_default());
            }
            (Some("thinking_delta"), Block::Thinking { thinking, .. }) => {
                thinking.push_str(delta["thinking"].as_str().unwrap_or_default());
            }
            (Some("signature_delta"), Block::Thinking { signature, .. }) => {
                signature.push_str(delta["signature"].as_str().unwrap_or_default());
            }
            // Citations and delta kinds added later.
            _ => {}
        }
        Ok(events)
    }

    fn block_stop(&mut self, json: &Value) -> Result<Vec<LlmEvent>, LlmError> {
        let index = block_index(json)?;
        let Some(block) = self.blocks.remove(&index) else {
            return Ok(Vec::new());
        };
        Ok(match block {
            Block::Text => Vec::new(),
            Block::Thinking {
                thinking,
                signature,
            } => vec![LlmEvent::ProviderBlock {
                block: index,
                provider: PROVIDER.into(),
                payload: json!({ "type": "thinking", "thinking": thinking, "signature": signature }),
            }],
            Block::ToolUse { id, name, json } => {
                // A tool call with no arguments streams no JSON at all.
                let source = if json.trim().is_empty() {
                    "{}"
                } else {
                    json.as_str()
                };
                // Unparsable input (cut short, or streamed eagerly and invalid)
                // is not an error of the stream: it goes up as a non-object
                // input, which the tool layer answers with "invalid input" or,
                // if the reply was cut, ignores along with the call.
                let input: Value =
                    serde_json::from_str(source).unwrap_or_else(|_| Value::String(json.clone()));
                vec![LlmEvent::ToolCallFinished {
                    block: index,
                    id,
                    name,
                    input,
                }]
            }
            Block::ServerToolUse { mut start, json } => {
                // No arguments streams no JSON; unparsable input is kept as the
                // text received, which the provider will refuse on replay.
                let input = if json.trim().is_empty() {
                    start["input"].clone()
                } else {
                    serde_json::from_str(&json).unwrap_or(Value::String(json))
                };
                start["input"] = input.clone();
                let name = start["name"].as_str().unwrap_or_default().to_owned();
                vec![
                    LlmEvent::ServerToolUse {
                        block: index,
                        name,
                        input,
                    },
                    LlmEvent::ProviderBlock {
                        block: index,
                        provider: PROVIDER.into(),
                        payload: start,
                    },
                ]
            }
            Block::Opaque(payload) => {
                let mut events = Vec::new();
                if payload["type"] == "web_search_tool_result" {
                    events.push(LlmEvent::ServerSearchResults {
                        block: index,
                        urls: search_result_urls(&payload),
                    });
                }
                if let Some(code) = fetch_error_code(&payload) {
                    events.push(LlmEvent::ServerFetchError {
                        block: index,
                        error_code: code,
                    });
                }
                events.push(LlmEvent::ProviderBlock {
                    block: index,
                    provider: PROVIDER.into(),
                    payload,
                });
                events
            }
        })
    }

    fn message_delta(&mut self, json: &Value) -> Vec<LlmEvent> {
        if let Some(reason) = json["delta"]["stop_reason"].as_str() {
            self.stop_reason = Some(stop_reason(reason));
        }
        self.merge_usage(&json["usage"]);
        vec![LlmEvent::Usage(self.usage)]
    }

    fn message_stop(&mut self) -> Result<Vec<LlmEvent>, LlmError> {
        let stop_reason = self.stop_reason.take().ok_or_else(|| {
            LlmError::MalformedResponse("message ended without a stop reason".into())
        })?;
        self.finished = true;
        Ok(vec![LlmEvent::Finished {
            stop_reason,
            usage: self.usage,
        }])
    }

    /// `usage` objects are cumulative; a field that is absent keeps its value.
    fn merge_usage(&mut self, usage: &Value) {
        let field = |name: &str| {
            usage[name]
                .as_u64()
                .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
        };
        if let Some(v) = field("input_tokens") {
            self.usage.input_tokens = v;
        }
        if let Some(v) = field("output_tokens") {
            self.usage.output_tokens = v;
        }
        if let Some(v) = usage["output_tokens_details"]["thinking_tokens"]
            .as_u64()
            .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
        {
            self.usage.thinking_tokens = v;
        }
        if let Some(v) = field("cache_read_input_tokens") {
            self.usage.cache_read_tokens = v;
        }
        if let Some(v) = field("cache_creation_input_tokens") {
            self.usage.cache_write_tokens = v;
        }
        let server = &usage["server_tool_use"];
        if let Some(v) = server["web_search_requests"]
            .as_u64()
            .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
        {
            self.usage.web_searches = v;
        }
        if let Some(v) = server["web_fetch_requests"]
            .as_u64()
            .map(|v| u32::try_from(v).unwrap_or(u32::MAX))
        {
            self.usage.web_fetches = v;
        }
    }
}

/// The error code of a `web_fetch_tool_result` whose read failed, else `None`.
/// A failed read is a 200 response with an error object in place of the
/// page: the model sees it and goes on, the logs record it.
pub fn fetch_error_code(block: &Value) -> Option<String> {
    if block["type"] != "web_fetch_tool_result"
        || block["content"]["type"] != "web_fetch_tool_result_error"
    {
        return None;
    }
    Some(
        block["content"]["error_code"]
            .as_str()
            .unwrap_or("unknown")
            .to_owned(),
    )
}

/// URLs of a `web_search_tool_result` block. A search that failed carries an
/// error object instead of a list: no URLs, and the model sees the error.
pub fn search_result_urls(block: &Value) -> Vec<String> {
    block["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["type"] == "web_search_result")
        .filter_map(|r| r["url"].as_str().map(str::to_owned))
        .collect()
}

/// A complete (non-streaming) Messages API response -> port response.
///
/// Same blocks and same mapping as the stream, in one pass. Fields the
/// provider adds (`OpenRouter`'s `cost`, `is_byok`...) are ignored.
pub fn parse_message(body: &Value, request_id: Option<String>) -> Result<LlmResponse, LlmError> {
    let content = body["content"]
        .as_array()
        .ok_or_else(|| LlmError::MalformedResponse("response without content".into()))?;
    let stop = body["stop_reason"]
        .as_str()
        .ok_or_else(|| LlmError::MalformedResponse("response without a stop reason".into()))?;

    let provider_block = |block: &Value| Part::ProviderBlock {
        provider: PROVIDER.into(),
        payload: block.clone(),
    };
    let mut parts = Vec::new();
    let mut server_tool_uses = Vec::new();
    let mut search_urls = Vec::new();
    let mut fetch_errors = Vec::new();
    for block in content {
        match block["type"].as_str() {
            Some("text") => parts.push(Part::Text(
                block["text"].as_str().unwrap_or_default().to_owned(),
            )),
            Some("tool_use") => parts.push(Part::ToolUse {
                id: block["id"].as_str().unwrap_or_default().to_owned(),
                name: block["name"].as_str().unwrap_or_default().to_owned(),
                input: block["input"].clone(),
            }),
            Some("server_tool_use") => {
                server_tool_uses.push(ServerToolCall {
                    name: block["name"].as_str().unwrap_or_default().to_owned(),
                    input: block["input"].clone(),
                });
                parts.push(provider_block(block));
            }
            Some("web_search_tool_result") => {
                search_urls.extend(search_result_urls(block));
                parts.push(provider_block(block));
            }
            // `web_fetch_tool_result`, signed thinking, redacted thinking and
            // whatever comes later: kept as they are and sent back as they are.
            _ => {
                fetch_errors.extend(fetch_error_code(block));
                parts.push(provider_block(block));
            }
        }
    }

    let mut translator = Translator::new(request_id.clone());
    translator.merge_usage(&body["usage"]);
    let mut response = LlmResponse {
        message: LlmMessage {
            role: MessageRole::Assistant,
            parts,
            cache_breakpoint: false,
        },
        stop_reason: stop_reason(stop),
        usage: translator.usage,
        model: body["model"].as_str().unwrap_or_default().to_owned(),
        request_id,
        server_tool_uses,
        search_result_urls: search_urls,
        fetch_errors,
    };
    // Through `OpenRouter` the usage does not count the page reads.
    response.reconcile_fetch_count();
    Ok(response)
}

fn block_index(json: &Value) -> Result<u32, LlmError> {
    json["index"]
        .as_u64()
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| LlmError::MalformedResponse("block event without an index".into()))
}

fn stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "tool_use" => StopReason::ToolUse,
        "refusal" => StopReason::Refusal,
        "pause_turn" => StopReason::PauseTurn,
        other => StopReason::Other(other.to_owned()),
    }
}

/// An `error` event on an open stream.
fn map_stream_error(error: &Value) -> LlmError {
    match error["type"].as_str() {
        Some("overloaded_error") => LlmError::Overloaded,
        Some("rate_limit_error") => LlmError::RateLimited { retry_after: None },
        Some(kind @ "billing_error") => {
            tracing::warn!(
                error_type = kind,
                "the LLM provider reports the credit exhausted mid-stream (reported as credit exhausted)"
            );
            LlmError::CreditExhausted
        }
        Some(kind @ ("authentication_error" | "permission_error")) => {
            tracing::warn!(
                error_type = kind,
                "the LLM provider refused the credentials or the account mid-stream (reported as key rejected)"
            );
            LlmError::Authentication
        }
        Some("request_too_large") => LlmError::ContextTooLong,
        Some("invalid_request_error") => {
            LlmError::InvalidRequest(error["message"].as_str().unwrap_or_default().to_owned())
        }
        // `api_error` and anything unknown: the provider's fault.
        _ => LlmError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut Translator, events: &[(&str, Value)]) -> Vec<LlmEvent> {
        events
            .iter()
            .flat_map(|(name, data)| t.handle(name, &data.to_string()).unwrap())
            .collect()
    }

    #[test]
    fn a_failed_page_read_is_reported_and_a_good_one_is_not() {
        let failed = json!({ "type": "web_fetch_tool_result", "tool_use_id": "s",
            "content": { "type": "web_fetch_tool_result_error", "error_code": "url_not_accessible" } });
        let good = json!({ "type": "web_fetch_tool_result", "tool_use_id": "s",
            "content": { "type": "web_fetch_result", "url": "https://a.example/" } });
        assert_eq!(
            fetch_error_code(&failed).as_deref(),
            Some("url_not_accessible")
        );
        assert_eq!(fetch_error_code(&good), None);

        // Streamed: the block is a replayable provider block, with the error
        // event before it.
        let mut t = Translator::new(None);
        let events = feed(
            &mut t,
            &[
                (
                    "content_block_start",
                    json!({ "type": "content_block_start", "index": 3, "content_block": failed }),
                ),
                (
                    "content_block_stop",
                    json!({ "type": "content_block_stop", "index": 3 }),
                ),
            ],
        );
        assert!(
            matches!(
                &events[0],
                LlmEvent::ServerFetchError { block: 3, error_code } if error_code == "url_not_accessible"
            ),
            "{events:?}"
        );
        assert!(
            matches!(&events[1], LlmEvent::ProviderBlock { payload, .. } if *payload == failed)
        );

        // `web_fetch_requests` of a message_delta lands in the usage.
        let events = feed(
            &mut t,
            &[(
                "message_delta",
                json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" },
                        "usage": { "output_tokens": 5, "server_tool_use": { "web_fetch_requests": 2 } } }),
            )],
        );
        assert!(matches!(&events[0], LlmEvent::Usage(u) if u.web_fetches == 2));

        // Thinking tokens come from `output_tokens_details`; absent means 0.
        let events = feed(
            &mut t,
            &[(
                "message_delta",
                json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" },
                        "usage": { "output_tokens": 50, "output_tokens_details": { "thinking_tokens": 30 } } }),
            )],
        );
        assert!(
            matches!(&events[0], LlmEvent::Usage(u) if u.thinking_tokens == 30 && u.output_tokens == 50)
        );
    }

    #[test]
    fn stream_errors_map_billing_to_credit_and_auth_to_key_rejected() {
        let billing = json!({"type": "billing_error", "message": "out of credit"});
        assert_eq!(map_stream_error(&billing), LlmError::CreditExhausted);
        for kind in ["authentication_error", "permission_error"] {
            let error = json!({"type": kind, "message": "no"});
            assert_eq!(map_stream_error(&error), LlmError::Authentication);
        }
    }

    #[test]
    fn invalid_tool_input_is_reported_as_a_non_object_call_not_as_a_stream_error() {
        let mut t = Translator::new(None);
        let events = feed(
            &mut t,
            &[
                (
                    "content_block_start",
                    json!({"type":"content_block_start","index":0,
                    "content_block":{"type":"tool_use","id":"tu","name":"save_note","input":{}}}),
                ),
                (
                    "content_block_delta",
                    json!({"type":"content_block_delta","index":0,
                    "delta":{"type":"input_json_delta","partial_json":"{\"a\": \"Ru"}}),
                ),
                (
                    "content_block_stop",
                    json!({"type":"content_block_stop","index":0}),
                ),
                (
                    "message_delta",
                    json!({"type":"message_delta",
                    "delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":5}}),
                ),
                ("message_stop", json!({"type":"message_stop"})),
            ],
        );
        let finished = events
            .iter()
            .find_map(|e| match e {
                LlmEvent::ToolCallFinished { input, name, .. } => {
                    Some((name.clone(), input.clone()))
                }
                _ => None,
            })
            .expect("the call is still reported");
        assert_eq!(finished.0, "save_note");
        assert!(!finished.1.is_object());
        assert!(matches!(
            events.last(),
            Some(LlmEvent::Finished {
                stop_reason: StopReason::MaxTokens,
                ..
            })
        ));
    }
}
