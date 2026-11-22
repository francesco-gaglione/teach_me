//! Learning record, glossary and review-card adapter.

use super::{as_conflict, count, to_i64};
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};
use async_trait::async_trait;
use domain::{
    CardOrigin, CardSchedule, CardStatus, CardTarget, CompletionSave, CompletionSummary,
    GlossaryTerm, GlossaryTermId, LearningRecord, LearningRecordId, LearningRepository, LessonId,
    LessonOutputs, Rating, RecordCursor, RecordKind, RecordStatus, RepositoryError, ReviewCard,
    ReviewCardId, Suspension, SuspensionReason, TermCursor, TopicId, is_to_consolidate,
    term_sort_key,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

fn record_from_row(row: &SqliteRow) -> Result<LearningRecord, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let number: i64 = row.try_get("number").map_err(storage)?;
    let kind: String = row.try_get("kind").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let by: Option<String> = row.try_get("superseded_by").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    Ok(LearningRecord {
        number: count(number, "record number", &id)?,
        kind: kind
            .parse::<RecordKind>()
            .map_err(|_| corrupt("record kind", &id))?,
        title: row.try_get("title").map_err(storage)?,
        body: row.try_get("body").map_err(storage)?,
        evidence: row.try_get("evidence").map_err(storage)?,
        status: status
            .parse::<RecordStatus>()
            .map_err(|_| corrupt("record status", &id))?,
        superseded_by: by
            .as_deref()
            .map(|b| parse_id(b).map(LearningRecordId::new))
            .transpose()?,
        lesson_id: LessonId::new(parse_id(&lesson)?),
        created_at: parse_ts(&created)?,
        topic_id: TopicId::new(parse_id(&topic)?),
        id: LearningRecordId::new(parse_id(&id)?),
    })
}

macro_rules! term_columns {
    () => {
        "id, topic_id, key, term, definition, avoid, lesson_id, first_lesson_id, created_at, \
        updated_at"
    };
}

fn term_from_row(row: &SqliteRow) -> Result<GlossaryTerm, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let avoid: String = row.try_get("avoid").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let first: Option<String> = row.try_get("first_lesson_id").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let updated: String = row.try_get("updated_at").map_err(storage)?;
    let avoid: Vec<String> =
        serde_json::from_str(&avoid).map_err(|_| corrupt("term avoid", &id))?;
    Ok(GlossaryTerm {
        key: row.try_get("key").map_err(storage)?,
        term: row.try_get("term").map_err(storage)?,
        definition: row.try_get("definition").map_err(storage)?,
        avoid,
        lesson_id: LessonId::new(parse_id(&lesson)?),
        first_lesson_id: first.map(|f| parse_id(&f).map(LessonId::new)).transpose()?,
        created_at: parse_ts(&created)?,
        updated_at: parse_ts(&updated)?,
        topic_id: TopicId::new(parse_id(&topic)?),
        id: GlossaryTermId::new(parse_id(&id)?),
    })
}

/// The ratings `card_from_row` needs to tell whether the card is still "to
/// consolidate": the votes not undone, in application order, as one
/// comma-separated column, fetched with the card to avoid a query per card.
/// Only for the cards born unpassed: the others never read it. `$card` is the
/// table or alias the card is selected from.
macro_rules! consolidation_ratings {
    ($card:literal) => {
        concat!(
            "CASE WHEN ",
            $card,
            ".unpassed_block IS NULL THEN NULL ELSE (SELECT group_concat(rating, ',' \
             ORDER BY created_at, rowid) FROM review_logs WHERE card_id = ",
            $card,
            ".id AND undone_at IS NULL) END AS ratings"
        )
    };
}
pub(crate) use consolidation_ratings;

/// Columns `card_from_row` reads.
macro_rules! card_columns {
    () => {
        concat!(
        "id, topic_id, lesson_id, record_id, term_id, front, back, status, suspension, \
         suspended_at, created_at, state, stability, difficulty, stability_fast, learning_step, due, last_reviewed_at, \
         review_count, lapses, unpassed_block, ",
        consolidation_ratings!("review_cards")
        )
    };
}

/// The suspension columns, checked against `status`, which is the source of
/// truth: an active card has neither, a suspended one has both. Suspended
/// cards without a reason are repaired by `backfill_suspensions` (at startup
/// and before the suspension use cases), so here they are plain corruption.
fn suspension_from_row(
    row: &SqliteRow,
    id: &str,
    status: CardStatus,
) -> Result<Option<Suspension>, RepositoryError> {
    let reason: Option<String> = row.try_get("suspension").map_err(storage)?;
    let at: Option<String> = row.try_get("suspended_at").map_err(storage)?;
    match (status, reason, at) {
        (CardStatus::Active, None, None) => Ok(None),
        (CardStatus::Suspended, Some(reason), Some(at)) => Ok(Some(Suspension {
            reason: reason
                .parse::<SuspensionReason>()
                .map_err(|_| corrupt("card suspension", id))?,
            at: parse_ts(&at)?,
        })),
        _ => Err(corrupt("card suspension", id)),
    }
}

pub(crate) fn card_from_row(row: &SqliteRow) -> Result<ReviewCard, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic: String = row.try_get("topic_id").map_err(storage)?;
    let lesson: String = row.try_get("lesson_id").map_err(storage)?;
    let record: Option<String> = row.try_get("record_id").map_err(storage)?;
    let term: Option<String> = row.try_get("term_id").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let status = status
        .parse::<CardStatus>()
        .map_err(|_| corrupt("card status", &id))?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let origin = match (record, term) {
        (Some(r), None) => CardOrigin::Record(LearningRecordId::new(parse_id(&r)?)),
        (None, Some(t)) => CardOrigin::Term(GlossaryTermId::new(parse_id(&t)?)),
        _ => return Err(corrupt("card origin", &id)),
    };
    let unpassed: Option<String> = row.try_get("unpassed_block").map_err(storage)?;
    let ratings: Option<String> = row.try_get("ratings").map_err(storage)?;
    let to_consolidate = is_to_consolidate(
        unpassed.is_some(),
        &ratings
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|r| !r.is_empty())
            .map(|r| {
                r.parse::<Rating>()
                    .map_err(|_| corrupt("review rating", &id))
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    Ok(ReviewCard {
        to_consolidate,
        topic_id: TopicId::new(parse_id(&topic)?),
        lesson_id: LessonId::new(parse_id(&lesson)?),
        origin,
        front: row.try_get("front").map_err(storage)?,
        back: row.try_get("back").map_err(storage)?,
        status,
        suspension: suspension_from_row(row, &id, status)?,
        schedule: crate::reviews::schedule_from_row(row, &id)?,
        created_at: parse_ts(&created)?,
        id: ReviewCardId::new(parse_id(&id)?),
    })
}

#[derive(Clone)]
pub struct SqliteLearningRepository {
    pool: SqlitePool,
}

impl SqliteLearningRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LearningRepository for SqliteLearningRepository {
    async fn list_records(
        &self,
        topic_id: TopicId,
        active_only: bool,
        after: Option<RecordCursor>,
        limit: u32,
    ) -> Result<Vec<LearningRecord>, RepositoryError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            record_columns!(),
            " FROM learning_records WHERE topic_id = ? \
             AND (? = 0 OR status = 'active') AND (? IS NULL OR number < ?) \
             ORDER BY number DESC LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(i64::from(active_only))
        .bind(after.map(|c| to_i64(c.number)))
        .bind(after.map(|c| to_i64(c.number)))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(record_from_row).collect()
    }

    async fn list_terms(
        &self,
        topic_id: TopicId,
        after: Option<TermCursor>,
        limit: u32,
    ) -> Result<Vec<GlossaryTerm>, RepositoryError> {
        let sort = after.as_ref().map(|c| c.sort_key.clone());
        let key = after.as_ref().map(|c| c.key.clone());
        let rows = sqlx::query(concat!(
            "SELECT ",
            term_columns!(),
            " FROM glossary_terms WHERE topic_id = ? \
             AND (? IS NULL OR term_key > ? OR (term_key = ? AND key > ?)) \
             ORDER BY term_key, key LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(&sort)
        .bind(&sort)
        .bind(&sort)
        .bind(&key)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(term_from_row).collect()
    }

    async fn lesson_outputs(&self, lesson_id: LessonId) -> Result<LessonOutputs, RepositoryError> {
        let lesson = lesson_id.to_string();
        let records = sqlx::query(concat!(
            "SELECT ",
            record_columns!(),
            " FROM learning_records WHERE lesson_id = ? ORDER BY number"
        ))
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let terms = sqlx::query(concat!(
            "SELECT ",
            term_columns!(),
            " FROM glossary_terms WHERE lesson_id = ? ORDER BY term_key, key"
        ))
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let cards: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM review_cards WHERE lesson_id = ?")
                .bind(&lesson)
                .fetch_one(&self.pool)
                .await
                .map_err(storage)?;
        Ok(LessonOutputs {
            records: records
                .iter()
                .map(record_from_row)
                .collect::<Result<_, _>>()?,
            terms: terms.iter().map(term_from_row).collect::<Result<_, _>>()?,
            review_card_count: count(cards, "card count", &lesson)?,
        })
    }

    async fn review_cards(&self, topic_id: TopicId) -> Result<Vec<ReviewCard>, RepositoryError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            card_columns!(),
            " FROM review_cards WHERE topic_id = ? ORDER BY created_at, id"
        ))
        .bind(topic_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(card_from_row).collect()
    }

    #[allow(clippy::too_many_lines)] // one transaction, in the order the rules need
    async fn save_completion(
        &self,
        save: &CompletionSave,
    ) -> Result<CompletionSummary, RepositoryError> {
        let topic = save.topic_id.to_string();
        let lesson = save.lesson_id.to_string();
        let at = format_ts(save.at);
        let summary = CompletionSummary {
            record_count: u32::try_from(save.records.len()).unwrap_or(u32::MAX),
            term_count: u32::try_from(save.terms.len()).unwrap_or(u32::MAX),
            review_card_count: u32::try_from(save.cards.len()).unwrap_or(u32::MAX),
        };
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // First statement: a write that also settles "already recorded".
        sqlx::query(
            "INSERT INTO lesson_completions (lesson_id, record_count, term_count, \
             review_card_count, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&lesson)
        .bind(to_i64(summary.record_count))
        .bind(to_i64(summary.term_count))
        .bind(to_i64(summary.review_card_count))
        .bind(&at)
        .execute(&mut *tx)
        .await
        .map_err(as_conflict)?;

        let mut record_ids = Vec::new();
        for new in &save.records {
            sqlx::query(
                "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, \
                 lesson_id, created_at) VALUES (?, ?, \
                 (SELECT COALESCE(MAX(number), 0) + 1 FROM learning_records WHERE topic_id = ?), \
                 ?, ?, ?, ?, ?, ?)",
            )
            .bind(new.id.to_string())
            .bind(&topic)
            .bind(&topic)
            .bind(new.kind.as_str())
            .bind(&new.title)
            .bind(&new.body)
            .bind(&new.evidence)
            .bind(&lesson)
            .bind(&at)
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
            record_ids.push(new.id);
            for number in &new.supersedes {
                let old: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM learning_records \
                     WHERE topic_id = ? AND number = ? AND status = 'active' AND id != ?",
                )
                .bind(&topic)
                .bind(to_i64(*number))
                .bind(new.id.to_string())
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage)?;
                // A replaced record that is not active any more: the whole
                // save is refused (dropping `tx` rolls it back).
                let old = old.ok_or(RepositoryError::Conflict)?;
                sqlx::query(
                    "UPDATE learning_records SET status = 'superseded', superseded_by = ? \
                     WHERE id = ?",
                )
                .bind(new.id.to_string())
                .bind(&old)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
                // Also a card the learner had suspended: from now on it is
                // superseded and cannot be reactivated.
                sqlx::query(
                    "UPDATE review_cards SET status = 'suspended', suspension = 'superseded', \
                     suspended_at = ? WHERE record_id = ? AND suspension IS NOT 'superseded'",
                )
                .bind(&at)
                .bind(&old)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            }
        }

        let mut term_ids = Vec::new();
        for up in &save.terms {
            let existing: Option<String> =
                sqlx::query_scalar("SELECT id FROM glossary_terms WHERE topic_id = ? AND key = ?")
                    .bind(&topic)
                    .bind(&up.key)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(storage)?;
            let avoid = serde_json::to_string(&up.avoid).unwrap_or_else(|_| "[]".to_owned());
            if let Some(id) = existing {
                // Revised in place: same row, same id, so its cards stay linked.
                sqlx::query(
                    "UPDATE glossary_terms SET term = ?, term_key = ?, definition = ?, avoid = ?, \
                     lesson_id = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&up.term)
                .bind(term_sort_key(&up.term))
                .bind(&up.definition)
                .bind(&avoid)
                .bind(&lesson)
                .bind(&at)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
                term_ids.push(GlossaryTermId::new(parse_id(&id)?));
            } else {
                sqlx::query(
                    "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, \
                     avoid, lesson_id, first_lesson_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(up.id.to_string())
                .bind(&topic)
                .bind(&up.key)
                .bind(&up.term)
                .bind(term_sort_key(&up.term))
                .bind(&up.definition)
                .bind(&avoid)
                .bind(&lesson)
                .bind(&lesson)
                .bind(&at)
                .bind(&at)
                .execute(&mut *tx)
                .await
                .map_err(constraint)?;
                term_ids.push(up.id);
            }
        }

        for card in &save.cards {
            let (record, term) = match card.target {
                CardTarget::Record(i) => (record_ids.get(i).map(ToString::to_string), None),
                CardTarget::Term(i) => (None, term_ids.get(i).map(ToString::to_string)),
            };
            sqlx::query(
                "INSERT INTO review_cards (id, topic_id, lesson_id, record_id, term_id, front, \
                 back, created_at, due, model, unpassed_block) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'fsrs7', ?)",
            )
            .bind(card.id.to_string())
            .bind(&topic)
            .bind(&lesson)
            .bind(record)
            .bind(term)
            .bind(&card.front)
            .bind(&card.back)
            .bind(&at)
            .bind(format_ts(CardSchedule::initial(save.at).due))
            .bind(card.unpassed_block.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        tx.commit().await.map_err(storage)?;
        Ok(summary)
    }

    async fn completion_summary(
        &self,
        lesson_id: LessonId,
    ) -> Result<Option<CompletionSummary>, RepositoryError> {
        let row = sqlx::query(
            "SELECT record_count, term_count, review_card_count FROM lesson_completions \
             WHERE lesson_id = ?",
        )
        .bind(lesson_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let id = lesson_id.to_string();
        row.map(|row| {
            let n = |col: &str| -> Result<u32, RepositoryError> {
                let v: i64 = row.try_get(col).map_err(storage)?;
                count(v, "completion count", &id)
            };
            Ok(CompletionSummary {
                record_count: n("record_count")?,
                term_count: n("term_count")?,
                review_card_count: n("review_card_count")?,
            })
        })
        .transpose()
    }
}
