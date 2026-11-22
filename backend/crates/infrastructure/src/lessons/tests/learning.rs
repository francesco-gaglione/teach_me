use super::*;

// ----------------------------------------------------------- learning

fn completion(env: &Env, l: &Lesson) -> CompletionSave {
    CompletionSave {
        topic_id: env.topic,
        lesson_id: l.id,
        at: T0,
        records: vec![NewRecord {
            id: LearningRecordId::new(uid(0x10)),
            kind: RecordKind::Demonstrated,
            title: "Compensare".into(),
            body: "Hai compensato.".into(),
            evidence: "Hai risposto bene.".into(),
            supersedes: vec![],
        }],
        terms: vec![TermUpsert {
            id: GlossaryTermId::new(uid(0x20)),
            key: "stop".into(),
            term: "Stop".into(),
            definition: "Unità di luce.".into(),
            avoid: vec!["step".into()],
        }],
        cards: vec![
            NewCard {
                id: ReviewCardId::new(uid(0x30)),
                target: CardTarget::Record(0),
                front: "f1".into(),
                back: "b1".into(),
                unpassed_block: None,
            },
            NewCard {
                id: ReviewCardId::new(uid(0x31)),
                target: CardTarget::Term(0),
                front: "f2".into(),
                back: "b2".into(),
                unpassed_block: None,
            },
        ],
    }
}

async fn counts(env: &Env) -> (i64, i64, i64, i64) {
    let n = |table: &'static str| {
        let pool = env.pool.clone();
        async move {
            let q = match table {
                "records" => "SELECT COUNT(*) FROM learning_records",
                "terms" => "SELECT COUNT(*) FROM glossary_terms",
                "cards" => "SELECT COUNT(*) FROM review_cards",
                _ => "SELECT COUNT(*) FROM lesson_completions",
            };
            sqlx::query_scalar::<_, i64>(q)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    (
        n("records").await,
        n("terms").await,
        n("cards").await,
        n("completions").await,
    )
}

#[tokio::test]
async fn a_completion_stores_records_terms_and_cards_and_is_idempotent() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let summary = env
        .learning
        .save_completion(&completion(&env, &l))
        .await
        .unwrap();
    assert_eq!(
        (
            summary.record_count,
            summary.term_count,
            summary.review_card_count
        ),
        (1, 1, 2)
    );
    assert_eq!(
        env.learning.completion_summary(l.id).await.unwrap(),
        Some(summary)
    );
    assert_eq!(counts(&env).await, (1, 1, 2, 1));

    let records = env
        .learning
        .list_records(env.topic, true, None, 10)
        .await
        .unwrap();
    assert_eq!(
        (records[0].number, records[0].status, records[0].lesson_id),
        (1, RecordStatus::Active, l.id)
    );
    let cards = env.learning.review_cards(env.topic).await.unwrap();
    assert_eq!(cards.len(), 2);
    assert!(
        matches!(cards[0].origin, CardOrigin::Record(_))
            && matches!(cards[1].origin, CardOrigin::Term(_))
    );
    // Born new, due a day after the lesson was closed.
    for card in &cards {
        assert_eq!(card.schedule, CardSchedule::initial(T0));
        assert_eq!(card.schedule.due, T0 + time::Duration::hours(24));
    }
    // New cards are born on the current model, not the column's default.
    let models: Vec<String> = sqlx::query_scalar("SELECT model FROM review_cards")
        .fetch_all(&env.pool)
        .await
        .unwrap();
    assert_eq!(models, ["fsrs7", "fsrs7"]);
    let outputs = env.learning.lesson_outputs(l.id).await.unwrap();
    assert_eq!(
        (
            outputs.records.len(),
            outputs.terms.len(),
            outputs.review_card_count
        ),
        (1, 1, 2)
    );
    assert_eq!(outputs.terms[0].avoid, ["step"]);

    // Recorded once: the same lesson again is refused and changes nothing.
    assert!(matches!(
        env.learning.save_completion(&completion(&env, &l)).await,
        Err(RepositoryError::Conflict)
    ));
    assert_eq!(counts(&env).await, (1, 1, 2, 1));
}

#[tokio::test]
async fn the_unpassed_block_is_saved_with_the_card_and_read_back_as_to_consolidate() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let mut save = completion(&env, &l);
    save.cards[0].unpassed_block = Some("q1".into());
    env.learning.save_completion(&save).await.unwrap();

    let stored: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT front, unpassed_block FROM review_cards ORDER BY front")
            .fetch_all(&env.pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        [
            ("f1".to_owned(), Some("q1".to_owned())),
            ("f2".to_owned(), None)
        ]
    );
    let cards = env.learning.review_cards(l.topic_id).await.unwrap();
    let flag = |front: &str| {
        cards
            .iter()
            .find(|c| c.front == front)
            .unwrap()
            .to_consolidate
    };
    assert!(flag("f1"));
    assert!(!flag("f2"));
}

#[tokio::test]
async fn votes_are_read_in_application_order_even_with_the_same_created_at() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let mut save = completion(&env, &l);
    save.cards[0].unpassed_block = Some("q1".into());
    env.learning.save_completion(&save).await.unwrap();
    let card = save.cards[0].id.to_string();
    // good, again, good: the series is broken, so the card stays to
    // consolidate. Ids are descending so only `rowid` gives the order.
    for same_instant in [false, true] {
        sqlx::query("DELETE FROM review_logs")
            .execute(&env.pool)
            .await
            .unwrap();
        for (n, rating) in ["good", "again", "good"].into_iter().enumerate() {
            let at = if same_instant {
                "2026-11-09T10:00:00.000Z".to_owned()
            } else {
                format!("2026-11-09T10:0{n}:00.000Z")
            };
            sqlx::query(
                "INSERT INTO review_logs (id, card_id, rating, reviewed_at, state_before, \
                 state_after, created_at) VALUES (?, ?, ?, ?, '{}', '{}', ?)",
            )
            .bind(uid(0x900 - n as u128).to_string())
            .bind(&card)
            .bind(rating)
            .bind(&at)
            .bind(&at)
            .execute(&env.pool)
            .await
            .unwrap();
        }
        let cards = env.learning.review_cards(l.topic_id).await.unwrap();
        assert!(
            cards
                .iter()
                .find(|c| c.front == "f1")
                .unwrap()
                .to_consolidate,
            "same_instant = {same_instant}"
        );
    }
}

#[tokio::test]
async fn an_empty_completion_is_still_recorded() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let mut save = completion(&env, &l);
    (save.records, save.terms, save.cards) = (vec![], vec![], vec![]);
    env.learning.save_completion(&save).await.unwrap();
    assert_eq!(
        env.learning.completion_summary(l.id).await.unwrap(),
        Some(CompletionSummary::default())
    );
    assert_eq!(
        env.learning
            .completion_summary(LessonId::new(uid(9)))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn supersession_keeps_the_instant_of_an_already_superseded_card() {
    let env = env().await;
    let first = lesson(&env, 1).await;
    let second = lesson(&env, 2).await;
    env.learning
        .save_completion(&completion(&env, &first))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE review_cards SET status = 'suspended', suspension = 'superseded', \
         suspended_at = '2026-11-09T10:30:00.000Z' WHERE front = 'f1'",
    )
    .execute(&env.pool)
    .await
    .unwrap();
    let mut save = completion(&env, &second);
    save.at = T0 + time::Duration::hours(1);
    save.records[0].id = LearningRecordId::new(uid(0x11));
    save.records[0].supersedes = vec![1];
    save.terms[0].id = GlossaryTermId::new(uid(0x21));
    save.cards = vec![];
    env.learning.save_completion(&save).await.unwrap();

    let cards = env.learning.review_cards(env.topic).await.unwrap();
    let f1 = cards.iter().find(|c| c.front == "f1").unwrap();
    assert_eq!(
        f1.suspension.map(|s| s.at),
        Some(datetime!(2026-11-09 10:30 UTC))
    );
}

#[tokio::test]
async fn supersession_suspends_cards_and_a_term_is_revised_in_place() {
    let env = env().await;
    let first = lesson(&env, 1).await;
    let second = lesson(&env, 2).await;
    env.learning
        .save_completion(&completion(&env, &first))
        .await
        .unwrap();

    let mut save = completion(&env, &second);
    save.at = T0 + time::Duration::hours(1);
    save.records[0].id = LearningRecordId::new(uid(0x11));
    save.records[0].supersedes = vec![1];
    save.terms[0].id = GlossaryTermId::new(uid(0x21));
    save.terms[0].definition = "Raddoppio o dimezzamento della luce.".into();
    save.terms[0].avoid = vec![];
    save.cards = vec![NewCard {
        id: ReviewCardId::new(uid(0x32)),
        target: CardTarget::Term(0),
        front: "f3".into(),
        back: "b3".into(),
        unpassed_block: None,
    }];
    // The learner had suspended that card already: superseding wins.
    sqlx::query(
        "UPDATE review_cards SET status = 'suspended', suspension = 'user', \
         suspended_at = '2026-11-09T10:30:00.000Z' WHERE front = 'f1'",
    )
    .execute(&env.pool)
    .await
    .unwrap();
    env.learning.save_completion(&save).await.unwrap();

    let all = env
        .learning
        .list_records(env.topic, false, None, 10)
        .await
        .unwrap();
    assert_eq!(
        all.iter().map(|r| r.number).collect::<Vec<_>>(),
        [2, 1],
        "newest first"
    );
    assert_eq!(
        (all[1].status, all[1].superseded_by),
        (RecordStatus::Superseded, Some(all[0].id))
    );
    let active = env
        .learning
        .list_records(env.topic, true, None, 10)
        .await
        .unwrap();
    assert_eq!(active.iter().map(|r| r.number).collect::<Vec<_>>(), [2]);
    let before = env
        .learning
        .list_records(env.topic, false, Some(RecordCursor { number: 2 }), 10)
        .await
        .unwrap();
    assert_eq!(before.len(), 1);

    let cards = env.learning.review_cards(env.topic).await.unwrap();
    let status = |front: &str| cards.iter().find(|c| c.front == front).unwrap().status;
    assert_eq!(
        status("f1"),
        CardStatus::Suspended,
        "the replaced record's card"
    );
    let f1 = cards.iter().find(|c| c.front == "f1").unwrap();
    assert_eq!(
        f1.suspension,
        Some(Suspension {
            reason: SuspensionReason::Superseded,
            at: T0 + time::Duration::hours(1)
        })
    );
    assert_eq!(status("f2"), CardStatus::Active, "the term's card stays");
    assert_eq!(status("f3"), CardStatus::Active);

    let terms = env.learning.list_terms(env.topic, None, 10).await.unwrap();
    assert_eq!(terms.len(), 1, "revised, not duplicated");
    assert_eq!(
        (terms[0].id, terms[0].lesson_id),
        (GlossaryTermId::new(uid(0x20)), second.id)
    );
    assert_eq!(terms[0].definition, "Raddoppio o dimezzamento della luce.");
    // `lesson_id` follows the reviser, `first_lesson_id` stays with the creator.
    assert_ne!(terms[0].first_lesson_id, Some(second.id));
    assert_eq!(terms[0].first_lesson_id, Some(first.id));
    assert_eq!(
        (terms[0].created_at, terms[0].updated_at),
        (T0, T0 + time::Duration::hours(1))
    );
    // The card of the new revision points to the same, original, term row.
    let c3 = cards.iter().find(|c| c.front == "f3").unwrap();
    assert_eq!(c3.origin, CardOrigin::Term(terms[0].id));
}

#[tokio::test]
async fn a_failing_completion_stores_nothing() {
    let env = env().await;
    let first = lesson(&env, 1).await;
    let second = lesson(&env, 2).await;
    env.learning
        .save_completion(&completion(&env, &first))
        .await
        .unwrap();

    // Replaces a record that does not exist: refused after the new record
    // and the completion marker were already written inside the transaction.
    let mut save = completion(&env, &second);
    save.records[0].id = LearningRecordId::new(uid(0x11));
    save.records[0].supersedes = vec![42];
    assert!(matches!(
        env.learning.save_completion(&save).await,
        Err(RepositoryError::Conflict)
    ));
    assert_eq!(
        counts(&env).await,
        (1, 1, 2, 1),
        "everything of the failed save rolled back"
    );
    assert_eq!(
        env.learning.completion_summary(second.id).await.unwrap(),
        None
    );

    // A card that points past the end fails midway too.
    let mut save = completion(&env, &second);
    save.cards[1].id = save.cards[0].id;
    assert!(env.learning.save_completion(&save).await.is_err());
    assert_eq!(counts(&env).await, (1, 1, 2, 1));
    // A save that would replace an already replaced record is refused as well.
    let mut again = completion(&env, &second);
    again.records[0].id = LearningRecordId::new(uid(0x12));
    again.records[0].supersedes = vec![1];
    again.cards.clear();
    again.terms.clear();
    env.learning.save_completion(&again).await.unwrap();
    let third = lesson(&env, 3).await;
    let mut stale = completion(&env, &third);
    stale.records[0].id = LearningRecordId::new(uid(0x13));
    stale.records[0].supersedes = vec![1];
    assert!(matches!(
        env.learning.save_completion(&stale).await,
        Err(RepositoryError::Conflict)
    ));
}

#[tokio::test]
async fn the_glossary_is_alphabetical_without_case_with_a_keyset_cursor() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let mut save = completion(&env, &l);
    save.records.clear();
    save.cards.clear();
    save.terms = [("b", "beta"), ("a", "Zeta"), ("c", "Alfa"), ("d", "alfa")]
        .into_iter()
        .enumerate()
        .map(|(i, (key, term))| TermUpsert {
            id: GlossaryTermId::new(uid(0x40 + i as u128)),
            key: key.into(),
            term: term.into(),
            definition: "d".into(),
            avoid: vec![],
        })
        .collect();
    env.learning.save_completion(&save).await.unwrap();

    let first = env.learning.list_terms(env.topic, None, 3).await.unwrap();
    assert_eq!(
        first.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
        ["c", "d", "b"]
    );
    let next = env
        .learning
        .list_terms(env.topic, Some(TermCursor::after(&first[2])), 3)
        .await
        .unwrap();
    assert_eq!(
        next.iter().map(|t| t.key.as_str()).collect::<Vec<_>>(),
        ["a"]
    );
}

#[tokio::test]
async fn deleting_a_topic_deletes_what_its_lessons_left() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    env.attempts
        .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
        .await
        .unwrap();
    env.learning
        .save_completion(&completion(&env, &l))
        .await
        .unwrap();
    SqliteTopicRepository::new(env.pool.clone())
        .delete(domain::UserId::OWNER, env.topic)
        .await
        .unwrap();
    assert_eq!(counts(&env).await, (0, 0, 0, 0));
    for table in ["lessons", "attempts", "component_requests", "jobs"] {
        let q = format!("SELECT COUNT(*) FROM {table}");
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(q))
            .fetch_one(&env.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
}
