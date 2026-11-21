//! SQLite adapter of the review queue (`docs/contracts/reviews.md`, phase 8).

use async_trait::async_trait;
use domain::{
    CardSchedule, CardVotes, DueCursor, DueStats, DueTopic, MemoryBasis, MemoryValues,
    OptimizationAttempt, OptimizationOutcome, Rating, RepositoryError, ReviewCardId, ReviewEvent,
    ReviewItem, ReviewLog, ReviewLogId, ReviewMigrationRepository, ReviewOptimizationRepository,
    ReviewRepository, ReviewState, SchedulerParameters, SuspendedCursor, TopicId, UserId,
    UserWeights, VoteRecord,
};
use serde_json::{Value, json};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::lessons::{card_from_row, consolidation_ratings};
use crate::repositories::{corrupt, format_ts, parse_id, parse_ts, storage};

#[derive(Clone)]
pub struct SqliteReviewRepository {
    pool: SqlitePool,
}

impl SqliteReviewRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Gives a reason to the cards left `suspended` without one, by the rule of
/// migration 0024: `superseded` if the record they come from is, else `user`;
/// the instant is the one of the repair, since the real one is unknown. Runs
/// at startup (a rolled-back binary writes such rows) and before the use cases
/// that read the reason, rather than deriving it on every read: one idempotent
/// statement instead of a second interpretation of the columns in each query.
pub(crate) async fn backfill_suspensions(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE review_cards \
         SET suspension = CASE \
                WHEN record_id IS NOT NULL AND EXISTS ( \
                    SELECT 1 FROM learning_records r \
                    WHERE r.id = review_cards.record_id AND r.status = 'superseded') \
                THEN 'superseded' ELSE 'user' END, \
             suspended_at = COALESCE(suspended_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) \
         WHERE status = 'suspended' AND suspension IS NULL",
    )
    .execute(pool)
    .await
    .map(|_| ())
}

macro_rules! item_select {
    () => {
        concat!(
        "SELECT c.id, c.topic_id, c.lesson_id, c.record_id, c.term_id, c.front, c.back, \
         c.status, c.suspension, c.suspended_at, c.created_at, c.state, c.stability, c.difficulty, c.stability_fast, \
         c.learning_step, c.due, c.last_reviewed_at, c.review_count, c.lapses, c.unpassed_block, \
         c.model, c.weights_adopted_at, t.title AS topic_title, l.title AS lesson_title, l.number AS lesson_number, ",
        consolidation_ratings!("c"),
        " FROM review_cards c \
         JOIN topics t ON t.id = c.topic_id \
         JOIN lessons l ON l.id = c.lesson_id"
        )
    };
}

fn int(n: i64, what: &str, id: &str) -> Result<u32, RepositoryError> {
    u32::try_from(n).map_err(|_| corrupt(what, id))
}

/// The scheduling columns of a `review_cards` row.
pub(crate) fn schedule_from_row(
    row: &SqliteRow,
    id: &str,
) -> Result<CardSchedule, RepositoryError> {
    let state: String = row.try_get("state").map_err(storage)?;
    let due: String = row.try_get("due").map_err(storage)?;
    let last: Option<String> = row.try_get("last_reviewed_at").map_err(storage)?;
    let count: i64 = row.try_get("review_count").map_err(storage)?;
    let lapses: i64 = row.try_get("lapses").map_err(storage)?;
    let step: i64 = row.try_get("learning_step").map_err(storage)?;
    Ok(CardSchedule {
        state: state
            .parse::<ReviewState>()
            .map_err(|_| corrupt("card state", id))?,
        stability: row.try_get("stability").map_err(storage)?,
        difficulty: row.try_get("difficulty").map_err(storage)?,
        stability_fast: row.try_get("stability_fast").map_err(storage)?,
        learning_step: int(step, "learning step", id)?,
        due: parse_ts(&due)?,
        last_reviewed_at: last.as_deref().map(parse_ts).transpose()?,
        review_count: int(count, "review count", id)?,
        lapses: int(lapses, "lapses", id)?,
    })
}

fn item_from_row(row: &SqliteRow) -> Result<ReviewItem, RepositoryError> {
    let card = card_from_row(row)?;
    let number: i64 = row.try_get("lesson_number").map_err(storage)?;
    let model: String = row.try_get("model").map_err(storage)?;
    let weights: Option<String> = row.try_get("weights_adopted_at").map_err(storage)?;
    Ok(ReviewItem {
        basis: MemoryBasis {
            current_model: model == CURRENT_MODEL,
            weights: weights.as_deref().map(parse_ts).transpose()?,
        },
        lesson_number: int(number, "lesson number", &card.id.to_string())?,
        topic_title: row.try_get("topic_title").map_err(storage)?,
        lesson_title: row.try_get("lesson_title").map_err(storage)?,
        card,
    })
}

/// The model of the cards this adapter writes votes for. Cards of the
/// previous model are `fsrs5` until the startup migration replays them.
const CURRENT_MODEL: &str = "fsrs7";
const PREVIOUS_MODEL: &str = "fsrs5";

/// `model` names the algorithm behind the memory values of the snapshot and
/// `weights` the adoption instant of the weights they were computed with.
fn schedule_json(s: &CardSchedule, model: &str, weights: Option<OffsetDateTime>) -> String {
    json!({
        "model": model,
        "weights": weights.map(format_ts),
        "state": s.state.as_str(),
        "stability": s.stability,
        "difficulty": s.difficulty,
        "stability_fast": s.stability_fast,
        "learning_step": s.learning_step,
        "due": format_ts(s.due),
        "last_reviewed_at": s.last_reviewed_at.map(format_ts),
        "review_count": s.review_count,
        "lapses": s.lapses,
    })
    .to_string()
}

/// The model of a stored snapshot; snapshots written before `model` existed
/// are the previous one.
fn snapshot_model(raw: &str, log_id: &str) -> Result<String, RepositoryError> {
    let v: Value = serde_json::from_str(raw).map_err(|_| corrupt("review log snapshot", log_id))?;
    Ok(v.get("model")
        .and_then(Value::as_str)
        .unwrap_or(PREVIOUS_MODEL)
        .to_owned())
}

/// The weights stamp of a stored snapshot; none when it lacks one.
fn snapshot_weights(raw: &str, log_id: &str) -> Result<Option<OffsetDateTime>, RepositoryError> {
    let v: Value = serde_json::from_str(raw).map_err(|_| corrupt("review log snapshot", log_id))?;
    v.get("weights")
        .and_then(Value::as_str)
        .map(parse_ts)
        .transpose()
}

/// Snapshots written before FSRS-7 lack `stability_fast` and `learning_step`;
/// they read as 0.
fn schedule_parse(raw: &str, log_id: &str) -> Result<CardSchedule, RepositoryError> {
    let bad = || corrupt("review log snapshot", log_id);
    let v: Value = serde_json::from_str(raw).map_err(|_| bad())?;
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let number = |k: &str| {
        v.get(k)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    Ok(CardSchedule {
        state: text("state")
            .and_then(|s| s.parse::<ReviewState>().ok())
            .ok_or_else(bad)?,
        stability: v.get("stability").and_then(Value::as_f64).ok_or_else(bad)?,
        difficulty: v
            .get("difficulty")
            .and_then(Value::as_f64)
            .ok_or_else(bad)?,
        stability_fast: v
            .get("stability_fast")
            .map_or(Some(0.0), Value::as_f64)
            .ok_or_else(bad)?,
        learning_step: v
            .get("learning_step")
            .map_or(Some(0), |n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
            .ok_or_else(bad)?,
        due: text("due").map(parse_ts).ok_or_else(bad)??,
        last_reviewed_at: match v.get("last_reviewed_at") {
            Some(Value::Null) => None,
            Some(Value::String(s)) => Some(parse_ts(s)?),
            _ => return Err(bad()),
        },
        review_count: number("review_count").ok_or_else(bad)?,
        lapses: number("lapses").ok_or_else(bad)?,
    })
}

fn log_from_row(row: &SqliteRow) -> Result<ReviewLog, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let card: String = row.try_get("card_id").map_err(storage)?;
    let rating: String = row.try_get("rating").map_err(storage)?;
    let reviewed: String = row.try_get("reviewed_at").map_err(storage)?;
    let before: String = row.try_get("state_before").map_err(storage)?;
    let after: String = row.try_get("state_after").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let undone: Option<String> = row.try_get("undone_at").map_err(storage)?;
    Ok(ReviewLog {
        card_id: ReviewCardId::new(parse_id(&card)?),
        rating: rating
            .parse::<Rating>()
            .map_err(|_| corrupt("review rating", &id))?,
        reviewed_at: parse_ts(&reviewed)?,
        before: schedule_parse(&before, &id)?,
        after: schedule_parse(&after, &id)?,
        created_at: parse_ts(&created)?,
        undone_at: undone.as_deref().map(parse_ts).transpose()?,
        id: ReviewLogId::new(parse_id(&id)?),
    })
}

macro_rules! log_select {
    () => {
        "SELECT id, card_id, rating, reviewed_at, state_before, state_after, created_at, \
         undone_at FROM review_logs"
    };
}

/// Writes `s` on the card row, with the weights stamp its memory carries, if
/// its `review_count` is `expected` and, when `must_be_active`, it is active.
/// Returns whether a row was changed.
async fn write_schedule(
    tx: &mut sqlx::SqliteConnection,
    card_id: &str,
    s: &CardSchedule,
    weights: Option<OffsetDateTime>,
    expected: u32,
    must_be_active: bool,
) -> Result<bool, RepositoryError> {
    let done = sqlx::query(
        "UPDATE review_cards SET state = ?, stability = ?, difficulty = ?, \
         stability_fast = ?, learning_step = ?, model = ?, weights_adopted_at = ?, \
         due = ?, last_reviewed_at = ?, review_count = ?, lapses = ? \
         WHERE id = ? AND review_count = ? AND (? = 0 OR status = 'active')",
    )
    .bind(s.state.as_str())
    .bind(s.stability)
    .bind(s.difficulty)
    .bind(s.stability_fast)
    .bind(i64::from(s.learning_step))
    .bind(CURRENT_MODEL)
    .bind(weights.map(format_ts))
    .bind(format_ts(s.due))
    .bind(s.last_reviewed_at.map(format_ts))
    .bind(i64::from(s.review_count))
    .bind(i64::from(s.lapses))
    .bind(card_id)
    .bind(i64::from(expected))
    .bind(i64::from(must_be_active))
    .execute(&mut *tx)
    .await
    .map_err(storage)?;
    Ok(done.rows_affected() == 1)
}

async fn card_exists(
    tx: &mut sqlx::SqliteConnection,
    card_id: &str,
) -> Result<bool, RepositoryError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM review_cards WHERE id = ?")
        .bind(card_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage)?;
    Ok(found.is_some())
}

impl SqliteReviewRepository {
    /// Whether the card exists and is in one of the user's topics.
    async fn owns(&self, user: UserId, id: ReviewCardId) -> Result<bool, RepositoryError> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM review_cards c JOIN topics t ON t.id = c.topic_id \
             WHERE c.id = ? AND t.user_id = ?",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(found.is_some())
    }
}

#[async_trait]
impl ReviewRepository for SqliteReviewRepository {
    async fn get(
        &self,
        user: UserId,
        id: ReviewCardId,
    ) -> Result<Option<ReviewItem>, RepositoryError> {
        let row = sqlx::query(concat!(item_select!(), " WHERE c.id = ? AND t.user_id = ?"))
            .bind(id.to_string())
            .bind(user.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(item_from_row).transpose()
    }

    async fn list_due(
        &self,
        user: UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
        after: Option<DueCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        let topic = topic.map(|t| t.to_string());
        let due = after.map(|c| format_ts(c.due));
        let id = after.map(|c| c.id.to_string());
        let rows = sqlx::query(concat!(
            item_select!(),
            " WHERE t.user_id = ? AND c.status = 'active' AND c.due <= ? \
             AND (? IS NULL OR c.topic_id = ?) \
             AND (? IS NULL OR c.due > ? OR (c.due = ? AND c.id > ?)) \
             ORDER BY c.due, c.id LIMIT ?"
        ))
        .bind(user.to_string())
        .bind(format_ts(now))
        .bind(&topic)
        .bind(&topic)
        .bind(&due)
        .bind(&due)
        .bind(&due)
        .bind(&id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(item_from_row).collect()
    }

    async fn due_stats(
        &self,
        user: UserId,
        now: OffsetDateTime,
        topic: Option<TopicId>,
    ) -> Result<DueStats, RepositoryError> {
        let now = format_ts(now);
        let topic = topic.map(|t| t.to_string());
        let due: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_cards c JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND c.status = 'active' AND c.due <= ? \
             AND (? IS NULL OR c.topic_id = ?)",
        )
        .bind(user.to_string())
        .bind(&now)
        .bind(&topic)
        .bind(&topic)
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        let next: Option<String> = sqlx::query_scalar(
            "SELECT MIN(c.due) FROM review_cards c JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND c.status = 'active' AND c.due > ? \
             AND (? IS NULL OR c.topic_id = ?)",
        )
        .bind(user.to_string())
        .bind(&now)
        .bind(&topic)
        .bind(&topic)
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        Ok(DueStats {
            due_count: int(due, "due count", "stats")?,
            next_due_at: next.as_deref().map(parse_ts).transpose()?,
        })
    }

    async fn due_topics(
        &self,
        user: UserId,
        now: OffsetDateTime,
    ) -> Result<Vec<DueTopic>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT t.id, t.title, COUNT(*) AS n FROM review_cards c \
             JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND c.status = 'active' AND c.due <= ? \
             GROUP BY t.id, t.title ORDER BY n DESC, t.title ASC, t.id ASC",
        )
        .bind(user.to_string())
        .bind(format_ts(now))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.get("id");
                Ok(DueTopic {
                    topic_id: TopicId::new(parse_id(&id)?),
                    topic_title: row.get("title"),
                    due_count: int(row.get("n"), "due count", &id)?,
                })
            })
            .collect()
    }

    async fn topic_exists(&self, user: UserId, topic: TopicId) -> Result<bool, RepositoryError> {
        let found: Option<String> =
            sqlx::query_scalar("SELECT id FROM topics WHERE id = ? AND user_id = ?")
                .bind(topic.to_string())
                .bind(user.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(found.is_some())
    }

    async fn apply_review(
        &self,
        expected_review_count: u32,
        after: &CardSchedule,
        log: &ReviewLog,
        weights: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError> {
        let card = log.card_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The snapshot of the state before the vote is of the model the card
        // was on when the vote came, read in the same transaction.
        let before_model: Option<String> =
            sqlx::query_scalar("SELECT model FROM review_cards WHERE id = ?")
                .bind(&card)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        if !write_schedule(&mut tx, &card, after, weights, expected_review_count, true).await? {
            // Dropping `tx` rolls back: nothing was written.
            return Err(if card_exists(&mut tx, &card).await? {
                RepositoryError::Conflict
            } else {
                RepositoryError::NotFound
            });
        }
        sqlx::query(
            "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
             state_after, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(log.id.to_string())
        .bind(&card)
        .bind(log.rating.as_str())
        .bind(format_ts(log.reviewed_at))
        .bind(schedule_json(
            &log.before,
            before_model.as_deref().unwrap_or(CURRENT_MODEL),
            weights,
        ))
        .bind(schedule_json(&log.after, CURRENT_MODEL, weights))
        .bind(format_ts(log.created_at))
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }

    async fn last_log(&self, card_id: ReviewCardId) -> Result<Option<ReviewLog>, RepositoryError> {
        let row = sqlx::query(concat!(
            log_select!(),
            " WHERE card_id = ? ORDER BY rowid DESC LIMIT 1"
        ))
        .bind(card_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(log_from_row).transpose()
    }

    async fn undo_review(
        &self,
        log_id: ReviewLogId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let row = sqlx::query(concat!(log_select!(), " WHERE id = ?"))
            .bind(log_id.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?;
        let log = row
            .as_ref()
            .map(log_from_row)
            .transpose()?
            .ok_or(RepositoryError::NotFound)?;
        if log.undone_at.is_some() {
            return Err(RepositoryError::Conflict);
        }
        // A vote given on another model cannot be put back: its snapshot
        // holds memory values this model does not read (`reviews.md` §5).
        let raw_before: String = row
            .as_ref()
            .ok_or(RepositoryError::NotFound)?
            .try_get("state_before")
            .map_err(storage)?;
        if snapshot_model(&raw_before, &log_id.to_string())? != CURRENT_MODEL {
            return Err(RepositoryError::RuleViolated);
        }
        let card = log.card_id.to_string();
        // The memory comes back with the stamp it had: if the weights changed
        // meanwhile it no longer matches, and the next vote or pass replays it.
        let stamp = snapshot_weights(&raw_before, &log_id.to_string())?;
        // The card must still be where the vote left it.
        if !write_schedule(
            &mut tx,
            &card,
            &log.before,
            stamp,
            log.after.review_count,
            false,
        )
        .await?
        {
            return Err(RepositoryError::Conflict);
        }
        sqlx::query("UPDATE review_logs SET undone_at = ? WHERE id = ?")
            .bind(format_ts(at))
            .bind(log_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }

    async fn suspend(
        &self,
        user: UserId,
        id: ReviewCardId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        // Only an active card changes: one that is suspended already keeps its
        // reason and instant.
        let done = sqlx::query(
            "UPDATE review_cards SET status = 'suspended', suspension = 'user', \
             suspended_at = ? WHERE id = ? AND status = 'active' \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(format_ts(at))
        .bind(id.to_string())
        .bind(user.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        if done.rows_affected() == 0 && !self.owns(user, id).await? {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn unsuspend(&self, user: UserId, id: ReviewCardId) -> Result<(), RepositoryError> {
        backfill_suspensions(&self.pool).await.map_err(storage)?;
        let done = sqlx::query(
            "UPDATE review_cards SET status = 'active', suspension = NULL, \
             suspended_at = NULL WHERE id = ? AND suspension = 'user' \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        if done.rows_affected() == 1 {
            return Ok(());
        }
        // Nothing changed: it is active already, superseded, or not there.
        let state: Option<String> = sqlx::query_scalar(
            "SELECT status FROM review_cards WHERE id = ? \
             AND topic_id IN (SELECT id FROM topics WHERE user_id = ?)",
        )
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        // `status` is the truth: only an active card is "already active".
        match state.as_deref() {
            None => Err(RepositoryError::NotFound),
            Some("active") => Ok(()),
            Some(_) => Err(RepositoryError::Conflict),
        }
    }

    async fn list_suspended(
        &self,
        user: UserId,
        after: Option<SuspendedCursor>,
        limit: u32,
    ) -> Result<Vec<ReviewItem>, RepositoryError> {
        backfill_suspensions(&self.pool).await.map_err(storage)?;
        let at = after.map(|c| format_ts(c.suspended_at));
        let id = after.map(|c| c.id.to_string());
        let rows = sqlx::query(concat!(
            item_select!(),
            " WHERE t.user_id = ? AND c.status = 'suspended' AND c.suspension = 'user' \
             AND (? IS NULL OR c.suspended_at < ? OR (c.suspended_at = ? AND c.id > ?)) \
             ORDER BY c.suspended_at DESC, c.id LIMIT ?"
        ))
        .bind(user.to_string())
        .bind(&at)
        .bind(&at)
        .bind(&at)
        .bind(&id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(item_from_row).collect()
    }

    async fn suspended_count(&self, user: UserId) -> Result<u32, RepositoryError> {
        backfill_suspensions(&self.pool).await.map_err(storage)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_cards c JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND c.status = 'suspended' AND c.suspension = 'user'",
        )
        .bind(user.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        int(count, "suspended count", "stats")
    }

    async fn card_history(&self, card: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError> {
        let mut conn = self.pool.acquire().await.map_err(storage)?;
        card_events(&mut conn, &card.to_string()).await
    }

    async fn parameters(&self, user: UserId) -> Result<Option<UserWeights>, RepositoryError> {
        let row = sqlx::query(
            "SELECT model, weights, adopted_at FROM user_review_parameters WHERE user_id = ?",
        )
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else { return Ok(None) };
        let model: String = row.try_get("model").map_err(storage)?;
        let weights: String = row.try_get("weights").map_err(storage)?;
        let adopted_at = parse_ts(&row.try_get::<String, _>("adopted_at").map_err(storage)?)?;
        // Unreadable weights are not a reason to fail a vote: the caller
        // schedules with the defaults, and the log says why.
        match serde_json::from_str::<Vec<f64>>(&weights) {
            Ok(weights) => Ok(Some(UserWeights {
                parameters: SchedulerParameters { model, weights },
                adopted_at,
            })),
            Err(error) => {
                tracing::warn!(%user, %error, "the stored review weights cannot be read");
                Ok(None)
            }
        }
    }
}

fn event_from_row(row: &SqliteRow) -> Result<ReviewEvent, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let rating: String = row.try_get("rating").map_err(storage)?;
    let at: String = row.try_get("reviewed_at").map_err(storage)?;
    Ok(ReviewEvent {
        rating: rating
            .parse::<Rating>()
            .map_err(|_| corrupt("review rating", &id))?,
        at: parse_ts(&at)?,
    })
}

/// The card's votes that were not undone, in the order they were given.
/// `reviewed_at` is a fixed-width UTC string: it sorts as time does.
async fn card_events(
    conn: &mut sqlx::SqliteConnection,
    card: &str,
) -> Result<Vec<ReviewEvent>, RepositoryError> {
    let rows = sqlx::query(
        "SELECT id, rating, reviewed_at FROM review_logs \
         WHERE card_id = ? AND undone_at IS NULL ORDER BY reviewed_at, rowid",
    )
    .bind(card)
    .fetch_all(&mut *conn)
    .await
    .map_err(storage)?;
    rows.iter().map(event_from_row).collect()
}

#[async_trait]
impl ReviewMigrationRepository for SqliteReviewRepository {
    async fn cards_to_migrate(&self) -> Result<Vec<ReviewCardId>, RepositoryError> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM review_cards WHERE model <> ? ORDER BY rowid")
                .bind(CURRENT_MODEL)
                .fetch_all(&self.pool)
                .await
                .map_err(storage)?;
        ids.iter()
            .map(|id| parse_id(id).map(ReviewCardId::new))
            .collect()
    }

    async fn vote_history(&self, card: ReviewCardId) -> Result<Vec<ReviewEvent>, RepositoryError> {
        let mut conn = self.pool.acquire().await.map_err(storage)?;
        card_events(&mut conn, &card.to_string()).await
    }

    async fn mark_migrated(
        &self,
        card: ReviewCardId,
        memory: Option<MemoryValues>,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Without votes the card keeps the values it has (none, in practice).
        let query = match memory {
            Some(m) => sqlx::query(
                "UPDATE review_cards SET stability = ?, difficulty = ?, stability_fast = ?, \
                 learning_step = 0, weights_adopted_at = NULL, model = ? \
                 WHERE id = ? AND model <> ?",
            )
            .bind(m.stability)
            .bind(m.difficulty)
            .bind(m.stability_fast),
            None => sqlx::query(
                "UPDATE review_cards SET learning_step = 0, weights_adopted_at = NULL, model = ? \
                 WHERE id = ? AND model <> ?",
            ),
        };
        query
            .bind(CURRENT_MODEL)
            .bind(card.to_string())
            .bind(CURRENT_MODEL)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        tx.commit().await.map_err(storage)
    }
}

fn log_loss_column(row: &SqliteRow, column: &str) -> Result<Option<f64>, RepositoryError> {
    row.try_get(column).map_err(storage)
}

#[async_trait]
impl ReviewOptimizationRepository for SqliteReviewRepository {
    async fn users_with_review_data(&self) -> Result<Vec<UserId>, RepositoryError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT t.user_id FROM review_logs l \
             JOIN review_cards c ON c.id = l.card_id JOIN topics t ON t.id = c.topic_id \
             WHERE l.undone_at IS NULL \
             UNION SELECT user_id FROM user_review_parameters ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        ids.iter().map(|id| parse_id(id).map(UserId::new)).collect()
    }

    async fn card_votes(&self, user: UserId) -> Result<Vec<CardVotes>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT l.id, l.card_id, l.rating, l.reviewed_at, l.created_at FROM review_logs l \
             JOIN review_cards c ON c.id = l.card_id JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND l.undone_at IS NULL \
             ORDER BY l.card_id, l.reviewed_at, l.rowid",
        )
        .bind(user.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let mut cards: Vec<CardVotes> = Vec::new();
        for row in &rows {
            let card = ReviewCardId::new(parse_id(
                &row.try_get::<String, _>("card_id").map_err(storage)?,
            )?);
            let created: String = row.try_get("created_at").map_err(storage)?;
            let vote = VoteRecord {
                event: event_from_row(row)?,
                recorded_at: parse_ts(&created)?,
            };
            match cards.last_mut() {
                Some(last) if last.card == card => last.votes.push(vote),
                _ => cards.push(CardVotes {
                    card,
                    votes: vec![vote],
                }),
            }
        }
        Ok(cards)
    }

    async fn last_attempt(
        &self,
        user: UserId,
    ) -> Result<Option<OptimizationAttempt>, RepositoryError> {
        let row = sqlx::query(
            "SELECT at, useful_votes, train_votes, test_votes, current_log_loss, \
             candidate_log_loss, outcome, error FROM review_optimization_attempts \
             WHERE user_id = ? ORDER BY at DESC, id DESC LIMIT 1",
        )
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else { return Ok(None) };
        let at: String = row.try_get("at").map_err(storage)?;
        let outcome: String = row.try_get("outcome").map_err(storage)?;
        let count = |column: &str| -> Result<u32, RepositoryError> {
            let n: i64 = row.try_get(column).map_err(storage)?;
            int(n, column, &user.to_string())
        };
        Ok(Some(OptimizationAttempt {
            at: parse_ts(&at)?,
            useful_votes: count("useful_votes")?,
            train_votes: count("train_votes")?,
            test_votes: count("test_votes")?,
            current_log_loss: log_loss_column(&row, "current_log_loss")?,
            candidate_log_loss: log_loss_column(&row, "candidate_log_loss")?,
            outcome: outcome
                .parse::<OptimizationOutcome>()
                .map_err(|_| corrupt("optimization outcome", &user.to_string()))?,
            error: row.try_get("error").map_err(storage)?,
        }))
    }

    async fn record_attempt(
        &self,
        user: UserId,
        attempt: &OptimizationAttempt,
        adopted: Option<&SchedulerParameters>,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        sqlx::query(
            "INSERT INTO review_optimization_attempts (user_id, at, useful_votes, train_votes, \
             test_votes, current_log_loss, candidate_log_loss, outcome, error) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user.to_string())
        .bind(format_ts(attempt.at))
        .bind(i64::from(attempt.useful_votes))
        .bind(i64::from(attempt.train_votes))
        .bind(i64::from(attempt.test_votes))
        .bind(attempt.current_log_loss)
        .bind(attempt.candidate_log_loss)
        .bind(attempt.outcome.as_str())
        .bind(&attempt.error)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        if let Some(parameters) = adopted {
            let weights = serde_json::to_string(&parameters.weights)
                .map_err(|e| RepositoryError::Storage(Box::new(e)))?;
            sqlx::query(
                "INSERT INTO user_review_parameters (user_id, model, weights, adopted_at) \
                 VALUES (?, ?, ?, ?) ON CONFLICT (user_id) DO UPDATE SET \
                 model = excluded.model, weights = excluded.weights, \
                 adopted_at = excluded.adopted_at",
            )
            .bind(user.to_string())
            .bind(&parameters.model)
            .bind(weights)
            .bind(format_ts(attempt.at))
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }

    async fn cards_out_of_date(
        &self,
        user: UserId,
        weights: OffsetDateTime,
    ) -> Result<Vec<ReviewCardId>, RepositoryError> {
        // `IS NOT`, not `<>`: a card with no stamp (NULL) is out of date too.
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT c.id FROM review_cards c JOIN topics t ON t.id = c.topic_id \
             WHERE t.user_id = ? AND (c.model <> ? OR c.weights_adopted_at IS NOT ?) \
             ORDER BY c.id",
        )
        .bind(user.to_string())
        .bind(CURRENT_MODEL)
        .bind(format_ts(weights))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        ids.iter()
            .map(|id| parse_id(id).map(ReviewCardId::new))
            .collect()
    }

    async fn card_snapshot(
        &self,
        card: ReviewCardId,
    ) -> Result<(u32, Vec<ReviewEvent>), RepositoryError> {
        let id = card.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        let count: Option<i64> =
            sqlx::query_scalar("SELECT review_count FROM review_cards WHERE id = ?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
        let count = count.ok_or(RepositoryError::NotFound)?;
        let events = card_events(&mut tx, &id).await?;
        Ok((int(count, "review count", &id)?, events))
    }

    async fn rebuild_memory(
        &self,
        card: ReviewCardId,
        expected_review_count: u32,
        memory: Option<MemoryValues>,
        weights: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        // A single statement: the guard and the write cannot be separated by
        // a vote. `model` is set because the values now come from the current
        // model, even for a card the startup migration could not reach. A card
        // without votes has no memory to write (COALESCE keeps its columns).
        let done = sqlx::query(
            "UPDATE review_cards SET stability = COALESCE(?, stability), \
             difficulty = COALESCE(?, difficulty), \
             stability_fast = COALESCE(?, stability_fast), \
             model = ?, weights_adopted_at = ? WHERE id = ? AND review_count = ?",
        )
        .bind(memory.map(|m| m.stability))
        .bind(memory.map(|m| m.difficulty))
        .bind(memory.map(|m| m.stability_fast))
        .bind(CURRENT_MODEL)
        .bind(format_ts(weights))
        .bind(card.to_string())
        .bind(i64::from(expected_review_count))
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(done.rows_affected() == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use domain::ReviewScheduler as _;
    use domain::{CardStatus, Suspension, SuspensionReason};
    use time::Duration;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.250 UTC);

    fn cid(n: u128) -> ReviewCardId {
        ReviewCardId::new(Uuid::from_u128(n))
    }

    const SEED: [&str; 3] = [
        "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-00000000000f', '00000000-0000-7000-8000-000000000001', 'F', 'f', 0, '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
        "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-0000000000a1', '00000000-0000-7000-8000-000000000001', '00000000-0000-0000-0000-00000000000f', 'Rust', 'rust', '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
         catalog_version, mission_version, document, plan, generation_job_id, created_at) \
         VALUES ('00000000-0000-0000-0000-0000000000b1', '00000000-0000-0000-0000-0000000000a1', 3, 'Result', 'o', 5, 1, 1, '{}', '{}', 'job1', '2026-11-09T09:00:00.000Z')",
    ];

    const SEED_BEFORE_USERS: [&str; 3] = [
        "INSERT INTO folders (id, name, name_key, position, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-00000000000f', 'F', 'f', 0, '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
        "INSERT INTO topics (id, folder_id, title, title_key, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-0000000000a1', '00000000-0000-0000-0000-00000000000f', 'Rust', 'rust', '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
         catalog_version, mission_version, document, plan, generation_job_id, created_at) \
         VALUES ('00000000-0000-0000-0000-0000000000b1', '00000000-0000-0000-0000-0000000000a1', 3, 'Result', 'o', 5, 1, 1, '{}', '{}', 'job1', '2026-11-09T09:00:00.000Z')",
    ];

    async fn seed(pool: &SqlitePool) {
        seed_with(pool, &SEED).await;
    }

    async fn seed_with(pool: &SqlitePool, queries: &[&'static str]) {
        for q in queries {
            sqlx::query(*q).execute(pool).await.unwrap();
        }
    }

    async fn env() -> (SqliteReviewRepository, SqlitePool) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        seed(&pool).await;
        (SqliteReviewRepository::new(pool.clone()), pool)
    }

    /// A glossary term and a card on it, created with the pre-FSRS columns.
    async fn insert_card(pool: &SqlitePool, n: u128, created: &str, due: Option<&str>) {
        let term = Uuid::from_u128(0x100 + n).to_string();
        sqlx::query(
            "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, \
             lesson_id, created_at, updated_at) \
             VALUES (?, '00000000-0000-0000-0000-0000000000a1', ?, 'x', 'x', 'd', '[]', '00000000-0000-0000-0000-0000000000b1', ?, ?)",
        )
        .bind(&term)
        .bind(&term)
        .bind(created)
        .bind(created)
        .execute(pool)
        .await
        .unwrap();
        let id = cid(n).to_string();
        match due {
            Some(due) => sqlx::query(
                "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, \
                 created_at, due) VALUES (?, '00000000-0000-0000-0000-0000000000a1', '00000000-0000-0000-0000-0000000000b1', ?, 'f', 'b', ?, ?)",
            )
            .bind(&id)
            .bind(&term)
            .bind(created)
            .bind(due),
            None => sqlx::query(
                "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, \
                 created_at) VALUES (?, '00000000-0000-0000-0000-0000000000a1', '00000000-0000-0000-0000-0000000000b1', ?, 'f', 'b', ?)",
            )
            .bind(&id)
            .bind(&term)
            .bind(created),
        }
        .execute(pool)
        .await
        .unwrap();
    }

    fn log_for(
        card: ReviewCardId,
        n: u128,
        before: CardSchedule,
        after: CardSchedule,
    ) -> ReviewLog {
        ReviewLog {
            id: ReviewLogId::new(Uuid::from_u128(n)),
            card_id: card,
            rating: Rating::Good,
            reviewed_at: T0,
            before,
            after,
            created_at: T0,
            undone_at: None,
        }
    }

    /// A card on a record (the first migrations know no other kind of origin
    /// that can be superseded), suspended or not, with the pre-0024 columns.
    async fn insert_record_card(pool: &SqlitePool, n: u128, record_status: &str, suspended: bool) {
        let record = Uuid::from_u128(0x200 + n).to_string();
        sqlx::query(
            "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, \
             status, lesson_id, created_at) VALUES (?, '00000000-0000-0000-0000-0000000000a1', ?, \
             'demonstrated', 't', 'b', 'e', ?, '00000000-0000-0000-0000-0000000000b1', \
             '2026-11-09T09:00:00.000Z')",
        )
        .bind(&record)
        .bind(i64::try_from(n).unwrap())
        .bind(record_status)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO review_cards (id, topic_id, lesson_id, record_id, front, back, status, \
             created_at, due) VALUES (?, '00000000-0000-0000-0000-0000000000a1', \
             '00000000-0000-0000-0000-0000000000b1', ?, 'f', 'b', ?, '2026-11-09T09:00:00.000Z', \
             '2026-11-10T09:00:00.000Z')",
        )
        .bind(cid(n).to_string())
        .bind(&record)
        .bind(if suspended { "suspended" } else { "active" })
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn the_suspension_migration_tells_superseded_cards_from_hand_made_ones() {
        let pool = connect("sqlite::memory:").await.unwrap();
        let mut m = sqlx::migrate!("../../migrations");
        let all = m.migrations.clone().into_owned();
        m.migrations = all.iter().filter(|x| x.version <= 23).cloned().collect();
        m.run(&pool).await.unwrap();
        seed(&pool).await;
        insert_record_card(&pool, 1, "superseded", true).await;
        insert_record_card(&pool, 2, "active", true).await;
        insert_record_card(&pool, 3, "active", false).await;
        // A card on a glossary term has no record that can be superseded.
        insert_card(
            &pool,
            4,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        sqlx::query("UPDATE review_cards SET status = 'suspended' WHERE id = ?")
            .bind(cid(4).to_string())
            .execute(&pool)
            .await
            .unwrap();
        let before = OffsetDateTime::now_utc();
        migrate(&pool).await.unwrap();

        let repo = SqliteReviewRepository::new(pool);
        let user = domain::UserId::OWNER;
        let card = |n: u128| {
            let repo = repo.clone();
            async move { repo.get(user, cid(n)).await.unwrap().unwrap().card }
        };
        let superseded = card(1).await.suspension.unwrap();
        assert_eq!(superseded.reason, SuspensionReason::Superseded);
        let by_hand = card(2).await.suspension.unwrap();
        assert_eq!(by_hand.reason, SuspensionReason::User);
        // The instant of the migration, to the millisecond.
        for at in [superseded.at, by_hand.at] {
            assert!(at >= before - time::Duration::seconds(1));
            assert!(at <= OffsetDateTime::now_utc() + time::Duration::seconds(1));
        }
        let active = card(3).await;
        assert_eq!(
            (active.status, active.suspension),
            (CardStatus::Active, None)
        );
        assert_eq!(
            card(4).await.suspension.unwrap().reason,
            SuspensionReason::User
        );
        // Only the hand-made ones are listed and counted.
        let listed = repo.list_suspended(user, None, 10).await.unwrap();
        let mut ids: Vec<_> = listed.iter().map(|i| i.card.id).collect();
        ids.sort();
        assert_eq!(ids, [cid(2), cid(4)]);
        assert_eq!(repo.suspended_count(user).await.unwrap(), 2);
    }

    async fn force(pool: &SqlitePool, n: u128, sql_set: &str) {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE review_cards SET {sql_set} WHERE id = ?"
        )))
        .bind(cid(n).to_string())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rows_whose_columns_disagree_follow_status() {
        let (repo, pool) = env().await;
        let user = domain::UserId::OWNER;
        for n in 1..=3 {
            insert_card(
                &pool,
                n,
                "2026-11-09T09:00:00.000Z",
                Some("2026-11-09T09:00:00.000Z"),
            )
            .await;
        }
        // 1: suspended without a reason (written by an older binary).
        force(&pool, 1, "status = 'suspended'").await;
        // 2: active with a reason left behind.
        force(
            &pool,
            2,
            "suspension = 'user', suspended_at = '2026-11-09T10:00:00.000Z'",
        )
        .await;
        // 3: suspended without a reason, on a superseded record.
        force(&pool, 3, "status = 'suspended'").await;
        sqlx::query(
            "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, \
             status, lesson_id, created_at) VALUES (?, '00000000-0000-0000-0000-0000000000a1', 1, \
             'demonstrated', 't', 'b', 'e', 'superseded', \
             '00000000-0000-0000-0000-0000000000b1', '2026-11-09T09:00:00.000Z')",
        )
        .bind(Uuid::from_u128(0x303).to_string())
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("UPDATE review_cards SET term_id = NULL, record_id = ? WHERE id = ?")
            .bind(Uuid::from_u128(0x303).to_string())
            .bind(cid(3).to_string())
            .execute(&pool)
            .await
            .unwrap();
        // The active card with a reason is corrupt, never silently trusted.
        assert!(matches!(
            repo.get(user, cid(2)).await,
            Err(RepositoryError::Storage(_))
        ));
        // The count repairs the others first: 1 is the learner's, 3 is not.
        assert_eq!(repo.suspended_count(user).await.unwrap(), 1);
        let listed = repo.list_suspended(user, None, 10).await.unwrap();
        assert_eq!(
            listed.iter().map(|i| i.card.id).collect::<Vec<_>>(),
            [cid(1)]
        );
        let repaired = repo.get(user, cid(3)).await.unwrap().unwrap().card;
        assert_eq!(
            repaired.suspension.unwrap().reason,
            SuspensionReason::Superseded
        );
        // A suspended card without a reason is reactivated if it is the
        // learner's, refused if superseded; "already active" needs status.
        force(&pool, 1, "suspension = NULL, suspended_at = NULL").await;
        repo.unsuspend(user, cid(1)).await.unwrap();
        let back = repo.get(user, cid(1)).await.unwrap().unwrap().card;
        assert_eq!((back.status, back.suspension), (CardStatus::Active, None));
        force(&pool, 3, "suspension = NULL, suspended_at = NULL").await;
        assert!(matches!(
            repo.unsuspend(user, cid(3)).await,
            Err(RepositoryError::Conflict)
        ));
    }

    #[tokio::test]
    async fn suspending_unsuspending_and_listing_follow_the_rules() {
        let (repo, pool) = env().await;
        let user = domain::UserId::OWNER;
        for n in 1..=4 {
            insert_card(
                &pool,
                n,
                "2026-11-09T09:00:00.000Z",
                Some("2026-11-09T09:00:00.000Z"),
            )
            .await;
        }
        let at = |h: i64| T0 + time::Duration::hours(h);
        repo.suspend(user, cid(1), at(1)).await.unwrap();
        repo.suspend(user, cid(2), at(3)).await.unwrap();
        repo.suspend(user, cid(3), at(3)).await.unwrap();
        // A second suspension changes neither reason nor instant.
        repo.suspend(user, cid(1), at(9)).await.unwrap();
        let first = repo.get(user, cid(1)).await.unwrap().unwrap().card;
        assert_eq!(
            first.suspension,
            Some(Suspension {
                reason: SuspensionReason::User,
                at: at(1)
            })
        );
        assert!(matches!(
            repo.suspend(user, cid(99), at(1)).await,
            Err(RepositoryError::NotFound)
        ));

        // Newest first, ties by id, keyset pages.
        let page = repo.list_suspended(user, None, 2).await.unwrap();
        let ids: Vec<_> = page.iter().map(|i| i.card.id).collect();
        assert_eq!(ids, [cid(2), cid(3)]);
        let cursor = SuspendedCursor {
            suspended_at: at(3),
            id: cid(3),
        };
        let rest = repo.list_suspended(user, Some(cursor), 2).await.unwrap();
        assert_eq!(rest.iter().map(|i| i.card.id).collect::<Vec<_>>(), [cid(1)]);
        assert_eq!(repo.suspended_count(user).await.unwrap(), 3);

        // Reactivation clears both fields and is idempotent.
        repo.unsuspend(user, cid(2)).await.unwrap();
        repo.unsuspend(user, cid(2)).await.unwrap();
        repo.unsuspend(user, cid(4)).await.unwrap();
        let back = repo.get(user, cid(2)).await.unwrap().unwrap().card;
        assert_eq!((back.status, back.suspension), (CardStatus::Active, None));
        assert_eq!(repo.suspended_count(user).await.unwrap(), 2);
        assert!(matches!(
            repo.unsuspend(user, cid(99)).await,
            Err(RepositoryError::NotFound)
        ));

        // A superseded card is refused, and a hand-made suspension does not
        // survive its record being superseded (see `lessons.rs`).
        sqlx::query("UPDATE review_cards SET suspension = 'superseded' WHERE id = ?")
            .bind(cid(3).to_string())
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            repo.unsuspend(user, cid(3)).await,
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(repo.suspended_count(user).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn another_users_cards_are_not_found() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        let stranger = domain::UserId::new(Uuid::from_u128(0xdead));
        assert!(matches!(
            repo.suspend(stranger, cid(1), T0).await,
            Err(RepositoryError::NotFound)
        ));
        assert!(matches!(
            repo.unsuspend(stranger, cid(1)).await,
            Err(RepositoryError::NotFound)
        ));
        repo.suspend(domain::UserId::OWNER, cid(1), T0)
            .await
            .unwrap();
        let none = repo.list_suspended(stranger, None, 10).await.unwrap();
        assert_eq!(none.len(), 0);
        assert_eq!(repo.suspended_count(stranger).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn the_migration_backfills_existing_cards_as_new_and_due_a_day_later() {
        let pool = connect("sqlite::memory:").await.unwrap();
        let mut m = sqlx::migrate!("../../migrations");
        let all = m.migrations.clone().into_owned();
        m.migrations = all.iter().filter(|x| x.version <= 11).cloned().collect();
        m.run(&pool).await.unwrap();
        seed_with(&pool, &SEED_BEFORE_USERS).await;
        insert_card(&pool, 1, "2026-11-09T09:15:30.500Z", None).await;
        migrate(&pool).await.unwrap();

        let repo = SqliteReviewRepository::new(pool);
        let item = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            item.card.schedule,
            CardSchedule::initial(datetime!(2026-11-09 09:15:30.500 UTC))
        );
        assert_eq!(
            item.card.schedule.due,
            datetime!(2026-11-10 09:15:30.500 UTC)
        );
        assert_eq!(
            (
                item.topic_title.as_str(),
                item.lesson_title.as_str(),
                item.lesson_number
            ),
            ("Rust", "Result", 3)
        );
    }

    #[tokio::test]
    async fn due_cards_are_filtered_ordered_and_counted() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T10:00:00.250Z"),
        )
        .await;
        insert_card(
            &pool,
            2,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        insert_card(
            &pool,
            3,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T10:00:00.251Z"),
        )
        .await;
        insert_card(
            &pool,
            4,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-12T10:00:00.000Z"),
        )
        .await;
        insert_card(
            &pool,
            5,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        repo.suspend(domain::UserId::OWNER, cid(5), T0)
            .await
            .unwrap();

        // `due` <= now is inclusive; the millisecond after is not due yet.
        let due = repo
            .list_due(domain::UserId::OWNER, T0, None, None, 10)
            .await
            .unwrap();
        let ids: Vec<_> = due.iter().map(|i| i.card.id).collect();
        assert_eq!(ids, [cid(2), cid(1)]);
        let stats = repo
            .due_stats(domain::UserId::OWNER, T0, None)
            .await
            .unwrap();
        assert_eq!(stats.due_count, 2);
        assert_eq!(
            stats.next_due_at,
            Some(datetime!(2026-11-09 10:00:00.251 UTC))
        );

        let c = DueCursor {
            due: due[0].card.schedule.due,
            id: due[0].card.id,
        };
        let rest = repo
            .list_due(domain::UserId::OWNER, T0, None, Some(c), 10)
            .await
            .unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].card.id, cid(1));
        // Suspended cards do not count for the next due date either.
        let none = repo
            .due_stats(domain::UserId::OWNER, T0 + Duration::days(30), None)
            .await
            .unwrap();
        assert_eq!((none.due_count, none.next_due_at), (4, None));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn due_can_be_limited_to_a_topic_and_the_topics_are_counted_over_all_cards() {
        let (repo, pool) = env().await;
        let owner = domain::UserId::OWNER;
        let a1 = TopicId::new(Uuid::from_u128(0xa1));
        let a2 = TopicId::new(Uuid::from_u128(0xa2));
        let a3 = TopicId::new(Uuid::from_u128(0xa3));
        let a9 = TopicId::new(Uuid::from_u128(0xa9));
        for (id, user, title) in [
            (0xa2u128, "00000000-0000-7000-8000-000000000001", "Algebra"),
            (0xa3, "00000000-0000-7000-8000-000000000001", "Algebra"),
            (0xa9, "00000000-0000-7000-8000-0000000000b2", "Other"),
        ] {
            if user.ends_with("b2") {
                sqlx::query(
                    "INSERT INTO users (id, username, display_name, created_at) \
                     VALUES (?, 'other', 'Other', '2026-11-09T09:00:00.000Z')",
                )
                .bind(user)
                .execute(&pool)
                .await
                .unwrap();
                sqlx::query(
                    "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) \
                     VALUES ('00000000-0000-0000-0000-0000000000f2', ?, 'F', 'f', 0, '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
                )
                .bind(user)
                .execute(&pool)
                .await
                .unwrap();
            }
            let folder = if user.ends_with("b2") {
                "00000000-0000-0000-0000-0000000000f2"
            } else {
                "00000000-0000-0000-0000-00000000000f"
            };
            sqlx::query(
                "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, '2026-11-09T09:00:00.000Z', '2026-11-09T09:00:00.000Z')",
            )
            .bind(Uuid::from_u128(id).to_string())
            .bind(user)
            .bind(folder)
            .bind(title)
            .bind(format!("{title}{id}"))
            .execute(&pool)
            .await
            .unwrap();
        }
        // Rust (a1): cards 1, 2 due, 3 not yet. Algebra a2: 4, 5 due. Algebra
        // a3: 6 due. Other user's a9: 7 due.
        let due = "2026-11-09T09:00:00.000Z";
        for (n, topic, at) in [
            (1u128, 0xa1u128, due),
            (2, 0xa1, due),
            (3, 0xa1, "2026-11-12T09:00:00.000Z"),
            (4, 0xa2, due),
            (5, 0xa2, due),
            (6, 0xa3, due),
            (7, 0xa9, due),
        ] {
            insert_card(&pool, n, due, Some(at)).await;
            sqlx::query("UPDATE review_cards SET topic_id = ? WHERE id = ?")
                .bind(Uuid::from_u128(topic).to_string())
                .bind(cid(n).to_string())
                .execute(&pool)
                .await
                .unwrap();
        }

        let only = repo.list_due(owner, T0, Some(a2), None, 10).await.unwrap();
        assert_eq!(
            only.iter().map(|i| i.card.id).collect::<Vec<_>>(),
            [cid(4), cid(5)]
        );
        let stats = repo.due_stats(owner, T0, Some(a1)).await.unwrap();
        assert_eq!(stats.due_count, 2);
        assert_eq!(
            stats.next_due_at,
            Some(datetime!(2026-11-12 09:00:00.000 UTC))
        );
        let stats = repo.due_stats(owner, T0, Some(a2)).await.unwrap();
        assert_eq!((stats.due_count, stats.next_due_at), (2, None));
        assert_eq!(repo.due_stats(owner, T0, None).await.unwrap().due_count, 5);
        // Another user's topic yields nothing.
        assert_eq!(
            repo.list_due(owner, T0, Some(a9), None, 10)
                .await
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            repo.due_stats(owner, T0, Some(a9)).await.unwrap().due_count,
            0
        );
        assert!(repo.topic_exists(owner, a1).await.unwrap());
        assert!(!repo.topic_exists(owner, a9).await.unwrap());
        assert!(
            !repo
                .topic_exists(owner, TopicId::new(Uuid::from_u128(0xff)))
                .await
                .unwrap()
        );

        // Count desc, then title, then id: a2 (2) and a1 (2, "Rust") by title,
        // then a3 (1).
        let topics = repo.due_topics(owner, T0).await.unwrap();
        let got: Vec<_> = topics
            .iter()
            .map(|t| (t.topic_id, t.topic_title.as_str(), t.due_count))
            .collect();
        assert_eq!(
            got,
            [(a2, "Algebra", 2), (a1, "Rust", 2), (a3, "Algebra", 1)]
        );
        // Nothing due: no topics.
        assert_eq!(
            repo.due_topics(owner, datetime!(2026-01-01 0:00 UTC))
                .await
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn a_vote_is_stored_with_its_log_and_refused_when_stale_or_suspended() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        let before = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        let scheduler = crate::FsrsScheduler::new();
        let params = scheduler.default_parameters();
        let after = scheduler
            .schedule(&before, Rating::Good, T0, &params)
            .unwrap();

        repo.apply_review(0, &after, &log_for(cid(1), 1, before, after), None)
            .await
            .unwrap();
        let stored = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        assert_eq!(stored, after);
        assert_eq!(stored.review_count, 1);
        // A day or more: the model graduates a card voted "good" at once.
        assert_eq!(stored.state, ReviewState::Review);
        assert!(stored.stability > 0.0 && stored.stability_fast > 0.0);
        let model: String = sqlx::query_scalar("SELECT model FROM review_cards WHERE id = ?")
            .bind(cid(1).to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(model, "fsrs7");

        // Same vote again: the version moved on, and no second log is written.
        let log = log_for(cid(1), 2, before, after);
        let again = repo.apply_review(0, &after, &log, None).await;
        assert!(matches!(again, Err(RepositoryError::Conflict)));
        let log = log_for(cid(9), 3, before, after);
        let missing = repo.apply_review(0, &after, &log, None).await;
        assert!(matches!(missing, Err(RepositoryError::NotFound)));
        let logs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM review_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(logs, 1);

        repo.suspend(domain::UserId::OWNER, cid(1), T0)
            .await
            .unwrap();
        let log = log_for(cid(1), 4, after, after);
        let suspended = repo.apply_review(1, &after, &log, None).await;
        assert!(matches!(suspended, Err(RepositoryError::Conflict)));
    }

    #[tokio::test]
    async fn undo_puts_back_the_exact_schedule_once() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        let scheduler = crate::FsrsScheduler::new();
        let params = scheduler.default_parameters();
        let s0 = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        let s1 = scheduler.schedule(&s0, Rating::Good, T0, &params).unwrap();
        let s2 = scheduler
            .schedule(&s1, Rating::Good, s1.due, &params)
            .unwrap();
        repo.apply_review(0, &s1, &log_for(cid(1), 1, s0, s1), None)
            .await
            .unwrap();
        repo.apply_review(1, &s2, &log_for(cid(1), 2, s1, s2), None)
            .await
            .unwrap();

        let last = repo.last_log(cid(1)).await.unwrap().unwrap();
        assert_eq!((last.id, last.before, last.after), (last.id, s1, s2));
        assert_eq!(last.id, ReviewLogId::new(Uuid::from_u128(2)));
        // The earlier vote is not the last one: it cannot be undone now.
        let old = repo
            .undo_review(ReviewLogId::new(Uuid::from_u128(1)), T0)
            .await;
        // Refused either way; this vote was also given on the previous model.
        assert!(matches!(old, Err(RepositoryError::RuleViolated)));

        repo.undo_review(last.id, T0).await.unwrap();
        assert_eq!(
            repo.get(domain::UserId::OWNER, cid(1))
                .await
                .unwrap()
                .unwrap()
                .card
                .schedule,
            s1
        );
        let undone = repo.last_log(cid(1)).await.unwrap().unwrap();
        assert_eq!(undone.undone_at, Some(T0));
        assert!(matches!(
            repo.undo_review(last.id, T0).await,
            Err(RepositoryError::Conflict)
        ));
        assert!(matches!(
            repo.undo_review(ReviewLogId::new(Uuid::from_u128(77)), T0)
                .await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn existing_cards_stay_on_the_previous_model_and_read_with_zero_fast_trace() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-10T09:00:00.000Z"),
        )
        .await;
        let model: String = sqlx::query_scalar("SELECT model FROM review_cards")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(model, "fsrs5");
        let s = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        assert_eq!((s.stability_fast, s.learning_step), (0.0, 0));
    }

    #[test]
    fn a_snapshot_written_before_fsrs7_still_reads() {
        let legacy = r#"{"state":"review","stability":12.5,"difficulty":4.25,
            "due":"2026-11-20T10:00:00.000Z","last_reviewed_at":"2026-11-09T10:00:00.000Z",
            "review_count":3,"lapses":1}"#;
        let s = schedule_parse(legacy, "log").unwrap();
        assert_eq!(
            (s.state, s.stability, s.stability_fast, s.learning_step),
            (ReviewState::Review, 12.5, 0.0, 0)
        );
        assert_eq!(snapshot_model(legacy, "log").unwrap(), "fsrs5");

        let now = CardSchedule {
            stability_fast: 2.5,
            learning_step: 1,
            ..s
        };
        let stamp = Some(T0);
        let json = schedule_json(&now, "fsrs7", stamp);
        assert_eq!(schedule_parse(&json, "log").unwrap(), now);
        assert_eq!(snapshot_model(&json, "log").unwrap(), "fsrs7");
        assert_eq!(snapshot_weights(&json, "log").unwrap(), stamp);
        assert_eq!(snapshot_weights(legacy, "log").unwrap(), None);
    }

    #[tokio::test]
    async fn a_vote_given_on_the_previous_model_cannot_be_undone() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        let scheduler = crate::FsrsScheduler::new();
        let params = scheduler.default_parameters();
        let s0 = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        let s1 = scheduler.schedule(&s0, Rating::Good, T0, &params).unwrap();
        // The card is still `fsrs5` in storage: the snapshot says so.
        repo.apply_review(0, &s1, &log_for(cid(1), 1, s0, s1), None)
            .await
            .unwrap();
        let raw: String = sqlx::query_scalar("SELECT state_before FROM review_logs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(snapshot_model(&raw, "x").unwrap(), "fsrs5");
        let undo = repo
            .undo_review(ReviewLogId::new(Uuid::from_u128(1)), T0)
            .await;
        assert!(matches!(undo, Err(RepositoryError::RuleViolated)));
        // The card is untouched by the refused undo.
        let after = repo
            .get(domain::UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        assert_eq!(after, s1);

        // The next vote starts from `fsrs7` and can be undone.
        let s2 = scheduler
            .schedule(&s1, Rating::Good, s1.due, &params)
            .unwrap();
        repo.apply_review(1, &s2, &log_for(cid(1), 2, s1, s2), None)
            .await
            .unwrap();
        repo.undo_review(ReviewLogId::new(Uuid::from_u128(2)), T0)
            .await
            .unwrap();
    }

    fn attempt(at: OffsetDateTime, outcome: OptimizationOutcome) -> OptimizationAttempt {
        OptimizationAttempt {
            at,
            useful_votes: 300,
            train_votes: 240,
            test_votes: 60,
            current_log_loss: Some(0.5),
            candidate_log_loss: None,
            outcome,
            error: Some("x".into()),
        }
    }

    #[tokio::test]
    async fn attempts_and_adopted_weights_are_stored_together_and_read_back() {
        let (repo, pool) = env().await;
        let user = UserId::OWNER;
        assert_eq!(repo.last_attempt(user).await.unwrap(), None);
        assert_eq!(repo.parameters(user).await.unwrap(), None);

        let weights = SchedulerParameters {
            model: "fsrs7".into(),
            weights: vec![0.1, 2.5, 1.0e-9],
        };
        let rejected = attempt(T0, OptimizationOutcome::Rejected);
        repo.record_attempt(user, &rejected, None).await.unwrap();
        assert_eq!(repo.parameters(user).await.unwrap(), None);
        let adopted = attempt(T0 + Duration::days(31), OptimizationOutcome::Adopted);
        repo.record_attempt(user, &adopted, Some(&weights))
            .await
            .unwrap();
        // The newest attempt, and the weights exactly as they were written.
        assert_eq!(repo.last_attempt(user).await.unwrap(), Some(adopted));
        assert_eq!(
            repo.parameters(user).await.unwrap(),
            Some(UserWeights {
                parameters: weights,
                adopted_at: T0 + Duration::days(31),
            })
        );

        // Unreadable weights read as none, not as an error.
        sqlx::query("UPDATE user_review_parameters SET weights = '{'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(repo.parameters(user).await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_memory_is_rebuilt_only_for_the_vote_count_that_was_read() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-10T09:00:00.000Z"),
        )
        .await;
        sqlx::query(
            "UPDATE review_cards SET state = 'review', review_count = 2, lapses = 1, \
             learning_step = 1, stability = 5, difficulty = 5, model = 'fsrs5' WHERE id = ?",
        )
        .bind(cid(1).to_string())
        .execute(&pool)
        .await
        .unwrap();
        for (n, rating, at) in [(1, "good", T0), (2, "again", T0 + Duration::days(2))] {
            sqlx::query(
                "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
                 state_after, created_at) VALUES (?, ?, ?, ?, '{}', '{}', ?)",
            )
            .bind(Uuid::from_u128(0x900 + n).to_string())
            .bind(cid(1).to_string())
            .bind(rating)
            .bind(format_ts(at))
            .bind(format_ts(at))
            .execute(&pool)
            .await
            .unwrap();
        }
        let (count, events) = repo.card_snapshot(cid(1)).await.unwrap();
        assert_eq!((count, events.len()), (2, 2));
        assert!(matches!(
            repo.card_snapshot(cid(99)).await,
            Err(RepositoryError::NotFound)
        ));

        let memory = MemoryValues {
            stability: 7.5,
            difficulty: 4.0,
            stability_fast: 2.0,
        };
        // A vote came in meanwhile: nothing is written.
        let stamp = T0 + Duration::days(31);
        assert!(
            !repo
                .rebuild_memory(cid(1), 1, Some(memory), stamp)
                .await
                .unwrap()
        );
        assert!(
            repo.rebuild_memory(cid(1), 2, Some(memory), stamp)
                .await
                .unwrap()
        );
        let row = sqlx::query(
            "SELECT stability, difficulty, stability_fast, model, state, learning_step, due, \
             review_count, lapses FROM review_cards WHERE id = ?",
        )
        .bind(cid(1).to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<f64, _>("stability_fast"), 2.0);
        assert_eq!(row.get::<String, _>("model"), "fsrs7");
        // What the card is due for stays.
        assert_eq!(row.get::<String, _>("state"), "review");
        assert_eq!(row.get::<i64, _>("learning_step"), 1);
        assert_eq!(row.get::<String, _>("due"), "2026-11-10T09:00:00.000Z");
        assert_eq!(
            (
                row.get::<i64, _>("review_count"),
                row.get::<i64, _>("lapses")
            ),
            (2, 1)
        );

        // The votes of the user, undone ones left out.
        sqlx::query("UPDATE review_logs SET undone_at = ? WHERE rating = 'again'")
            .bind(format_ts(T0))
            .execute(&pool)
            .await
            .unwrap();
        let cards = repo.card_votes(UserId::OWNER).await.unwrap();
        assert_eq!((cards.len(), cards[0].votes.len()), (1, 1));
        assert_eq!(
            repo.users_with_review_data().await.unwrap(),
            [UserId::OWNER]
        );
    }

    async fn stamp_of(pool: &SqlitePool, card: ReviewCardId) -> Option<String> {
        sqlx::query_scalar("SELECT weights_adopted_at FROM review_cards WHERE id = ?")
            .bind(card.to_string())
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn cards_out_of_date_are_listed_with_a_null_stamp_too_and_rebuilding_stamps_them() {
        let (repo, pool) = env().await;
        for n in [1, 2] {
            insert_card(
                &pool,
                n,
                "2026-11-09T09:00:00.000Z",
                Some("2026-11-10T09:00:00.000Z"),
            )
            .await;
        }
        // Card 2 is on the current model, with no stamp (the default weights).
        sqlx::query("UPDATE review_cards SET model = 'fsrs7' WHERE id = ?")
            .bind(cid(2).to_string())
            .execute(&pool)
            .await
            .unwrap();
        let user = UserId::OWNER;
        let s1 = T0;
        // `NULL IS NOT stamp`: the unstamped card is listed, not skipped.
        assert_eq!(
            repo.cards_out_of_date(user, s1).await.unwrap(),
            [cid(1), cid(2)]
        );

        // A card without votes has no memory: only the stamp is written.
        assert!(repo.rebuild_memory(cid(2), 0, None, s1).await.unwrap());
        assert_eq!(
            stamp_of(&pool, cid(2)).await.as_deref(),
            Some(format_ts(s1).as_str())
        );
        assert_eq!(repo.cards_out_of_date(user, s1).await.unwrap(), [cid(1)]);
        let item = repo.get(user, cid(2)).await.unwrap().unwrap();
        assert_eq!(item.basis, MemoryBasis::current(Some(s1)));
        assert_eq!(item.card.schedule.stability, 0.0);

        // New weights make every card out of date again.
        let s2 = s1 + Duration::days(40);
        assert_eq!(
            repo.cards_out_of_date(user, s2).await.unwrap(),
            [cid(1), cid(2)]
        );
        // The startup migration puts a card on the default weights: no stamp.
        repo.mark_migrated(cid(1), None).await.unwrap();
        assert_eq!(stamp_of(&pool, cid(1)).await, None);
        let item = repo.get(user, cid(1)).await.unwrap().unwrap();
        assert_eq!(item.basis, MemoryBasis::current(None));
    }

    #[tokio::test]
    async fn a_vote_stamps_the_card_and_an_undo_puts_the_stamp_back() {
        let (repo, pool) = env().await;
        insert_card(
            &pool,
            1,
            "2026-11-09T09:00:00.000Z",
            Some("2026-11-09T09:00:00.000Z"),
        )
        .await;
        repo.mark_migrated(cid(1), None).await.unwrap();
        let scheduler = crate::FsrsScheduler::new();
        let params = scheduler.default_parameters();
        let s0 = repo
            .get(UserId::OWNER, cid(1))
            .await
            .unwrap()
            .unwrap()
            .card
            .schedule;
        let s1 = scheduler.schedule(&s0, Rating::Good, T0, &params).unwrap();
        let stamp = T0 - Duration::days(3);
        repo.apply_review(0, &s1, &log_for(cid(1), 1, s0, s1), Some(stamp))
            .await
            .unwrap();
        assert_eq!(
            stamp_of(&pool, cid(1)).await.as_deref(),
            Some(format_ts(stamp).as_str())
        );
        // Weights changed after the vote and the card was rebuilt with them.
        sqlx::query("UPDATE review_cards SET weights_adopted_at = ? WHERE id = ?")
            .bind(format_ts(T0))
            .bind(cid(1).to_string())
            .execute(&pool)
            .await
            .unwrap();
        repo.undo_review(ReviewLogId::new(Uuid::from_u128(1)), T0)
            .await
            .unwrap();
        // The memory that comes back is the one of the old stamp.
        assert_eq!(
            stamp_of(&pool, cid(1)).await.as_deref(),
            Some(format_ts(stamp).as_str())
        );
    }
}
