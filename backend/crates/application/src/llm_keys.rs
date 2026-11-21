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
    LlmCapabilities, LlmError, LlmEventStream, LlmProvider, LlmRequest, LlmResponse, LlmRole,
};
use domain::{CryptoError, RepositoryError, Secret, SecretCipher, UserId, UserRepository};

use crate::error::AppError;

tokio::task_local! {
    static KEY: Secret;
}

/// Runs `work` with `key` as the key of every model call it makes (also the
/// ones of the futures it awaits; not of tasks it spawns, which must scope
/// their own).
pub async fn with_key<F: Future>(key: Secret, work: F) -> F::Output {
    KEY.scope(key, work).await
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
}

impl LlmKeys {
    #[must_use]
    pub fn new(users: Arc<dyn UserRepository>, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { users, cipher }
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
    /// before anything is created.
    pub async fn require(&self, user: UserId) -> Result<Secret, AppError> {
        Ok(self.for_user(user).await?)
    }
}

/// Puts the scoped key on the requests that have none.
pub struct KeyScopedLlm {
    inner: Arc<dyn LlmProvider>,
}

impl KeyScopedLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>) -> Self {
        Self { inner }
    }

    fn keyed(mut req: LlmRequest) -> LlmRequest {
        if req.api_key.is_none() {
            req.api_key = KEY.try_with(Clone::clone).ok();
        }
        req
    }
}

#[async_trait]
impl LlmProvider for KeyScopedLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        self.inner.stream(Self::keyed(req)).await
    }

    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        self.inner.complete(Self::keyed(req)).await
    }
}
