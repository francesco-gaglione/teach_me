use super::*;

fn hl(block: &str, start: u32, end: u32, color: HighlightColor) -> Highlight {
    Highlight {
        block_id: block.into(),
        start,
        end,
        color,
    }
}

#[tokio::test]
async fn highlights_are_replaced_whole_and_kept_in_the_order_given() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let other = lesson(&env, 2).await;
    let repo = SqliteHighlightRepository::new(env.pool.clone());
    assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());

    let set = vec![
        hl("p2", 5, 9, HighlightColor::Pink),
        hl("p1", 0, 3, HighlightColor::Yellow),
        hl("p1", 3, 4, HighlightColor::Blue),
    ];
    repo.replace(one.id, &set).await.unwrap();
    repo.replace(other.id, &[hl("p1", 0, 1, HighlightColor::Green)])
        .await
        .unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), set);

    // Sending the same set again, then a smaller one, then none.
    repo.replace(one.id, &set).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), set);
    repo.replace(one.id, &set[..1]).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), &set[..1]);
    repo.replace(one.id, &[]).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());
    assert_eq!(
        repo.list(other.id).await.unwrap().len(),
        1,
        "other lessons are left alone"
    );
}

fn st(id: u128, block: &str, points: &[i32]) -> Stroke {
    Stroke {
        id: Uuid::from_u128(id).to_string(),
        block_id: block.into(),
        color: StrokeColor::Red,
        width: StrokeWidth::Thick,
        points: points.to_vec(),
    }
}

#[tokio::test]
async fn strokes_are_replaced_whole_and_kept_in_the_order_given() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let other = lesson(&env, 2).await;
    let repo = SqliteStrokeRepository::new(env.pool.clone());
    assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());

    // Negative values and the extremes of the contract's ranges survive.
    let set = vec![
        st(3, "p2", &[-40_000, 200_000, 50_000, -40_000]),
        st(1, "p1", &[0, 0]),
        st(2, "p1", &[7, 8, 9, 10, 11, 12]),
    ];
    repo.replace(one.id, &set).await.unwrap();
    repo.replace(other.id, &[st(1, "p1", &[1, 1])])
        .await
        .unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), set);

    repo.replace(one.id, &set).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), set);
    repo.replace(one.id, &set[..1]).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), &set[..1]);
    repo.replace(one.id, &[]).await.unwrap();
    assert_eq!(repo.list(one.id).await.unwrap(), Vec::new());
    assert_eq!(repo.list(other.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn strokes_of_a_missing_lesson_are_not_found_and_go_with_their_lesson() {
    let env = env().await;
    let repo = SqliteStrokeRepository::new(env.pool.clone());
    let r = repo.replace(LessonId::new(uid(0x999)), &[]).await;
    assert!(matches!(r, Err(RepositoryError::NotFound)));

    let one = lesson(&env, 1).await;
    repo.replace(one.id, &[st(1, "p1", &[1, 2])]).await.unwrap();
    sqlx::query("DELETE FROM lessons WHERE id = ?")
        .bind(one.id.to_string())
        .execute(&env.pool)
        .await
        .unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM strokes")
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn a_damaged_points_blob_is_reported_not_served() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let repo = SqliteStrokeRepository::new(env.pool.clone());
    repo.replace(one.id, &[st(1, "p1", &[1, 2])]).await.unwrap();
    // The CHECK refuses a length that is not a whole number of points.
    let r = sqlx::query("UPDATE strokes SET points = x'0102030405'")
        .execute(&env.pool)
        .await;
    assert!(r.is_err());
    // No blob that passes the CHECK is refused by `unpack_points` (they
    // accept the same lengths), so the guard is tested directly.
    for len in [0usize, 1, 4, 7, 9, 12, 15] {
        let r = unpack_points(&vec![0u8; len], "l");
        assert!(
            matches!(&r, Err(RepositoryError::Storage(e)) if e.to_string().contains("corrupt stroke points")),
            "{len}"
        );
    }
    assert_eq!(
        unpack_points(&[1, 0, 0, 0, 2, 0, 0, 0], "l").unwrap(),
        [1, 2]
    );
}

#[tokio::test]
async fn replacing_the_highlights_of_a_missing_lesson_is_not_found() {
    let env = env().await;
    let repo = SqliteHighlightRepository::new(env.pool.clone());
    let r = repo.replace(LessonId::new(uid(0x999)), &[]).await;
    assert!(matches!(r, Err(RepositoryError::NotFound)));
}

#[tokio::test]
async fn highlights_go_with_their_lesson_and_with_their_topic() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let repo = SqliteHighlightRepository::new(env.pool.clone());
    repo.replace(one.id, &[hl("p1", 0, 3, HighlightColor::Yellow)])
        .await
        .unwrap();
    sqlx::query("DELETE FROM lessons WHERE id = ?")
        .bind(one.id.to_string())
        .execute(&env.pool)
        .await
        .unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM highlights")
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "deleting the lesson");

    let two = lesson(&env, 2).await;
    repo.replace(two.id, &[hl("p1", 0, 3, HighlightColor::Yellow)])
        .await
        .unwrap();
    sqlx::query("DELETE FROM topics WHERE id = ?")
        .bind(env.topic.to_string())
        .execute(&env.pool)
        .await
        .unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM highlights")
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "deleting the topic");
}

#[tokio::test]
async fn two_replaces_at_once_end_with_one_of_the_two_sets_whole() {
    // A file database with several connections: in memory there is one.
    let path = std::env::temp_dir().join(format!("teach_me_hl_{}.db", uid(0).simple()));
    let _ = std::fs::remove_file(&path);
    let pool = connect(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let lessons = SqliteLessonRepository::new(pool.clone());
    let repo = SqliteHighlightRepository::new(pool.clone());
    let folder = FolderId::new(uid(1));
    SqliteFolderRepository::new(pool.clone())
        .insert(NewFolder {
            user_id: domain::UserId::OWNER,
            id: folder,
            name: FolderName::parse("F").unwrap(),
            created_at: T0,
        })
        .await
        .unwrap();
    let topic = Topic::new(
        TopicId::new(uid(2)),
        domain::UserId::OWNER,
        folder,
        TopicTitle::parse("T").unwrap(),
        T0,
    );
    SqliteTopicRepository::new(pool.clone())
        .insert(&topic)
        .await
        .unwrap();
    let new = GeneratedLesson {
        topic_id: topic.id,
        ..generated(
            &Env {
                lessons: lessons.clone(),
                attempts: SqliteAttemptRepository::new(pool.clone()),
                learning: SqliteLearningRepository::new(pool.clone()),
                jobs: SqliteJobRepository::new(pool.clone()),
                topic: topic.id,
                pool: pool.clone(),
            },
            1,
        )
    };
    let lesson = lessons.insert_generated(&new).await.unwrap();

    let a: Vec<Highlight> = (0..40)
        .map(|i| hl("p1", i, i + 1, HighlightColor::Yellow))
        .collect();
    let b: Vec<Highlight> = (0..30)
        .map(|i| hl("p2", i, i + 1, HighlightColor::Pink))
        .collect();
    for _ in 0..10 {
        let (ra, rb) = tokio::join!(repo.replace(lesson.id, &a), repo.replace(lesson.id, &b));
        ra.unwrap();
        rb.unwrap();
        let stored = repo.list(lesson.id).await.unwrap();
        assert!(stored == a || stored == b, "a mix of the two sets");
    }
    pool.close().await;
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
}
