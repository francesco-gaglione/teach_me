//! SQLite adapter for the daily LLM token counts.

use async_trait::async_trait;
use domain::{LlmUsageRepository, RepositoryError, UserId};
use sqlx::SqlitePool;
use time::Date;

use crate::repositories::{constraint, storage};

#[derive(Clone)]
pub struct SqliteLlmUsageRepository {
    pool: SqlitePool,
}

impl SqliteLlmUsageRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LlmUsageRepository for SqliteLlmUsageRepository {
    async fn tokens_on(&self, user: UserId, day: Date) -> Result<u64, RepositoryError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT tokens FROM llm_usage WHERE user_id = ? AND day = ?")
                .bind(user.to_string())
                .bind(day.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(row.map_or(0, |(n,)| u64::try_from(n).unwrap_or(0)))
    }

    async fn add_tokens(
        &self,
        user: UserId,
        day: Date,
        tokens: u64,
    ) -> Result<(), RepositoryError> {
        // Saturates instead of failing: a count past i64::MAX is nonsense.
        let tokens = i64::try_from(tokens).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO llm_usage (user_id, day, tokens) VALUES (?, ?, ?) \
             ON CONFLICT (user_id, day) DO UPDATE SET tokens = MIN(tokens + excluded.tokens, 9223372036854775807)",
        )
        .bind(user.to_string())
        .bind(day.to_string())
        .bind(tokens)
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use time::macros::date;

    async fn repo() -> SqliteLlmUsageRepository {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        SqliteLlmUsageRepository::new(pool)
    }

    #[tokio::test]
    async fn counts_accumulate_per_user_and_day() {
        let repo = repo().await;
        let user = UserId::OWNER;
        assert_eq!(
            repo.tokens_on(user, date!(2026 - 11 - 22)).await.unwrap(),
            0
        );
        repo.add_tokens(user, date!(2026 - 11 - 22), 100)
            .await
            .unwrap();
        repo.add_tokens(user, date!(2026 - 11 - 22), 50)
            .await
            .unwrap();
        repo.add_tokens(user, date!(2026 - 11 - 23), 7)
            .await
            .unwrap();
        assert_eq!(
            repo.tokens_on(user, date!(2026 - 11 - 22)).await.unwrap(),
            150
        );
        assert_eq!(
            repo.tokens_on(user, date!(2026 - 11 - 23)).await.unwrap(),
            7
        );
    }
}
