//! Shared test doubles for the HTTP tests.
#![allow(dead_code)]

pub mod harness;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmRole,
    StopReason, StructuredOutput, Usage,
};
use tokio::sync::Notify;

use api::state::{AppState, AuthSetup};
use domain::{TokenIssuer, UserId};
use infrastructure::sqlite::Pool;
use infrastructure::{Argon2idHasher, OsTokenIssuer, ServerKey, XChaChaSecretCipher};

/// The bearer token of the signed-in `owner` in these tests.
pub const TOKEN: &str = "tm_test-token-0123456789-abcdefghijklmnop";

/// Credential adapters with the cheapest Argon2 cost: tests hash a lot.
pub fn auth() -> AuthSetup {
    AuthSetup {
        hasher: Arc::new(Argon2idHasher::with_costs(8, 1, 1).unwrap()),
        cipher: Arc::new(XChaChaSecretCipher::new(&ServerKey::from_bytes([3; 32])).unwrap()),
        tokens: Arc::new(OsTokenIssuer),
        llm_provider: "openrouter".into(),
    }
}

/// Gives `owner` an LLM key and a session whose token is [`TOKEN`]. Safe to
/// call again on the same database (a second app instance, a restart).
pub async fn sign_in_owner(state: &AppState, pool: &Pool) {
    state
        .account
        .set_llm_key(UserId::OWNER, "sk-test-owner")
        .await
        .unwrap();
    let digest = OsTokenIssuer.digest(TOKEN);
    sqlx::query(
        "INSERT OR IGNORE INTO sessions (id, user_id, token_digest, device_name, created_at, \
         last_seen_at, expires_at) VALUES (?, ?, ?, NULL, '2026-01-01T00:00:00.000Z', \
         '2026-01-01T00:00:00.000Z', '2099-01-01T00:00:00.000Z')",
    )
    .bind("00000000-0000-7000-8000-0000000000aa")
    .bind(UserId::OWNER.to_string())
    .bind(digest.as_bytes())
    .execute(pool)
    .await
    .unwrap();
}

#[derive(Clone)]
pub enum Step {
    Delta(&'static str),
    /// Parks the reply until the test calls `notify_one`.
    Wait(Arc<Notify>),
    Fail(LlmError),
    /// Any event, as the adapter would emit it (server-side tool activity).
    Event(LlmEvent),
    /// A complete tool call; the reply then ends with `stop_reason: tool_use`.
    Tool {
        id: &'static str,
        name: &'static str,
        input: serde_json::Value,
    },
}

/// Fake model: each `stream` call plays the next script, then `Finished`.
#[derive(Default)]
pub struct ScriptedLlm {
    scripts: Mutex<VecDeque<Vec<Step>>>,
    pub requests: Mutex<Vec<LlmRequest>>,
}

impl ScriptedLlm {
    pub fn with(scripts: Vec<Vec<Step>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into()),
            requests: Mutex::default(),
        })
    }
}

/// Remaining steps, whether the stream is over, whether a tool was called.
struct Script(VecDeque<Step>, bool, bool);

#[async_trait]
impl LlmEventStream for Script {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        loop {
            match self.0.pop_front() {
                Some(Step::Delta(text)) => {
                    return Some(Ok(LlmEvent::TextDelta {
                        block: 0,
                        text: text.to_owned(),
                    }));
                }
                Some(Step::Tool { id, name, input }) => {
                    self.2 = true;
                    return Some(Ok(LlmEvent::ToolCallFinished {
                        block: 1,
                        id: id.to_owned(),
                        name: name.to_owned(),
                        input,
                    }));
                }
                Some(Step::Event(event)) => return Some(Ok(event)),
                Some(Step::Wait(gate)) => gate.notified().await,
                Some(Step::Fail(e)) => {
                    self.0.clear();
                    self.1 = true;
                    return Some(Err(e));
                }
                None if self.1 => return None,
                None => {
                    self.1 = true;
                    return Some(Ok(LlmEvent::Finished {
                        stop_reason: if self.2 {
                            StopReason::ToolUse
                        } else {
                            StopReason::EndTurn
                        },
                        usage: Usage::default(),
                    }));
                }
            }
        }
    }
}

#[async_trait]
impl LlmProvider for ScriptedLlm {
    fn capabilities(&self, _role: LlmRole) -> LlmCapabilities {
        LlmCapabilities {
            tools: true,
            parallel_tool_calls: false,
            forced_tool_choice: false,
            structured_output: StructuredOutput::None,
            streaming_tool_input: false,
            prompt_caching: false,
            max_cache_breakpoints: 0,
            context_window_tokens: 100_000,
            max_output_tokens: 4096,
            effort: None,
            server_web_search: true,
            server_web_fetch: false,
        }
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        self.requests.lock().unwrap().push(req);
        let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
        Ok(Box::new(Script(script.into(), false, false)))
    }
}
