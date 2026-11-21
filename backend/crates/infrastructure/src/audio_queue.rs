//! The persistent audio queue in SQLite (ADR 0013 §11, migration 0018).

use std::collections::HashMap;

use async_trait::async_trait;
use domain::{AudioQueue, AudioRequest, BlockAudioState, LessonId, QueuedAudio, RepositoryError};
use sqlx::{Row, SqlitePool};

use crate::repositories::{constraint, corrupt, parse_id, storage};

#[derive(Clone)]
pub struct SqliteAudioQueue {
    pool: SqlitePool,
}

impl SqliteAudioQueue {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn state_of(raw: &str) -> Result<BlockAudioState, RepositoryError> {
    match raw {
        "queued" => Ok(BlockAudioState::Queued),
        "running" => Ok(BlockAudioState::Running),
        "failed" => Ok(BlockAudioState::Failed),
        _ => Err(corrupt("audio queue state", raw)),
    }
}

#[async_trait]
impl AudioQueue for SqliteAudioQueue {
    async fn enqueue(
        &self,
        lesson: LessonId,
        voice: &str,
        requests: &[AudioRequest],
        front: bool,
    ) -> Result<(), RepositoryError> {
        let lesson = lesson.to_string();
        // One transaction: a reader never sees a lesson half-queued.
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The lesson is now wanted in `voice`: what is still waiting for
        // another voice (the user changed it) would only burn engine time.
        // A running entry is left to finish.
        sqlx::query(
            "DELETE FROM audio_queue WHERE lesson_id = ? AND voice <> ? AND state = 'queued'",
        )
        .bind(&lesson)
        .bind(voice)
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        for request in requests {
            // A failed entry starts over only when the lesson is opened.
            if front {
                sqlx::query(
                    "DELETE FROM audio_queue \
                 WHERE lesson_id = ? AND voice = ? AND block_id = ? AND state = 'failed'",
                )
                .bind(&lesson)
                .bind(voice)
                .bind(&request.block_id)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            }
            sqlx::query(
                "INSERT OR IGNORE INTO audio_queue (lesson_id, voice, language, block_id, text, state) \
                 VALUES (?, ?, ?, ?, ?, 'queued')",
            )
            .bind(&lesson)
            .bind(voice)
            .bind(request.language.code())
            .bind(&request.block_id)
            .bind(&request.text)
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        if front {
            sqlx::query(
                "UPDATE audio_queue \
                 SET bump = (SELECT COALESCE(MAX(bump), 0) + 1 FROM audio_queue) \
                 WHERE lesson_id = ? AND voice = ? AND state = 'queued'",
            )
            .bind(&lesson)
            .bind(voice)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }

    async fn claim_next(&self) -> Result<Option<QueuedAudio>, RepositoryError> {
        // One statement: two claimers cannot take the same entry.
        let row = sqlx::query(
            "UPDATE audio_queue SET state = 'running' WHERE id = ( \
                 SELECT id FROM audio_queue WHERE state = 'queued' \
                 ORDER BY bump DESC, id ASC LIMIT 1) \
             RETURNING id, lesson_id, voice, language, block_id, text",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let Some(row) = row else { return Ok(None) };
        let lesson: String = row.try_get("lesson_id").map_err(storage)?;
        let language: String = row.try_get("language").map_err(storage)?;
        Ok(Some(QueuedAudio {
            entry: row.try_get("id").map_err(storage)?,
            request: AudioRequest {
                lesson_id: LessonId::new(parse_id(&lesson)?),
                voice: row.try_get("voice").map_err(storage)?,
                language: language
                    .parse()
                    .map_err(|_| corrupt("audio queue language", &language))?,
                block_id: row.try_get("block_id").map_err(storage)?,
                text: row.try_get("text").map_err(storage)?,
            },
        }))
    }

    async fn complete(&self, entry: i64) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM audio_queue WHERE id = ?")
            .bind(entry)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn release(&self, entry: i64) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE audio_queue SET state = 'queued' WHERE id = ? AND state = 'running'")
            .bind(entry)
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn record_failure(
        &self,
        entry: i64,
        max_attempts: u32,
    ) -> Result<BlockAudioState, RepositoryError> {
        let row = sqlx::query(
            "UPDATE audio_queue SET attempts = attempts + 1, \
                 state = CASE WHEN attempts + 1 >= ? THEN 'failed' ELSE 'queued' END \
             WHERE id = ? RETURNING state",
        )
        .bind(i64::from(max_attempts))
        .bind(entry)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        match row {
            Some(row) => state_of(&row.try_get::<String, _>("state").map_err(storage)?),
            // The lesson went away meanwhile: nothing left to retry.
            None => Ok(BlockAudioState::Failed),
        }
    }

    async fn recover(&self) -> Result<u64, RepositoryError> {
        let done = sqlx::query("UPDATE audio_queue SET state = 'queued' WHERE state = 'running'")
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected())
    }

    async fn states(
        &self,
        lesson: LessonId,
        voice: &str,
    ) -> Result<HashMap<String, BlockAudioState>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT block_id, state FROM audio_queue WHERE lesson_id = ? AND voice = ?",
        )
        .bind(lesson.to_string())
        .bind(voice)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let state: String = row.try_get("state").map_err(storage)?;
                Ok((row.try_get("block_id").map_err(storage)?, state_of(&state)?))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use uuid::Uuid;

    fn lesson(n: u128) -> LessonId {
        LessonId::new(Uuid::from_u128(n))
    }

    fn req(l: LessonId, block: &str) -> AudioRequest {
        AudioRequest {
            lesson_id: l,
            voice: "a".into(),
            language: domain::ContentLanguage::It,
            block_id: block.into(),
            text: format!("text of {block}"),
        }
    }

    async fn queue() -> SqliteAudioQueue {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        // The tests queue for lessons they do not insert. The pool has one
        // connection, so the pragma holds for all of them.
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
        SqliteAudioQueue::new(pool)
    }

    async fn drain(q: &SqliteAudioQueue) -> Vec<(u128, String)> {
        let mut out = Vec::new();
        while let Some(next) = q.claim_next().await.unwrap() {
            out.push((
                next.request.lesson_id.as_uuid().as_u128(),
                next.request.block_id,
            ));
            q.complete(next.entry).await.unwrap();
        }
        out
    }

    fn pairs(items: &[(u128, &str)]) -> Vec<(u128, String)> {
        items.iter().map(|(l, b)| (*l, (*b).to_owned())).collect()
    }

    #[tokio::test]
    async fn entries_come_out_in_the_order_they_were_requested() {
        let q = queue().await;
        q.enqueue(
            lesson(1),
            "a",
            &[req(lesson(1), "x"), req(lesson(1), "y")],
            false,
        )
        .await
        .unwrap();
        q.enqueue(lesson(2), "a", &[req(lesson(2), "z")], false)
            .await
            .unwrap();
        assert_eq!(drain(&q).await, pairs(&[(1, "x"), (1, "y"), (2, "z")]));
    }

    #[tokio::test]
    async fn the_lesson_opened_last_goes_first_and_keeps_its_own_order() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        q.enqueue(
            lesson(2),
            "a",
            &[req(lesson(2), "y"), req(lesson(2), "z")],
            false,
        )
        .await
        .unwrap();
        q.enqueue(lesson(3), "a", &[req(lesson(3), "w")], false)
            .await
            .unwrap();
        // Opening lesson 2, then lesson 3, then lesson 2 again.
        q.enqueue(lesson(2), "a", &[], true).await.unwrap();
        q.enqueue(lesson(3), "a", &[], true).await.unwrap();
        q.enqueue(lesson(2), "a", &[req(lesson(2), "y")], true)
            .await
            .unwrap();
        assert_eq!(
            drain(&q).await,
            pairs(&[(2, "y"), (2, "z"), (3, "w"), (1, "x")])
        );
    }

    #[tokio::test]
    async fn a_lesson_queued_after_an_open_stays_behind_it() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], true)
            .await
            .unwrap();
        // A generation finishing meanwhile queues at the back.
        q.enqueue(lesson(2), "a", &[req(lesson(2), "y")], false)
            .await
            .unwrap();
        assert_eq!(drain(&q).await, pairs(&[(1, "x"), (2, "y")]));
    }

    #[tokio::test]
    async fn a_request_in_a_new_voice_drops_the_queued_entries_of_the_old_one_only() {
        let q = queue().await;
        let two = [req(lesson(1), "x"), req(lesson(1), "y")];
        q.enqueue(lesson(1), "a", &two, false).await.unwrap();
        q.enqueue(lesson(2), "a", &[req(lesson(2), "z")], false)
            .await
            .unwrap();
        let running = q.claim_next().await.unwrap().unwrap();
        assert_eq!(running.request.block_id, "x");

        // The user switched to "b": lesson 1 is opened again in that voice.
        q.enqueue(lesson(1), "b", &two, true).await.unwrap();
        let old = q.states(lesson(1), "a").await.unwrap();
        assert_eq!(old.len(), 1, "{old:?}");
        assert_eq!(old["x"], BlockAudioState::Running);
        assert_eq!(q.states(lesson(1), "b").await.unwrap().len(), 2);
        // Another lesson is not about this request.
        assert_eq!(q.states(lesson(2), "a").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn requesting_again_does_not_duplicate_or_reorder() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        q.enqueue(lesson(2), "a", &[req(lesson(2), "y")], false)
            .await
            .unwrap();
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        assert_eq!(drain(&q).await, pairs(&[(1, "x"), (2, "y")]));
    }

    #[tokio::test]
    async fn three_failures_make_an_entry_failed_and_a_new_request_restarts_it() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        for expected in [
            BlockAudioState::Queued,
            BlockAudioState::Queued,
            BlockAudioState::Failed,
        ] {
            let next = q.claim_next().await.unwrap().unwrap();
            assert_eq!(q.record_failure(next.entry, 3).await.unwrap(), expected);
        }
        assert!(q.claim_next().await.unwrap().is_none());
        assert_eq!(
            q.states(lesson(1), "a").await.unwrap()["x"],
            BlockAudioState::Failed
        );

        // Only the opening of the lesson starts a failed block over.
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        assert_eq!(
            q.states(lesson(1), "a").await.unwrap()["x"],
            BlockAudioState::Failed
        );

        // Reopened: queued again, with a fresh set of attempts.
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], true)
            .await
            .unwrap();
        assert_eq!(
            q.states(lesson(1), "a").await.unwrap()["x"],
            BlockAudioState::Queued
        );
        let next = q.claim_next().await.unwrap().unwrap();
        assert_eq!(
            q.record_failure(next.entry, 3).await.unwrap(),
            BlockAudioState::Queued
        );
    }

    #[tokio::test]
    async fn releasing_uses_no_attempt() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        for _ in 0..5 {
            let next = q.claim_next().await.unwrap().unwrap();
            q.release(next.entry).await.unwrap();
        }
        let next = q.claim_next().await.unwrap().unwrap();
        assert_eq!(
            q.record_failure(next.entry, 3).await.unwrap(),
            BlockAudioState::Queued
        );
    }

    #[tokio::test]
    async fn running_entries_are_queued_again_after_a_restart() {
        let q = queue().await;
        q.enqueue(
            lesson(1),
            "a",
            &[req(lesson(1), "x"), req(lesson(1), "y")],
            false,
        )
        .await
        .unwrap();
        let taken = q.claim_next().await.unwrap().unwrap();
        let states = q.states(lesson(1), "a").await.unwrap();
        assert_eq!(states["x"], BlockAudioState::Running);
        assert_eq!(states["y"], BlockAudioState::Queued);

        assert_eq!(q.recover().await.unwrap(), 1);
        let again = q.claim_next().await.unwrap().unwrap();
        assert_eq!(again.entry, taken.entry);
    }

    #[tokio::test]
    async fn states_are_per_lesson_and_voice() {
        let q = queue().await;
        q.enqueue(lesson(1), "a", &[req(lesson(1), "x")], false)
            .await
            .unwrap();
        assert!(q.states(lesson(1), "b").await.unwrap().is_empty());
        assert!(q.states(lesson(2), "a").await.unwrap().is_empty());
        let next = q.claim_next().await.unwrap().unwrap();
        q.complete(next.entry).await.unwrap();
        assert!(q.states(lesson(1), "a").await.unwrap().is_empty());
    }
}
