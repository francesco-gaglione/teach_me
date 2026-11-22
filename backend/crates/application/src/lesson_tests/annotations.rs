use super::*;

// ----------------------------------------------------------- highlights

fn highlight_doc() -> Value {
    json!({"catalog_version": 1, "title": "Stop", "objective": "Saper compensare.",
    "estimated_minutes": 8, "blocks": [
        {"id": "h1", "type": "heading", "level": 1, "text": "Stop"},
        {"id": "p1", "type": "paragraph", "text": "Uno stop raddoppia la luce."},
        {"id": "c1", "type": "callout", "variant": "info", "title": "Nota", "text": "Occhio a 😀 qui."}
    ]})
}

fn hl(block: &str, start: u32, end: u32, color: &str) -> Value {
    json!({"block_id": block, "start": start, "end": end, "color": color})
}

fn highlights(env: &LessonEnv) -> crate::HighlightService {
    crate::HighlightService::new(env.f.repo.clone(), env.f.repo.clone())
}

fn as_json(stored: &[domain::Highlight]) -> Value {
    Value::Array(
        stored
            .iter()
            .map(|h| hl(&h.block_id, h.start, h.end, h.color.as_str()))
            .collect(),
    )
}

#[tokio::test]
async fn highlights_replace_the_set_in_document_order_and_resending_changes_nothing() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = highlights(&env);
    let user = domain::UserId::OWNER;

    // Sent out of order, one touching another; the callout's text has a
    // character outside the BMP (2 UTF-16 units): "Occhio a 😀 qui." is 16.
    let body = json!({"highlights": [
        hl("c1", 10, 16, "pink"),
        hl("p1", 4, 8, "green"),
        hl("p1", 0, 4, "yellow"),
    ]});
    let first = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(
        as_json(&first),
        json!([
            hl("p1", 0, 4, "yellow"),
            hl("p1", 4, 8, "green"),
            hl("c1", 10, 16, "pink"),
        ])
    );
    let view = env.lessons().view(user, lesson.id).await.unwrap();
    assert_eq!(view.highlights, first, "the view uses the same order");

    let again = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(again, first);
    assert_eq!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights,
        first
    );

    // A different set replaces, it does not add.
    let second = svc
        .replace(
            user,
            lesson.id,
            &json!({"highlights": [hl("p1", 5, 9, "blue")]}),
        )
        .await
        .unwrap();
    assert_eq!(as_json(&second), json!([hl("p1", 5, 9, "blue")]));
    assert_eq!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights,
        second
    );

    // An empty set clears.
    assert!(
        svc.replace(user, lesson.id, &json!({"highlights": []}))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .highlights
            .is_empty()
    );
}

#[tokio::test]
async fn a_rejected_set_changes_nothing_and_reports_the_contract_fields() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = highlights(&env);
    let user = domain::UserId::OWNER;
    let good = json!({"highlights": [hl("p1", 0, 4, "yellow")]});
    svc.replace(user, lesson.id, &good).await.unwrap();

    let fields = |r: Result<Vec<domain::Highlight>, AppError>| match r {
        Err(AppError::InvalidFields(f)) => f
            .into_iter()
            .map(|i| (i.field, i.issue.as_str()))
            .collect::<Vec<_>>(),
        other => panic!("expected InvalidFields, got {other:?}"),
    };
    let cases = [
        (json!({}), "highlights", "invalid"),
        (json!({"highlights": "x"}), "highlights", "invalid"),
        (
            json!({"highlights": [hl("zz", 0, 1, "yellow")]}),
            "highlights[0].block_id",
            "invalid",
        ),
        (
            json!({"highlights": [hl("h1", 0, 1, "yellow")]}),
            "highlights[0].block_id",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 1, "red")]}),
            "highlights[0].color",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 99, "blue")]}),
            "highlights[0].end",
            "out_of_range",
        ),
        (
            json!({"highlights": [hl("p1", 3, 3, "blue")]}),
            "highlights[0].start",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": 0, "end": u64::MAX, "color": "blue"}]}),
            "highlights[0].end",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": -1e30, "end": 3, "color": "blue"}]}),
            "highlights[0].start",
            "out_of_range",
        ),
        (
            json!({"highlights": [{"block_id": "p1", "start": 1.5, "end": 3, "color": "blue"}]}),
            "highlights[0].start",
            "invalid",
        ),
        (
            json!({"highlights": [hl("p1", 0, 5, "blue"), hl("p1", 4, 8, "blue")]}),
            "highlights[1].start",
            "overlap",
        ),
    ];
    for (body, field, issue) in cases {
        assert_eq!(
            fields(svc.replace(user, lesson.id, &body).await),
            [(field.to_owned(), issue)],
            "{body}"
        );
    }
    // 501 entries (the limit is on the count, checked first).
    let many: Vec<Value> = (0..501).map(|_| hl("p1", 0, 1, "yellow")).collect();
    assert_eq!(
        fields(
            svc.replace(user, lesson.id, &json!({"highlights": many}))
                .await
        ),
        [("highlights".to_owned(), "too_many")]
    );
    // One bad entry among good ones rejects the lot.
    let mixed = json!({"highlights": [hl("p1", 0, 4, "green"), hl("p1", 0, 99, "green")]});
    assert!(svc.replace(user, lesson.id, &mixed).await.is_err());

    assert_eq!(
        as_json(
            &env.lessons()
                .view(user, lesson.id)
                .await
                .unwrap()
                .highlights
        ),
        json!([hl("p1", 0, 4, "yellow")]),
        "the stored set is untouched"
    );
}

#[tokio::test]
async fn highlights_are_accepted_on_a_completed_lesson_and_do_not_change_it() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let svc = highlights(&env);
    let stored = svc
        .replace(
            domain::UserId::OWNER,
            lesson.id,
            &json!({"highlights": [hl("p1", 0, 4, "yellow")]}),
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    let after = env.lesson(lesson.id);
    assert_eq!(after.status, LessonStatus::Completed);
    assert!(after.completed_at.is_some());
}

#[tokio::test]
async fn highlights_of_a_missing_lesson_are_not_found() {
    let env = LessonEnv::new(vec![]).await;
    let r = highlights(&env)
        .replace(
            domain::UserId::OWNER,
            LessonId::new(Uuid::from_u128(0x999)),
            &json!({"highlights": []}),
        )
        .await;
    assert!(matches!(r, Err(AppError::NotFound)));
}

// -------------------------------------------------------------- strokes

fn stroke_json(id: u128, block: &str, points: &[i64]) -> Value {
    json!({"id": Uuid::from_u128(id).to_string(), "block_id": block, "color": "blue",
           "width": "thin", "points": points})
}

fn strokes(env: &LessonEnv) -> crate::StrokeService {
    crate::StrokeService::new(env.f.repo.clone(), env.f.repo.clone())
}

#[tokio::test]
async fn strokes_replace_the_set_keep_the_drawing_order_and_resending_changes_nothing() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = strokes(&env);
    let user = domain::UserId::OWNER;

    // Not in document order: the last block first.
    let body = json!({"strokes": [
        stroke_json(2, "c1", &[10, 20, 30, 40]),
        stroke_json(1, "p1", &[5, 6]),
    ]});
    let first = svc.replace(user, lesson.id, &body).await.unwrap();
    let ids: Vec<_> = first.iter().map(|s| s.block_id.as_str()).collect();
    assert_eq!(ids, ["c1", "p1"]);
    assert_eq!(first[0].points, [10, 20, 30, 40]);
    let view = env.lessons().view(user, lesson.id).await.unwrap();
    assert_eq!(view.strokes, first, "the view uses the same order");

    let again = svc.replace(user, lesson.id, &body).await.unwrap();
    assert_eq!(again, first);

    // A smaller set replaces; an empty one clears.
    let one = json!({"strokes": [stroke_json(3, "h1", &[1, 1])]});
    assert_eq!(svc.replace(user, lesson.id, &one).await.unwrap().len(), 1);
    svc.replace(user, lesson.id, &json!({"strokes": []}))
        .await
        .unwrap();
    assert!(
        env.lessons()
            .view(user, lesson.id)
            .await
            .unwrap()
            .strokes
            .is_empty()
    );
}

#[tokio::test]
async fn a_rejected_stroke_set_changes_nothing_and_reports_the_contract_fields() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    let svc = strokes(&env);
    let user = domain::UserId::OWNER;
    let good = json!({"strokes": [stroke_json(1, "p1", &[1, 2])]});
    let stored = svc.replace(user, lesson.id, &good).await.unwrap();

    let bad = json!({"strokes": [stroke_json(1, "p1", &[1, 2]), stroke_json(1, "zz", &[1, 2, 3])]});
    let fields = match svc.replace(user, lesson.id, &bad).await {
        Err(AppError::InvalidFields(f)) => f
            .iter()
            .map(|i| (i.field.clone(), i.issue.as_str()))
            .collect::<Vec<_>>(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        fields,
        [
            ("strokes[1].id".to_owned(), "duplicate"),
            ("strokes[1].block_id".to_owned(), "invalid"),
            ("strokes[1].points".to_owned(), "invalid"),
        ]
    );
    assert_eq!(
        env.lessons().view(user, lesson.id).await.unwrap().strokes,
        stored
    );
}

#[tokio::test]
async fn strokes_are_accepted_on_a_completed_lesson_and_do_not_change_it() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(highlight_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let stored = strokes(&env)
        .replace(
            domain::UserId::OWNER,
            lesson.id,
            &json!({"strokes": [stroke_json(1, "p1", &[1, 2])]}),
        )
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    let after = env.lesson(lesson.id);
    assert_eq!(after.status, LessonStatus::Completed);
    assert!(after.completed_at.is_some());
}

#[tokio::test]
async fn strokes_of_a_missing_lesson_are_not_found() {
    let env = LessonEnv::new(vec![]).await;
    let r = strokes(&env)
        .replace(
            domain::UserId::OWNER,
            LessonId::new(Uuid::from_u128(0x999)),
            &json!({"strokes": []}),
        )
        .await;
    assert!(matches!(r, Err(AppError::NotFound)));
}
