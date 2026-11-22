//! SQLite connection pool and migrations.

use std::str::FromStr;
use std::time::Duration;

pub use sqlx::SqlitePool as Pool;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

/// Errors raised while opening the database or applying migrations.
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("invalid database url: {0}")]
    InvalidUrl(#[source] sqlx::Error),
    #[error("cannot open database: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("migration failed: {0}")]
    Migrate(#[source] sqlx::migrate::MigrateError),
    #[error("startup repair failed: {0}")]
    Query(#[source] sqlx::Error),
}

/// Opens the pool: creates the file if missing, WAL, foreign keys enforced.
///
/// For `sqlite::memory:` the pool is limited to one connection, because each
/// in-memory connection would otherwise see its own empty database.
pub async fn connect(url: &str) -> Result<SqlitePool, DatabaseError> {
    let in_memory = url.contains(":memory:") || url.contains("mode=memory");
    let options = SqliteConnectOptions::from_str(url)
        .map_err(DatabaseError::InvalidUrl)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let max = if in_memory { 1 } else { 5 };
    SqlitePoolOptions::new()
        .max_connections(max)
        .connect_with(options)
        .await
        .map_err(DatabaseError::Connect)
}

/// Whether the database answers a trivial query (readiness check).
pub async fn ping(pool: &SqlitePool) -> bool {
    sqlx::query("SELECT 1").execute(pool).await.is_ok()
}

/// Applies the migrations embedded from `backend/migrations/`.
pub async fn migrate(pool: &SqlitePool) -> Result<(), DatabaseError> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .map_err(DatabaseError::Migrate)?;
    // A binary older than migration 0024 may have suspended cards since.
    crate::reviews::backfill_suspensions(pool)
        .await
        .map_err(DatabaseError::Query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrates_in_memory_with_foreign_keys_on() {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let (fk,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(fk, 1);
        let (v,): (String,) =
            sqlx::query_as("SELECT value FROM schema_info WHERE key = 'created_by'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v, "teach-me");
    }

    #[tokio::test]
    async fn creates_file_in_wal_mode_and_migrates_twice() {
        let dir = std::env::temp_dir().join(format!("teach_me_test_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let url = format!("sqlite://{}", dir.join("t.db").display());
        let pool = connect(&url).await.unwrap();
        migrate(&pool).await.unwrap();
        migrate(&pool).await.unwrap(); // idempotent
        let (mode,): (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(mode, "wal");
        pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn foreign_keys_are_on_for_every_pooled_connection() {
        let dir = std::env::temp_dir().join(format!("teach_me_fk_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = connect(&format!("sqlite://{}", dir.join("t.db").display()))
            .await
            .unwrap();
        // Hold several connections at once so the pool has to open distinct ones.
        let mut held = Vec::new();
        for _ in 0..4 {
            let mut conn = pool.acquire().await.unwrap();
            let (fk,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
            assert_eq!(fk, 1);
            held.push(conn);
        }
        drop(held);
        pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
