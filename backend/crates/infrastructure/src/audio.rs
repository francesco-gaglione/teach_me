//! Clip cache adapters: SQLite metadata and files on disk (ADR 0013).

use std::path::PathBuf;

use async_trait::async_trait;
use domain::{
    Clip, ClipId, ClipRepository, ClipStore, GRANT_REFRESH, RepositoryError, StoredFile, UserId,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

#[derive(Clone)]
pub struct SqliteClipRepository {
    pool: SqlitePool,
}

impl SqliteClipRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

macro_rules! clip_select {
    () => {
        "SELECT id, key, text_hash, voice, duration_ms, size_bytes, created_at FROM audio_clips"
    };
}

fn clip_from_row(row: &SqliteRow) -> Result<Clip, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let duration: i64 = row.try_get("duration_ms").map_err(storage)?;
    let size: i64 = row.try_get("size_bytes").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    Ok(Clip {
        id: ClipId::new(parse_id(&id)?),
        key: row.try_get("key").map_err(storage)?,
        text_hash: row.try_get("text_hash").map_err(storage)?,
        voice: row.try_get("voice").map_err(storage)?,
        duration_ms: u32::try_from(duration).map_err(|_| corrupt("clip duration", &id))?,
        size_bytes: u64::try_from(size).map_err(|_| corrupt("clip size", &id))?,
        created_at: parse_ts(&created)?,
    })
}

#[async_trait]
impl ClipRepository for SqliteClipRepository {
    async fn get(&self, id: ClipId) -> Result<Option<Clip>, RepositoryError> {
        let row = sqlx::query(concat!(clip_select!(), " WHERE id = ?"))
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(clip_from_row).transpose()
    }

    async fn find_by_key(&self, key: &str) -> Result<Option<Clip>, RepositoryError> {
        let row = sqlx::query(concat!(clip_select!(), " WHERE key = ?"))
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(clip_from_row).transpose()
    }

    async fn find_other_voice(
        &self,
        text_hash: &str,
        voice: &str,
    ) -> Result<Option<Clip>, RepositoryError> {
        // `rowid` breaks ties between clips made in the same millisecond.
        let row = sqlx::query(concat!(
            clip_select!(),
            " WHERE text_hash = ? AND voice <> ? ORDER BY created_at DESC, rowid DESC LIMIT 1"
        ))
        .bind(text_hash)
        .bind(voice)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(clip_from_row).transpose()
    }

    async fn insert(&self, clip: &Clip) -> Result<(), RepositoryError> {
        let size = i64::try_from(clip.size_bytes).map_err(|_| corrupt("clip size", "new"))?;
        sqlx::query(
            "INSERT INTO audio_clips (id, key, text_hash, voice, duration_ms, size_bytes, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(clip.id.to_string())
        .bind(&clip.key)
        .bind(&clip.text_hash)
        .bind(&clip.voice)
        .bind(i64::from(clip.duration_ms))
        .bind(size)
        .bind(format_ts(clip.created_at))
        .execute(&self.pool)
        .await
        .map_err(constraint)?;
        Ok(())
    }

    async fn grant(
        &self,
        user: UserId,
        clips: &[ClipId],
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        if clips.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await.map_err(storage)?;
        for clip in clips {
            // The time kept is the last use, rewritten only when it is a day
            // old: timestamps are fixed-width UTC text, so `<` on text is `<`
            // on time.
            sqlx::query(
                "INSERT INTO audio_clip_grants (user_id, clip_id, granted_at) VALUES (?, ?, ?) \
                 ON CONFLICT (user_id, clip_id) DO UPDATE SET granted_at = excluded.granted_at \
                 WHERE granted_at < ?",
            )
            .bind(user.to_string())
            .bind(clip.to_string())
            .bind(format_ts(at))
            .bind(format_ts(at - GRANT_REFRESH))
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        tx.commit().await.map_err(storage)
    }

    async fn is_granted(&self, user: UserId, clip: ClipId) -> Result<bool, RepositoryError> {
        let found = sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM audio_clip_grants WHERE user_id = ? AND clip_id = ?",
        )
        .bind(user.to_string())
        .bind(clip.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        Ok(found.is_some())
    }

    async fn list_with_last_use(&self) -> Result<Vec<(Clip, OffsetDateTime)>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, key, text_hash, voice, duration_ms, size_bytes, created_at, \
                    (SELECT MAX(granted_at) FROM audio_clip_grants g WHERE g.clip_id = c.id) \
                        AS last_grant \
             FROM audio_clips c ORDER BY created_at, rowid",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter()
            .map(|row| {
                let clip = clip_from_row(row)?;
                let granted: Option<String> = row.try_get("last_grant").map_err(storage)?;
                let last_use = match granted {
                    Some(g) => parse_ts(&g)?.max(clip.created_at),
                    None => clip.created_at,
                };
                Ok((clip, last_use))
            })
            .collect()
    }

    async fn delete_if_unused_since(
        &self,
        id: ClipId,
        cutoff: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        // The test and the delete are one statement, so a grant that lands
        // between the listing and here keeps the clip. Grants go by cascade.
        let done = sqlx::query(
            "DELETE FROM audio_clips WHERE id = ?1 AND created_at < ?2 AND COALESCE( \
                 (SELECT MAX(granted_at) FROM audio_clip_grants WHERE clip_id = audio_clips.id), \
                 created_at) < ?2",
        )
        .bind(id.to_string())
        .bind(format_ts(cutoff))
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        Ok(done.rows_affected() > 0)
    }
}

/// Clip files in one directory, named `<clip id>.ogg`.
#[derive(Debug, Clone)]
pub struct FsClipStore {
    dir: PathBuf,
}

impl FsClipStore {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Creates the directory if needed and sweeps the temporary files a crash
    /// left behind (they belong to no clip).
    pub async fn prepare(&self) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().is_some_and(|e| e == "part") {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
        Ok(())
    }

    fn path(&self, id: ClipId) -> PathBuf {
        self.dir.join(format!("{id}.ogg"))
    }
}

#[async_trait]
impl ClipStore for FsClipStore {
    async fn put(&self, id: ClipId, audio: &[u8]) -> std::io::Result<()> {
        // The temporary file sits next to the final one: a rename across
        // filesystems (a volume boundary) would not be atomic, or would fail.
        let tmp = self.dir.join(format!("{id}.part"));
        let result = async {
            tokio::fs::write(&tmp, audio).await?;
            tokio::fs::rename(&tmp, self.path(id)).await
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
        result
    }

    async fn exists(&self, id: ClipId) -> std::io::Result<bool> {
        tokio::fs::try_exists(self.path(id)).await
    }

    async fn remove(&self, id: ClipId) -> std::io::Result<Option<u64>> {
        let path = self.path(id);
        let size = match tokio::fs::metadata(&path).await {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(Some(size)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn list(&self) -> std::io::Result<Vec<StoredFile>> {
        let mut found = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(entries) => entries,
            // No directory, no files (the volume was emptied).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
            Err(e) => return Err(e),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "ogg") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|n| n.to_str())
                .and_then(|n| Uuid::parse_str(n).ok())
            else {
                continue;
            };
            // A file removed since the listing is not a failure.
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            found.push(StoredFile {
                id: ClipId::new(id),
                size: meta.len(),
                modified: meta.modified()?.into(),
            });
        }
        Ok(found)
    }

    async fn read(&self, id: ClipId) -> std::io::Result<Option<Vec<u8>>> {
        match tokio::fs::read(self.path(id)).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn size(&self, id: ClipId) -> std::io::Result<Option<u64>> {
        match tokio::fs::metadata(self.path(id)).await {
            Ok(meta) => Ok(Some(meta.len())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn read_range(
        &self,
        id: ClipId,
        offset: u64,
        len: usize,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let mut file = match tokio::fs::File::open(self.path(id)).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        // `take` bounds the read; `read_to_end` loops until the bound or EOF.
        let mut buf = Vec::with_capacity(len);
        file.take(len as u64).read_to_end(&mut buf).await?;
        Ok(Some(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect, migrate};
    use time::macros::datetime;

    fn clip(n: u128, key: &str, text: &str, voice: &str, minute: u8) -> Clip {
        Clip {
            id: ClipId::new(Uuid::from_u128(n)),
            key: key.into(),
            text_hash: text.into(),
            voice: voice.into(),
            duration_ms: 1500,
            size_bytes: 4096,
            created_at: datetime!(2026-11-13 10:00:00 UTC) + time::Duration::minutes(minute.into()),
        }
    }

    async fn repo() -> SqliteClipRepository {
        let pool = connect("sqlite::memory:").await.unwrap();
        migrate(&pool).await.unwrap();
        SqliteClipRepository::new(pool)
    }

    #[tokio::test]
    async fn a_clip_is_found_by_id_and_by_key() {
        let repo = repo().await;
        let c = clip(1, "k1", "t1", "a", 0);
        repo.insert(&c).await.unwrap();
        assert_eq!(repo.get(c.id).await.unwrap(), Some(c.clone()));
        assert_eq!(repo.find_by_key("k1").await.unwrap(), Some(c));
        assert_eq!(repo.find_by_key("nope").await.unwrap(), None);
        assert_eq!(
            repo.get(ClipId::new(Uuid::from_u128(9))).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn a_key_or_id_is_stored_once() {
        let repo = repo().await;
        repo.insert(&clip(1, "k1", "t1", "a", 0)).await.unwrap();
        let same_key = repo.insert(&clip(2, "k1", "t1", "a", 1)).await;
        assert!(matches!(same_key, Err(RepositoryError::Duplicate)));
        let same_id = repo.insert(&clip(1, "k2", "t1", "a", 1)).await;
        assert!(matches!(same_id, Err(RepositoryError::Duplicate)));
    }

    #[tokio::test]
    async fn another_voice_of_the_same_text_is_found_newest_first() {
        let repo = repo().await;
        repo.insert(&clip(1, "k1", "t1", "a", 0)).await.unwrap();
        repo.insert(&clip(2, "k2", "t1", "b", 1)).await.unwrap();
        repo.insert(&clip(3, "k3", "t1", "c", 2)).await.unwrap();
        repo.insert(&clip(4, "k4", "t2", "b", 3)).await.unwrap();

        let found = repo.find_other_voice("t1", "c").await.unwrap().unwrap();
        assert_eq!(found.voice, "b");
        let found = repo.find_other_voice("t1", "a").await.unwrap().unwrap();
        assert_eq!(found.voice, "c");
        assert_eq!(repo.find_other_voice("t3", "a").await.unwrap(), None);
        // The only voice there is, is the one excluded.
        assert_eq!(repo.find_other_voice("t2", "b").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_grant_is_per_user_and_clip_and_idempotent() {
        let repo = repo().await;
        let c1 = clip(1, "k1", "t1", "a", 0);
        let c2 = clip(2, "k2", "t2", "a", 1);
        repo.insert(&c1).await.unwrap();
        repo.insert(&c2).await.unwrap();
        let at = datetime!(2026-11-13 12:00:00 UTC);
        assert!(!repo.is_granted(UserId::OWNER, c1.id).await.unwrap());
        repo.grant(UserId::OWNER, &[c1.id], at).await.unwrap();
        repo.grant(UserId::OWNER, &[c1.id, c1.id], at)
            .await
            .unwrap();
        repo.grant(UserId::OWNER, &[], at).await.unwrap();
        assert!(repo.is_granted(UserId::OWNER, c1.id).await.unwrap());
        assert!(!repo.is_granted(UserId::OWNER, c2.id).await.unwrap());
        assert!(
            !repo
                .is_granted(UserId::new(Uuid::from_u128(5)), c1.id)
                .await
                .unwrap()
        );
    }

    async fn granted_at(repo: &SqliteClipRepository, clip: ClipId) -> String {
        sqlx::query_scalar("SELECT granted_at FROM audio_clip_grants WHERE clip_id = ?")
            .bind(clip.to_string())
            .fetch_one(&repo.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_grant_keeps_the_last_use_but_is_not_rewritten_within_a_day() {
        let repo = repo().await;
        let c = clip(1, "k1", "t1", "a", 0);
        repo.insert(&c).await.unwrap();
        let first = datetime!(2026-11-13 12:00:00 UTC);
        repo.grant(UserId::OWNER, &[c.id], first).await.unwrap();
        // An hour later, and exactly a day later: the stored time stays.
        repo.grant(UserId::OWNER, &[c.id], first + time::Duration::hours(1))
            .await
            .unwrap();
        repo.grant(UserId::OWNER, &[c.id], first + GRANT_REFRESH)
            .await
            .unwrap();
        assert_eq!(granted_at(&repo, c.id).await, format_ts(first));
        // More than a day later: it moves forward.
        let later = first + GRANT_REFRESH + time::Duration::seconds(1);
        repo.grant(UserId::OWNER, &[c.id], later).await.unwrap();
        assert_eq!(granted_at(&repo, c.id).await, format_ts(later));
        // Never backwards.
        repo.grant(UserId::OWNER, &[c.id], first).await.unwrap();
        assert_eq!(granted_at(&repo, c.id).await, format_ts(later));
    }

    #[tokio::test]
    async fn the_last_use_is_the_newest_grant_or_the_creation() {
        let repo = repo().await;
        let never = clip(1, "k1", "t1", "a", 0);
        let shown = clip(2, "k2", "t2", "a", 1);
        repo.insert(&never).await.unwrap();
        repo.insert(&shown).await.unwrap();
        let a = datetime!(2026-12-01 00:00:00 UTC);
        repo.grant(UserId::OWNER, &[shown.id], a).await.unwrap();
        let all = repo.list_with_last_use().await.unwrap();
        assert_eq!(all[0], (never.clone(), never.created_at));
        assert_eq!(all[1], (shown.clone(), a));
    }

    #[tokio::test]
    async fn a_delete_is_refused_when_the_clip_was_used_since_the_cutoff() {
        let repo = repo().await;
        let used = clip(1, "k1", "t1", "a", 0);
        let idle = clip(2, "k2", "t2", "a", 1);
        repo.insert(&used).await.unwrap();
        repo.insert(&idle).await.unwrap();
        repo.grant(
            UserId::OWNER,
            &[used.id],
            datetime!(2026-12-01 00:00:00 UTC),
        )
        .await
        .unwrap();
        let cutoff = datetime!(2026-11-20 00:00:00 UTC);
        // Created before the cutoff, but shown after it.
        assert!(!repo.delete_if_unused_since(used.id, cutoff).await.unwrap());
        assert!(repo.delete_if_unused_since(idle.id, cutoff).await.unwrap());
        assert!(!repo.delete_if_unused_since(idle.id, cutoff).await.unwrap());
        assert_eq!(repo.get(idle.id).await.unwrap(), None);
        assert!(repo.get(used.id).await.unwrap().is_some());
        // Deleting takes the grants with it.
        assert!(
            repo.delete_if_unused_since(used.id, datetime!(2027-01-01 00:00:00 UTC))
                .await
                .unwrap()
        );
        assert!(!repo.is_granted(UserId::OWNER, used.id).await.unwrap());
    }

    #[tokio::test]
    async fn the_schema_refuses_empty_clips() {
        let repo = repo().await;
        let mut c = clip(1, "k1", "t1", "a", 0);
        c.duration_ms = 0;
        assert!(repo.insert(&c).await.is_err());
    }

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("teach_me_clips_{}", Uuid::now_v7()))
    }

    #[tokio::test]
    async fn a_file_is_read_in_slices_and_has_a_size() {
        let dir = temp_dir();
        let store = FsClipStore::new(dir.clone());
        store.prepare().await.unwrap();
        let id = ClipId::new(Uuid::from_u128(8));
        assert_eq!(store.size(id).await.unwrap(), None);
        assert_eq!(store.read_range(id, 0, 4).await.unwrap(), None);
        store.put(id, b"0123456789").await.unwrap();
        assert_eq!(store.size(id).await.unwrap(), Some(10));
        assert_eq!(store.read_range(id, 2, 3).await.unwrap().unwrap(), b"234");
        // Past the end: what is there, then nothing.
        assert_eq!(store.read_range(id, 8, 50).await.unwrap().unwrap(), b"89");
        assert_eq!(store.read_range(id, 10, 5).await.unwrap().unwrap(), b"");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn files_are_written_whole_and_read_back() {
        let dir = temp_dir();
        let store = FsClipStore::new(dir.clone());
        store.prepare().await.unwrap();
        let id = ClipId::new(Uuid::from_u128(7));
        assert_eq!(store.read(id).await.unwrap(), None);
        assert!(!store.exists(id).await.unwrap());
        store.put(id, b"OggS-data").await.unwrap();
        assert!(store.exists(id).await.unwrap());
        assert_eq!(store.read(id).await.unwrap().unwrap(), b"OggS-data");
        // No temporary file is left behind.
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [format!("{id}.ogg")]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn files_are_listed_and_removed() {
        let dir = temp_dir();
        let store = FsClipStore::new(dir.clone());
        store.prepare().await.unwrap();
        let id = ClipId::new(Uuid::from_u128(7));
        let missing = FsClipStore::new(temp_dir());
        assert_eq!(missing.list().await.unwrap(), []);
        store.put(id, b"12345").await.unwrap();
        // Neither a temporary file nor a foreign name is a clip.
        std::fs::write(dir.join("half.part"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        std::fs::write(dir.join("not-a-uuid.ogg"), b"x").unwrap();
        let listed = store.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id, listed[0].size), (id, 5));
        assert!(listed[0].modified > OffsetDateTime::now_utc() - time::Duration::minutes(1));
        assert_eq!(store.remove(id).await.unwrap(), Some(5));
        assert_eq!(store.remove(id).await.unwrap(), None);
        assert!(!store.exists(id).await.unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn prepare_sweeps_partial_files_and_keeps_clips() {
        let dir = temp_dir();
        let store = FsClipStore::new(dir.clone());
        store.prepare().await.unwrap();
        let id = ClipId::new(Uuid::from_u128(7));
        store.put(id, b"ok").await.unwrap();
        std::fs::write(dir.join("half.part"), b"x").unwrap();
        store.prepare().await.unwrap();
        assert!(!dir.join("half.part").exists());
        assert!(store.read(id).await.unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_failed_write_leaves_nothing_visible() {
        // The directory does not exist and `prepare` was not called.
        let store = FsClipStore::new(temp_dir());
        let id = ClipId::new(Uuid::from_u128(7));
        assert!(store.put(id, b"x").await.is_err());
        assert_eq!(store.read(id).await.unwrap(), None);
    }
}
