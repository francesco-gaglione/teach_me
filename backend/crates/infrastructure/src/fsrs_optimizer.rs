//! `ParameterOptimizer` on the `fsrs` crate (ADR 0014 point 11): trains
//! FSRS-7 weights and scores weights by log loss. Training is CPU-bound and
//! runs on the blocking pool, never on the async workers.
//!
//! CPU: with card ids, the crate trains FSRS-7 on its windowed path, on the
//! calling thread plus one helper thread of its own (`std::thread::scope`
//! in the crate's `training_windowed.rs`), so at most 2 threads; scoring is on
//! the calling thread alone. Neither uses rayon (the crate calls it only for
//! time-series cross-validation, which this adapter does not use), so a rayon
//! pool would bound nothing. Read from the crate's source, not measured; the
//! windowed path does not exist on aarch64 with NEON, where it was not traced.
//!
//! Shutdown: [`FsrsOptimizer::stop`] makes a running training or scoring end
//! early with [`OptimizerError::Interrupted`] and refuses new ones, so the
//! blocking thread does not hold the process up.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use domain::{
    OptimizerError, ParameterOptimizer, ReviewCardId, ReviewSample, SchedulerError,
    SchedulerParameters,
};
use fsrs::{
    CombinedProgressState, ComputeParametersInput, DEFAULT_PARAMETERS, FSRSError, FSRSItem,
    compute_parameters,
};

use crate::fsrs::{MODEL, item_of, model};

type Progress = Arc<Mutex<CombinedProgressState>>;

#[derive(Debug, Clone, Default)]
pub struct FsrsOptimizer {
    stopped: Arc<AtomicBool>,
    /// The progress state of the training in flight, so `stop` can reach it.
    running: Arc<Mutex<Option<Progress>>>,
}

impl FsrsOptimizer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ends the training in flight, if any, and refuses every later call.
    /// For shutdown: it is not undone.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(progress) = running.as_ref() {
            progress
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .want_abort = true;
        }
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

/// The samples as the crate reads them, with an integer id per card (in order
/// of first appearance: the same samples always get the same ids).
fn items(samples: &[ReviewSample]) -> Result<(Vec<FSRSItem>, Vec<i64>), OptimizerError> {
    if samples.is_empty() {
        return Err(OptimizerError::NotEnoughData);
    }
    // The crate needs a vote to predict and at least one before it.
    if samples.iter().any(|s| s.votes.len() < 2) {
        return Err(OptimizerError::Failed(
            "a sample has no vote before the one to predict".into(),
        ));
    }
    let mut ids: HashMap<ReviewCardId, i64> = HashMap::new();
    let mut card_ids = Vec::with_capacity(samples.len());
    for sample in samples {
        let next = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        card_ids.push(*ids.entry(sample.card).or_insert(next));
    }
    Ok((
        samples.iter().map(|s| item_of(&s.votes)).collect(),
        card_ids,
    ))
}

fn failed(error: &FSRSError) -> OptimizerError {
    match error {
        FSRSError::Interrupted => OptimizerError::Interrupted,
        other => OptimizerError::Failed(other.to_string()),
    }
}

/// Runs `work` on the blocking pool; a panic in the crate becomes an error.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, OptimizerError> + Send + 'static,
) -> Result<T, OptimizerError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| OptimizerError::Failed(format!("the optimizer task did not finish: {e}")))?
}

#[async_trait]
impl ParameterOptimizer for FsrsOptimizer {
    async fn train(&self, samples: &[ReviewSample]) -> Result<SchedulerParameters, OptimizerError> {
        let (train_set, card_ids) = items(samples)?;
        let this = self.clone();
        blocking(move || {
            // Registered before the flag is read: a `stop` between the two is
            // seen by one of them.
            let progress = CombinedProgressState::new_shared();
            *this.running.lock().unwrap_or_else(PoisonError::into_inner) = Some(progress.clone());
            let result = if this.is_stopped() {
                Err(OptimizerError::Interrupted)
            } else {
                compute_parameters(ComputeParametersInput {
                    train_set,
                    card_ids: Some(card_ids),
                    progress: Some(progress),
                    ..ComputeParametersInput::default()
                })
                .map_err(|e| failed(&e))
            };
            *this.running.lock().unwrap_or_else(PoisonError::into_inner) = None;
            let weights = result?;
            if weights.len() != DEFAULT_PARAMETERS.len() || weights.iter().any(|w| !w.is_finite()) {
                return Err(OptimizerError::Failed(
                    "the trained weights are not usable".into(),
                ));
            }
            Ok(SchedulerParameters {
                model: MODEL.to_owned(),
                weights: weights.into_iter().map(f64::from).collect(),
            })
        })
        .await
    }

    async fn evaluate(
        &self,
        parameters: &SchedulerParameters,
        samples: &[ReviewSample],
    ) -> Result<f64, OptimizerError> {
        let model = model(parameters).map_err(|e| match e {
            SchedulerError::InvalidParameters | SchedulerError::InvalidState => {
                OptimizerError::InvalidParameters
            }
        })?;
        let (items, card_ids) = items(samples)?;
        let this = self.clone();
        blocking(move || {
            let evaluation = model
                .evaluate_with_card_ids(items, card_ids, |_| !this.is_stopped())
                .map_err(|e| failed(&e))?;
            let loss = f64::from(evaluation.log_loss);
            if loss.is_finite() {
                Ok(loss)
            } else {
                Err(OptimizerError::Failed("the log loss is not finite".into()))
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use domain::{Rating, ReviewEvent, ReviewScheduler};
    use fsrs::FSRSItem;
    use time::macros::datetime;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use super::*;
    use crate::fsrs::FsrsScheduler;

    const T0: OffsetDateTime = datetime!(2026-01-05 08:00 UTC);

    /// ~400 samples: 40 cards of 11 votes, with a deterministic generator for
    /// ratings and waits. Like real use: minutes at first, then days.
    fn samples() -> Vec<ReviewSample> {
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move |n: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % n
        };
        let mut all: Vec<(OffsetDateTime, ReviewSample)> = Vec::new();
        for card in 0..40_i64 {
            let mut at = T0 + Duration::hours(card * 3);
            let mut votes: Vec<ReviewEvent> = Vec::new();
            for k in 0..11 {
                let rating = match next(10) {
                    0 => Rating::Again,
                    1 | 2 => Rating::Hard,
                    3..=7 => Rating::Good,
                    _ => Rating::Easy,
                };
                votes.push(ReviewEvent { rating, at });
                if k > 0 {
                    all.push((
                        at,
                        ReviewSample {
                            card: ReviewCardId::new(Uuid::from_u128(u128::from(
                                card.unsigned_abs(),
                            ))),
                            votes: votes.clone(),
                        },
                    ));
                }
                at += match k {
                    0 => Duration::minutes(1),
                    1 => Duration::minutes(10),
                    _ => Duration::days(1 + i64::try_from(next(12)).unwrap_or(0) * k),
                };
            }
        }
        all.sort_by_key(|(at, _)| *at);
        all.into_iter().map(|(_, s)| s).collect()
    }

    #[tokio::test]
    async fn it_trains_valid_weights_and_scores_them_with_a_finite_loss() {
        let samples = samples();
        assert!(samples.len() >= 400);
        let optimizer = FsrsOptimizer::new();
        let trained = optimizer.train(&samples).await.unwrap();
        assert_eq!(trained.weights.len(), 34);
        assert_eq!(trained.model, "fsrs7");
        // Usable by the scheduler, and really trained, not the defaults.
        FsrsScheduler::new().check_parameters(&trained).unwrap();
        assert_ne!(trained, FsrsScheduler::new().default_parameters());

        let defaults = FsrsScheduler::new().default_parameters();
        for weights in [&defaults, &trained] {
            let loss = optimizer.evaluate(weights, &samples).await.unwrap();
            assert!(loss.is_finite() && loss > 0.0, "{loss}");
        }
        // Same input, same weights.
        assert_eq!(optimizer.train(&samples).await.unwrap(), trained);
    }

    #[tokio::test]
    async fn scoring_with_card_ids_gives_the_loss_of_scoring_without() {
        let samples = samples();
        let defaults = FsrsScheduler::new().default_parameters();
        let with_ids = FsrsOptimizer::new()
            .evaluate(&defaults, &samples)
            .await
            .unwrap();
        let items: Vec<FSRSItem> = samples.iter().map(|s| item_of(&s.votes)).collect();
        let plain = model(&defaults)
            .unwrap()
            .evaluate(items, |_| true)
            .unwrap()
            .log_loss;
        assert!(
            (with_ids - f64::from(plain)).abs() < 1e-6,
            "{with_ids} != {plain}"
        );
    }

    /// The use case scores the newest 20%: there a card's samples are a chain
    /// of prefixes that does not start at the shortest one.
    #[tokio::test]
    async fn scoring_the_newest_fifth_with_card_ids_gives_the_loss_without() {
        let samples = samples();
        let test = &samples[samples.len() - samples.len() / 5..];
        let defaults = FsrsScheduler::new().default_parameters();
        let with_ids = FsrsOptimizer::new()
            .evaluate(&defaults, test)
            .await
            .unwrap();
        let items: Vec<FSRSItem> = test.iter().map(|s| item_of(&s.votes)).collect();
        let plain = model(&defaults)
            .unwrap()
            .evaluate(items, |_| true)
            .unwrap()
            .log_loss;
        assert!(
            (with_ids - f64::from(plain)).abs() < 1e-6,
            "{with_ids} != {plain}"
        );
    }

    #[tokio::test]
    async fn a_stopped_optimizer_refuses_to_train_or_score() {
        let samples = samples();
        let defaults = FsrsScheduler::new().default_parameters();
        let optimizer = FsrsOptimizer::new();
        optimizer.stop();
        assert_eq!(
            optimizer.train(&samples).await,
            Err(OptimizerError::Interrupted)
        );
        assert_eq!(
            optimizer.evaluate(&defaults, &samples).await,
            Err(OptimizerError::Interrupted)
        );
    }

    /// What `stop` relies on: the crate reads the flag during training even on
    /// a set as small as the smallest one the use case sends (300 votes).
    #[test]
    fn the_crate_ends_a_training_whose_abort_flag_is_set() {
        let (train_set, card_ids) = items(&samples()).unwrap();
        let progress = CombinedProgressState::new_shared();
        progress.lock().unwrap().want_abort = true;
        let result = compute_parameters(ComputeParametersInput {
            train_set,
            card_ids: Some(card_ids),
            progress: Some(progress),
            ..ComputeParametersInput::default()
        });
        assert!(matches!(result, Err(FSRSError::Interrupted)), "{result:?}");
    }

    #[tokio::test]
    async fn stopping_during_a_training_ends_it() {
        let samples = samples();
        let optimizer = FsrsOptimizer::new();
        let running = {
            let optimizer = optimizer.clone();
            let samples = samples.clone();
            tokio::spawn(async move { optimizer.train(&samples).await })
        };
        // Stop as soon as the training has registered itself, then wait for it.
        while optimizer.running.lock().unwrap().is_none() {
            tokio::task::yield_now().await;
        }
        optimizer.stop();
        let result = running.await.unwrap();
        // It either noticed the flag or had already finished: never a failure.
        assert!(
            matches!(result, Ok(_) | Err(OptimizerError::Interrupted)),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn bad_input_is_an_error_not_a_panic() {
        let optimizer = FsrsOptimizer::new();
        let defaults = FsrsScheduler::new().default_parameters();
        assert_eq!(
            optimizer.train(&[]).await,
            Err(OptimizerError::NotEnoughData)
        );
        let one = ReviewSample {
            card: ReviewCardId::new(Uuid::from_u128(1)),
            votes: vec![ReviewEvent {
                rating: Rating::Good,
                at: T0,
            }],
        };
        assert!(optimizer.evaluate(&defaults, &[one]).await.is_err());
        let mut bad = defaults;
        bad.weights.pop();
        assert_eq!(
            optimizer.evaluate(&bad, &samples()).await,
            Err(OptimizerError::InvalidParameters)
        );
    }
}
