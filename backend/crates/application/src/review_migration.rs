//! Startup migration of the review cards to the current memory model
//! (ADR 0014 point 9): the votes of every card still on the previous model are
//! replayed, and only the memory values change.

use std::sync::Arc;

use domain::{ReviewCardId, ReviewMigrationRepository, ReviewScheduler};

use crate::error::AppError;

/// What one run did.
#[derive(Debug, Default)]
pub struct MigrationReport {
    pub migrated: u32,
    /// Cards that stay on the previous model; the next run tries them again.
    pub failed: Vec<(ReviewCardId, AppError)>,
}

pub struct MigrateReviewModel {
    cards: Arc<dyn ReviewMigrationRepository>,
    scheduler: Arc<dyn ReviewScheduler>,
}

impl MigrateReviewModel {
    #[must_use]
    pub fn new(
        cards: Arc<dyn ReviewMigrationRepository>,
        scheduler: Arc<dyn ReviewScheduler>,
    ) -> Self {
        Self { cards, scheduler }
    }

    /// Migrates every card that needs it, one at a time. A card that fails is
    /// reported and skipped; only failing to list the cards is an error.
    /// Safe to run again after an interruption: migrated cards are not listed.
    pub async fn run(&self) -> Result<MigrationReport, AppError> {
        let pending = self.cards.cards_to_migrate().await?;
        let parameters = self.scheduler.default_parameters();
        let mut report = MigrationReport::default();
        for card in pending {
            match self.migrate(card, &parameters).await {
                Ok(()) => report.migrated += 1,
                Err(e) => report.failed.push((card, e)),
            }
        }
        Ok(report)
    }

    async fn migrate(
        &self,
        card: ReviewCardId,
        parameters: &domain::SchedulerParameters,
    ) -> Result<(), AppError> {
        let history = self.cards.vote_history(card).await?;
        let memory = self.scheduler.replay(parameters, &history)?;
        self.cards.mark_migrated(card, memory).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::{
        CardSchedule, MemoryValues, Rating, RatingIntervals, RepositoryError, ReviewEvent,
        SchedulerError, SchedulerParameters,
    };
    use time::OffsetDateTime;
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;

    const T0: OffsetDateTime = datetime!(2026-11-09 09:00 UTC);

    fn cid(n: u128) -> ReviewCardId {
        ReviewCardId::new(Uuid::from_u128(n))
    }

    #[derive(Default)]
    struct Card {
        history: Vec<ReviewEvent>,
        memory: Option<MemoryValues>,
        migrated: bool,
        /// Reading the history fails.
        broken: bool,
    }

    #[derive(Default)]
    struct FakeCards(Mutex<HashMap<ReviewCardId, Card>>);

    #[async_trait]
    impl ReviewMigrationRepository for FakeCards {
        async fn cards_to_migrate(&self) -> Result<Vec<ReviewCardId>, RepositoryError> {
            let m = self.0.lock().unwrap();
            let mut ids: Vec<_> = m
                .iter()
                .filter(|(_, c)| !c.migrated)
                .map(|(i, _)| *i)
                .collect();
            ids.sort_by_key(ToString::to_string);
            Ok(ids)
        }
        async fn vote_history(
            &self,
            card: ReviewCardId,
        ) -> Result<Vec<ReviewEvent>, RepositoryError> {
            let m = self.0.lock().unwrap();
            let c = m.get(&card).ok_or(RepositoryError::NotFound)?;
            if c.broken {
                return Err(RepositoryError::Storage("disk".into()));
            }
            Ok(c.history.clone())
        }
        async fn mark_migrated(
            &self,
            card: ReviewCardId,
            memory: Option<MemoryValues>,
        ) -> Result<(), RepositoryError> {
            let mut m = self.0.lock().unwrap();
            let c = m.get_mut(&card).ok_or(RepositoryError::NotFound)?;
            if memory.is_some() {
                c.memory = memory;
            }
            c.migrated = true;
            Ok(())
        }
    }

    /// Memory = number of votes; a vote of "easy" makes the replay fail.
    struct Counting;

    impl ReviewScheduler for Counting {
        fn default_parameters(&self) -> SchedulerParameters {
            SchedulerParameters {
                model: "counting".into(),
                weights: Vec::new(),
            }
        }
        fn check_parameters(&self, _: &SchedulerParameters) -> Result<(), SchedulerError> {
            Ok(())
        }
        fn schedule(
            &self,
            _: &CardSchedule,
            _: Rating,
            _: OffsetDateTime,
            _: &SchedulerParameters,
        ) -> Result<CardSchedule, SchedulerError> {
            Err(SchedulerError::InvalidState)
        }
        fn preview(
            &self,
            _: &CardSchedule,
            _: OffsetDateTime,
            _: &SchedulerParameters,
        ) -> Result<RatingIntervals, SchedulerError> {
            Err(SchedulerError::InvalidState)
        }
        fn replay(
            &self,
            _: &SchedulerParameters,
            history: &[ReviewEvent],
        ) -> Result<Option<MemoryValues>, SchedulerError> {
            if history.iter().any(|e| e.rating == Rating::Easy) {
                return Err(SchedulerError::InvalidState);
            }
            #[allow(clippy::cast_precision_loss)]
            let n = history.len() as f64;
            Ok((n > 0.0).then_some(MemoryValues {
                stability: n,
                difficulty: 5.0,
                stability_fast: n / 2.0,
            }))
        }
    }

    fn votes(ratings: &[Rating]) -> Vec<ReviewEvent> {
        ratings
            .iter()
            .map(|&rating| ReviewEvent { rating, at: T0 })
            .collect()
    }

    fn setup(cards: Vec<(u128, Card)>) -> (Arc<FakeCards>, MigrateReviewModel) {
        let repo = Arc::new(FakeCards::default());
        for (n, c) in cards {
            repo.0.lock().unwrap().insert(cid(n), c);
        }
        let uc = MigrateReviewModel::new(repo.clone(), Arc::new(Counting));
        (repo, uc)
    }

    fn card(history: Vec<ReviewEvent>) -> Card {
        Card {
            history,
            ..Card::default()
        }
    }

    #[tokio::test]
    async fn every_card_gets_the_replayed_memory_and_a_card_without_votes_only_the_model() {
        let (repo, uc) = setup(vec![
            (1, card(votes(&[Rating::Good, Rating::Hard, Rating::Again]))),
            (2, card(Vec::new())),
        ]);
        let report = uc.run().await.unwrap();
        assert_eq!((report.migrated, report.failed.len()), (2, 0));
        let m = repo.0.lock().unwrap();
        assert_eq!(m[&cid(1)].memory.unwrap().stability, 3.0);
        assert!(m[&cid(1)].migrated);
        assert_eq!(m[&cid(2)].memory, None);
        assert!(m[&cid(2)].migrated);
    }

    #[tokio::test]
    async fn running_again_changes_nothing() {
        let (repo, uc) = setup(vec![(1, card(votes(&[Rating::Good])))]);
        uc.run().await.unwrap();
        let before = repo.0.lock().unwrap()[&cid(1)].memory;
        let report = uc.run().await.unwrap();
        assert_eq!((report.migrated, report.failed.len()), (0, 0));
        assert_eq!(repo.0.lock().unwrap()[&cid(1)].memory, before);
    }

    #[tokio::test]
    async fn a_card_that_fails_does_not_stop_the_others_and_is_tried_again_next_run() {
        let broken = Card {
            broken: true,
            ..card(votes(&[Rating::Good]))
        };
        let (repo, uc) = setup(vec![
            (1, card(votes(&[Rating::Good]))),
            (2, broken),
            (3, card(votes(&[Rating::Easy]))),
            (4, card(votes(&[Rating::Good, Rating::Good]))),
        ]);
        let report = uc.run().await.unwrap();
        assert_eq!(report.migrated, 2);
        let mut failed: Vec<_> = report.failed.iter().map(|(id, _)| *id).collect();
        failed.sort_by_key(ToString::to_string);
        assert_eq!(failed, [cid(2), cid(3)]);
        {
            let m = repo.0.lock().unwrap();
            assert!(m[&cid(1)].migrated && m[&cid(4)].migrated);
            assert!(!m[&cid(2)].migrated && !m[&cid(3)].migrated);
        }
        // Repaired, the card goes through at the next run.
        repo.0.lock().unwrap().get_mut(&cid(2)).unwrap().broken = false;
        let again = uc.run().await.unwrap();
        assert_eq!((again.migrated, again.failed.len()), (1, 1));
    }
}
