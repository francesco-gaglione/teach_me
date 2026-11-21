//! SQLite adapter for the Researcher's sources.

use async_trait::async_trait;
use domain::{
    DocumentId, Gap, JobId, RepositoryError, ResearchInfo, ResearchOutcome, Resource, ResourceId,
    ResourceKind, ResourceRepository, SourceType, TopicId, TopicResources,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

fn resource_from_row(row: &SqliteRow) -> Result<Resource, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic_id: String = row.try_get("topic_id").map_err(storage)?;
    let kind: String = row.try_get("kind").map_err(storage)?;
    let source_type: String = row.try_get("source_type").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let document_id: Option<String> = row.try_get("document_id").map_err(storage)?;
    Ok(Resource {
        id: ResourceId::new(parse_id(&id)?),
        topic_id: TopicId::new(parse_id(&topic_id)?),
        kind: kind
            .parse::<ResourceKind>()
            .map_err(|_| corrupt("resource kind", &id))?,
        source_type: source_type
            .parse::<SourceType>()
            .map_err(|_| corrupt("resource source type", &id))?,
        title: row.try_get("title").map_err(storage)?,
        url: row.try_get("url").map_err(storage)?,
        language: row.try_get("language").map_err(storage)?,
        annotation: row.try_get("annotation").map_err(storage)?,
        use_for: row.try_get("use_for").map_err(storage)?,
        document_id: document_id
            .as_deref()
            .map(|d| parse_id(d).map(DocumentId::new))
            .transpose()?,
        created_at: parse_ts(&created)?,
    })
}

#[derive(Clone)]
pub struct SqliteResourceRepository {
    pool: SqlitePool,
}

impl SqliteResourceRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResourceRepository for SqliteResourceRepository {
    async fn get(&self, topic_id: TopicId) -> Result<TopicResources, RepositoryError> {
        let topic = topic_id.to_string();
        let rows = sqlx::query(
            "SELECT id, topic_id, kind, source_type, title, url, language, annotation, use_for, \
             document_id, created_at FROM resources WHERE topic_id = ? \
             ORDER BY (kind <> 'document'), \
             CASE WHEN kind = 'document' THEN 0 ELSE position END, \
             CASE WHEN kind = 'document' THEN created_at END DESC, id",
        )
        .bind(&topic)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        let resources = rows
            .iter()
            .map(resource_from_row)
            .collect::<Result<Vec<_>, _>>()?;

        let gaps = sqlx::query(
            "SELECT area, note FROM research_gaps WHERE topic_id = ? ORDER BY position",
        )
        .bind(&topic)
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?
        .iter()
        .map(|row| {
            Ok(Gap {
                area: row.try_get("area").map_err(storage)?,
                note: row.try_get("note").map_err(storage)?,
            })
        })
        .collect::<Result<Vec<_>, RepositoryError>>()?;

        let research = sqlx::query(
            "SELECT job_id, mission_version, finished_at, communities_skipped \
             FROM topic_research WHERE topic_id = ?",
        )
        .bind(&topic)
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        let (research, communities_skipped) = match research {
            None => (None, false),
            Some(row) => {
                let job_id: String = row.try_get("job_id").map_err(storage)?;
                let version: i64 = row.try_get("mission_version").map_err(storage)?;
                let finished: String = row.try_get("finished_at").map_err(storage)?;
                let skipped: i64 = row.try_get("communities_skipped").map_err(storage)?;
                (
                    Some(ResearchInfo {
                        job_id: JobId::new(parse_id(&job_id)?),
                        mission_version: u32::try_from(version)
                            .map_err(|_| corrupt("research mission version", &topic))?,
                        finished_at: parse_ts(&finished)?,
                    }),
                    skipped != 0,
                )
            }
        };
        Ok(TopicResources {
            resources,
            gaps,
            communities_skipped,
            research,
        })
    }

    async fn replace(
        &self,
        outcome: &ResearchOutcome,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        let topic = outcome.topic_id.to_string();
        let mut tx = self.pool.begin().await.map_err(storage)?;
        // The first statement is a write: the transaction takes the lock up
        // front. The topic move doubles as the existence check.
        let moved = sqlx::query(
            "UPDATE topics SET state = 'ready', updated_at = ? WHERE id = ? AND state = 'researching'",
        )
        .bind(format_ts(at))
        .bind(&topic)
        .execute(&mut *tx)
        .await
        .map_err(storage)?
        .rows_affected()
            > 0;
        let exists = sqlx::query("SELECT 1 FROM topics WHERE id = ?")
            .bind(&topic)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage)?
            .is_some();
        if !exists {
            return Err(RepositoryError::NotFound);
        }

        for delete in [
            // Uploaded documents are not the Researcher's to replace.
            "DELETE FROM resources WHERE topic_id = ? AND kind <> 'document'",
            "DELETE FROM research_gaps WHERE topic_id = ?",
            "DELETE FROM topic_research WHERE topic_id = ?",
        ] {
            sqlx::query(delete)
                .bind(&topic)
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
        }
        for (position, (id, draft)) in outcome.resources.iter().enumerate() {
            sqlx::query(
                "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, \
                 language, annotation, use_for, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id.to_string())
            .bind(&topic)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .bind(draft.kind.as_str())
            .bind(draft.source_type.as_str())
            .bind(&draft.title)
            .bind(&draft.url)
            .bind(&draft.language)
            .bind(&draft.annotation)
            .bind(&draft.use_for)
            .bind(format_ts(at))
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        for (position, gap) in outcome.gaps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO research_gaps (topic_id, position, area, note) VALUES (?, ?, ?, ?)",
            )
            .bind(&topic)
            .bind(i64::try_from(position).unwrap_or(i64::MAX))
            .bind(&gap.area)
            .bind(&gap.note)
            .execute(&mut *tx)
            .await
            .map_err(constraint)?;
        }
        sqlx::query(
            "INSERT INTO topic_research (topic_id, job_id, mission_version, finished_at, \
             communities_skipped) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&topic)
        .bind(outcome.info.job_id.to_string())
        .bind(i64::from(outcome.info.mission_version))
        .bind(format_ts(outcome.info.finished_at))
        .bind(i64::from(outcome.communities_skipped))
        .execute(&mut *tx)
        .await
        .map_err(constraint)?;
        tx.commit().await.map_err(storage)?;
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repositories::{SqliteFolderRepository, SqliteTopicRepository};
    use crate::sqlite::{connect, migrate};
    use domain::{
        FolderId, FolderName, FolderRepository, NewFolder, ResourceDraft, Topic, TopicRepository,
        TopicState, TopicTitle,
    };
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.5 UTC);
    const T1: OffsetDateTime = datetime!(2026-11-09 10:05:00.25 UTC);

    struct Env {
        resources: SqliteResourceRepository,
        topics: SqliteTopicRepository,
        topic: TopicId,
    }

    async fn env(state: TopicState) -> Env {
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
        let topic = Topic::new(
            TopicId::new(Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        if state != TopicState::New {
            topics
                .transition_state(topic.id, TopicState::New, state, T0)
                .await
                .unwrap();
        }
        Env {
            resources: SqliteResourceRepository::new(pool),
            topics,
            topic: topic.id,
        }
    }

    fn draft(n: u128, kind: ResourceKind, url: Option<&str>) -> (ResourceId, ResourceDraft) {
        (
            ResourceId::new(Uuid::from_u128(100 + n)),
            ResourceDraft {
                kind,
                source_type: if kind == ResourceKind::Community {
                    SourceType::Forum
                } else {
                    SourceType::Documentation
                },
                title: format!("Fonte {n}"),
                url: url.map(str::to_owned),
                language: "en".into(),
                annotation: "Annotazione.".into(),
                use_for: "capire X".into(),
            },
        )
    }

    fn outcome(env: &Env, resources: Vec<(ResourceId, ResourceDraft)>) -> ResearchOutcome {
        ResearchOutcome {
            topic_id: env.topic,
            info: ResearchInfo {
                job_id: JobId::new(Uuid::from_u128(9)),
                mission_version: 2,
                finished_at: T1,
            },
            communities_skipped: true,
            resources,
            gaps: vec![Gap {
                area: "Migrazioni".into(),
                note: "Nulla di recente.".into(),
            }],
        }
    }

    #[tokio::test]
    async fn an_unresearched_topic_has_nothing() {
        let env = env(TopicState::Researching).await;
        assert_eq!(
            env.resources.get(env.topic).await.unwrap(),
            TopicResources::default()
        );
    }

    #[tokio::test]
    async fn replace_stores_in_order_and_makes_a_researching_topic_ready() {
        let env = env(TopicState::Researching).await;
        let o = outcome(
            &env,
            vec![
                draft(
                    2,
                    ResourceKind::Knowledge,
                    Some("https://www.example.org/b"),
                ),
                draft(1, ResourceKind::Community, None),
            ],
        );
        assert!(env.resources.replace(&o, T1).await.unwrap());
        assert_eq!(
            env.topics
                .get(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
                .unwrap()
                .state,
            TopicState::Ready
        );

        let got = env.resources.get(env.topic).await.unwrap();
        assert_eq!(
            got.resources
                .iter()
                .map(|r| r.title.as_str())
                .collect::<Vec<_>>(),
            ["Fonte 2", "Fonte 1"]
        );
        assert_eq!(got.resources[0].domain().as_deref(), Some("example.org"));
        assert_eq!(got.resources[1].url, None);
        assert_eq!(got.resources[0].created_at, T1);
        assert_eq!(got.gaps, o.gaps);
        assert!(got.communities_skipped);
        assert_eq!(got.research, Some(o.info));
    }

    #[tokio::test]
    async fn a_second_research_replaces_everything_and_a_ready_topic_stays_ready() {
        let env = env(TopicState::Researching).await;
        let first = outcome(
            &env,
            vec![draft(
                1,
                ResourceKind::Knowledge,
                Some("https://a.example.org"),
            )],
        );
        env.resources.replace(&first, T0).await.unwrap();

        let mut second = outcome(
            &env,
            vec![draft(
                5,
                ResourceKind::Knowledge,
                Some("https://b.example.org"),
            )],
        );
        second.gaps.clear();
        second.communities_skipped = false;
        second.info.mission_version = 3;
        assert!(
            !env.resources.replace(&second, T1).await.unwrap(),
            "already ready"
        );

        let got = env.resources.get(env.topic).await.unwrap();
        assert_eq!(got.resources.len(), 1);
        assert_eq!(got.resources[0].title, "Fonte 5");
        assert!(got.gaps.is_empty() && !got.communities_skipped);
        assert_eq!(got.research.unwrap().mission_version, 3);
    }

    #[tokio::test]
    async fn a_failed_replace_leaves_the_previous_set_and_the_topic_alone() {
        let env = env(TopicState::Researching).await;
        // Two rows with the same id break the primary key halfway through.
        let bad = outcome(
            &env,
            vec![
                draft(1, ResourceKind::Knowledge, Some("https://a.example.org")),
                draft(1, ResourceKind::Knowledge, Some("https://b.example.org")),
            ],
        );
        assert!(env.resources.replace(&bad, T1).await.is_err());
        assert_eq!(
            env.topics
                .get(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
                .unwrap()
                .state,
            TopicState::Researching
        );
        assert_eq!(
            env.resources.get(env.topic).await.unwrap(),
            TopicResources::default()
        );
    }

    #[tokio::test]
    async fn a_missing_topic_is_not_found_and_deleting_the_topic_cascades() {
        let env = env(TopicState::Researching).await;
        let mut ghost = outcome(&env, vec![]);
        ghost.topic_id = TopicId::new(Uuid::from_u128(77));
        assert!(matches!(
            env.resources.replace(&ghost, T1).await,
            Err(RepositoryError::NotFound)
        ));

        let o = outcome(
            &env,
            vec![draft(
                1,
                ResourceKind::Knowledge,
                Some("https://a.example.org"),
            )],
        );
        env.resources.replace(&o, T1).await.unwrap();
        assert!(
            env.topics
                .delete(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
        );
        assert_eq!(
            env.resources.get(env.topic).await.unwrap(),
            TopicResources::default()
        );
    }
}
