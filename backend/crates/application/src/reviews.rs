//! The spaced-repetition review: what is due, voting, undoing and suspending
//! (`docs/contracts/reviews.md`).

use std::sync::Arc;

use domain::{
    CardStatus, Clock, DueCursor, DueTopic, IdGenerator, MemoryBasis, Page, PageRequest, Rating,
    RatingIntervals, RepositoryError, ReviewCardId, ReviewItem, ReviewLog, ReviewLogId,
    ReviewRepository, ReviewScheduler, ReviewState, SchedulerParameters, SuspendedCursor, TopicId,
    UNDO_WINDOW, UserId, estimated_minutes,
};
use time::OffsetDateTime;

use crate::error::{AppError, ConflictReason};
use crate::paging::paginate;
use crate::time_util::now_millis;

/// A card as the review screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewView {
    pub item: ReviewItem,
    /// What each vote would do if given now; `None` for a suspended card.
    pub intervals_preview: Option<RatingIntervals>,
}

/// `GET /reviews/due`.
#[derive(Debug, Clone, PartialEq)]
pub struct DueReviews {
    pub page: Page<ReviewView, DueCursor>,
    /// Every card due now, not only the ones of the page.
    pub due_count: u32,
    pub estimated_minutes: u32,
    pub next_due_at: Option<OffsetDateTime>,
    /// Topics with cards due now, over all the user's cards whatever the filter.
    pub topics: Vec<DueTopic>,
}

/// `GET /reviews/suspended`.
#[derive(Debug, Clone, PartialEq)]
pub struct SuspendedReviews {
    pub page: Page<ReviewView, SuspendedCursor>,
    /// Every card the learner suspended, not only the ones of the page.
    pub suspended_count: u32,
}

/// A vote as the app sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vote {
    pub rating: Rating,
    /// When the learner voted, by the app's clock; `None` means now.
    pub reviewed_at: Option<OffsetDateTime>,
    /// The `review_count` the app read: the version of the card it voted on.
    pub review_count: u32,
}

pub struct ReviewService {
    reviews: Arc<dyn ReviewRepository>,
    scheduler: Arc<dyn ReviewScheduler>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl ReviewService {
    #[must_use]
    pub fn new(
        reviews: Arc<dyn ReviewRepository>,
        scheduler: Arc<dyn ReviewScheduler>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            reviews,
            scheduler,
            clock,
            ids,
        }
    }

    /// The weights the user's cards are scheduled with.
    async fn parameters(&self, user: UserId) -> Result<WeightsInUse, AppError> {
        Ok(weights_in_use(&*self.reviews, &*self.scheduler, user).await?)
    }

    /// The card with memory values computed by the current model with the
    /// weights in use. One that was not (still on the previous model, or
    /// computed with other weights) has its votes replayed first, exactly as
    /// the startup migration and the rebuild do, so a card is never scheduled
    /// from a mix of models or weights. Previews and votes both read it, so
    /// the interval shown is the one applied. Only memory changes: `state`,
    /// `due` and the counts stay.
    async fn aligned(
        &self,
        mut item: ReviewItem,
        weights: &WeightsInUse,
    ) -> Result<ReviewItem, AppError> {
        let wanted = MemoryBasis::current(weights.stamp);
        // A card never voted has no memory to be out of date.
        if item.basis == wanted || item.card.schedule.state == ReviewState::New {
            return Ok(item);
        }
        let history = self.reviews.card_history(item.card.id).await?;
        if let Some(memory) = self.scheduler.replay(&weights.parameters, &history)? {
            let schedule = &mut item.card.schedule;
            schedule.stability = memory.stability;
            schedule.difficulty = memory.difficulty;
            schedule.stability_fast = memory.stability_fast;
        }
        if !item.basis.current_model {
            // As the migration: the previous model had no short steps to resume.
            item.card.schedule.learning_step = 0;
        }
        item.basis = wanted;
        Ok(item)
    }

    fn view(
        &self,
        item: ReviewItem,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<ReviewView, AppError> {
        let intervals_preview = if item.card.status == CardStatus::Active {
            Some(
                self.scheduler
                    .preview(&item.card.schedule, now, parameters)?,
            )
        } else {
            None
        };
        Ok(ReviewView {
            item,
            intervals_preview,
        })
    }

    pub async fn due(
        &self,
        user: UserId,
        topic: Option<TopicId>,
        request: PageRequest<DueCursor>,
    ) -> Result<DueReviews, AppError> {
        let now = now_millis(&*self.clock);
        if let Some(topic) = topic
            && !self.reviews.topic_exists(user, topic).await?
        {
            return Err(AppError::NotFound);
        }
        let weights = self.parameters(user).await?;
        let page = paginate(
            request,
            |after, limit| self.reviews.list_due(user, now, topic, after, limit),
            |item| DueCursor {
                due: item.card.schedule.due,
                id: item.card.id,
            },
        )
        .await?;
        let stats = self.reviews.due_stats(user, now, topic).await?;
        let topics = self.reviews.due_topics(user, now).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for item in page.items {
            // One card that cannot be replayed must not take the list down.
            let item = self.aligned_or_stored(item, &weights).await;
            items.push(self.view(item, now, &weights.parameters)?);
        }
        Ok(DueReviews {
            page: Page {
                items,
                next: page.next,
            },
            due_count: stats.due_count,
            estimated_minutes: estimated_minutes(stats.due_count),
            next_due_at: stats.next_due_at,
            topics,
        })
    }

    /// [`Self::aligned`] for a preview only: a card that cannot be replayed
    /// previews from its stored state, and the error is logged.
    async fn aligned_or_stored(&self, item: ReviewItem, weights: &WeightsInUse) -> ReviewItem {
        match self.aligned(item.clone(), weights).await {
            Ok(aligned) => aligned,
            Err(error) => {
                tracing::error!(card = %item.card.id, %error, "could not replay the review card; previewing its stored state");
                item
            }
        }
    }

    async fn item(&self, user: UserId, id: ReviewCardId) -> Result<ReviewItem, AppError> {
        self.reviews.get(user, id).await?.ok_or(AppError::NotFound)
    }

    /// Applies a vote (`reviews.md` §4).
    pub async fn rate(
        &self,
        user: UserId,
        id: ReviewCardId,
        vote: Vote,
    ) -> Result<ReviewView, AppError> {
        let item = self.item(user, id).await?;
        let weights = self.parameters(user).await?;
        if item.card.status != CardStatus::Active {
            return Err(AppError::Conflict(ConflictReason::CardSuspended));
        }
        if item.card.schedule.review_count != vote.review_count {
            return Err(AppError::Conflict(ConflictReason::StaleReview));
        }
        // A vote that races a change of the card is refused by the repository
        // (its `review_count` guard), so the replay cannot overwrite a newer vote.
        let item = self.aligned(item, &weights).await?;
        let before = item.card.schedule;
        let now = now_millis(&*self.clock);
        // Never in the future, never before the previous vote (or, for a card
        // never voted, before the card existed).
        let floor = before.last_reviewed_at.unwrap_or(item.card.created_at);
        let reviewed_at = vote.reviewed_at.unwrap_or(now).min(now).max(floor);
        let after =
            self.scheduler
                .schedule(&before, vote.rating, reviewed_at, &weights.parameters)?;
        let log = ReviewLog {
            id: ReviewLogId::new(self.ids.new_id()),
            card_id: id,
            rating: vote.rating,
            reviewed_at,
            before,
            after,
            created_at: now,
            undone_at: None,
        };
        match self
            .reviews
            .apply_review(vote.review_count, &after, &log, weights.stamp)
            .await
        {
            Ok(()) => {}
            // A concurrent vote or suspension got there first.
            Err(RepositoryError::Conflict) => {
                let suspended = self
                    .reviews
                    .get(user, id)
                    .await?
                    .is_some_and(|i| i.card.status != CardStatus::Active);
                return Err(AppError::Conflict(if suspended {
                    ConflictReason::CardSuspended
                } else {
                    ConflictReason::StaleReview
                }));
            }
            Err(e) => return Err(e.into()),
        }
        let mut item = item;
        item.card.schedule = after;
        // The vote may have ended the consolidation: the flag comes from the
        // votes just stored, not from the card as read before.
        item.card.to_consolidate = self.item(user, id).await?.card.to_consolidate;
        self.view(item, now, &weights.parameters)
    }

    /// Undoes the last vote of the card, if recent (`reviews.md` §5).
    pub async fn undo(&self, user: UserId, id: ReviewCardId) -> Result<ReviewView, AppError> {
        self.item(user, id).await?;
        let now = now_millis(&*self.clock);
        let log = self
            .reviews
            .last_log(id)
            .await?
            .filter(|log| log.undone_at.is_none())
            .ok_or(AppError::Conflict(ConflictReason::NothingToUndo))?;
        if now - log.created_at > UNDO_WINDOW {
            return Err(AppError::Conflict(ConflictReason::UndoExpired));
        }
        match self.reviews.undo_review(log.id, now).await {
            Ok(()) => {}
            // Undone meanwhile.
            Err(RepositoryError::Conflict) => {
                return Err(AppError::Conflict(ConflictReason::NothingToUndo));
            }
            // Given on a previous model: as good as expired.
            Err(RepositoryError::RuleViolated) => {
                return Err(AppError::Conflict(ConflictReason::UndoExpired));
            }
            Err(e) => return Err(e.into()),
        }
        // What is stored now: the memory that comes back may not match the
        // weights in use any more, and then the preview is from a replay. The
        // undo is already saved, so a failed replay must not turn it into an
        // error (a retry would then get 409).
        let weights = self.parameters(user).await?;
        let item = self
            .aligned_or_stored(self.item(user, id).await?, &weights)
            .await;
        self.view(item, now, &weights.parameters)
    }

    /// Takes the card out of the review. Idempotent.
    pub async fn suspend(&self, user: UserId, id: ReviewCardId) -> Result<ReviewView, AppError> {
        let now = now_millis(&*self.clock);
        self.reviews.suspend(user, id, now).await?;
        let weights = self.parameters(user).await?;
        // A suspended card has no preview: nothing to replay for.
        self.view(self.item(user, id).await?, now, &weights.parameters)
    }

    /// Puts a card the learner suspended back in the review (`reviews.md`
    /// §3.2). Idempotent on an active card; a superseded one is refused.
    pub async fn unsuspend(&self, user: UserId, id: ReviewCardId) -> Result<ReviewView, AppError> {
        match self.reviews.unsuspend(user, id).await {
            Ok(()) => {}
            Err(RepositoryError::Conflict) => {
                return Err(AppError::Conflict(ConflictReason::CardSuperseded));
            }
            Err(e) => return Err(e.into()),
        }
        let now = now_millis(&*self.clock);
        let weights = self.parameters(user).await?;
        // The change is saved: a failed replay only costs the preview its precision.
        let item = self
            .aligned_or_stored(self.item(user, id).await?, &weights)
            .await;
        self.view(item, now, &weights.parameters)
    }

    /// The cards the learner suspended, most recent first (`reviews.md` §3.3).
    pub async fn suspended(
        &self,
        user: UserId,
        request: PageRequest<SuspendedCursor>,
    ) -> Result<SuspendedReviews, AppError> {
        let now = now_millis(&*self.clock);
        let weights = self.parameters(user).await?;
        let page = paginate(
            request,
            |after, limit| self.reviews.list_suspended(user, after, limit),
            |item| SuspendedCursor {
                // A suspended card always has its instant; the epoch only
                // stands in for a row that breaks that.
                suspended_at: item
                    .card
                    .suspension
                    .map_or(OffsetDateTime::UNIX_EPOCH, |s| s.at),
                id: item.card.id,
            },
        )
        .await?;
        let suspended_count = self.reviews.suspended_count(user).await?;
        let mut items = Vec::with_capacity(page.items.len());
        for item in page.items {
            items.push(self.view(item, now, &weights.parameters)?);
        }
        Ok(SuspendedReviews {
            page: Page {
                items,
                next: page.next,
            },
            suspended_count,
        })
    }
}

/// The weights a user's cards are scheduled with, and the stamp the memory
/// computed with them carries.
#[derive(Debug, Clone)]
pub(crate) struct WeightsInUse {
    pub parameters: SchedulerParameters,
    /// When the user's weights were adopted; `None` for the defaults.
    pub stamp: Option<OffsetDateTime>,
}

/// The weights a user's cards are scheduled with: their own when they have
/// some the scheduler can use, otherwise the defaults (no stamp). Weights that
/// cannot be used are logged and never fail a request.
pub(crate) async fn weights_in_use(
    reviews: &dyn ReviewRepository,
    scheduler: &dyn ReviewScheduler,
    user: UserId,
) -> Result<WeightsInUse, RepositoryError> {
    if let Some(own) = reviews.parameters(user).await? {
        match scheduler.check_parameters(&own.parameters) {
            Ok(()) => {
                return Ok(WeightsInUse {
                    parameters: own.parameters,
                    stamp: Some(own.adopted_at),
                });
            }
            Err(error) => {
                tracing::warn!(%user, %error, "the stored review weights are not usable; using the defaults");
            }
        }
    }
    Ok(WeightsInUse {
        parameters: scheduler.default_parameters(),
        stamp: None,
    })
}
