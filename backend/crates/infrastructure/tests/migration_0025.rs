//! Migration `0025` on a database at schema `0024` with glossary
//! terms: the origin of a never-revised term is backfilled, the origin of a
//! revised one is unknown and stays NULL.

use infrastructure::sqlite::{self, Pool};
use sqlx::Row;
use uuid::Uuid;

const OWNER: &str = "00000000-0000-7000-8000-000000000001";
const T: &str = "2026-11-09T09:00:00.000Z";
const LATER: &str = "2026-11-10T09:00:00.000Z";

async fn exec(pool: &Pool, sql: &'static str, binds: &[&str]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

async fn database_at_0024(path: &std::path::Path) -> Pool {
    let pool = sqlite::connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let mut m = sqlx::migrate!("../../migrations");
    let all = m.migrations.clone().into_owned();
    m.migrations = all.iter().filter(|x| x.version <= 24).cloned().collect();
    m.run(&pool).await.unwrap();

    exec(
        &pool,
        "INSERT INTO folders (id, user_id, name, name_key, position, created_at, updated_at) VALUES ('f1', ?, 'Lavoro', 'lavoro', 0, ?, ?)",
        &[OWNER, T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO topics (id, user_id, folder_id, title, title_key, state, created_at, updated_at) VALUES ('t1', ?, 'f1', 'Rust', 'rust', 'ready', ?, ?)",
        &[OWNER, T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, mission_version, document, plan, generation_job_id, created_at) VALUES ('l1', 't1', 1, 'L', 'o', 5, 1, 1, '{}', '{}', 'gj1', ?)",
        &[T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, catalog_version, mission_version, document, plan, generation_job_id, created_at) VALUES ('l2', 't1', 2, 'L', 'o', 5, 1, 1, '{}', '{}', 'gj2', ?)",
        &[T],
    )
    .await;
    // `fresh` was never revised; `revised` was created by l1 and revised by l2.
    exec(
        &pool,
        "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, lesson_id, created_at, updated_at) VALUES ('g1', 't1', 'fresh', 'Fresh', 'fresh', 'd', '[]', 'l1', ?, ?)",
        &[T, T],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, lesson_id, created_at, updated_at) VALUES ('g2', 't1', 'revised', 'Revised', 'revised', 'd', '[]', 'l2', ?, ?)",
        &[T, LATER],
    )
    .await;
    pool
}

#[tokio::test]
async fn first_lesson_is_backfilled_only_for_terms_never_revised() {
    let dir = std::env::temp_dir().join(format!("teach_me_m25_{}", Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let pool = database_at_0024(&dir.join("t.db")).await;

    sqlite::migrate(&pool).await.unwrap();

    let rows = sqlx::query("SELECT id, lesson_id, first_lesson_id FROM glossary_terms ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    let got: Vec<(String, String, Option<String>)> = rows
        .iter()
        .map(|r| (r.get("id"), r.get("lesson_id"), r.get("first_lesson_id")))
        .collect();
    assert_eq!(
        got,
        vec![
            ("g1".into(), "l1".into(), Some("l1".into())),
            ("g2".into(), "l2".into(), None),
        ]
    );
    let (fk,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fk, 0);
}
