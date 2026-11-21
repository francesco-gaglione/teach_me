//! SQLite adapters for users and sessions (ADR 0008).

use async_trait::async_trait;
use domain::{
    DisplayName, NewUser, RepositoryError, SealedSecret, Session, SessionId, SessionRepository,
    StoredPasswordHash, TokenDigest, User, UserCredentials, UserId, UserRepository, Username,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

macro_rules! user_select {
    () => {
        "SELECT id, username, display_name, password_hash, llm_key IS NOT NULL AS has_llm_key, tts_voice, \
         disabled_at, last_login_at, created_at FROM users"
    };
}

fn user_from_row(row: &SqliteRow) -> Result<User, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let username: String = row.try_get("username").map_err(storage)?;
    let display_name: String = row.try_get("display_name").map_err(storage)?;
    let has_key: bool = row.try_get("has_llm_key").map_err(storage)?;
    let tts_voice: Option<String> = row.try_get("tts_voice").map_err(storage)?;
    let disabled: Option<String> = row.try_get("disabled_at").map_err(storage)?;
    let last_login: Option<String> = row.try_get("last_login_at").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    Ok(User {
        id: UserId::new(parse_id(&id)?),
        // The initial `owner` row was written by the migration; both names
        // were validated on the way in otherwise.
        username: Username::from_stored(username),
        display_name: DisplayName::parse(&display_name)
            .map_err(|_| corrupt("display name", &id))?,
        has_llm_key: has_key,
        tts_voice,
        disabled_at: disabled.as_deref().map(parse_ts).transpose()?,
        last_login_at: last_login.as_deref().map(parse_ts).transpose()?,
        created_at: parse_ts(&created)?,
    })
}

#[derive(Clone)]
pub struct SqliteUserRepository {
    pool: SqlitePool,
}

impl SqliteUserRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn touched_one(done: &sqlx::sqlite::SqliteQueryResult) -> Result<(), RepositoryError> {
    if done.rows_affected() == 0 {
        return Err(RepositoryError::NotFound);
    }
    Ok(())
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn insert(&self, user: &NewUser) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO users (id, username, display_name, password_hash, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user.id.to_string())
        .bind(user.username.as_str())
        .bind(user.display_name.as_str())
        .bind(user.password_hash.as_ref().map(StoredPasswordHash::as_str))
        .bind(format_ts(user.created_at))
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }

    async fn get(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        let row = sqlx::query(concat!(user_select!(), " WHERE id = ?"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn credentials(&self, name: &str) -> Result<Option<UserCredentials>, RepositoryError> {
        let row = sqlx::query(concat!(user_select!(), " WHERE username = ?"))
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        let Some(row) = row else { return Ok(None) };
        let hash: Option<String> = row.try_get("password_hash").map_err(storage)?;
        Ok(Some(UserCredentials {
            user: user_from_row(&row)?,
            password_hash: hash.map(StoredPasswordHash::new),
        }))
    }

    async fn by_username(&self, name: &Username) -> Result<Option<User>, RepositoryError> {
        let row = sqlx::query(concat!(user_select!(), " WHERE username = ?"))
            .bind(name.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(user_from_row).transpose()
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        let rows = sqlx::query(concat!(user_select!(), " ORDER BY username"))
            .fetch_all(&self.pool)
            .await
            .map_err(storage)?;
        rows.iter().map(user_from_row).collect()
    }

    async fn password_hash(
        &self,
        id: UserId,
    ) -> Result<Option<StoredPasswordHash>, RepositoryError> {
        let hash: Option<Option<String>> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(hash.flatten().map(StoredPasswordHash::new))
    }

    async fn set_password_hash(
        &self,
        id: UserId,
        hash: &StoredPasswordHash,
    ) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash.as_str())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        touched_one(&done)
    }

    async fn rename(&self, id: UserId, name: &Username) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET username = ? WHERE id = ?")
            .bind(name.as_str())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(constraint)?;
        touched_one(&done)
    }

    async fn set_disabled(
        &self,
        id: UserId,
        at: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET disabled_at = ? WHERE id = ?")
            .bind(at.map(format_ts))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        touched_one(&done)
    }

    async fn llm_key(&self, id: UserId) -> Result<Option<SealedSecret>, RepositoryError> {
        let key: Option<Option<Vec<u8>>> =
            sqlx::query_scalar("SELECT llm_key FROM users WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(storage)?;
        Ok(key.flatten().map(SealedSecret::new))
    }

    async fn set_llm_key(
        &self,
        id: UserId,
        key: Option<&SealedSecret>,
    ) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET llm_key = ? WHERE id = ?")
            .bind(key.map(SealedSecret::as_bytes))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        touched_one(&done)
    }

    async fn set_llm_key_if_missing(
        &self,
        id: UserId,
        key: &SealedSecret,
    ) -> Result<bool, RepositoryError> {
        let done = sqlx::query("UPDATE users SET llm_key = ? WHERE id = ? AND llm_key IS NULL")
            .bind(key.as_bytes())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected() > 0)
    }

    async fn set_last_login(&self, id: UserId, at: OffsetDateTime) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
            .bind(format_ts(at))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        touched_one(&done)
    }

    async fn set_tts_voice(&self, id: UserId, voice: &str) -> Result<(), RepositoryError> {
        let done = sqlx::query("UPDATE users SET tts_voice = ? WHERE id = ?")
            .bind(voice)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        touched_one(&done)
    }

    async fn delete(&self, id: UserId) -> Result<bool, RepositoryError> {
        // Folders, topics, preferences and sessions go with the user (ON DELETE
        // CASCADE), and everything below the topics with them.
        let done = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected() > 0)
    }
}

#[derive(Clone)]
pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn session_from_row(row: &SqliteRow) -> Result<Session, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let user_id: String = row.try_get("user_id").map_err(storage)?;
    let digest: Vec<u8> = row.try_get("token_digest").map_err(storage)?;
    let device: Option<String> = row.try_get("device_name").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let seen: String = row.try_get("last_seen_at").map_err(storage)?;
    let expires: String = row.try_get("expires_at").map_err(storage)?;
    Ok(Session {
        id: SessionId::new(parse_id(&id)?),
        user_id: UserId::new(parse_id(&user_id)?),
        token_digest: TokenDigest::new(digest),
        device_name: device,
        created_at: parse_ts(&created)?,
        last_seen_at: parse_ts(&seen)?,
        expires_at: parse_ts(&expires)?,
    })
}

#[async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn insert(&self, session: &Session) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_digest, device_name, created_at, \
             last_seen_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.to_string())
        .bind(session.user_id.to_string())
        .bind(session.token_digest.as_bytes())
        .bind(session.device_name.as_deref())
        .bind(format_ts(session.created_at))
        .bind(format_ts(session.last_seen_at))
        .bind(format_ts(session.expires_at))
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }

    async fn by_digest(&self, digest: &TokenDigest) -> Result<Option<Session>, RepositoryError> {
        let row = sqlx::query(
            "SELECT id, user_id, token_digest, device_name, created_at, last_seen_at, expires_at \
             FROM sessions WHERE token_digest = ?",
        )
        .bind(digest.as_bytes())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(session_from_row).transpose()
    }

    async fn renew(
        &self,
        id: SessionId,
        last_seen_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE id = ?")
            .bind(format_ts(last_seen_at))
            .bind(format_ts(expires_at))
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(())
    }

    async fn revoke(&self, id: SessionId) -> Result<bool, RepositoryError> {
        let done = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected() > 0)
    }

    async fn revoke_all(
        &self,
        user: UserId,
        keep: Option<SessionId>,
    ) -> Result<u64, RepositoryError> {
        let done = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND (? IS NULL OR id <> ?)")
            .bind(user.to_string())
            .bind(keep.map(|k| k.to_string()))
            .bind(keep.map(|k| k.to_string()))
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected())
    }

    async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, RepositoryError> {
        let done = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(format_ts(now))
            .execute(&self.pool)
            .await
            .map_err(storage)?;
        Ok(done.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-10 10:00:00.5 UTC);

    async fn repos() -> (SqliteUserRepository, SqliteSessionRepository, SqlitePool) {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        (
            SqliteUserRepository::new(pool.clone()),
            SqliteSessionRepository::new(pool.clone()),
            pool,
        )
    }

    fn uid(n: u128) -> UserId {
        UserId::new(Uuid::from_u128(n))
    }

    fn new_user(n: u128, name: &str) -> NewUser {
        NewUser {
            id: uid(n),
            username: Username::parse(name).unwrap(),
            display_name: DisplayName::parse(name).unwrap(),
            password_hash: Some(StoredPasswordHash::new("hash".into())),
            created_at: T0,
        }
    }

    fn session(n: u128, user: UserId, digest: u8) -> Session {
        Session {
            id: SessionId::new(Uuid::from_u128(0x500 + n)),
            user_id: user,
            token_digest: TokenDigest::new(vec![digest; 32]),
            device_name: Some("Tablet".into()),
            created_at: T0,
            last_seen_at: T0,
            expires_at: T0 + time::Duration::days(90),
        }
    }

    #[tokio::test]
    async fn the_migration_leaves_an_owner_who_cannot_log_in_yet() {
        let (users, _, _) = repos().await;
        let owner = users.credentials("owner").await.unwrap().unwrap();
        assert_eq!(owner.user.id, UserId::OWNER);
        assert!(owner.password_hash.is_none());
        assert!(!owner.user.has_llm_key);
    }

    #[tokio::test]
    async fn usernames_are_unique_and_looked_up_in_lowercase() {
        let (users, _, _) = repos().await;
        users.insert(&new_user(1, "Ann")).await.unwrap();
        assert!(matches!(
            users.insert(&new_user(2, "ann")).await,
            Err(RepositoryError::Duplicate)
        ));
        let found = users.credentials("ann").await.unwrap().unwrap();
        assert_eq!(found.user.id, uid(1));
        assert_eq!(found.password_hash.unwrap().as_str(), "hash");
        assert!(users.credentials("ghost").await.unwrap().is_none());
        users.insert(&new_user(3, "bob")).await.unwrap();
        assert!(matches!(
            users.rename(uid(3), &Username::parse("ann").unwrap()).await,
            Err(RepositoryError::Duplicate)
        ));
        users
            .rename(uid(3), &Username::parse("rob").unwrap())
            .await
            .unwrap();
        let names: Vec<_> = users
            .list()
            .await
            .unwrap()
            .iter()
            .map(|u| u.username.to_string())
            .collect();
        assert_eq!(names, ["ann", "owner", "rob"]);
    }

    #[tokio::test]
    async fn the_chosen_voice_is_kept_as_saved() {
        let (users, _, _) = repos().await;
        assert_eq!(
            users.get(UserId::OWNER).await.unwrap().unwrap().tts_voice,
            None
        );
        users.set_tts_voice(UserId::OWNER, "serena").await.unwrap();
        assert_eq!(
            users
                .get(UserId::OWNER)
                .await
                .unwrap()
                .unwrap()
                .tts_voice
                .as_deref(),
            Some("serena")
        );
        let nobody = UserId::new(Uuid::from_u128(0x999));
        assert!(matches!(
            users.set_tts_voice(nobody, "x").await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn the_key_is_stored_as_given_and_set_only_if_missing_once() {
        let (users, _, pool) = repos().await;
        let sealed = SealedSecret::new(vec![1, 2, 3, 4]);
        assert!(users.llm_key(UserId::OWNER).await.unwrap().is_none());
        assert!(
            users
                .set_llm_key_if_missing(UserId::OWNER, &sealed)
                .await
                .unwrap()
        );
        assert!(
            !users
                .set_llm_key_if_missing(UserId::OWNER, &SealedSecret::new(vec![9]))
                .await
                .unwrap()
        );
        assert_eq!(users.llm_key(UserId::OWNER).await.unwrap().unwrap(), sealed);
        assert!(users.get(UserId::OWNER).await.unwrap().unwrap().has_llm_key);
        users.set_llm_key(UserId::OWNER, None).await.unwrap();
        assert!(!users.get(UserId::OWNER).await.unwrap().unwrap().has_llm_key);
        // What the database holds is the blob and nothing readable beside it.
        users
            .set_llm_key(UserId::OWNER, Some(&sealed))
            .await
            .unwrap();
        let raw: Vec<u8> = sqlx::query_scalar("SELECT llm_key FROM users WHERE id = ?")
            .bind(UserId::OWNER.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(raw, [1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn sessions_are_found_by_digest_renewed_and_revoked() {
        let (users, sessions, _) = repos().await;
        users.insert(&new_user(1, "ann")).await.unwrap();
        let (a, b) = (session(1, uid(1), 1), session(2, uid(1), 2));
        sessions.insert(&a).await.unwrap();
        sessions.insert(&b).await.unwrap();
        assert_eq!(
            sessions.by_digest(&a.token_digest).await.unwrap().unwrap(),
            a
        );
        assert!(
            sessions
                .by_digest(&TokenDigest::new(vec![7; 32]))
                .await
                .unwrap()
                .is_none()
        );

        let later = T0 + time::Duration::days(3);
        sessions
            .renew(a.id, later, later + time::Duration::days(90))
            .await
            .unwrap();
        let renewed = sessions.by_digest(&a.token_digest).await.unwrap().unwrap();
        assert_eq!(renewed.last_seen_at, later);

        assert_eq!(sessions.revoke_all(uid(1), Some(a.id)).await.unwrap(), 1);
        assert!(sessions.by_digest(&b.token_digest).await.unwrap().is_none());
        assert!(sessions.by_digest(&a.token_digest).await.unwrap().is_some());
        assert_eq!(
            sessions
                .delete_expired(T0 + time::Duration::days(200))
                .await
                .unwrap(),
            1
        );
        assert!(!sessions.revoke(a.id).await.unwrap());
    }

    #[tokio::test]
    async fn deleting_a_user_takes_their_sessions_and_data_with_them() {
        let (users, sessions, pool) = repos().await;
        users.insert(&new_user(1, "ann")).await.unwrap();
        sessions.insert(&session(1, uid(1), 1)).await.unwrap();
        sqlx::query(
            "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) \
             VALUES ('f', ?, 'F', 'f', 0, '2026-11-10T10:00:00.000Z', '2026-11-10T10:00:00.000Z')",
        )
        .bind(uid(1).to_string())
        .execute(&pool)
        .await
        .unwrap();
        assert!(users.delete(uid(1)).await.unwrap());
        assert!(!users.delete(uid(1)).await.unwrap());
        for table in ["sessions", "folders"] {
            let n: i64 =
                sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(n, 0, "{table}");
        }
    }
}
