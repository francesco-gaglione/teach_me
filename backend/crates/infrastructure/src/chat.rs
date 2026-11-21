//! SQLite adapter for the chat message repository.

use async_trait::async_trait;
use domain::{
    ChatCursor, ChatErrorCode, ChatMessage, ChatRepository, ContentLanguage, MessageError,
    MessageId, MessagePart, MessageRole, MessageStatus, Mission, MissionContent, MissionStatus,
    RepositoryError, TopicId,
};
use serde_json::{Value, json};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::repositories::{constraint, corrupt, format_ts, parse_id, parse_ts, storage};

/// Event ids stay below 2^53, so they fit SQLite's signed 64-bit integer.
fn event_id_column(id: Option<u64>) -> Option<i64> {
    id.and_then(|v| i64::try_from(v).ok())
}

fn mission_to_json(m: &Mission) -> Value {
    json!({
        "topic_id": m.topic_id.to_string(),
        "version": m.version,
        "status": m.status.as_str(),
        "why": m.content.why(),
        "starting_point": m.content.starting_point(),
        "success_criteria": m.content.success_criteria(),
        "constraints": m.content.constraints(),
        "out_of_scope": m.content.out_of_scope(),
        "created_at": format_ts(m.created_at),
        "confirmed_at": m.confirmed_at.map(format_ts),
        "superseded_at": m.superseded_at.map(format_ts),
    })
}

fn string_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|i| i.as_str().map(str::to_owned))
        .collect()
}

fn opt_ts(v: &Value) -> Result<Option<OffsetDateTime>, RepositoryError> {
    match v.as_str() {
        None => Ok(None),
        Some(s) => parse_ts(s).map(Some),
    }
}

fn mission_from_json(v: &Value) -> Option<Result<Mission, RepositoryError>> {
    // Snapshots saved before contract v1.2 have no `starting_point` key: an
    // absent (or null) key reads as `None`, and they are not rewritten.
    let content = MissionContent::parse_stored(
        v["why"].as_str()?,
        v["starting_point"].as_str(),
        &string_list(&v["success_criteria"])?,
        &string_list(&v["constraints"])?,
        &string_list(&v["out_of_scope"])?,
    )
    .ok()?;
    let topic_id = parse_id(v["topic_id"].as_str()?).ok()?;
    let version = u32::try_from(v["version"].as_u64()?).ok()?;
    let status = v["status"].as_str()?.parse::<MissionStatus>().ok()?;
    let created_at = parse_ts(v["created_at"].as_str()?).ok()?;
    Some(
        opt_ts(&v["confirmed_at"])
            .and_then(|confirmed_at| Ok((confirmed_at, opt_ts(&v["superseded_at"])?)))
            .map(|(confirmed_at, superseded_at)| Mission {
                topic_id: TopicId::new(topic_id),
                version,
                status,
                content,
                created_at,
                confirmed_at,
                superseded_at,
            }),
    )
}

fn parts_to_json(parts: &[MessagePart]) -> String {
    let items: Vec<Value> = parts
        .iter()
        .map(|part| match part {
            MessagePart::Text(text) => json!({ "type": "text", "text": text }),
            MessagePart::MissionProposal {
                mission,
                replaces_version,
            } => json!({
                "type": "mission_proposal",
                "mission": mission_to_json(mission),
                "replaces_version": replaces_version,
            }),
            MessagePart::LessonRequest { job_id } => json!({
                "type": "lesson_request",
                "job_id": job_id.to_string(),
            }),
        })
        .collect();
    Value::Array(items).to_string()
}

fn parts_from_json(raw: &str, row_id: &str) -> Result<Vec<MessagePart>, RepositoryError> {
    let bad = || corrupt("message parts", row_id);
    let Value::Array(items) = serde_json::from_str(raw).map_err(|_| bad())? else {
        return Err(bad());
    };
    items
        .iter()
        .map(|item| match item["type"].as_str() {
            Some("text") => item["text"]
                .as_str()
                .map(|t| MessagePart::Text(t.to_owned()))
                .ok_or_else(bad),
            Some("mission_proposal") => {
                let mission = mission_from_json(&item["mission"]).ok_or_else(bad)??;
                let replaces_version = match &item["replaces_version"] {
                    Value::Null => None,
                    v => Some(
                        v.as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or_else(bad)?,
                    ),
                };
                Ok(MessagePart::MissionProposal {
                    mission,
                    replaces_version,
                })
            }
            Some("lesson_request") => item["job_id"]
                .as_str()
                .and_then(domain::parse_uuid)
                .map(|u| MessagePart::LessonRequest {
                    job_id: domain::JobId::new(u),
                })
                .ok_or_else(bad),
            _ => Err(bad()),
        })
        .collect()
}

macro_rules! message_select {
    () => {
        "SELECT id, topic_id, role, status, parts, error_code, error_message, \
         created_at, last_event_id, updated_at FROM chat_messages"
    };
}

fn message_from_row(row: &SqliteRow) -> Result<ChatMessage, RepositoryError> {
    let id: String = row.try_get("id").map_err(storage)?;
    let topic_id: String = row.try_get("topic_id").map_err(storage)?;
    let role: String = row.try_get("role").map_err(storage)?;
    let status: String = row.try_get("status").map_err(storage)?;
    let parts: String = row.try_get("parts").map_err(storage)?;
    let code: Option<String> = row.try_get("error_code").map_err(storage)?;
    let message: Option<String> = row.try_get("error_message").map_err(storage)?;
    let created: String = row.try_get("created_at").map_err(storage)?;
    let last_event_id: Option<i64> = row.try_get("last_event_id").map_err(storage)?;
    let updated: String = row.try_get("updated_at").map_err(storage)?;
    let error = match (code, message) {
        (Some(code), Some(message)) => Some(MessageError {
            code: code
                .parse::<ChatErrorCode>()
                .map_err(|_| corrupt("error code", &id))?,
            message,
        }),
        _ => None,
    };
    Ok(ChatMessage {
        id: MessageId::new(parse_id(&id)?),
        topic_id: TopicId::new(parse_id(&topic_id)?),
        role: match role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            _ => return Err(corrupt("role", &id)),
        },
        status: match status.as_str() {
            "streaming" => MessageStatus::Streaming,
            "complete" => MessageStatus::Complete,
            "failed" => MessageStatus::Failed,
            _ => return Err(corrupt("status", &id)),
        },
        parts: parts_from_json(&parts, &id)?,
        error,
        created_at: parse_ts(&created)?,
        last_event_id: last_event_id.and_then(|v| u64::try_from(v).ok()),
        updated_at: parse_ts(&updated)?,
    })
}

#[derive(Clone)]
pub struct SqliteChatRepository {
    pool: SqlitePool,
}

impl SqliteChatRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const INSERT: &str = "INSERT INTO chat_messages \
    (id, topic_id, role, status, parts, error_code, error_message, created_at, last_event_id, \
     updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// A uniqueness violation here can only be the one-streaming-per-topic index
/// (ids are generated), so it is reported as a conflict.
fn turn_constraint(err: sqlx::Error) -> RepositoryError {
    match constraint(err) {
        RepositoryError::Duplicate => RepositoryError::Conflict,
        other => other,
    }
}

#[async_trait]
impl ChatRepository for SqliteChatRepository {
    async fn insert_turn(
        &self,
        user: &ChatMessage,
        assistant: &ChatMessage,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(storage)?;
        for m in [user, assistant] {
            sqlx::query(INSERT)
                .bind(m.id.to_string())
                .bind(m.topic_id.to_string())
                .bind(m.role.as_str())
                .bind(m.status.as_str())
                .bind(parts_to_json(&m.parts))
                .bind(m.error.as_ref().map(|e| e.code.as_str()))
                .bind(m.error.as_ref().map(|e| e.message.as_str()))
                .bind(format_ts(m.created_at))
                .bind(event_id_column(m.last_event_id))
                .bind(format_ts(m.updated_at))
                .execute(&mut *tx)
                .await
                .map_err(turn_constraint)?;
        }
        tx.commit().await.map_err(storage)
    }

    async fn get(
        &self,
        topic_id: TopicId,
        id: MessageId,
    ) -> Result<Option<ChatMessage>, RepositoryError> {
        let row = sqlx::query(concat!(message_select!(), " WHERE topic_id = ? AND id = ?"))
            .bind(topic_id.to_string())
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?;
        row.as_ref().map(message_from_row).transpose()
    }

    async fn latest(&self, topic_id: TopicId) -> Result<Option<ChatMessage>, RepositoryError> {
        let row = sqlx::query(concat!(
            message_select!(),
            " WHERE topic_id = ? ORDER BY created_at DESC, id DESC LIMIT 1"
        ))
        .bind(topic_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?;
        row.as_ref().map(message_from_row).transpose()
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<ChatCursor>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        let rows = match after {
            None => {
                sqlx::query(concat!(
                    message_select!(),
                    " WHERE topic_id = ? ORDER BY created_at DESC, id DESC LIMIT ?"
                ))
                .bind(topic_id.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
            Some(c) => {
                sqlx::query(concat!(
                    message_select!(),
                    " WHERE topic_id = ? AND (created_at, id) < (?, ?) \
                     ORDER BY created_at DESC, id DESC LIMIT ?"
                ))
                .bind(topic_id.to_string())
                .bind(format_ts(c.created_at))
                .bind(c.id.to_string())
                .bind(i64::from(limit))
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(storage)?;
        rows.iter().map(message_from_row).collect()
    }

    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE chat_messages SET status = ?, parts = ?, error_code = ?, error_message = ?, \
             last_event_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(message.status.as_str())
        .bind(parts_to_json(&message.parts))
        .bind(message.error.as_ref().map(|e| e.code.as_str()))
        .bind(message.error.as_ref().map(|e| e.message.as_str()))
        .bind(event_id_column(message.last_event_id))
        .bind(format_ts(message.updated_at))
        .bind(message.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(storage)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }

    async fn restart_failed(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            "UPDATE chat_messages SET status = ?, parts = ?, error_code = NULL, \
             error_message = NULL, last_event_id = ?, updated_at = ? \
             WHERE id = ? AND status = 'failed'",
        )
        .bind(message.status.as_str())
        .bind(parts_to_json(&message.parts))
        .bind(event_id_column(message.last_event_id))
        .bind(format_ts(message.updated_at))
        .bind(message.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(turn_constraint)?;
        if result.rows_affected() > 0 {
            return Ok(());
        }
        let exists = sqlx::query("SELECT 1 FROM chat_messages WHERE id = ?")
            .bind(message.id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(storage)?
            .is_some();
        Err(if exists {
            RepositoryError::Conflict
        } else {
            RepositoryError::NotFound
        })
    }

    async fn fail_streaming(
        &self,
        error: &(dyn Fn(ContentLanguage) -> MessageError + Sync),
        at: OffsetDateTime,
    ) -> Result<u64, RepositoryError> {
        // One pass per language: the text follows the message's topic.
        let mut changed = 0;
        for language in [ContentLanguage::It, ContentLanguage::En] {
            let error = error(language);
            let result = sqlx::query(
                "UPDATE chat_messages SET status = 'failed', error_code = ?, error_message = ?, \
                 updated_at = ? WHERE status = 'streaming' \
                 AND topic_id IN (SELECT id FROM topics WHERE language = ?)",
            )
            .bind(error.code.as_str())
            .bind(&error.message)
            .bind(format_ts(at))
            .bind(language.code())
            .execute(&self.pool)
            .await
            .map_err(storage)?;
            changed += result.rows_affected();
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteTopicRepository;
    use crate::repositories::SqliteFolderRepository;
    use crate::sqlite::{connect, migrate};
    use domain::{
        FolderId, FolderName, FolderRepository, MessageText, NewFolder, Topic, TopicRepository,
        TopicState, TopicTitle,
    };
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-08 10:00:00.5 UTC);

    struct Env {
        chat: SqliteChatRepository,
        topics: SqliteTopicRepository,
        pool: SqlitePool,
        topic: TopicId,
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
        let topic = Topic::new(
            TopicId::new(Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        );
        topics.insert(&topic).await.unwrap();
        Env {
            chat: SqliteChatRepository::new(pool.clone()),
            topics,
            pool,
            topic: topic.id,
        }
    }

    fn mid(n: u128) -> MessageId {
        MessageId::new(Uuid::from_u128(n))
    }

    fn turn(env: &Env, n: u128, secs: i64) -> (ChatMessage, ChatMessage) {
        let at = T0 + time::Duration::seconds(secs);
        (
            ChatMessage::user(
                mid(n),
                env.topic,
                MessageText::parse("domanda").unwrap(),
                at,
            ),
            ChatMessage::assistant_streaming(mid(n + 1), env.topic, at),
        )
    }

    #[tokio::test]
    async fn turn_round_trips_with_parts_and_errors() {
        let env = env().await;
        let (user, mut assistant) = turn(&env, 10, 0);
        env.chat.insert_turn(&user, &assistant).await.unwrap();
        assert_eq!(
            env.chat.get(env.topic, user.id).await.unwrap().unwrap(),
            user
        );
        assert_eq!(
            env.chat
                .get(env.topic, assistant.id)
                .await
                .unwrap()
                .unwrap(),
            assistant
        );

        assistant.append_text("Perché \"così\" 日本", T0);
        env.chat.save(&assistant).await.unwrap();
        let stored = env
            .chat
            .get(env.topic, assistant.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.text(), "Perché \"così\" 日本");

        assistant
            .fail(
                MessageError::from_code(ChatErrorCode::RateLimited, ContentLanguage::It),
                T0,
            )
            .unwrap();
        env.chat.save(&assistant).await.unwrap();
        let stored = env
            .chat
            .get(env.topic, assistant.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, assistant);

        // Another topic cannot see it.
        let other = TopicId::new(Uuid::from_u128(99));
        assert!(env.chat.get(other, assistant.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_mission_proposal_part_round_trips_and_stays_last() {
        let env = env().await;
        let (user, mut assistant) = turn(&env, 10, 0);
        env.chat.insert_turn(&user, &assistant).await.unwrap();
        let content = MissionContent::parse(
            "Per lavoro \"x\"",
            "Kotlin sì, Rust no",
            &["So fare X".to_owned()],
            &["30 min".to_owned()],
            &[],
        )
        .unwrap();
        let mut mission = Mission::draft(env.topic, 3, content, T0);
        mission.confirmed_at = Some(T0);
        mission.superseded_at = Some(T0);
        mission.status = MissionStatus::Superseded;
        assistant.append_text("Ecco", T0);
        assistant.add_mission_proposal(mission, Some(2), T0);
        env.chat.save(&assistant).await.unwrap();
        let stored = env
            .chat
            .get(env.topic, assistant.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, assistant);
        assert!(matches!(
            stored.parts[1],
            MessagePart::MissionProposal {
                replaces_version: Some(2),
                ..
            }
        ));

        // A snapshot saved before contract v1.2 has no `starting_point` key.
        let old = json!([{
            "type": "mission_proposal",
            "replaces_version": null,
            "mission": {
                "topic_id": env.topic.to_string(),
                "version": 1,
                "status": "draft",
                "why": "Per lavoro",
                "success_criteria": ["So fare X"],
                "constraints": [],
                "out_of_scope": [],
                "created_at": format_ts(T0),
                "confirmed_at": null,
                "superseded_at": null,
            }
        }]);
        sqlx::query("UPDATE chat_messages SET parts = ? WHERE id = ?")
            .bind(old.to_string())
            .bind(assistant.id.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        let stored = env
            .chat
            .get(env.topic, assistant.id)
            .await
            .unwrap()
            .unwrap();
        let MessagePart::MissionProposal { mission, .. } = &stored.parts[0] else {
            panic!("expected a mission proposal");
        };
        assert_eq!(mission.content.starting_point(), None);
        assert_eq!(mission.content.why(), "Per lavoro");

        // A part of an unknown type is corruption, not silently dropped.
        sqlx::query("UPDATE chat_messages SET parts = '[{\"type\":\"zzz\"}]' WHERE id = ?")
            .bind(assistant.id.to_string())
            .execute(&env.pool)
            .await
            .unwrap();
        assert!(matches!(
            env.chat.get(env.topic, assistant.id).await,
            Err(RepositoryError::Storage(_))
        ));
    }

    #[tokio::test]
    async fn second_streaming_message_in_a_topic_conflicts_and_stores_nothing() {
        let env = env().await;
        let (u1, a1) = turn(&env, 10, 0);
        env.chat.insert_turn(&u1, &a1).await.unwrap();
        let (u2, a2) = turn(&env, 20, 1);
        assert!(matches!(
            env.chat.insert_turn(&u2, &a2).await,
            Err(RepositoryError::Conflict)
        ));
        // The transaction rolled back: not even the user message is there.
        assert!(env.chat.get(env.topic, u2.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn missing_topic_is_missing_parent() {
        let env = env().await;
        let ghost = TopicId::new(Uuid::from_u128(77));
        let user = ChatMessage::user(mid(1), ghost, MessageText::parse("x").unwrap(), T0);
        let assistant = ChatMessage::assistant_streaming(mid(2), ghost, T0);
        assert!(matches!(
            env.chat.insert_turn(&user, &assistant).await,
            Err(RepositoryError::MissingParent)
        ));
    }

    #[tokio::test]
    async fn list_is_newest_first_with_keyset_paging_and_latest() {
        let env = env().await;
        for (i, n) in [10u128, 20, 30].into_iter().enumerate() {
            let (mut u, mut a) = turn(&env, n, i64::try_from(i).unwrap());
            env.chat.insert_turn(&u, &a).await.unwrap();
            a.complete(T0).unwrap();
            env.chat.save(&a).await.unwrap();
            u.updated_at = T0;
        }
        let all = env.chat.list(env.topic, None, 50).await.unwrap();
        let ids: Vec<_> = all.iter().map(|m| m.id).collect();
        assert_eq!(ids, [mid(31), mid(30), mid(21), mid(20), mid(11), mid(10)]);
        assert_eq!(
            env.chat.latest(env.topic).await.unwrap().unwrap().id,
            mid(31)
        );

        let page = env.chat.list(env.topic, None, 2).await.unwrap();
        let next = env
            .chat
            .list(env.topic, Some(ChatCursor::after(&page[1])), 2)
            .await
            .unwrap();
        assert_eq!(
            next.iter().map(|m| m.id).collect::<Vec<_>>(),
            [mid(21), mid(20)]
        );
    }

    #[tokio::test]
    async fn restart_requires_failed_status() {
        let env = env().await;
        let (u, mut a) = turn(&env, 10, 0);
        env.chat.insert_turn(&u, &a).await.unwrap();

        let mut attempt = a.clone();
        attempt.status = MessageStatus::Streaming;
        assert!(matches!(
            env.chat.restart_failed(&attempt).await,
            Err(RepositoryError::Conflict)
        ));

        a.append_text("x", T0);
        a.fail(
            MessageError::from_code(ChatErrorCode::Internal, ContentLanguage::It),
            T0,
        )
        .unwrap();
        env.chat.save(&a).await.unwrap();
        a.restart(true, T0).unwrap();
        env.chat.restart_failed(&a).await.unwrap();
        let stored = env.chat.get(env.topic, a.id).await.unwrap().unwrap();
        assert_eq!(stored, a);
        assert!(stored.parts.is_empty() && stored.error.is_none());

        let ghost = ChatMessage::assistant_streaming(mid(500), env.topic, T0);
        assert!(matches!(
            env.chat.restart_failed(&ghost).await,
            Err(RepositoryError::NotFound)
        ));
        assert!(matches!(
            env.chat.save(&ghost).await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn fail_streaming_keeps_partial_text() {
        let env = env().await;
        let (u, mut a) = turn(&env, 10, 0);
        env.chat.insert_turn(&u, &a).await.unwrap();
        a.append_text("meta", T0);
        env.chat.save(&a).await.unwrap();

        let error = MessageError::from_code(ChatErrorCode::Internal, ContentLanguage::It);
        let make = |l| MessageError::from_code(ChatErrorCode::Internal, l);
        let at = T0 + time::Duration::hours(1);
        assert_eq!(env.chat.fail_streaming(&make, at).await.unwrap(), 1);
        assert_eq!(env.chat.fail_streaming(&make, at).await.unwrap(), 0);
        let stored = env.chat.get(env.topic, a.id).await.unwrap().unwrap();
        assert_eq!(stored.status, MessageStatus::Failed);
        assert_eq!(stored.text(), "meta");
        assert_eq!(stored.error, Some(error));
        assert_eq!(stored.updated_at, at);
        let user = env.chat.get(env.topic, u.id).await.unwrap().unwrap();
        assert_eq!(user.status, MessageStatus::Complete);
    }

    #[tokio::test]
    async fn fail_streaming_writes_each_error_in_the_language_of_its_topic() {
        let env = env().await;
        let english = Topic::new(
            TopicId::new(Uuid::from_u128(3)),
            domain::UserId::OWNER,
            FolderId::new(Uuid::from_u128(1)),
            TopicTitle::parse("E").unwrap(),
            T0,
        )
        .with_language(ContentLanguage::En);
        env.topics.insert(&english).await.unwrap();
        let (u_it, a_it) = turn(&env, 10, 0);
        let u_en = ChatMessage::user(
            mid(20),
            english.id,
            MessageText::parse("question").unwrap(),
            T0,
        );
        let a_en = ChatMessage::assistant_streaming(mid(21), english.id, T0);
        env.chat.insert_turn(&u_it, &a_it).await.unwrap();
        env.chat.insert_turn(&u_en, &a_en).await.unwrap();

        let make = |l| MessageError::from_code(ChatErrorCode::Internal, l);
        let at = T0 + time::Duration::hours(1);
        assert_eq!(env.chat.fail_streaming(&make, at).await.unwrap(), 2);

        let stored_it = env.chat.get(env.topic, a_it.id).await.unwrap().unwrap();
        let stored_en = env.chat.get(english.id, a_en.id).await.unwrap().unwrap();
        assert_eq!(stored_it.error.unwrap().message, "Errore interno.");
        assert_eq!(stored_en.error.unwrap().message, "Internal error.");
    }

    #[tokio::test]
    async fn deleting_the_topic_cascades_to_messages() {
        let env = env().await;
        let (u, a) = turn(&env, 10, 0);
        env.chat.insert_turn(&u, &a).await.unwrap();
        assert!(
            env.topics
                .delete(domain::UserId::OWNER, env.topic)
                .await
                .unwrap()
        );
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM chat_messages")
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        // A late write from a still-running generation finds nothing.
        assert!(matches!(
            env.chat.save(&a).await,
            Err(RepositoryError::NotFound)
        ));
    }

    #[tokio::test]
    async fn state_transition_applies_only_from_the_expected_state() {
        let env = env().await;
        let at = T0 + time::Duration::seconds(5);
        assert!(
            env.topics
                .transition_state(env.topic, TopicState::New, TopicState::Interviewing, at)
                .await
                .unwrap()
        );
        assert!(
            !env.topics
                .transition_state(env.topic, TopicState::New, TopicState::Interviewing, at)
                .await
                .unwrap()
        );
        let topic = env
            .topics
            .get(domain::UserId::OWNER, env.topic)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (topic.state, topic.updated_at),
            (TopicState::Interviewing, at)
        );
    }
}
