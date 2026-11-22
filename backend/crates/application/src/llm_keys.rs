//! The LLM key of a user, and how it reaches the provider (ADR 0008 §6).
//!
//! The key is resolved from the owner of what the work is about, never from
//! the server's configuration, and travels with the *call*: work that needs
//! the model runs inside [`with_key`], and [`KeyScopedLlm`] puts the key on
//! every request made from there. Outside a scope a request carries no key
//! and the adapter refuses it (`Authentication`): forgetting the scope fails
//! closed, it can never run on another user's credit.

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmResponse,
    LlmRole, Usage,
};
use domain::{CryptoError, RepositoryError, Secret, SecretCipher, UserId, UserRepository};

use crate::budget::{LlmBudget, billable_tokens};
use crate::error::AppError;

/// Whose work is running: the key to call the provider with and the user the
/// tokens are counted against.
#[derive(Clone)]
struct Scope {
    user: UserId,
    key: Secret,
}

tokio::task_local! {
    static SCOPE: Scope;
}

/// Runs `work` as `user` with `key` as the key of every model call it makes
/// (also the ones of the futures it awaits; not of tasks it spawns, which must
/// scope their own). The tokens those calls use count against the user's
/// daily budget.
pub async fn with_key<F: Future>(user: UserId, key: Secret, work: F) -> F::Output {
    SCOPE.scope(Scope { user, key }, work).await
}

/// Why a key could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("the user has no LLM key")]
    Missing,
    /// Sealed with another server key, or tampered with. The user has to
    /// enter the key again, which is what "missing" asks for.
    #[error("the stored LLM key cannot be opened")]
    Unreadable,
    #[error("storage failure: {0}")]
    Storage(#[source] RepositoryError),
}

impl From<KeyError> for AppError {
    fn from(e: KeyError) -> Self {
        match e {
            KeyError::Missing | KeyError::Unreadable => Self::LlmKeyMissing,
            KeyError::Storage(e) => e.into(),
        }
    }
}

/// Context bound to a sealed key: the owner's id.
#[must_use]
pub fn key_context(user: UserId) -> Vec<u8> {
    user.to_string().into_bytes()
}

pub struct LlmKeys {
    users: Arc<dyn UserRepository>,
    cipher: Arc<dyn SecretCipher>,
    budget: Option<Arc<LlmBudget>>,
}

impl LlmKeys {
    #[must_use]
    pub fn new(users: Arc<dyn UserRepository>, cipher: Arc<dyn SecretCipher>) -> Self {
        Self {
            users,
            cipher,
            budget: None,
        }
    }

    /// Makes [`require`](Self::require) refuse users who spent their daily
    /// token budget.
    #[must_use]
    pub fn with_budget(mut self, budget: Arc<LlmBudget>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// `429 llm_budget_exceeded` when the user spent the day's tokens.
    pub async fn check_budget(&self, user: UserId) -> Result<(), AppError> {
        match &self.budget {
            Some(budget) => budget.check(user).await,
            None => Ok(()),
        }
    }

    pub async fn for_user(&self, user: UserId) -> Result<Secret, KeyError> {
        let sealed = self
            .users
            .llm_key(user)
            .await
            .map_err(KeyError::Storage)?
            .ok_or(KeyError::Missing)?;
        self.cipher
            .open(&key_context(user), &sealed)
            .map_err(|e| match e {
                CryptoError::Unreadable | CryptoError::Failed => {
                    tracing::error!(%user, "a stored LLM key cannot be opened (wrong TEACH_ME_SECRET_KEY?)");
                    KeyError::Unreadable
                }
            })
    }

    /// For requests whose purpose is an LLM operation: `409 llm_key_missing`
    /// or `429 llm_budget_exceeded` before anything is created.
    pub async fn require(&self, user: UserId) -> Result<Secret, AppError> {
        let key = self.for_user(user).await?;
        self.check_budget(user).await?;
        Ok(key)
    }
}

/// Puts the scoped key on the requests that have none and counts the tokens
/// of the calls made inside a scope against the scope's user.
pub struct KeyScopedLlm {
    inner: Arc<dyn LlmProvider>,
    budget: Option<Arc<LlmBudget>>,
}

impl KeyScopedLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self {
            inner,
            budget: None,
        }
    }

    /// Records the usage of every call into `budget`.
    #[must_use]
    pub fn with_budget(mut self, budget: Arc<LlmBudget>) -> Self {
        self.budget = Some(budget);
        self
    }

    fn keyed(mut req: LlmRequest) -> LlmRequest {
        if req.api_key.is_none() {
            req.api_key = SCOPE.try_with(|s| s.key.clone()).ok();
        }
        req
    }

    /// The meter for a call made now, if there is a budget and a user.
    fn meter(&self) -> Option<(Arc<LlmBudget>, UserId)> {
        let budget = self.budget.as_ref().filter(|b| b.is_enabled())?;
        let user = SCOPE.try_with(|s| s.user).ok()?;
        Some((budget.clone(), user))
    }
}

#[async_trait]
impl LlmProvider for KeyScopedLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        let meter = self.meter();
        let stream = self.inner.stream(Self::keyed(req)).await?;
        Ok(match meter {
            Some((budget, user)) => Box::new(MeteredStream {
                inner: stream,
                budget,
                user,
                seen: Usage::default(),
                recorded: false,
            }),
            None => stream,
        })
    }

    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let meter = self.meter();
        let response = self.inner.complete(Self::keyed(req)).await?;
        if let Some((budget, user)) = meter {
            budget.record(user, &response.usage).await;
        }
        Ok(response)
    }
}

/// Counts the usage of one stream once, when it ends. A stream dropped early
/// (cancelled reply) still pays for what the provider had reported so far.
struct MeteredStream {
    inner: Box<dyn LlmEventStream>,
    budget: Arc<LlmBudget>,
    user: UserId,
    /// Usage events are cumulative: the last one is the total.
    seen: Usage,
    recorded: bool,
}

impl MeteredStream {
    /// Both kinds of event are cumulative totals; a provider that reports
    /// only on one of them leaves zeros on the other, so the larger wins.
    fn keep_larger(&mut self, usage: Usage) {
        if billable_tokens(&usage) >= billable_tokens(&self.seen) {
            self.seen = usage;
        }
    }

    async fn settle(&mut self) {
        if !self.recorded {
            self.recorded = true;
            self.budget.record(self.user, &self.seen).await;
        }
    }
}

#[async_trait]
impl LlmEventStream for MeteredStream {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        let item = self.inner.next().await;
        match &item {
            Some(Ok(LlmEvent::Usage(u))) => self.keep_larger(*u),
            Some(Ok(LlmEvent::Finished { usage, .. })) => {
                self.keep_larger(*usage);
                self.settle().await;
            }
            Some(Err(_)) | None => self.settle().await,
            Some(Ok(_)) => {}
        }
        item
    }
}

impl Drop for MeteredStream {
    fn drop(&mut self) {
        if self.recorded {
            return;
        }
        // Cancelled mid-flight: settle in the background (no runtime, no count).
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (budget, user, seen) = (self.budget.clone(), self.user, self.seen);
            runtime.spawn(async move { budget.record(user, &seen).await });
        }
    }
}
