//! Retry decorator for any [`LlmProvider`] (ADR 0004).
//!
//! Lives here, not in the adapters, so every adapter gets the same policy and
//! the policy is testable without a network. It only retries before the first
//! content event: after that the user may already have seen text, and the
//! message fails so that "Riprova" regenerates it.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmResponse,
    LlmRole,
};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Total attempts, the first one included.
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    /// A longer `retry-after` than this is not waited for: the error is returned.
    pub max_retry_after: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(20),
            max_retry_after: Duration::from_secs(60),
        }
    }
}

/// Time and randomness, injected so tests wait for nothing.
#[async_trait]
pub trait RetryEnv: Send + Sync {
    async fn sleep(&self, duration: Duration);
    /// A random duration in `[0, max]`.
    fn jitter(&self, max: Duration) -> Duration;
}

pub struct SystemRetryEnv;

#[async_trait]
impl RetryEnv for SystemRetryEnv {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    fn jitter(&self, max: Duration) -> Duration {
        // `RandomState` is seeded randomly per instance: enough entropy for
        // spreading retries, without a dependency on a RNG crate.
        let random = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        let millis = u64::try_from(max.as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(random % millis.saturating_add(1))
    }
}

pub struct RetryingLlm {
    inner: Arc<dyn LlmProvider>,
    policy: RetryPolicy,
    env: Arc<dyn RetryEnv>,
}

impl RetryingLlm {
    #[must_use]
    pub fn new(inner: Arc<dyn LlmProvider>, policy: RetryPolicy, env: Arc<dyn RetryEnv>) -> Self {
        Self { inner, policy, env }
    }

    /// How long to wait before attempt `next_attempt` (2-based), or `None` to
    /// give up on this error.
    fn delay(&self, error: &LlmError, failed_attempts: u32) -> Option<Duration> {
        if !error.is_retryable() {
            return None;
        }
        if let LlmError::RateLimited {
            retry_after: Some(wait),
        } = error
        {
            if *wait > self.policy.max_retry_after {
                return None;
            }
            return Some(*wait + self.env.jitter(Duration::from_millis(500)));
        }
        let exponent = failed_attempts.saturating_sub(1).min(16);
        let ceiling = self
            .policy
            .base_delay
            .saturating_mul(1 << exponent)
            .min(self.policy.max_delay);
        Some(self.env.jitter(ceiling))
    }

    /// Opens the stream and reads until the first content event.
    async fn attempt(&self, req: &LlmRequest) -> Result<PrimedStream, LlmError> {
        let mut stream = self.inner.stream(req.clone()).await?;
        let mut buffered = VecDeque::new();
        loop {
            match stream.next().await {
                Some(Ok(event)) => {
                    let content = event.is_content();
                    buffered.push_back(event);
                    if content {
                        break;
                    }
                }
                Some(Err(e)) => return Err(e),
                None => break,
            }
        }
        Ok(PrimedStream {
            buffered,
            rest: stream,
        })
    }
}

/// The events already read while looking for the first content event, then
/// the live stream.
struct PrimedStream {
    buffered: VecDeque<LlmEvent>,
    rest: Box<dyn LlmEventStream>,
}

#[async_trait]
impl LlmEventStream for PrimedStream {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        if let Some(event) = self.buffered.pop_front() {
            return Some(Ok(event));
        }
        self.rest.next().await
    }
}

#[async_trait]
impl LlmProvider for RetryingLlm {
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities {
        self.inner.capabilities(role)
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        let mut attempt = 1;
        loop {
            match self.attempt(&req).await {
                Ok(stream) => return Ok(Box::new(stream)),
                Err(error) => {
                    let Some(delay) = self
                        .delay(&error, attempt)
                        .filter(|_| attempt < self.policy.max_attempts)
                    else {
                        return Err(error);
                    };
                    tracing::warn!(
                        error = %error.log_summary(), attempt, delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                        trace_label = req.trace_label, "LLM call failed before any content; retrying"
                    );
                    self.env.sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Same policy for a whole answer. Nothing was shown to anyone before it
    /// completes, so any transient failure may be retried, except a timeout:
    /// these calls run for minutes and cost money, and a call that timed out
    /// would only time out again.
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let mut attempt = 1;
        loop {
            match self.inner.complete(req.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let Some(delay) = Some(&error)
                        .filter(|e| **e != LlmError::Timeout)
                        .and_then(|e| self.delay(e, attempt))
                        .filter(|_| attempt < self.policy.max_attempts)
                    else {
                        return Err(error);
                    };
                    tracing::warn!(
                        error = %error.log_summary(), attempt, delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                        trace_label = req.trace_label, "LLM call failed; retrying"
                    );
                    self.env.sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }
}
