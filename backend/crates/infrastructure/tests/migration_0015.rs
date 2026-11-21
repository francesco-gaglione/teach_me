//! Migration `0015` (phase 12, block B) on a database at schema `0014` with
//! data: purely additive. Nothing is rewritten, old missions read back with no
//! starting point, and new drafts can carry one.

use domain::{ChatRepository, MessageId, MessagePart, MissionContent, MissionRepository, TopicId};
use infrastructure::sqlite::{self, Pool};
use infrastructure::{SqliteChatRepository, SqliteMissionRepository};
use sqlx::Row;
use time::macros::datetime;
use uuid::Uuid;

const OWNER: &str = "00000000-0000-7000-8000-000000000001";
const T: &str = "2026-11-09T09:00:00.000Z";
const TOPIC: &str = "00000000-0000-7000-8000-0000000000a1";
const MESSAGE: &str = "00000000-0000-7000-8000-0000000000b1";

/// A mission snapshot as a Phase 11 backend saved it: no `starting_point` key.
fn old_snapshot() -> String {
    serde_json::json!([{
        "type": "mission_proposal",
        "replaces_version": null,
        "mission": {
            "topic_id": TOPIC, "version": 1, "status": "confirmed",
            "why": "Per lavoro", "success_criteria": ["So fare X"],
            "constraints": [], "out_of_scope": [],
            "created_at": T, "confirmed_at": T, "superseded_at": null
        }
    }])
    .to_string()
}

async fn exec(pool: &Pool, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

/// A database as the Phase 11 backend left it (schema `0014`), with a topic
/// that has a confirmed mission, an older superseded one and a chat message.
async fn database_at_0014(path: &std::path::Path) -> Pool {
    let pool = sqlite::connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let mut m = sqlx::migrate!("../../migrations");
    let all = m.migrations.clone().into_owned();
    m.migrations = all.iter().filter(|x| x.version <= 14).cloned().collect();
    m.run(&pool).await.unwrap();

    exec(
        &pool,
        "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES ('f1', ?, 'Lavoro', 'lavoro', 0, ?, ?)",
        &[OWNER, T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO topics (id, user_id, folder_id, title, title_key, state, created_at, updated_at) VALUES (?, ?, 'f1', 'Rust', 'rust', 'ready', ?, ?)",
        &[TOPIC, OWNER, T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, out_of_scope, created_at, confirmed_at, superseded_at) VALUES (?, 1, 'superseded', 'Prima', '[\"a\"]', '[]', '[]', ?, ?, ?)",
        &[TOPIC, T, T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, out_of_scope, created_at, confirmed_at) VALUES (?, 2, 'confirmed', 'Per lavoro', '[\"So fare X\"]', '[\"30 minuti\"]', '[\"embedded\"]', ?, ?)",
        &[TOPIC, T, T],
    )
    .await;
    let snapshot = old_snapshot();
    sqlx::query(
        "INSERT INTO chat_messages (id, topic_id, role, status, parts, created_at, updated_at) VALUES (?, ?, 'assistant', 'complete', ?, ?, ?)",
    )
    .bind(MESSAGE)
    .bind(TOPIC)
    .bind(&snapshot)
    .bind(T)
    .bind(T)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// Every column of every mission row, as text, to compare before and after.
async fn mission_dump(pool: &Pool) -> Vec<String> {
    sqlx::query(
        "SELECT topic_id || '|' || version || '|' || status || '|' || why || '|' || success_criteria \
         || '|' || mission_constraints || '|' || out_of_scope || '|' || created_at \
         || '|' || COALESCE(confirmed_at, '-') || '|' || COALESCE(superseded_at, '-') AS row \
         FROM missions ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .iter()
    .map(|r| r.get::<String, _>("row"))
    .collect()
}

#[tokio::test]
async fn the_column_is_added_and_nothing_existing_is_rewritten() {
    let dir = std::env::temp_dir().join(format!("teach_me_m15_{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let pool = database_at_0014(&dir.join("t.db")).await;
    let before = mission_dump(&pool).await;
    assert_eq!(before.len(), 2);

    sqlite::migrate(&pool).await.unwrap();

    // Same rows, byte for byte; the new column is NULL everywhere.
    assert_eq!(mission_dump(&pool).await, before);
    let filled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM missions WHERE starting_point IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(filled, 0);
    let parts: String = sqlx::query_scalar("SELECT parts FROM chat_messages WHERE id = ?")
        .bind(MESSAGE)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        parts,
        old_snapshot(),
        "saved chat messages are not rewritten"
    );
    let (fk_violations,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fk_violations, 0);

    // The repositories read the old data: no starting point, still valid.
    let topic = TopicId::new(Uuid::parse_str(TOPIC).unwrap());
    let missions = SqliteMissionRepository::new(pool.clone());
    let current = missions.current(topic).await.unwrap().unwrap();
    assert_eq!(current.version, 2);
    assert_eq!(current.content.starting_point(), None);
    assert_eq!(current.content.constraints(), ["30 minuti"]);
    let chat = SqliteChatRepository::new(pool.clone());
    let message = chat
        .get(topic, MessageId::new(Uuid::parse_str(MESSAGE).unwrap()))
        .await
        .unwrap()
        .unwrap();
    let MessagePart::MissionProposal { mission, .. } = &message.parts[0] else {
        panic!("expected the mission card");
    };
    assert_eq!(mission.content.starting_point(), None);

    // A new draft stores and returns its starting point.
    let content = MissionContent::parse(
        "Nuova",
        "Kotlin sì, Rust no",
        &["So fare Y".to_owned()],
        &[],
        &[],
    )
    .unwrap();
    let draft = missions
        .insert_draft(topic, &content, datetime!(2026-11-11 10:00 UTC), None)
        .await
        .unwrap();
    assert_eq!(draft.version, 3);
    let read = missions.open_draft(topic).await.unwrap().unwrap();
    assert_eq!(read.content.starting_point(), Some("Kotlin sì, Rust no"));
    // The confirmed version is still the old one, untouched.
    assert_eq!(
        missions
            .current(topic)
            .await
            .unwrap()
            .unwrap()
            .content
            .starting_point(),
        None
    );

    drop(pool);
    std::fs::remove_dir_all(dir).ok();
}
