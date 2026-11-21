//! SQLite adapter for the preference repository.

use async_trait::async_trait;
use domain::{
    Preference, PreferenceCursor, PreferenceId, PreferenceRepository, PreferenceScope,
    PreferenceText, RepositoryError, TopicId, UserId,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

macro_rules! preference_select {
    () => {
        "SELECT id, user_id, scope, topic_id, text, created_at FROM preferences"
    };
}

fn preference_from_row(row: &SqliteRow) -> Result<Preference, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let user_id: String = row.try_get("user_id").map_err(storage)?;
    let scope: String = row.try_get("scope").map_err(storage)?;
    let topic_id: Option<String> = row.try_get("topic_id").map_err(storage)?;
    let text: String = row.try_get("text").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let scope = match (scope.as_str(), topic_id) {
        ("global", None) => PreferenceScope::Global,
        ("topic", Some(topic)) => PreferenceScope::Topic(TopicId::new(parse_id(&topic)?)),
        _ => return Err(corrupt("preference scope", &id)),
    };
    Ok(Preference {
        id: PreferenceId::new(parse_id(&id)?),
        user_id: UserId::new(parse_id(&user_id)?),
        scope,
        text: PreferenceText::parse(&text).map_err(|_| corrupt("preference text", &id))?,
        created_at: parse_ts(&created)?,
    })
}

/// `''` for global: the unique index and `count` compare this key.
fn topic_key(scope: PreferenceScope) -> String {
    scope.topic_id().map_or_default(|t| t.to_string())
}

#[derive(Clone)]
pub struct SqlitePreferenceRepository {
    pool: SqlitePool,
}

impl SqlitePreferenceRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PreferenceRepository for SqlitePreferenceRepository {
    async fn insert(&self, preference: &Preference) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(preference.id.to_string())
        .bind(preference.user_id.to_string())
        .bind(preference.scope.as_str())
        .bind(preference.scope.topic_id().map(|t| t.to_string()))
        .bind(preference.text.as_str())
        .bind(preference.text.unique_key())
        .bind(format_ts(preference.created_at))
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }

    async fn count(&self, user: UserId, scope: PreferenceScope) -> Result<u64, RepositoryError> {
        let (n,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM preferences WHERE user_id = ? AND COALESCE(topic_id, '') = ?",
        )
        .bind(user.to_string())
        .bind(topic_key(scope))
        .fetch_one(&self.pool)
        .await
        .map_err(storage)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    async fn get(
        &self,
        user: UserId,
        id: PreferenceId,
    ) -> Result<Option<Preference>, RepositoryError> {
        let row = sqlx::query(concat!(
            preference_select!(),
            " WHERE id = ? AND user_id = ?"
        ))
        .bind(id.to_string())
        .bind(user.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(preference_from_row).transpose()
    }

    async fn list(
        &self,
        user: UserId,
        topic: Option<TopicId>,
        after: Option<PreferenceCursor>,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError> {
        let topic = topic.map(|t| t.to_string());
        let (at, id) = match after {
            Some(c) => (Some(format_ts(c.created_at)), Some(c.id.to_string())),
            None => (None, None),
        };
        let rows = sqlx::query(concat!(
            preference_select!(),
            " WHERE user_id = ? AND (? IS NULL OR scope = 'global' OR topic_id = ?) \
             AND (? IS NULL OR (created_at, id) > (?, ?)) \
             ORDER BY created_at, id LIMIT ?"
        ))
        .bind(user.to_string())
        .bind(topic.clone())
        .bind(topic)
        .bind(at.clone())
        .bind(at)
        .bind(id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(preference_from_row).collect()
    }

    async fn latest_for_topic(
        &self,
        user: UserId,
        topic_id: TopicId,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError> {
        let rows = sqlx::query(concat!(
            preference_select!(),
            " WHERE user_id = ? AND (scope = 'global' OR topic_id = ?) \
             ORDER BY created_at DESC, id DESC LIMIT ?"
        ))
        .bind(user.to_string())
        .bind(topic_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(preference_from_row).collect()
    }

    async fn delete(&self, user: UserId, id: PreferenceId) -> Result<bool, RepositoryError> {
        let result = sqlx::query("DELETE FROM preferences WHERE id = ? AND user_id = ?")
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
    use crate::SqliteTopicRepository;
    use crate::repositories::SqliteFolderRepository;
    use crate::sqlite::{connect, migrate};
    use domain::{
        FolderId, FolderName, FolderRepository, NewFolder, Topic, TopicRepository, TopicTitle,
    };
    use time::OffsetDateTime;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-08 10:00:00.5 UTC);

    struct Env {
        prefs: SqlitePreferenceRepository,
        topics: SqliteTopicRepository,
        pool: SqlitePool,
        a: TopicId,
        b: TopicId,
    }

    async fn env() -> Env {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        let folders = SqliteFolderRepository::new(pool.clone());
        let topics = SqliteTopicRepository::new(pool.clone());
        let folder = FolderId::new(Uuid::from_u128(1));
        folders
            .insert(NewFolder {
                user_id: domain::UserId::OWNER,
                id: folder,
                name: FolderName::parse("F").unwrap(),
                created_at: T0,
            })
            .await
            .unwrap();
        let mut ids = [
            TopicId::new(Uuid::from_u128(2)),
            TopicId::new(Uuid::from_u128(3)),
        ];
        for (i, id) in ids.iter_mut().enumerate() {
            let t = Topic::new(
                *id,
                domain::UserId::OWNER,
                folder,
                TopicTitle::parse(&format!("T{i}")).unwrap(),
                T0,
            );
            topics.insert(&t).await.unwrap();
        }
        Env {
            prefs: SqlitePreferenceRepository::new(pool.clone()),
            topics,
            pool,
            a: ids[0],
            b: ids[1],
        }
    }

    fn pref(n: u128, scope: PreferenceScope, text: &str) -> Preference {
        Preference {
            user_id: domain::UserId::OWNER,
            id: PreferenceId::new(Uuid::from_u128(n)),
            scope,
            text: PreferenceText::parse(text).unwrap(),
            created_at: T0 + time::Duration::seconds(i64::try_from(n).unwrap()),
        }
    }

    fn texts(v: &[Preference]) -> Vec<&str> {
        v.iter().map(|p| p.text.as_str()).collect()
    }

    #[tokio::test]
    async fn preferences_round_trip_with_their_scope() {
        let env = env().await;
        let g = pref(10, PreferenceScope::Global, "Risposte \"brevi\" 日本");
        let t = pref(11, PreferenceScope::Topic(env.a), "Esempi in Python");
        env.prefs.insert(&g).await.unwrap();
        env.prefs.insert(&t).await.unwrap();
        assert_eq!(
            env.prefs.get(domain::UserId::OWNER, g.id).await.unwrap(),
            Some(g.clone())
        );
        assert_eq!(
            env.prefs.get(domain::UserId::OWNER, t.id).await.unwrap(),
            Some(t.clone())
        );
        assert!(
            env.prefs
                .get(domain::UserId::OWNER, PreferenceId::new(Uuid::from_u128(1)))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn the_same_text_in_the_same_scope_is_a_duplicate_ignoring_case() {
        let env = env().await;
        env.prefs
            .insert(&pref(1, PreferenceScope::Global, "Risposte brevi"))
            .await
            .unwrap();
        let dup = pref(2, PreferenceScope::Global, "  RISPOSTE BREVI ");
        assert!(matches!(
            env.prefs.insert(&dup).await,
            Err(RepositoryError::Duplicate)
        ));
        // Inner whitespace does not make a new preference either.
        let spaced = pref(7, PreferenceScope::Global, "risposte \t  brevi");
        assert!(matches!(
            env.prefs.insert(&spaced).await,
            Err(RepositoryError::Duplicate)
        ));

        // Same text, other scope: distinct.
        env.prefs
            .insert(&pref(3, PreferenceScope::Topic(env.a), "Risposte brevi"))
            .await
            .unwrap();
        env.prefs
            .insert(&pref(4, PreferenceScope::Topic(env.b), "Risposte brevi"))
            .await
            .unwrap();
        let again = pref(5, PreferenceScope::Topic(env.a), "risposte brevi");
        assert!(matches!(
            env.prefs.insert(&again).await,
            Err(RepositoryError::Duplicate)
        ));
        assert_eq!(
            env.prefs
                .count(domain::UserId::OWNER, PreferenceScope::Global)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            env.prefs
                .count(domain::UserId::OWNER, PreferenceScope::Topic(env.a))
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn a_missing_topic_is_a_missing_parent() {
        let env = env().await;
        let ghost = TopicId::new(Uuid::from_u128(99));
        assert!(matches!(
            env.prefs
                .insert(&pref(1, PreferenceScope::Topic(ghost), "x"))
                .await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn listing_filters_by_topic_orders_by_creation_and_pages() {
        let env = env().await;
        for p in [
            pref(1, PreferenceScope::Global, "G1"),
            pref(2, PreferenceScope::Topic(env.a), "A1"),
            pref(3, PreferenceScope::Topic(env.b), "B1"),
            pref(4, PreferenceScope::Global, "G2"),
        ] {
            env.prefs.insert(&p).await.unwrap();
        }
        let all = env
            .prefs
            .list(domain::UserId::OWNER, None, None, 10)
            .await
            .unwrap();
        assert_eq!(texts(&all), ["G1", "A1", "B1", "G2"]);
        let for_a = env
            .prefs
            .list(domain::UserId::OWNER, Some(env.a), None, 10)
            .await
            .unwrap();
        assert_eq!(texts(&for_a), ["G1", "A1", "G2"]);

        let page = env
            .prefs
            .list(domain::UserId::OWNER, None, None, 2)
            .await
            .unwrap();
        let rest = env
            .prefs
            .list(
                domain::UserId::OWNER,
                None,
                Some(PreferenceCursor::after(&page[1])),
                10,
            )
            .await
            .unwrap();
        assert_eq!(texts(&rest), ["B1", "G2"]);
        let rest_a = env
            .prefs
            .list(
                domain::UserId::OWNER,
                Some(env.a),
                Some(PreferenceCursor::after(&page[1])),
                10,
            )
            .await
            .unwrap();
        assert_eq!(texts(&rest_a), ["G2"]);

        let latest = env
            .prefs
            .latest_for_topic(domain::UserId::OWNER, env.a, 2)
            .await
            .unwrap();
        assert_eq!(
            texts(&latest),
            ["G2", "A1"],
            "newest first, only what applies"
        );
    }

    #[tokio::test]
    async fn delete_reports_absence_and_the_topic_takes_its_preferences_along() {
        let env = env().await;
        let g = pref(1, PreferenceScope::Global, "G");
        env.prefs.insert(&g).await.unwrap();
        env.prefs
            .insert(&pref(2, PreferenceScope::Topic(env.a), "A"))
            .await
            .unwrap();
        assert!(env.prefs.delete(domain::UserId::OWNER, g.id).await.unwrap());
        assert!(!env.prefs.delete(domain::UserId::OWNER, g.id).await.unwrap());

        assert!(
            env.topics
                .delete(domain::UserId::OWNER, env.a)
                .await
                .unwrap()
        );
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM preferences")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
}
