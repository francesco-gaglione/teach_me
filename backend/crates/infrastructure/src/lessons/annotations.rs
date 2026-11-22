//! Highlight and stroke adapters.

use super::{count, to_i64};
use crate::repositories::{corrupt, storage};
use async_trait::async_trait;
use domain::{
    Highlight, HighlightColor, HighlightRepository, LessonId, RepositoryError, Stroke, StrokeColor,
    StrokeRepository, StrokeWidth,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct SqliteHighlightRepository {
    pool: SqlitePool,
}

impl SqliteHighlightRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn highlight_from_row(row: &SqliteRow, lesson: &str) -> Result<Highlight, RepositoryError> {
    let block_id: String = row.try_get("block_id").map_err(storage)?;
    let start: i64 = row.try_get("range_start").map_err(storage)?;
    let end: i64 = row.try_get("range_end").map_err(storage)?;
    let color: String = row.try_get("color").map_err(storage)?;
    Ok(Highlight {
        block_id,
        start: count(start, "highlight start", lesson)?,
        end: count(end, "highlight end", lesson)?,
        color: color
            .parse::<HighlightColor>()
            .map_err(|_| corrupt("highlight color", lesson))?,
    })
}

#[async_trait]
impl HighlightRepository for SqliteHighlightRepository {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Highlight>, RepositoryError> {
        let lesson = lesson_id.to_string();
        let rows = sqlx::query(
            "SELECT block_id, range_start, range_end, color FROM highlights \
             WHERE lesson_id = ? ORDER BY position",
        )
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|r| highlight_from_row(r, &lesson))
            .collect()
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        highlights: &[Highlight],
    ) -> Result<(), RepositoryError> {
        let lesson = lesson_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The first statement is a write, so the transaction takes the write
        // lock up front: a read first would fail with SQLITE_BUSY_SNAPSHOT
        // when two replaces race, instead of "the last one wins".
        let touched = sqlx::query("UPDATE lessons SET id = id WHERE id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        if touched == 0 {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query("DELETE FROM highlights WHERE lesson_id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        for (position, h) in highlights.iter().enumerate() {
            sqlx::query(
                "INSERT INTO highlights (lesson_id, position, block_id, range_start, range_end, color) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&lesson)
            .bind(i64::try_from(position).map_err(|_| corrupt("highlight position", &lesson))?)
            .bind(&h.block_id)
            .bind(to_i64(h.start))
            .bind(to_i64(h.end))
            .bind(h.color.as_str())
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }
}

// --------------------------------------------------------------- strokes

#[derive(Clone)]
pub struct SqliteStrokeRepository {
    pool: SqlitePool,
}

impl SqliteStrokeRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// The points as the packed little-endian `i32` the `points` column holds.
fn pack_points(points: &[i32]) -> Vec<u8> {
    points.iter().flat_map(|p| p.to_le_bytes()).collect()
}

pub(super) fn unpack_points(blob: &[u8], lesson: &str) -> Result<Vec<i32>, RepositoryError> {
    if blob.is_empty() || !blob.len().is_multiple_of(8) {
        return Err(corrupt("stroke points", lesson));
    }
    Ok(blob
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| i32::from_le_bytes(*c))
        .collect())
}

fn stroke_from_row(row: &SqliteRow, lesson: &str) -> Result<Stroke, RepositoryError> {
    let color: String = row.try_get("color").map_err(storage)?;
    let width: String = row.try_get("width").map_err(storage)?;
    let points: Vec<u8> = row.try_get("points").map_err(storage)?;
    Ok(Stroke {
        id: row.try_get("id").map_err(storage)?,
        block_id: row.try_get("block_id").map_err(storage)?,
        color: color
            .parse::<StrokeColor>()
            .map_err(|_| corrupt("stroke color", lesson))?,
        width: width
            .parse::<StrokeWidth>()
            .map_err(|_| corrupt("stroke width", lesson))?,
        points: unpack_points(&points, lesson)?,
    })
}

#[async_trait]
impl StrokeRepository for SqliteStrokeRepository {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Stroke>, RepositoryError> {
        let lesson = lesson_id.to_string();
        let rows = sqlx::query(
            "SELECT id, block_id, color, width, points FROM strokes \
             WHERE lesson_id = ? ORDER BY position",
        )
        .bind(&lesson)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(|r| stroke_from_row(r, &lesson)).collect()
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        strokes: &[Stroke],
    ) -> Result<(), RepositoryError> {
        let lesson = lesson_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // Write first, as in `SqliteHighlightRepository::replace`: racing
        // replaces then serialize ("the last one wins") instead of failing.
        let touched = sqlx::query("UPDATE lessons SET id = id WHERE id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        if touched == 0 {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query("DELETE FROM strokes WHERE lesson_id = ?")
            .bind(&lesson)
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        for (position, s) in strokes.iter().enumerate() {
            sqlx::query(
                "INSERT INTO strokes (lesson_id, position, id, block_id, color, width, points) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&lesson)
            .bind(i64::try_from(position).map_err(|_| corrupt("stroke position", &lesson))?)
            .bind(&s.id)
            .bind(&s.block_id)
            .bind(s.color.as_str())
            .bind(s.width.as_str())
            .bind(pack_points(&s.points))
            .execute(&mut *tx)
            .await
            .map_err(storage)?;
        }
        tx.commit().await.map_err(storage)
    }
}

// -------------------------------------------------------------- learning
