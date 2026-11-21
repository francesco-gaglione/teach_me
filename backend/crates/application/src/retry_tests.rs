use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use domain::llm::{
    LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmRole, StopReason, Usage,
    collect_response,
};

use crate::fakes::FakeLlm;
use crate::retry::{RetryEnv, RetryPolicy, RetryingLlm};

/// Records requested sleeps; jitter is half the maximum, so delays are exact.
#[derive(Default)]
struct FakeEnv {
    sleeps: Mutex<Vec<Duration>>,
}

#[async_trait]
impl RetryEnv for FakeEnv {
    async fn sleep(&self, duration: Duration) {
        self.sleeps.lock().unwrap().push(duration);
    }

    fn jitter(&self, max: Duration) -> Duration {
        max / 2
    }
}

type Attempt = Result<Vec<Result<LlmEvent, LlmError>>, LlmError>;

/// One scripted attempt per `stream` call: either refused up front or a list
/// of events (that may contain a mid-stream error).
struct Flaky {
    attempts: Mutex<VecDeque<Attempt>>,
    calls: Mutex<u32>,
}

struct Events(VecDeque<Result<LlmEvent, LlmError>>);

#[async_trait]
impl LlmEventStream for Events {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        self.0.pop_front()
    }
}

#[async_trait]
impl LlmProvider for Flaky {
    fn capabilities(&self, role: LlmRole) -> domain::llm::LlmCapabilities {
        FakeLlm::default().capabilities(role)
    }

    async fn stream(&self, _req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        *self.calls.lock().unwrap() += 1;
        let next = self
            .attempts
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra attempt");
        next.map(|events| Box::new(Events(events.into())) as Box<dyn LlmEventStream>)
    }
}

fn setup(attempts: Vec<Attempt>) -> (RetryingLlm, Arc<Flaky>, Arc<FakeEnv>) {
    let flaky = Arc::new(Flaky {
        attempts: Mutex::new(attempts.into()),
        calls: Mutex::new(0),
    });
    let env = Arc::new(FakeEnv::default());
    let retrying = RetryingLlm::new(flaky.clone(), RetryPolicy::default(), env.clone());
    (retrying, flaky, env)
}

#[allow(clippy::unnecessary_wraps)]
fn text(t: &str) -> Result<LlmEvent, LlmError> {
    Ok(LlmEvent::TextDelta {
        block: 0,
        text: t.into(),
    })
}

#[allow(clippy::unnecessary_wraps)]
fn finished() -> Result<LlmEvent, LlmError> {
    Ok(LlmEvent::Finished {
        stop_reason: StopReason::EndTurn,
        usage: Usage::default(),
    })
}

fn request() -> LlmRequest {
    LlmRequest::new(LlmRole::Tutor, "test")
}

async fn answer(llm: &RetryingLlm) -> Result<String, LlmError> {
    Ok(collect_response(llm.stream(request()).await?).await?.text())
}

#[tokio::test]
async fn retries_transient_failures_with_capped_exponential_backoff() {
    let (llm, flaky, env) = setup(vec![
        Err(LlmError::Overloaded),
        Err(LlmError::Network("reset".into())),
        Ok(vec![text("ok"), finished()]),
    ]);
    assert_eq!(answer(&llm).await.unwrap(), "ok");
    assert_eq!(*flaky.calls.lock().unwrap(), 3);
    // Full jitter is half of the ceiling here: 1 s then 2 s -> 0.5 s, 1 s.
    assert_eq!(
        *env.sleeps.lock().unwrap(),
        [Duration::from_millis(500), Duration::from_secs(1)]
    );
}

#[tokio::test]
async fn gives_up_after_three_attempts_with_the_last_error() {
    let (llm, flaky, _) = setup(vec![
        Err(LlmError::Overloaded),
        Err(LlmError::Unavailable),
        Err(LlmError::Timeout),
    ]);
    assert_eq!(answer(&llm).await.unwrap_err(), LlmError::Timeout);
    assert_eq!(*flaky.calls.lock().unwrap(), 3);
}

#[tokio::test]
async fn permanent_errors_are_not_retried() {
    for error in [
        LlmError::Authentication,
        LlmError::CreditExhausted,
        LlmError::InvalidRequest("x".into()),
        LlmError::ContextTooLong,
        LlmError::MalformedResponse("x".into()),
    ] {
        let (llm, flaky, env) = setup(vec![Err(error.clone())]);
        assert_eq!(answer(&llm).await.unwrap_err(), error);
        assert_eq!(*flaky.calls.lock().unwrap(), 1);
        assert!(env.sleeps.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn retry_after_is_respected_up_to_sixty_seconds() {
    let limited = |secs| LlmError::RateLimited {
        retry_after: Some(Duration::from_secs(secs)),
    };
    let (llm, _, env) = setup(vec![Err(limited(7)), Ok(vec![text("ok"), finished()])]);
    assert_eq!(answer(&llm).await.unwrap(), "ok");
    // 7 s plus half of the 500 ms jitter window.
    assert_eq!(*env.sleeps.lock().unwrap(), [Duration::from_millis(7250)]);

    let (llm, flaky, env) = setup(vec![Err(limited(61))]);
    assert_eq!(answer(&llm).await.unwrap_err(), limited(61));
    assert_eq!(*flaky.calls.lock().unwrap(), 1);
    assert!(env.sleeps.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_error_before_any_content_on_an_open_stream_is_retried() {
    let started = || {
        Ok(LlmEvent::Started {
            model: "m".into(),
            request_id: None,
        })
    };
    let (llm, flaky, _) = setup(vec![
        Ok(vec![started(), Err(LlmError::Overloaded)]),
        Ok(vec![started(), text("ok"), finished()]),
    ]);
    assert_eq!(answer(&llm).await.unwrap(), "ok");
    assert_eq!(*flaky.calls.lock().unwrap(), 2);
}

#[tokio::test]
async fn an_error_after_content_is_not_retried_and_reaches_the_caller() {
    let (llm, flaky, env) = setup(vec![Ok(vec![text("parziale"), Err(LlmError::Overloaded)])]);
    let mut stream = llm.stream(request()).await.unwrap();
    assert!(matches!(
        stream.next().await,
        Some(Ok(LlmEvent::TextDelta { .. }))
    ));
    assert_eq!(
        stream.next().await.unwrap().unwrap_err(),
        LlmError::Overloaded
    );
    assert_eq!(*flaky.calls.lock().unwrap(), 1);
    assert!(env.sleeps.lock().unwrap().is_empty());
}

#[tokio::test]
async fn events_read_while_priming_are_not_lost_or_reordered() {
    let (llm, _, _) = setup(vec![Ok(vec![
        Ok(LlmEvent::Started {
            model: "m".into(),
            request_id: Some("r".into()),
        }),
        Ok(LlmEvent::Usage(Usage::default())),
        text("a"),
        text("b"),
        finished(),
    ])]);
    let response = collect_response(llm.stream(request()).await.unwrap())
        .await
        .unwrap();
    assert_eq!(response.text(), "ab");
    assert_eq!(response.request_id.as_deref(), Some("r"));
}

#[tokio::test]
async fn a_stream_with_no_content_passes_through() {
    let (llm, _, _) = setup(vec![Ok(vec![finished()])]);
    let mut stream = llm.stream(request()).await.unwrap();
    assert!(matches!(
        stream.next().await,
        Some(Ok(LlmEvent::Finished { .. }))
    ));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn a_whole_answer_is_retried_on_transient_errors_but_not_after_a_timeout() {
    let (retrying, flaky, env) = setup(vec![
        Err(LlmError::Overloaded),
        Ok(vec![text("ok"), finished()]),
    ]);
    let response = retrying
        .complete(LlmRequest::new(LlmRole::Researcher, "t"))
        .await
        .unwrap();
    assert_eq!(response.text(), "ok");
    assert_eq!(*flaky.calls.lock().unwrap(), 2);
    assert_eq!(env.sleeps.lock().unwrap().len(), 1);

    // A call that ran into its time limit is not paid for twice.
    let (retrying, flaky, _) = setup(vec![Err(LlmError::Timeout)]);
    let error = retrying
        .complete(LlmRequest::new(LlmRole::Researcher, "t"))
        .await
        .unwrap_err();
    assert_eq!(error, LlmError::Timeout);
    assert_eq!(*flaky.calls.lock().unwrap(), 1);
}
