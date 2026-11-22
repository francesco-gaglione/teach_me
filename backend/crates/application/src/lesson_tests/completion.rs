use super::*;

// ----------------------------------------------------------- completion

fn recorder_reply(extra: Value) -> String {
    let mut reply = json!({
        "records": [{
            "kind": "demonstrated",
            "title": "Compensare uno stop con un altro",
            "body": "Hai scelto la compensazione senza rumore.",
            "evidence": "Hai risposto bene alla domanda sulla compensazione.",
            "evidence_blocks": ["q1"],
            "supersedes": []
        }],
        "terms": [{
            "key": "stop", "term": "stop", "definition": "Unità di misura della luce.",
            "avoid": [], "evidence_blocks": ["q1"]
        }],
        "cards": [
            {"front": "Da f/8 a f/5.6 la luce?", "back": "Raddoppia.", "record_index": 0, "term_key": null, "unpassed_block": null},
            {"front": "Che cos'è uno stop?", "back": "Un raddoppio o dimezzamento della luce.", "record_index": null, "term_key": "stop", "unpassed_block": null}
        ]
    });
    if let (Some(base), Some(over)) = (reply.as_object_mut(), extra.as_object()) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    reply.to_string()
}

#[tokio::test]
async fn completing_closes_the_lesson_and_refuses_later_attempts() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let job = env
        .lessons()
        .complete(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(
        (job.kind, job.status),
        (JobKind::LessonCompletion, JobStatus::Queued)
    );
    assert_eq!(env.lesson(lesson.id).status, LessonStatus::Completed);
    assert!(env.lesson(lesson.id).completed_at.is_some());
    assert!(matches!(
        env.lessons()
            .complete(domain::UserId::OWNER, lesson.id)
            .await,
        Err(AppError::Conflict(ConflictReason::LessonCompleted))
    ));
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "c"}, "outcome": "correct", "answered_at": t("08:40")}
    ]});
    assert!(matches!(
        env.attempts()
            .record(domain::UserId::OWNER, lesson.id, &body)
            .await,
        Err(AppError::Conflict(ConflictReason::LessonCompleted))
    ));
}

#[tokio::test]
async fn two_lessons_can_be_completing_at_the_same_time() {
    let env = LessonEnv::new(vec![]).await;
    let one = env.store_lesson(lesson_doc()).await;
    let two = env.store_lesson(lesson_doc()).await;
    env.lessons()
        .complete(domain::UserId::OWNER, one.id)
        .await
        .unwrap();
    env.lessons()
        .complete(domain::UserId::OWNER, two.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn the_recorder_output_is_stored_atomically_with_cards_and_terms() {
    let env = LessonEnv::new(vec![
        say(&grade_json("correct", Value::Null)),
        say(&recorder_reply(json!({}))),
    ])
    .await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    // An open answer left pending (the grader had failed): the job grades it.
    env.f.repo.0.lock().unwrap().attempts.push(Attempt {
        id: domain::AttemptId::new(Uuid::from_u128(0x4444)),
        lesson_id: lesson.id,
        block_id: "oa1".into(),
        kind: domain::ExerciseKind::OpenAnswer,
        attempt_number: 1,
        answer: domain::Answer::Text("Accorcio il tempo e apro il diaframma".into()),
        outcome: Outcome::Pending,
        grade: None,
        answered_at: datetime!(2026-11-09 15:10 UTC),
        created_at: datetime!(2026-11-09 15:10 UTC),
    });
    let job = completed(&env, &lesson).await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        job.result,
        Some(JobResult::LessonCompletion {
            lesson_id: lesson.id,
            record_count: 1,
            term_count: 1,
            review_card_count: 2
        })
    );
    assert_eq!(env.steps(), ["grading", "recording", "saving"]);
    let s = env.f.repo.0.lock().unwrap();
    assert_eq!(
        s.attempts
            .iter()
            .find(|a| a.block_id == "oa1")
            .unwrap()
            .outcome,
        Outcome::Correct
    );
    assert_eq!((s.records.len(), s.terms.len(), s.cards.len()), (1, 1, 2));
    assert_eq!(s.records[0].number, 1);
    assert_eq!(s.records[0].lesson_id, lesson.id);
    assert!(matches!(s.cards[0].origin, CardOrigin::Record(_)));
    assert!(matches!(s.cards[1].origin, CardOrigin::Term(_)));
    drop(s);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].role, domain::llm::LlmRole::Recorder);
    let ctx = &calls[1].system[1].text;
    assert!(ctx.contains("[q1] quiz_single"), "{ctx}");
    assert!(ctx.contains("Tentativo 1:"), "{ctx}");
    assert!(
        ctx.contains("Nessun tentativo."),
        "exercises without attempts are listed too"
    );
}

#[tokio::test]
async fn without_a_passed_exercise_nothing_is_recorded_and_the_recorder_is_not_called() {
    let env = LessonEnv::new(vec![]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let body = json!({"attempts": [
        {"block_id": "q1", "attempt_number": 1, "answer": {"option": "a"}, "outcome": "incorrect", "answered_at": t("08:40")}
    ]});
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(
        job.result,
        Some(JobResult::LessonCompletion {
            lesson_id: lesson.id,
            record_count: 0,
            term_count: 0,
            review_card_count: 0
        })
    );
    assert_eq!(env.calls(), 0);
    assert!(env.f.repo.0.lock().unwrap().records.is_empty());
}

#[tokio::test]
async fn a_pending_answer_that_cannot_be_graded_does_not_count_as_passed() {
    // The request's grading fails, then the job's two tries fail too: no
    // passed attempt remains, so the Recorder is never asked.
    let body = json!({"attempts": [
        {"block_id": "oa1", "attempt_number": 1, "answer": {"text": "risposta"}, "answered_at": t("08:40")}
    ]});
    let env = LessonEnv::new(vec![say("no"), say("no"), say("no")]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    env.attempts()
        .record(domain::UserId::OWNER, lesson.id, &body)
        .await
        .unwrap();
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(env.attempts_of(lesson.id)[0].outcome, Outcome::Pending);
    assert_eq!(
        env.calls(),
        3,
        "one in the request, two in the job, no Recorder"
    );
}

#[tokio::test]
async fn a_misconception_needs_a_first_failure_and_a_later_pass() {
    // q1 passed at the first try: calling it a corrected misconception is invalid.
    let wrong = recorder_reply(json!({"records": [{
        "kind": "misconception_corrected", "title": "t", "body": "b", "evidence": "e",
        "evidence_blocks": ["q1"], "supersedes": []
    }], "cards": []}));
    let env = LessonEnv::new(vec![
        say(&wrong),
        say(&recorder_reply(json!({"cards": []}))),
    ])
    .await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[1].trace_label, "recorder.repair");
        assert_eq!(calls[1].system, calls[0].system);
        let text = format!("{:?}", calls[1].messages);
        assert!(text.contains("misconception_corrected"), "{text}");
    }
    assert_eq!(
        env.f.repo.0.lock().unwrap().records[0].kind,
        RecordKind::Demonstrated
    );

    // Still wrong after the repair: invalid_output, nothing stored.
    let env = LessonEnv::new(vec![say(&wrong), say(&wrong)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    let s = env.f.repo.0.lock().unwrap();
    assert_eq!(
        (
            s.records.len(),
            s.terms.len(),
            s.cards.len(),
            s.completions.len()
        ),
        (0, 0, 0, 0)
    );
}

#[tokio::test]
async fn a_misconception_corrected_by_a_second_attempt_is_recorded() {
    let reply = recorder_reply(json!({"records": [{
        "kind": "misconception_corrected", "title": "Più opzioni corrette", "body": "Hai visto che serve ogni opzione corretta.",
        "evidence": "Hai sbagliato e poi corretto.", "evidence_blocks": ["qm"], "supersedes": []
    }], "terms": [], "cards": []}));
    let env = LessonEnv::new(vec![say(&reply)]).await;
    let lesson = env.store_lesson(doc_with_retryable_exercises()).await;
    for (n, answer, outcome) in [(1, "b", "incorrect"), (2, "a", "correct")] {
        let body = json!({"attempts": [
            {"block_id": "qm", "attempt_number": n, "answer": {"options": [answer]}, "outcome": outcome, "answered_at": t("08:40")}
        ]});
        env.attempts()
            .record(domain::UserId::OWNER, lesson.id, &body)
            .await
            .unwrap();
    }
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.f.repo.0.lock().unwrap().records[0].kind,
        RecordKind::MisconceptionCorrected
    );
}

#[tokio::test]
async fn a_superseding_record_suspends_the_cards_of_the_one_it_replaces() {
    let reply = recorder_reply(json!({
        "records": [{
            "kind": "demonstrated", "title": "Compensazione completa", "body": "Ora compensi con ogni leva.",
            "evidence": "Hai risposto bene.", "evidence_blocks": ["q1"], "supersedes": [1]
        }],
        "terms": [{"key": "stop", "term": "Stop", "definition": "Raddoppio o dimezzamento della luce.", "avoid": ["step"], "evidence_blocks": ["q1"]}],
        "cards": [{"front": "Compensare?", "back": "Con un'altra leva.", "record_index": 0, "term_key": null, "unpassed_block": null}]
    }));
    let env = LessonEnv::new(vec![say(&reply)]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let topic = env.f.topic;
    {
        let mut s = env.f.repo.0.lock().unwrap();
        let old = LearningRecordId::new(Uuid::from_u128(0x91));
        s.records.push(LearningRecord {
            id: old,
            topic_id: topic,
            number: 1,
            kind: RecordKind::Demonstrated,
            title: "Primo".into(),
            body: "b".into(),
            evidence: "e".into(),
            status: RecordStatus::Active,
            superseded_by: None,
            lesson_id: earlier.id,
            created_at: datetime!(2026-11-08 10:00 UTC),
        });
        let term = GlossaryTermId::new(Uuid::from_u128(0x92));
        s.terms.push(GlossaryTerm {
            id: term,
            topic_id: topic,
            key: "stop".into(),
            term: "stop".into(),
            definition: "Vecchia.".into(),
            avoid: vec![],
            lesson_id: earlier.id,
            first_lesson_id: Some(earlier.id),
            created_at: datetime!(2026-11-08 10:00 UTC),
            updated_at: datetime!(2026-11-08 10:00 UTC),
        });
        for (n, origin) in [
            (0x93, CardOrigin::Record(old)),
            (0x94, CardOrigin::Term(term)),
        ] {
            s.cards.push(ReviewCard {
                id: ReviewCardId::new(Uuid::from_u128(n)),
                topic_id: topic,
                lesson_id: earlier.id,
                origin,
                front: "f".into(),
                back: "b".into(),
                status: CardStatus::Active,
                suspension: None,
                schedule: domain::CardSchedule::initial(datetime!(2026-11-08 10:00 UTC)),
                created_at: datetime!(2026-11-08 10:00 UTC),
                to_consolidate: false,
            });
        }
    }
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let s = env.f.repo.0.lock().unwrap();
    let old = s.records.iter().find(|r| r.number == 1).unwrap();
    let new = s.records.iter().find(|r| r.number == 2).unwrap();
    assert_eq!(
        (old.status, old.superseded_by),
        (RecordStatus::Superseded, Some(new.id))
    );
    assert_eq!(new.status, RecordStatus::Active);
    let by_front = |f: &str| s.cards.iter().find(|c| c.front == f).unwrap();
    assert_eq!(
        by_front("f").status,
        CardStatus::Suspended,
        "the replaced record's card"
    );
    assert_eq!(
        s.cards
            .iter()
            .filter(|c| c.status == CardStatus::Suspended)
            .count(),
        1,
        "the term's card stays"
    );
    assert_eq!(by_front("Compensare?").status, CardStatus::Active);
    // The term was revised in place, not duplicated.
    assert_eq!(s.terms.len(), 1);
    assert_eq!(
        (s.terms[0].term.as_str(), s.terms[0].lesson_id),
        ("Stop", lesson.id)
    );
    assert_eq!(s.terms[0].avoid, ["step"]);
    assert_eq!(s.terms[0].created_at, datetime!(2026-11-08 10:00 UTC));
}

#[tokio::test]
async fn superseding_a_record_that_is_not_active_is_rejected() {
    let reply = recorder_reply(json!({"records": [{
        "kind": "demonstrated", "title": "t", "body": "b", "evidence": "e",
        "evidence_blocks": ["q1"], "supersedes": [7]
    }], "terms": [], "cards": []}));
    let env = LessonEnv::new(vec![say(&reply), say(&reply)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
}

#[tokio::test]
async fn two_records_replacing_the_same_one_are_repaired_not_an_internal_error() {
    let two = |()| {
        json!({"records": [
            {"kind": "demonstrated", "title": "a", "body": "b", "evidence": "e", "evidence_blocks": ["q1"], "supersedes": [1]},
            {"kind": "demonstrated", "title": "c", "body": "d", "evidence": "e", "evidence_blocks": ["q1"], "supersedes": [1]}
        ], "terms": [], "cards": []})
        .to_string()
    };
    let env = LessonEnv::new(vec![say(&two(())), say(&two(()))]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    env.f.repo.0.lock().unwrap().records.push(LearningRecord {
        id: LearningRecordId::new(Uuid::from_u128(0x91)),
        topic_id: env.f.topic,
        number: 1,
        kind: RecordKind::Demonstrated,
        title: "Primo".into(),
        body: "b".into(),
        evidence: "e".into(),
        status: RecordStatus::Active,
        superseded_by: None,
        lesson_id: earlier.id,
        created_at: datetime!(2026-11-08 10:00 UTC),
    });
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 2, "one repair was asked");
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(format!("{:?}", calls[1].messages).contains("già sostituito"));
}

#[tokio::test]
async fn a_storage_failure_while_recording_stores_nothing_and_fails_the_job() {
    let env = LessonEnv::new(vec![say(&recorder_reply(json!({})))]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    env.f.repo.0.lock().unwrap().fail_learning_writes = true;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    let s = env.f.repo.0.lock().unwrap();
    assert!(s.records.is_empty() && s.terms.is_empty() && s.cards.is_empty());
}

#[tokio::test]
async fn the_view_hides_rubrics_and_lists_what_the_completion_produced() {
    let env = LessonEnv::new(vec![say(&recorder_reply(json!({})))]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let before = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert!(before.completion.is_none());
    assert!(!before.document.to_string().contains("rubric"));
    assert_eq!(before.attempts.len(), 1);
    assert_eq!(before.lesson.status, LessonStatus::InProgress);

    completed(&env, &lesson).await;
    let after = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let completion = after.completion.unwrap();
    assert_eq!(completion.job.unwrap().status, JobStatus::Succeeded);
    assert_eq!(
        (
            completion.outputs.records.len(),
            completion.outputs.terms.len(),
            completion.outputs.review_card_count
        ),
        (1, 1, 2)
    );
}

#[tokio::test]
async fn a_failed_completion_job_can_be_retried_per_lesson() {
    let env = LessonEnv::new(vec![say("non json"), say("ancora no")]).await;
    let one = env.store_lesson(lesson_doc()).await;
    let two = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &one).await;
    let failed = completed(&env, &one).await;
    assert_eq!(failed.status, JobStatus::Failed);
    // Another lesson's completion does not make this failure "not the newest".
    env.lessons()
        .complete(domain::UserId::OWNER, two.id)
        .await
        .unwrap();
    let retried = env
        .f
        .jobs
        .retry(domain::UserId::OWNER, env.f.topic, failed.id)
        .await
        .unwrap();
    assert_eq!(
        (retried.status, retried.attempt, retried.lesson_id),
        (JobStatus::Queued, 0, Some(one.id))
    );
}

#[tokio::test]
async fn lessons_list_newest_first_and_the_glossary_is_alphabetical() {
    let env = LessonEnv::new(vec![]).await;
    for _ in 0..3 {
        env.store_lesson(lesson_doc()).await;
    }
    let page = env
        .lessons()
        .list(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|l| l.lesson.number)
            .collect::<Vec<_>>(),
        [3, 2]
    );
    let next = env
        .lessons()
        .list(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: page.next,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        next.items
            .iter()
            .map(|l| l.lesson.number)
            .collect::<Vec<_>>(),
        [1]
    );
    assert!(next.next.is_none());

    let lesson_id = LessonId::new(Uuid::from_u128(0x700));
    {
        let mut s = env.f.repo.0.lock().unwrap();
        for (i, (key, term)) in [("b", "beta"), ("a", "Zeta"), ("c", "alfa")]
            .into_iter()
            .enumerate()
        {
            s.terms.push(GlossaryTerm {
                id: GlossaryTermId::new(Uuid::from_u128(i as u128 + 1)),
                topic_id: env.f.topic,
                key: key.into(),
                term: term.into(),
                definition: "d".into(),
                avoid: vec![],
                lesson_id,
                first_lesson_id: Some(lesson_id),
                created_at: datetime!(2026-11-08 10:00 UTC),
                updated_at: datetime!(2026-11-08 10:00 UTC),
            });
        }
    }
    let terms = env
        .lessons()
        .glossary(
            domain::UserId::OWNER,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        terms
            .items
            .iter()
            .map(|t| t.term.as_str())
            .collect::<Vec<_>>(),
        ["alfa", "beta", "Zeta"]
    );
}

#[tokio::test]
async fn a_saved_lesson_with_long_inline_code_is_still_served() {
    // Lessons stored before contract v1.4 may break `inline_code_too_long`:
    // reading never re-validates.
    let env = LessonEnv::new(vec![]).await;
    let mut doc = lesson_doc();
    doc["blocks"][1]["text"] = json!("Vedi `let a = 1; let b = a; let c = b; println!(\"{c}\");`.");
    let lesson = env.store_lesson(doc).await;
    let view = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert!(view.document.to_string().contains("let c = b;"));
}
