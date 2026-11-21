//! SQLite adapters for the folder and topic repositories.

use async_trait::async_trait;
use domain::{
    ContentLanguage, Folder, FolderCursor, FolderId, FolderName, FolderRepository, FolderSummary,
    NewFolder, RepositoryError, Topic, TopicCursor, TopicId, TopicRepository, TopicState,
    TopicTitle, UserId,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use uuid::Uuid;

/// Stored timestamp format: fixed width, so text order is chronological order.
pub(crate) fn format_ts(at: OffsetDateTime) -> String {
    at.to_offset(time::UtcOffset::UTC)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .unwrap_or_default()
}

/// Never includes the stored value: it may be user text, and logs must not carry it.
pub(crate) fn corrupt(what: &str, row_id: &str) -> RepositoryError {
    RepositoryError::Storage(format!("corrupt {what} in database (row `{row_id}`)").into())
}

pub(crate) fn parse_ts(s: &str) -> Result<OffsetDateTime, RepositoryError> {
    OffsetDateTime::parse(s, &Rfc3339).map_err(|_| corrupt("timestamp", "unknown"))
}

pub(crate) fn parse_id(s: &str) -> Result<Uuid, RepositoryError> {
    Uuid::parse_str(s).map_err(|_| corrupt("id", "unknown"))
}

pub(crate) fn storage(err: sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(Box::new(err))
}

/// Maps constraint violations to the port's vocabulary; anything else is a
/// storage failure.
pub(crate) fn constraint(err: sqlx::Error) -> RepositoryError {
    if let sqlx::Error::Database(db) = &err {
        if db.is_unique_violation() {
            return RepositoryError::Duplicate;
        }
        if db.is_foreign_key_violation() {
            return RepositoryError::MissingParent;
        }
    }
    storage(err)
}

// Macros (not consts) so queries stay `&'static str` literals via `concat!`,
// which sqlx 0.9 requires to rule out injection.
macro_rules! folder_select {
    () => {
        "SELECT f.id, f.user_id, f.name, f.position, f.created_at, f.updated_at, \
         (SELECT COUNT(*) FROM topics t WHERE t.folder_id = f.id) AS topic_count \
         FROM folders f"
    };
}

fn folder_from_row(row: &SqliteRow) -> Result<FolderSummary, RepositoryError> {
    let name: String = row.try_get("name").map_err(storage)?;
    let position: i64 = row.try_get("position").map_err(storage)?;
    let count: i64 = row.try_get("topic_count").map_err(storage)?;
    let id: String = row.try_get("id").map_err(storage)?;
    let user_id: String = row.try_get("user_id").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let updated: String = row.try_get("updated_at").map_err(storage)?;
    Ok(FolderSummary {
        folder: Folder {
            id: FolderId::new(parse_id(&id)?),
            user_id: UserId::new(parse_id(&user_id)?),
            // Stored names were validated on the way in; re-parsing keeps the
            // invariant without trusting the file.
            name: FolderName::parse(&name).map_err(|_| corrupt("folder name", &id))?,
            position: u32::try_from(position).map_err(|_| corrupt("position", &id))?,
            created_at: parse_ts(&created)?,
            updated_at: parse_ts(&updated)?,
        },
        topic_count: u64::try_from(count).unwrap_or(0),
    })
}

fn topic_from_row(row: &SqliteRow) -> Result<Topic, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let user_id: String = row.try_get("user_id").map_err(storage)?;
    let folder_id: String = row.try_get("folder_id").map_err(storage)?;
    let title: String = row.try_get("title").map_err(storage)?;
    let language: String = row.try_get("language").map_err(storage)?;
    let state: String = row.try_get("state").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let updated: String = row.try_get("updated_at").map_err(storage)?;
    Ok(Topic {
        id: TopicId::new(parse_id(&id)?),
        user_id: UserId::new(parse_id(&user_id)?),
        folder_id: FolderId::new(parse_id(&folder_id)?),
        title: TopicTitle::parse(&title).map_err(|_| corrupt("topic title", &id))?,
        language: language
            .parse::<ContentLanguage>()
            .map_err(|_| corrupt("topic language", &id))?,
        state: state
            .parse::<TopicState>()
            .map_err(|_| corrupt("state", &id))?,
        created_at: parse_ts(&created)?,
        updated_at: parse_ts(&updated)?,
    })
}

#[derive(Clone)]
pub struct SqliteFolderRepository {
    pool: SqlitePool,
}

impl SqliteFolderRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FolderRepository for SqliteFolderRepository {
    async fn insert(&self, folder: NewFolder) -> Result<FolderSummary, RepositoryError> {
        let created = format_ts(folder.created_at);
        // Position is computed inside the INSERT so concurrent creations cannot
        // pick the same value (SQLite serialises writers).
        sqlx::query(
            "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) \
             SELECT ?, ?, ?, ?, COALESCE(MAX(position) + 1, 0), ?, ? FROM folders WHERE user_id = ?",
        )
        .bind(folder.id.to_string())
        .bind(folder.user_id.to_string())
        .bind(folder.name.as_str())
        .bind(folder.name.unique_key())
        .bind(&created)
        .bind(&created)
        .bind(folder.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        self.get(folder.user_id, folder.id)
            .await?
            .ok_or_else(|| corrupt("folder", "vanished after insert"))
    }

    async fn get(
        &self,
        user: UserId,
        id: FolderId,
    ) -> Result<Option<FolderSummary>, RepositoryError> {
        let row = sqlx::query(concat!(
            folder_select!(),
            " WHERE f.id = ? AND f.user_id = ?"
        ))
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(folder_from_row).transpose()
    }

    async fn list(
        &self,
        user: UserId,
        after: Option<FolderCursor>,
        limit: u32,
    ) -> Result<Vec<FolderSummary>, RepositoryError> {
        let rows = match after {
            None => {
                sqlx::query(concat!(
                    folder_select!(),
                    " WHERE f.user_id = ? ORDER BY f.position, f.id LIMIT ?"
                ))
                .bind(user.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
            Some(c) => {
                sqlx::query(concat!(
                    folder_select!(),
                    " WHERE f.user_id = ? AND (f.position, f.id) > (?, ?) \
                     ORDER BY f.position, f.id LIMIT ?"
                ))
                .bind(user.to_string())
                .bind(i64::from(c.position))
                .bind(c.id.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(storage)?;
        rows.iter().map(folder_from_row).collect()
    }

    async fn update(&self, folder: &Folder) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE folders SET name = ?, name_key = ?, updated_at = ? WHERE id = ? AND user_id = ?",
        )
        .bind(folder.name.as_str())
        .bind(folder.name.unique_key())
        .bind(format_ts(folder.updated_at))
        .bind(folder.id.to_string())
        .bind(folder.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn delete(&self, user: UserId, id: FolderId) -> Result<bool, RepositoryError> {
        // Topics go away through ON DELETE CASCADE (foreign_keys is on for
        // every pooled connection, see `sqlite::connect`).
        let result = sqlx::query("DELETE FROM folders WHERE id = ? AND user_id = ?")
            .bind(id.to_string())
            .bind(user.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(Clone)]
pub struct SqliteTopicRepository {
    pool: SqlitePool,
}

impl SqliteTopicRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

macro_rules! topic_select {
    () => {
        "SELECT id, user_id, folder_id, title, language, state, created_at, updated_at FROM topics"
    };
}

#[async_trait]
impl TopicRepository for SqliteTopicRepository {
    async fn insert(&self, topic: &Topic) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO topics (id, user_id, folder_id, title, title_key, language, state, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(topic.id.to_string())
        .bind(topic.user_id.to_string())
        .bind(topic.folder_id.to_string())
        .bind(topic.title.as_str())
        .bind(topic.title.unique_key())
        .bind(topic.language.code())
        .bind(topic.state.as_str())
        .bind(format_ts(topic.created_at))
        .bind(format_ts(topic.updated_at))
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }

    async fn get(&self, user: UserId, id: TopicId) -> Result<Option<Topic>, RepositoryError> {
        let row = sqlx::query(concat!(topic_select!(), " WHERE id = ? AND user_id = ?"))
            .bind(id.to_string())
            .bind(user.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(topic_from_row).transpose()
    }

    async fn get_system(&self, id: TopicId) -> Result<Option<Topic>, RepositoryError> {
        let row = sqlx::query(concat!(topic_select!(), " WHERE id = ?"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(topic_from_row).transpose()
    }

    async fn list_by_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
        after: Option<TopicCursor>,
        limit: u32,
    ) -> Result<Vec<Topic>, RepositoryError> {
        let rows = match after {
            None => {
                sqlx::query(concat!(
                    topic_select!(),
                    " WHERE user_id = ? AND folder_id = ? ORDER BY created_at, id LIMIT ?"
                ))
                .bind(user.to_string())
                .bind(folder_id.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
            Some(c) => {
                sqlx::query(concat!(
                    topic_select!(),
                    " WHERE user_id = ? AND folder_id = ? AND (created_at, id) > (?, ?) \
                     ORDER BY created_at, id LIMIT ?"
                ))
                .bind(user.to_string())
                .bind(folder_id.to_string())
                .bind(format_ts(c.created_at))
                .bind(c.id.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(storage)?;
        rows.iter().map(topic_from_row).collect()
    }

    /// Writes title, folder and `updated_at` only. `state` is deliberately
    /// left out so a stale entity cannot overwrite a concurrent state change.
    async fn update(&self, topic: &Topic) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE topics SET folder_id = ?, title = ?, title_key = ?, updated_at = ? \
             WHERE id = ? AND user_id = ?",
        )
        .bind(topic.folder_id.to_string())
        .bind(topic.title.as_str())
        .bind(topic.title.unique_key())
        .bind(format_ts(topic.updated_at))
        .bind(topic.id.to_string())
        .bind(topic.user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn transition_state(
        &self,
        id: TopicId,
        from: TopicState,
        to: TopicState,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        let result =
            sqlx::query("UPDATE topics SET state = ?, updated_at = ? WHERE id = ? AND state = ?")
                .bind(to.as_str())
                .bind(format_ts(at))
                .bind(id.to_string())
                .bind(from.as_str())
                .execute(&self.pool)
                .await
                .map_err(storage)?;
        Ok(result.rows_affected() > 0)
    }

    async fn ids_in_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
    ) -> Result<Vec<TopicId>, RepositoryError> {
        let rows = sqlx::query("SELECT id FROM topics WHERE folder_id = ? AND user_id = ?")
            .bind(folder_id.to_string())
            .bind(user.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let id: String = row.try_get("id").map_err(storage)?;
                Ok(TopicId::new(parse_id(&id)?))
            })
            .collect()
    }

    async fn delete(&self, user: UserId, id: TopicId) -> Result<bool, RepositoryError> {
        let result = sqlx::query("DELETE FROM topics WHERE id = ? AND user_id = ?")
            .bind(id.to_string())
            .bind(user.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use time::macros::datetime;

    async fn repos() -> (SqliteFolderRepository, SqliteTopicRepository, SqlitePool) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        (
            SqliteFolderRepository::new(pool.clone()),
            SqliteTopicRepository::new(pool.clone()),
            pool,
        )
    }

    fn fid(n: u128) -> FolderId {
        FolderId::new(Uuid::from_u128(n))
    }

    fn new_folder(n: u128, name: &str) -> NewFolder {
        NewFolder {
            user_id: domain::UserId::OWNER,
            id: fid(n),
            name: FolderName::parse(name).unwrap(),
            created_at: datetime!(2026-11-08 10:00:00.123 UTC),
        }
    }

    fn topic(n: u128, folder: u128, title: &str, secs: i64) -> Topic {
        Topic::new(
            TopicId::new(Uuid::from_u128(n)),
            domain::UserId::OWNER,
            fid(folder),
            TopicTitle::parse(title).unwrap(),
            datetime!(2026-11-08 10:00:00.5 UTC) + time::Duration::seconds(secs),
        )
    }

    #[tokio::test]
    async fn folder_round_trip_and_positions() {
        let (folders, _, _) = repos().await;
        let a = folders.insert(new_folder(1, "Lavoro")).await.unwrap();
        let b = folders.insert(new_folder(2, "Studio")).await.unwrap();
        assert_eq!((a.folder.position, b.folder.position), (0, 1));
        assert_eq!(a.folder.created_at, datetime!(2026-11-08 10:00:00.123 UTC));
        assert_eq!(
            folders
                .get(domain::UserId::OWNER, fid(1))
                .await
                .unwrap()
                .unwrap(),
            a
        );
        assert!(
            folders
                .get(domain::UserId::OWNER, fid(9))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn position_does_not_reuse_gaps_after_delete() {
        let (folders, _, _) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        folders.insert(new_folder(2, "B")).await.unwrap();
        assert!(folders.delete(domain::UserId::OWNER, fid(1)).await.unwrap());
        let c = folders.insert(new_folder(3, "C")).await.unwrap();
        assert_eq!(c.folder.position, 2);
    }

    #[tokio::test]
    async fn unique_index_is_case_insensitive_beyond_ascii() {
        let (folders, _, _) = repos().await;
        folders.insert(new_folder(1, "Èconomia")).await.unwrap();
        let err = folders.insert(new_folder(2, "èCONOMIA")).await.unwrap_err();
        assert!(matches!(err, RepositoryError::Duplicate));
    }

    #[tokio::test]
    async fn topic_unique_per_folder_and_fk_enforced() {
        let (folders, topics, _) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        folders.insert(new_folder(2, "B")).await.unwrap();
        topics.insert(&topic(10, 1, "Rust", 0)).await.unwrap();
        assert!(matches!(
            topics.insert(&topic(11, 1, "RUST", 1)).await,
            Err(RepositoryError::Duplicate)
        ));
        topics.insert(&topic(12, 2, "Rust", 2)).await.unwrap();
        assert!(matches!(
            topics.insert(&topic(13, 99, "X", 3)).await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn delete_folder_cascades_and_topic_count_is_computed() {
        let (folders, topics, _) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        topics.insert(&topic(10, 1, "T1", 0)).await.unwrap();
        topics.insert(&topic(11, 1, "T2", 1)).await.unwrap();
        assert_eq!(
            folders
                .get(domain::UserId::OWNER, fid(1))
                .await
                .unwrap()
                .unwrap()
                .topic_count,
            2
        );
        assert!(folders.delete(domain::UserId::OWNER, fid(1)).await.unwrap());
        assert!(
            topics
                .get(domain::UserId::OWNER, TopicId::new(Uuid::from_u128(10)))
                .await
                .unwrap()
                .is_none()
        );
        assert!(!folders.delete(domain::UserId::OWNER, fid(1)).await.unwrap());
    }

    #[tokio::test]
    async fn keyset_pagination_for_topics_uses_created_at_then_id() {
        let (folders, topics, _) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        // Same created_at for 10 and 11: the id breaks the tie.
        topics.insert(&topic(11, 1, "B", 0)).await.unwrap();
        topics.insert(&topic(10, 1, "A", 0)).await.unwrap();
        topics.insert(&topic(12, 1, "C", 1)).await.unwrap();
        let first = topics
            .list_by_folder(domain::UserId::OWNER, fid(1), None, 2)
            .await
            .unwrap();
        let titles: Vec<_> = first.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["A", "B"]);
        let rest = topics
            .list_by_folder(
                domain::UserId::OWNER,
                fid(1),
                Some(TopicCursor::after(&first[1])),
                10,
            )
            .await
            .unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].title.as_str(), "C");
    }

    #[tokio::test]
    async fn the_language_is_stored_and_a_row_without_one_is_italian() {
        let (folders, topics, pool) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        let en = topic(10, 1, "Rust", 0).with_language(ContentLanguage::En);
        topics.insert(&en).await.unwrap();
        let stored = topics.get(en.user_id, en.id).await.unwrap().unwrap();
        assert_eq!(stored.language, ContentLanguage::En);

        // An update (rename/move) never touches it.
        let mut renamed = stored.clone();
        renamed.title = TopicTitle::parse("Rust 2").unwrap();
        renamed.language = ContentLanguage::It;
        topics.update(&renamed).await.unwrap();
        let after = topics.get(en.user_id, en.id).await.unwrap().unwrap();
        assert_eq!(after.language, ContentLanguage::En);

        // The migration default: a row written without the column.
        sqlx::query(
            "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) \
             SELECT ?, user_id, folder_id, 'Old', 'old', created_at, updated_at FROM topics LIMIT 1",
        )
        .bind(topic(11, 1, "x", 0).id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        let old = topics
            .get(en.user_id, topic(11, 1, "x", 0).id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old.language, ContentLanguage::It);
        // The database refuses anything else.
        assert!(
            sqlx::query("UPDATE topics SET language = 'fr'")
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn topic_update_does_not_write_state_and_reports_missing_parent() {
        let (folders, topics, pool) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        let t = topic(10, 1, "Rust", 0);
        topics.insert(&t).await.unwrap();
        sqlx::query("UPDATE topics SET state = 'ready'")
            .execute(&pool)
            .await
            .unwrap();
        let mut stale = t.clone();
        stale.title = TopicTitle::parse("Rust 2").unwrap();
        topics.update(&stale).await.unwrap();
        let stored = topics
            .get(domain::UserId::OWNER, t.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, TopicState::Ready);
        assert_eq!(stored.title.as_str(), "Rust 2");

        let mut moved = stored;
        moved.folder_id = fid(99);
        assert!(matches!(
            topics.update(&moved).await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn corrupt_rows_do_not_leak_values_into_errors() {
        let (folders, topics, pool) = repos().await;
        folders.insert(new_folder(1, "A")).await.unwrap();
        sqlx::query(
            "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) \
             VALUES (?, '00000000-0000-7000-8000-000000000001', ?, '   ', 'x', 'not-a-date', 'not-a-date')",
        )
        .bind(Uuid::from_u128(5).to_string())
        .bind(fid(1).to_string())
        .execute(&pool)
        .await
        .unwrap();
        let err = topics
            .get(domain::UserId::OWNER, TopicId::new(Uuid::from_u128(5)))
            .await
            .unwrap_err();
        assert!(!err.to_string().contains("not-a-date"));
        assert!(!format!("{err:?}").contains("not-a-date"));
    }

    #[tokio::test]
    async fn update_reports_missing_rows_and_duplicates() {
        let (folders, topics, _) = repos().await;
        let a = folders.insert(new_folder(1, "A")).await.unwrap();
        folders.insert(new_folder(2, "B")).await.unwrap();
        let mut renamed = a.folder.clone();
        renamed.name = FolderName::parse("b").unwrap();
        assert!(matches!(
            folders.update(&renamed).await,
            Err(RepositoryError::Duplicate)
        ));
        let mut ghost = a.folder.clone();
        ghost.id = fid(77);
        assert!(matches!(
            folders.update(&ghost).await,
            Err(RepositoryError::NotFound)
        ));
        assert!(matches!(
            topics.update(&topic(55, 1, "x", 0)).await,
            Err(RepositoryError::NotFound)
        ));
    }
}
