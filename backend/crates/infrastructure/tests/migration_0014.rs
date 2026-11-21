//! Migration `0014` on a database at schema `0013` with real data:
//! nothing is lost, everything becomes the initial `owner`'s, and the
//! cascades and the new per-user rules work afterwards.

use infrastructure::sqlite::{self, Pool};
use sqlx::Row;

const OWNER: &str = "00000000-0000-7000-8000-000000000001";
const T: &str = "2026-11-09T09:00:00.000Z";

/// Every table the migration rebuilds or that points at what it rebuilds.
const TABLES: [&str; 18] = [
    "folders",
    "topics",
    "preferences",
    "chat_messages",
    "missions",
    "jobs",
    "resources",
    "research_gaps",
    "topic_research",
    "lessons",
    "component_requests",
    "attempts",
    "learning_records",
    "glossary_terms",
    "review_cards",
    "lesson_completions",
    "documents",
    "document_pages",
];

async fn count(pool: &Pool, table: &str) -> i64 {
    // `table` is one of the constants above, never input.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn exec(pool: &Pool, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

/// A database as the previous backend left it, with data in every table.
#[allow(clippy::too_many_lines)] // a list of inserts, one per table
async fn database_at_0013(path: &std::path::Path) -> Pool {
    let pool = sqlite::connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let mut m = sqlx::migrate!("../../migrations");
    let all = m.migrations.clone().into_owned();
    m.migrations = all.iter().filter(|x| x.version <= 13).cloned().collect();
    m.run(&pool).await.unwrap();

    for (id, name, key, pos) in [
        ("f1", "Lavoro", "lavoro", 0),
        ("f2", "Èconomia", "èconomia", 1),
    ] {
        sqlx::query(
            "INSERT INTO folders (id, name, name_key, position, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(key)
        .bind(pos)
        .bind(T)
        .bind(T)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (id, folder, title, state) in [
        ("t1", "f1", "Rust", "ready"),
        ("t2", "f1", "SQL", "new"),
        ("t3", "f2", "Inflazione", "interviewing"),
    ] {
        sqlx::query(
            "INSERT INTO topics (id, folder_id, title, title_key, state, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(folder)
        .bind(title)
        .bind(title.to_lowercase())
        .bind(state)
        .bind(T)
        .bind(T)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Global and per-topic preferences.
    exec(
        &pool,
        "INSERT INTO preferences (id, scope, topic_id, text, text_key, created_at) VALUES ('p1', 'global', NULL, 'Sii breve', 'sii breve', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO preferences (id, scope, topic_id, text, text_key, created_at) VALUES ('p2', 'topic', 't1', 'Esempi in Rust', 'esempi in rust', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO chat_messages (id, topic_id, role, status, parts, created_at, updated_at) VALUES ('m1', 't1', 'user', 'complete', '[]', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO missions (topic_id, version, status, why, success_criteria, mission_constraints, out_of_scope, created_at, confirmed_at) VALUES ('t1', 1, 'confirmed', 'w', '[]', '[]', '[]', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO jobs (id, topic_id, kind, status, attempt, created_at) VALUES ('j1', 't1', 'research', 'queued', 0, ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, language, annotation, use_for, created_at) VALUES ('r1', 't1', 0, 'knowledge', 'docs', 'Book', 'https://x.example', 'en', 'a', 'u', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO research_gaps (topic_id, position, area, note) VALUES ('t1', 0, 'a', 'n')",
        &[],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO topic_research (topic_id, job_id, mission_version, finished_at, communities_skipped) VALUES ('t1', 'j0', 1, ?, 0)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, mission_version, document, plan, generation_job_id, created_at) VALUES ('l1', 't1', 1, 'Lezione', 'o', 5, 1, 1, '{}', '{}', 'gj1', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO component_requests (id, topic_id, lesson_id, block_id, wanted, purpose, fallback, created_at) VALUES ('cr1', 't1', 'l1', 'b1', 'slider', 'p', 'f', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO attempts (id, lesson_id, block_id, kind, attempt_number, answer, outcome, answered_at, created_at) VALUES ('a1', 'l1', 'b1', 'quiz', 1, '{}', 'correct', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, lesson_id, created_at) VALUES ('lr1', 't1', 1, 'demonstrated', 't', 'b', 'e', 'l1', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, lesson_id, created_at, updated_at) VALUES ('g1', 't1', 'k', 'Term', 'term', 'd', '[]', 'l1', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO review_cards (id, topic_id, lesson_id, term_id, front, back, created_at, due) VALUES ('c1', 't1', 'l1', 'g1', 'f', 'b', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO lesson_completions (lesson_id, record_count, term_count, review_card_count, created_at) VALUES ('l1', 1, 1, 1, ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO documents (id, topic_id, filename, title, status, size_bytes, created_at) VALUES ('d1', 't1', 'a.pdf', 'A', 'ready', 10, ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO document_pages (document_id, page, text) VALUES ('d1', 1, 'text')",
        &[],
    )
    .await;
    pool
}

fn temp_db() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("teach_me_m14_{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    (dir.clone(), dir.join("t.db"))
}

#[tokio::test]
async fn nothing_is_lost_and_everything_becomes_the_owners() {
    let (dir, path) = temp_db();
    let pool = database_at_0013(&path).await;
    let mut before = Vec::new();
    for t in TABLES {
        before.push((t, count(&pool, t).await));
    }
    assert!(
        before.iter().all(|(_, n)| *n >= 1),
        "every table has data: {before:?}"
    );

    sqlite::migrate(&pool).await.unwrap();

    for (table, n) in &before {
        assert_eq!(count(&pool, table).await, *n, "{table}");
    }
    for table in ["folders", "topics", "preferences"] {
        let others: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE user_id <> ?"
        )))
        .bind(OWNER)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(others, 0, "{table} all belongs to owner");
    }
    // Names, titles, states and both kinds of preference survive as they were.
    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM folders ORDER BY position")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(names, ["Lavoro", "Èconomia"]);
    let topics: Vec<(String, String, String)> =
        sqlx::query("SELECT title, state, folder_id FROM topics ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect();
    assert_eq!(
        topics,
        [
            ("Rust".into(), "ready".into(), "f1".into()),
            ("SQL".into(), "new".into(), "f1".into()),
            ("Inflazione".into(), "interviewing".into(), "f2".into()),
        ]
    );
    let prefs: Vec<(String, String, Option<String>)> =
        sqlx::query("SELECT scope, text, topic_id FROM preferences ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1), r.get(2)))
            .collect();
    assert_eq!(
        prefs,
        [
            ("global".into(), "Sii breve".into(), None),
            ("topic".into(), "Esempi in Rust".into(), Some("t1".into())),
        ]
    );
    // The owner exists without a password and without a key.
    let owner =
        sqlx::query("SELECT username, password_hash, llm_key, disabled_at FROM users WHERE id = ?")
            .bind(OWNER)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner.get::<String, _>(0), "owner");
    assert_eq!(owner.get::<Option<String>, _>(1), None);
    assert_eq!(owner.get::<Option<Vec<u8>>, _>(2), None);
    assert_eq!(owner.get::<Option<String>, _>(3), None);
    assert_eq!(count_users(&pool).await, 1);

    // No dangling reference, foreign keys are back on, a second run changes nothing.
    let dangling = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(dangling.is_empty());
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fk, 1);
    sqlite::migrate(&pool).await.unwrap();
    assert_eq!(count(&pool, "topics").await, 3);

    pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

async fn count_users(pool: &Pool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn cascades_still_work_and_rules_are_now_per_user() {
    let (dir, path) = temp_db();
    let pool = database_at_0013(&path).await;
    sqlite::migrate(&pool).await.unwrap();
    let other = "00000000-0000-7000-8000-0000000000b2";
    exec(
        &pool,
        "INSERT INTO users (id, username, display_name, created_at) VALUES ('00000000-0000-7000-8000-0000000000b2', 'other', 'Other', ?)",
        &[T],
    )
    .await;

    // The same folder name for another user is fine, for the same user it is not.
    let insert = |id: &'static str, user: &'static str, name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES (?, ?, ?, ?, 0, ?, ?)",
            )
            .bind(id)
            .bind(user)
            .bind(name)
            .bind(name.to_lowercase())
            .bind(T)
            .bind(T)
            .execute(&pool)
            .await
        }
    };
    insert("f3", other, "Lavoro").await.unwrap();
    assert!(insert("f4", OWNER, "Lavoro").await.is_err());

    // A topic cannot sit in a folder of another user, nor a topic preference
    // in a topic of another user.
    let bad_topic = sqlx::query(
        "INSERT INTO topics (id, user_id, folder_id, title, title_key, created_at, updated_at) VALUES ('t9', ?, 'f1', 'x', 'x', ?, ?)",
    )
    .bind(other)
    .bind(T)
    .bind(T)
    .execute(&pool)
    .await;
    assert!(bad_topic.is_err());
    let bad_pref = sqlx::query(
        "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) VALUES ('p9', ?, 'topic', 't1', 'x', 'x', ?)",
    )
    .bind(other)
    .bind(T)
    .execute(&pool)
    .await;
    assert!(bad_pref.is_err());
    // Global preferences with the same text: one per user.
    for (id, user) in [("p5", other), ("p6", OWNER)] {
        let r = sqlx::query(
            "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) VALUES (?, ?, 'global', NULL, 'Sii breve', 'sii breve', ?)",
        )
        .bind(id)
        .bind(user)
        .bind(T)
        .execute(&pool)
        .await;
        assert_eq!(r.is_ok(), user == other, "{user}");
    }

    // Deleting a topic still removes what hangs below it...
    exec(&pool, "DELETE FROM topics WHERE id = 't1'", &[]).await;
    for t in [
        "lessons",
        "review_cards",
        "chat_messages",
        "documents",
        "document_pages",
        "jobs",
        "attempts",
    ] {
        assert_eq!(count(&pool, t).await, 0, "{t}");
    }
    // ...and deleting a user removes their folders, topics and preferences.
    exec(
        &pool,
        "DELETE FROM users WHERE id = '00000000-0000-7000-8000-000000000001'",
        &[],
    )
    .await;
    for t in ["folders", "topics"] {
        let left: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {t} WHERE user_id = '00000000-0000-7000-8000-000000000001'"
        )))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0, "{t}");
    }
    assert_eq!(
        count(&pool, "folders").await,
        1,
        "the other user's folder stays"
    );

    pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}
