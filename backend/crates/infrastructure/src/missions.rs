//! SQLite adapter for the mission repository.

use async_trait::async_trait;
use domain::{
    Job, Mission, MissionContent, MissionCursor, MissionRepository, MissionStatus, RepositoryError,
    StateChange, TopicId,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::jobs::insert_job;
use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

macro_rules! mission_select {
    () => {
        "SELECT topic_id, version, status, why, starting_point, success_criteria, \
         mission_constraints, out_of_scope, created_at, confirmed_at, superseded_at FROM missions"
    };
}

fn list_from_json(raw: &str, row_id: &str) -> Result<Vec<String>, RepositoryError> {
    serde_json::from_str(raw).map_err(|_| corrupt("mission list", row_id))
}

fn list_to_json(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_owned())
}

fn mission_from_row(row: &SqliteRow) -> Result<Mission, RepositoryError> {
    let topic_id: String = row.try_get("topic_id").map_err(storage)?;
    let version: i64 = row.try_get("version").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let why: String = row.try_get("why").map_err(storage)?;
    let starting_point: Option<String> = row.try_get("starting_point").map_err(storage)?;
    let success: String = row.try_get("success_criteria").map_err(storage)?;
    let constraints: String = row.try_get("mission_constraints").map_err(storage)?;
    let out_of_scope: String = row.try_get("out_of_scope").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let confirmed: Option<String> = row.try_get("confirmed_at").map_err(storage)?;
    let superseded: Option<String> = row.try_get("superseded_at").map_err(storage)?;
    // Re-validated on the way out, like titles: the file is not trusted.
    let content = MissionContent::parse_stored(
        &why,
        starting_point.as_deref(),
        &list_from_json(&success, &topic_id)?,
        &list_from_json(&constraints, &topic_id)?,
        &list_from_json(&out_of_scope, &topic_id)?,
    )
    .map_err(|_| corrupt("mission content", &topic_id))?;
    Ok(Mission {
        topic_id: TopicId::new(parse_id(&topic_id)?),
        version: u32::try_from(version).map_err(|_| corrupt("mission version", &topic_id))?,
        status: status
            .parse::<MissionStatus>()
            .map_err(|_| corrupt("mission status", &topic_id))?,
        content,
        created_at: parse_ts(&created)?,
        confirmed_at: confirmed.as_deref().map(parse_ts).transpose()?,
        superseded_at: superseded.as_deref().map(parse_ts).transpose()?,
    })
}

/// Moves the topic inside the caller's transaction; `Conflict` if it is no
/// longer in the expected state (the transaction is then dropped, rolling back).
async fn move_topic(
    tx: &mut sqlx::SqliteConnection,
    topic_id: TopicId,
    change: StateChange,
    at: OffsetDateTime,
) -> Result<(), RepositoryError> {
    let result =
        sqlx::query("UPDATE topics SET state = ?, updated_at = ? WHERE id = ? AND state = ?")
            .bind(change.to.as_str())
            .bind(format_ts(at))
            .bind(topic_id.to_string())
            .bind(change.from.as_str())
            .execute(tx)
            .await
            .map_err(storage)?;
    if result.rows_affected() == 0 {
        return Err(RepositoryError::Conflict);
    }
    Ok(())
}

#[derive(Clone)]
pub struct SqliteMissionRepository {
    pool: SqlitePool,
}

impl SqliteMissionRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn one_with_status(
        &self,
        topic_id: TopicId,
        status: MissionStatus,
    ) -> Result<Option<Mission>, RepositoryError> {
        let row = sqlx::query(concat!(
            mission_select!(),
            " WHERE topic_id = ? AND status = ?"
        ))
        .bind(topic_id.to_string())
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(mission_from_row).transpose()
    }
}

#[async_trait]
impl MissionRepository for SqliteMissionRepository {
    async fn insert_draft(
        &self,
        topic_id: TopicId,
        content: &MissionContent,
        at: OffsetDateTime,
        topic_change: Option<StateChange>,
    ) -> Result<Mission, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if let Some(change) = topic_change {
            // First statement of the transaction is a write: it takes the lock
            // up front instead of upgrading a read snapshot later.
            move_topic(&mut tx, topic_id, change, at).await?;
        }
        // Write first: the transaction takes SQLite's write lock before it
        // reads the next version number, so two writers cannot pick the same one.
        sqlx::query(
            "UPDATE missions SET status = 'superseded', superseded_at = ? \
             WHERE topic_id = ? AND status = 'draft'",
        )
        .bind(format_ts(at))
        .bind(topic_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        let (max,): (Option<i64>,) =
            sqlx::query_as("SELECT MAX(version) FROM missions WHERE topic_id = ?")
                .bind(topic_id.to_string())
                .fetch_one(&mut *tx)
                .await
                .map_err(storage)?;
        let version = u32::try_from(max.unwrap_or(0) + 1)
            .map_err(|_| corrupt("mission version", &topic_id.to_string()))?;
        let mission = Mission::draft(topic_id, version, content.clone(), at);
        sqlx::query(
            "INSERT INTO missions (topic_id, version, status, why, starting_point, \
             success_criteria, mission_constraints, out_of_scope, created_at) \
             VALUES (?, ?, 'draft', ?, ?, ?, ?, ?, ?)",
        )
        .bind(topic_id.to_string())
        .bind(i64::from(version))
        .bind(content.why())
        .bind(content.starting_point())
        .bind(list_to_json(content.success_criteria()))
        .bind(list_to_json(content.constraints()))
        .bind(list_to_json(content.out_of_scope()))
        .bind(format_ts(at))
        .execute(&mut *tx)
        .await
        .map_err(constraint)?;
        tx.commit().await.map_err(storage)?;
        Ok(mission)
    }

    async fn current(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError> {
        self.one_with_status(topic_id, MissionStatus::Confirmed)
            .await
    }

    async fn open_draft(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError> {
        self.one_with_status(topic_id, MissionStatus::Draft).await
    }

    async fn confirm(
        &self,
        mission: &Mission,
        superseded_at: OffsetDateTime,
        topic_change: Option<StateChange>,
        follow_up: Option<&Job>,
    ) -> Result<(), RepositoryError> {
        let Some(confirmed_at) = mission.confirmed_at else {
            return Err(RepositoryError::Conflict);
        };
        let mut tx = self.pool.begin().await.map_err(storage)?;
        if let Some(change) = topic_change {
            move_topic(&mut tx, mission.topic_id, change, confirmed_at).await?;
        }
        // The old confirmed version must go first: the unique index allows one.
        sqlx::query(
            "UPDATE missions SET status = 'superseded', superseded_at = ? \
             WHERE topic_id = ? AND status = 'confirmed'",
        )
        .bind(format_ts(superseded_at))
        .bind(mission.topic_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        let result = sqlx::query(
            "UPDATE missions SET status = 'confirmed', confirmed_at = ? \
             WHERE topic_id = ? AND version = ? AND status = 'draft'",
        )
        .bind(format_ts(confirmed_at))
        .bind(mission.topic_id.to_string())
        .bind(i64::from(mission.version))
        .execute(&mut *tx)
        .await
        .map_err(storage)?;
        if result.rows_affected() == 0 {
            // Dropping the transaction rolls the first update back.
            return Err(RepositoryError::Conflict);
        }
        if let Some(job) = follow_up {
            // A job of that kind already active is not a stale draft: it is
            // reported apart (`Duplicate`).
            insert_job(&mut tx, job).await.map_err(|e| match e {
                RepositoryError::Conflict => RepositoryError::Duplicate,
                other => other,
            })?;
        }
        tx.commit().await.map_err(storage)
    }

    async fn list_versions(
        &self,
        topic_id: TopicId,
        after: Option<MissionCursor>,
        limit: u32,
    ) -> Result<Vec<Mission>, RepositoryError> {
        let rows = sqlx::query(concat!(
            mission_select!(),
            " WHERE topic_id = ? AND (? IS NULL OR version < ?) ORDER BY version DESC LIMIT ?"
        ))
        .bind(topic_id.to_string())
        .bind(after.map(|c| i64::from(c.version)))
        .bind(after.map(|c| i64::from(c.version)))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.iter().map(mission_from_row).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteTopicRepository;
    use crate::repositories::SqliteFolderRepository;
    use crate::sqlite::{connect, migrate};
    use domain::{
        FolderId, FolderName, FolderRepository, NewFolder, StateChange, Topic, TopicRepository,
        TopicState, TopicTitle,
    };
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-08 10:00:00.5 UTC);

    struct Env {
        missions: SqliteMissionRepository,
        topics: SqliteTopicRepository,
        pool: SqlitePool,
        topic: TopicId,
    }

    async fn env() -> Env {
        let pool = connect("sqlite::memory:").await.unwrap();
        env_on(pool).await
    }

    async fn env_on(pool: SqlitePool) -> Env {
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
        let topic = Topic::new(
            TopicId::new(Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        Env {
            missions: SqliteMissionRepository::new(pool.clone()),
            topics,
            pool,
            topic: topic.id,
        }
    }

    fn content(why: &str) -> MissionContent {
        MissionContent::parse(
            why,
            "Kotlin sì, Rust no",
            &["So fare \"X\" 日本".to_owned(), "So fare Y".to_owned()],
            &["30 minuti".to_owned()],
            &[],
        )
        .unwrap()
    }

    fn at(secs: i64) -> OffsetDateTime {
        T0 + time::Duration::seconds(secs)
    }

    #[tokio::test]
    async fn starting_point_round_trips_and_an_old_row_reads_as_none() {
        let env = env().await;
        let saved = env
            .missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        assert_eq!(saved.content.starting_point(), Some("Kotlin sì, Rust no"));
        let read = env.missions.open_draft(env.topic).await.unwrap().unwrap();
        assert_eq!(read, saved);

        // A row written before the column existed has NULL there.
        sqlx::query("UPDATE missions SET starting_point = NULL WHERE topic_id = ?")
            .bind(env.topic.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let old = env.missions.open_draft(env.topic).await.unwrap().unwrap();
        assert_eq!(old.content.starting_point(), None);
        assert_eq!(old.content.why(), "Uno");
    }

    #[tokio::test]
    async fn drafts_get_increasing_versions_and_round_trip() {
        let env = env().await;
        let first = env
            .missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        assert_eq!((first.version, first.status), (1, MissionStatus::Draft));
        assert_eq!(
            env.missions.open_draft(env.topic).await.unwrap(),
            Some(first.clone())
        );
        assert!(env.missions.current(env.topic).await.unwrap().is_none());

        // A second draft supersedes the first and takes the next version.
        let second = env
            .missions
            .insert_draft(env.topic, &content("Due"), at(5), None)
            .await
            .unwrap();
        assert_eq!(second.version, 2);
        let open = env.missions.open_draft(env.topic).await.unwrap().unwrap();
        assert_eq!(open, second);
        let all = env
            .missions
            .list_versions(env.topic, None, 10)
            .await
            .unwrap();
        assert_eq!(all.iter().map(|m| m.version).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(all[1].status, MissionStatus::Superseded);
        assert_eq!(all[1].superseded_at, Some(at(5)));
        assert_eq!(all[1].content, content("Uno"), "text survives intact");
    }

    #[tokio::test]
    async fn confirming_supersedes_the_previous_confirmed_version_atomically() {
        let env = env().await;
        let mut v1 = env
            .missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        v1.confirm(at(1)).unwrap();
        env.missions.confirm(&v1, at(1), None, None).await.unwrap();
        assert_eq!(
            env.missions.current(env.topic).await.unwrap(),
            Some(v1.clone())
        );
        assert!(env.missions.open_draft(env.topic).await.unwrap().is_none());

        let mut v2 = env
            .missions
            .insert_draft(env.topic, &content("Due"), at(2), None)
            .await
            .unwrap();
        v2.confirm(at(3)).unwrap();
        env.missions.confirm(&v2, at(3), None, None).await.unwrap();
        let current = env.missions.current(env.topic).await.unwrap().unwrap();
        assert_eq!((current.version, current.confirmed_at), (2, Some(at(3))));
        let all = env
            .missions
            .list_versions(env.topic, None, 10)
            .await
            .unwrap();
        assert_eq!(all[1].status, MissionStatus::Superseded);
        assert_eq!(all[1].confirmed_at, Some(at(1)));
        assert_eq!(all[1].superseded_at, Some(at(3)));
    }

    #[tokio::test]
    async fn confirming_something_that_is_not_an_open_draft_conflicts_and_changes_nothing() {
        let env = env().await;
        let mut v1 = env
            .missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        v1.confirm(at(1)).unwrap();
        env.missions.confirm(&v1, at(1), None, None).await.unwrap();

        // Confirmed twice: the first confirmation must stay as it is.
        assert!(matches!(
            env.missions.confirm(&v1, at(9), None, None).await,
            Err(RepositoryError::Conflict)
        ));
        let current = env.missions.current(env.topic).await.unwrap().unwrap();
        assert_eq!(
            (current.version, current.status),
            (1, MissionStatus::Confirmed)
        );

        // A version that never existed rolls the supersede of the old one back.
        let mut ghost = Mission::draft(env.topic, 7, content("Boh"), at(2));
        ghost.confirm(at(2)).unwrap();
        assert!(matches!(
            env.missions.confirm(&ghost, at(2), None, None).await,
            Err(RepositoryError::Conflict)
        ));
        assert_eq!(
            env.missions
                .current(env.topic)
                .await
                .unwrap()
                .unwrap()
                .version,
            1
        );
        // A mission that was not confirmed by the domain is refused outright.
        let unconfirmed = Mission::draft(env.topic, 1, content("Uno"), at(0));
        assert!(matches!(
            env.missions.confirm(&unconfirmed, at(0), None, None).await,
            Err(RepositoryError::Conflict)
        ));
    }

    #[tokio::test]
    async fn the_database_itself_allows_one_open_draft_and_one_confirmed_per_topic() {
        let env = env().await;
        env.missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        let second_draft = sqlx::query(
            "INSERT INTO missions (topic_id, version, status, why, success_criteria, \\
             mission_constraints, out_of_scope, created_at) \\
             VALUES (?, 9, 'draft', 'w', '[]', '[]', '[]', 'x')",
        )
        .bind(env.topic.to_string())
        .execute(&env.pool)
        .await;
        assert!(second_draft.is_err());
        let two_confirmed = sqlx::query(
            "UPDATE missions SET status = 'confirmed', confirmed_at = 'x' WHERE topic_id = ?",
        )
        .bind(env.topic.to_string())
        .execute(&env.pool)
        .await;
        assert!(two_confirmed.is_ok());
        env.missions
            .insert_draft(env.topic, &content("Due"), at(1), None)
            .await
            .unwrap();
        let again = sqlx::query(
            "UPDATE missions SET status = 'confirmed', confirmed_at = 'x' WHERE status = 'draft'",
        )
        .execute(&env.pool)
        .await;
        assert!(again.is_err(), "a second confirmed mission is refused");
    }

    #[tokio::test]
    async fn versions_page_newest_first_and_a_missing_topic_is_a_missing_parent() {
        let env = env().await;
        for n in 0..4 {
            env.missions
                .insert_draft(env.topic, &content("x"), at(n), None)
                .await
                .unwrap();
        }
        let page = env
            .missions
            .list_versions(env.topic, None, 2)
            .await
            .unwrap();
        assert_eq!(page.iter().map(|m| m.version).collect::<Vec<_>>(), [4, 3]);
        let next = env
            .missions
            .list_versions(env.topic, Some(MissionCursor::after(&page[1])), 2)
            .await
            .unwrap();
        assert_eq!(next.iter().map(|m| m.version).collect::<Vec<_>>(), [2, 1]);

        let ghost = TopicId::new(Uuid::from_u128(99));
        assert!(matches!(
            env.missions
                .insert_draft(ghost, &content("x"), at(0), None)
                .await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn deleting_the_topic_cascades_to_its_missions() {
        let env = env().await;
        env.missions
            .insert_draft(env.topic, &content("x"), at(0), None)
            .await
            .unwrap();
        assert!(
            env.topics
                .delete(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
        );
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM missions")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    async fn state_of(env: &Env) -> TopicState {
        env.topics
            .get(domain::UserId::OWNER, env.topic)
            .await
            .unwrap()
            .unwrap()
            .state
    }

    #[tokio::test]
    async fn the_topic_moves_in_the_same_transaction_as_the_mission() {
        let env = env().await;
        let propose = StateChange {
            from: TopicState::New,
            to: TopicState::MissionProposed,
        };
        env.missions
            .insert_draft(env.topic, &content("Uno"), at(0), Some(propose))
            .await
            .unwrap();
        assert_eq!(state_of(&env).await, TopicState::MissionProposed);

        // The topic is no longer in `from`: no new version, the old draft stays.
        let stale = env
            .missions
            .insert_draft(env.topic, &content("Due"), at(1), Some(propose))
            .await;
        assert!(matches!(stale, Err(RepositoryError::Conflict)));
        let all = env
            .missions
            .list_versions(env.topic, None, 10)
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, MissionStatus::Draft);

        // Confirmation with a wrong expected state leaves everything open.
        let mut draft = env.missions.open_draft(env.topic).await.unwrap().unwrap();
        draft.confirm(at(2)).unwrap();
        let wrong = StateChange {
            from: TopicState::Interviewing,
            to: TopicState::Researching,
        };
        assert!(matches!(
            env.missions.confirm(&draft, at(2), Some(wrong), None).await,
            Err(RepositoryError::Conflict)
        ));
        assert!(env.missions.current(env.topic).await.unwrap().is_none());
        assert!(env.missions.open_draft(env.topic).await.unwrap().is_some());
        assert_eq!(state_of(&env).await, TopicState::MissionProposed);

        let right = StateChange {
            from: TopicState::MissionProposed,
            to: TopicState::Researching,
        };
        env.missions
            .confirm(&draft, at(2), Some(right), None)
            .await
            .unwrap();
        assert_eq!(state_of(&env).await, TopicState::Researching);
        assert!(env.missions.current(env.topic).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn follow_up_job_is_stored_with_the_confirmation_or_not_at_all() {
        use crate::jobs::SqliteJobRepository;
        use domain::{JobId, JobKind, JobRepository};

        let env = env().await;
        let jobs = SqliteJobRepository::new(env.pool.clone());
        let to_researching = StateChange {
            from: TopicState::New,
            to: TopicState::Researching,
        };
        let job = |n: u128| {
            Job::queued(
                JobId::new(Uuid::from_u128(n)),
                env.topic,
                JobKind::Research,
                at(2),
            )
        };

        // A job of that kind is already active: the whole confirmation is refused.
        jobs.insert(&job(10)).await.unwrap();
        let mut draft = env
            .missions
            .insert_draft(env.topic, &content("Uno"), at(0), None)
            .await
            .unwrap();
        draft.confirm(at(2)).unwrap();
        let queued = job(11);
        assert!(matches!(
            env.missions
                .confirm(&draft, at(2), Some(to_researching), Some(&queued))
                .await,
            Err(RepositoryError::Duplicate)
        ));
        assert!(env.missions.current(env.topic).await.unwrap().is_none());
        assert_eq!(state_of(&env).await, TopicState::New);

        // Free slot: mission, state and job land together.
        sqlx::query("DELETE FROM jobs")
            .execute(&env.pool)
            .await
            .unwrap();
        env.missions
            .confirm(&draft, at(2), Some(to_researching), Some(&queued))
            .await
            .unwrap();
        assert!(env.missions.current(env.topic).await.unwrap().is_some());
        assert_eq!(state_of(&env).await, TopicState::Researching);
        assert_eq!(
            jobs.get(domain::UserId::OWNER, env.topic, queued.id)
                .await
                .unwrap(),
            Some(queued)
        );
    }

    #[tokio::test]
    async fn concurrent_writers_never_share_a_version_or_confirm_twice() {
        let dir = std::env::temp_dir().join(format!("teach_me_race_{}", Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = connect(&format!("sqlite://{}", dir.join("t.db").display()))
            .await
            .unwrap();
        let env = env_on(pool).await;

        let mut tasks = Vec::new();
        for n in 0..8 {
            let repo = env.missions.clone();
            let topic = env.topic;
            tasks.push(tokio::spawn(async move {
                repo.insert_draft(topic, &content(&format!("v{n}")), at(n), None)
                    .await
            }));
        }
        let mut versions = Vec::new();
        for t in tasks {
            versions.push(t.await.unwrap().unwrap().version);
        }
        versions.sort_unstable();
        assert_eq!(versions, (1..=8).collect::<Vec<u32>>());
        let (open,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM missions WHERE status = 'draft'")
                .fetch_one(&env.pool)
                .await
                .unwrap();
        assert_eq!(open, 1);

        // The same draft confirmed from two places at once: one wins.
        let mut draft = env.missions.open_draft(env.topic).await.unwrap().unwrap();
        draft.confirm(at(20)).unwrap();
        let results = {
            let (a, b) = (env.missions.clone(), env.missions.clone());
            let (d1, d2) = (draft.clone(), draft.clone());
            let t1 = tokio::spawn(async move { a.confirm(&d1, at(20), None, None).await });
            let t2 = tokio::spawn(async move { b.confirm(&d2, at(20), None, None).await });
            [t1.await.unwrap(), t2.await.unwrap()]
        };
        let ok = results.iter().filter(|r| r.is_ok()).count();
        let conflicts = results
            .iter()
            .filter(|r| matches!(r, Err(RepositoryError::Conflict)))
            .count();
        assert_eq!((ok, conflicts), (1, 1), "{results:?}");
        env.pool.close().await;
        std::fs::remove_dir_all(dir).unwrap();
    }
}
