use async_trait::async_trait;

use crate::learning::{ReviewSample, SchedulerParameters};

/// The optimizer cannot answer for the data it was given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizerError {
    /// Nothing to train or evaluate on.
    #[error("not enough votes")]
    NotEnoughData,
    /// The weights to evaluate are not usable.
    #[error("the scheduler parameters are not valid")]
    InvalidParameters,
    /// The optimizer was asked to stop (shutdown). Not a failure of the data:
    /// nothing should be recorded about it.
    #[error("the optimizer was stopped")]
    Interrupted,
    /// Training or evaluation broke; the text is for the log.
    #[error("optimizer failure: {0}")]
    Failed(String),
}

/// Learns weights from a user's votes and scores weights against votes.
/// Samples must be in chronological order of the vote they predict: the
/// model weighs recent samples more.
#[async_trait]
pub trait ParameterOptimizer: Send + Sync {
    /// Weights fitted to `samples`. Deterministic for the same samples.
    async fn train(&self, samples: &[ReviewSample]) -> Result<SchedulerParameters, OptimizerError>;

    /// Mean log loss of the recall predicted by `parameters` over `samples`
    /// (lower is better); always finite.
    async fn evaluate(
        &self,
        parameters: &SchedulerParameters,
        samples: &[ReviewSample],
    ) -> Result<f64, OptimizerError>;
}
