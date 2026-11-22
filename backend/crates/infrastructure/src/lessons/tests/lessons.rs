use super::*;

// ------------------------------------------------------------ lessons

#[tokio::test]
async fn lessons_are_numbered_per_topic_and_round_trip() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let two = lesson(&env, 2).await;
    assert_eq!((one.number, two.number), (1, 2));
    assert_eq!(one.status, LessonStatus::NotStarted);
    assert_eq!(
        (
            one.mission_version,
            one.catalog_version,
            one.estimated_minutes
        ),
        (2, 1, 8)
    );

    let stored = env
        .lessons
        .get(domain::UserId::OWNER, two.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.lesson, two);
    assert_eq!(stored.document, document());
    assert_eq!(stored.plan, json!({"objective": "Saper compensare."}));
    assert!(
        env.lessons
            .get(domain::UserId::OWNER, LessonId::new(uid(9)))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        env.lessons
            .by_generation_job(JobId::new(uid(0x201)))
            .await
            .unwrap(),
        Some(one.clone())
    );
    let requests = env.lessons.component_requests(one.id).await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (requests[0].wanted.as_str(), &requests[0].fallback),
        ("slider", &json!([{"id": "f"}]))
    );
}

#[tokio::test]
async fn lessons_list_newest_first_with_a_keyset_cursor() {
    let env = env().await;
    for n in 1..=3 {
        lesson(&env, n).await;
    }
    let first = env.lessons.list(env.topic, None, 2).await.unwrap();
    assert_eq!(first.iter().map(|l| l.number).collect::<Vec<_>>(), [3, 2]);
    let next = env
        .lessons
        .list(env.topic, Some(LessonCursor { number: 2 }), 2)
        .await
        .unwrap();
    assert_eq!(next.iter().map(|l| l.number).collect::<Vec<_>>(), [1]);
    let all = env.lessons.all_in_topic(env.topic).await.unwrap();
    assert_eq!(all.iter().map(|l| l.number).collect::<Vec<_>>(), [1, 2, 3]);
}

#[tokio::test]
async fn saving_a_lesson_is_atomic_and_idempotent_per_job() {
    let env = env().await;
    lesson(&env, 1).await;
    // The same generation job cannot save a second lesson.
    let mut again = generated(&env, 2);
    again.job_id = JobId::new(uid(0x201));
    assert!(matches!(
        env.lessons.insert_generated(&again).await,
        Err(RepositoryError::Conflict)
    ));
    // A request that cannot be stored takes the lesson down with it.
    let mut broken = generated(&env, 3);
    broken.component_requests[0].topic_id = TopicId::new(uid(0x999));
    assert!(env.lessons.insert_generated(&broken).await.is_err());
    let all = env.lessons.all_in_topic(env.topic).await.unwrap();
    assert_eq!(all.len(), 1, "no half-saved lesson");
    assert!(
        env.lessons
            .by_generation_job(JobId::new(uid(0x203)))
            .await
            .unwrap()
            .is_none()
    );
    // The numbering has no hole after the failure.
    assert_eq!(lesson(&env, 4).await.number, 2);
    // A missing topic is a missing parent.
    let mut orphan = generated(&env, 5);
    orphan.topic_id = TopicId::new(uid(0x998));
    orphan.component_requests.clear();
    assert!(matches!(
        env.lessons.insert_generated(&orphan).await,
        Err(RepositoryError::MissingParent)
    ));
}

#[tokio::test]
async fn completing_stores_the_lesson_state_and_the_job_together() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let two = lesson(&env, 2).await;
    let job = |n: u128, l: &Lesson| {
        Job::queued_for_lesson(
            JobId::new(uid(0x500 + n)),
            env.topic,
            l.id,
            JobKind::LessonCompletion,
            T0,
        )
    };

    // A job that cannot be stored (its id is taken) leaves the lesson open.
    env.jobs
        .insert(&Job::queued(
            JobId::new(uid(0x501)),
            env.topic,
            JobKind::Research,
            T0,
        ))
        .await
        .unwrap();
    assert!(
        env.lessons
            .complete(one.id, T0, &job(1, &one))
            .await
            .is_err()
    );
    assert_eq!(
        env.lessons
            .get(domain::UserId::OWNER, one.id)
            .await
            .unwrap()
            .unwrap()
            .lesson
            .status,
        LessonStatus::NotStarted
    );

    let done = env
        .lessons
        .complete(one.id, T0, &job(2, &one))
        .await
        .unwrap();
    assert_eq!(
        (done.status, done.completed_at),
        (LessonStatus::Completed, Some(T0))
    );
    let stored = env
        .jobs
        .get(domain::UserId::OWNER, env.topic, JobId::new(uid(0x502)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (stored.lesson_id, stored.kind, stored.status),
        (Some(one.id), JobKind::LessonCompletion, JobStatus::Queued)
    );

    assert!(matches!(
        env.lessons.complete(one.id, T0, &job(3, &one)).await,
        Err(RepositoryError::Conflict)
    ));
    // Another lesson may be completing at the same time.
    env.lessons
        .complete(two.id, T0, &job(4, &two))
        .await
        .unwrap();
    assert!(matches!(
        env.lessons
            .complete(LessonId::new(uid(0x9)), T0, &job(5, &one))
            .await,
        Err(RepositoryError::NotFound)
    ));
}

#[tokio::test]
async fn the_one_active_job_rule_is_per_topic_except_for_completions() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let queued = |n: u128, kind: JobKind, lesson: Option<LessonId>| match lesson {
        Some(l) => Job::queued_for_lesson(JobId::new(uid(0x600 + n)), env.topic, l, kind, T0),
        None => Job::queued(JobId::new(uid(0x600 + n)), env.topic, kind, T0),
    };
    env.jobs
        .insert(&queued(1, JobKind::LessonGeneration, None))
        .await
        .unwrap();
    assert!(matches!(
        env.jobs
            .insert(&queued(2, JobKind::LessonGeneration, None))
            .await,
        Err(RepositoryError::Conflict)
    ));
    env.jobs
        .insert(&queued(3, JobKind::Research, None))
        .await
        .unwrap();
    env.jobs
        .insert(&queued(4, JobKind::LessonCompletion, Some(one.id)))
        .await
        .unwrap();
    assert!(matches!(
        env.jobs
            .insert(&queued(5, JobKind::LessonCompletion, Some(one.id)))
            .await,
        Err(RepositoryError::Conflict)
    ));
    let two = lesson(&env, 2).await;
    env.jobs
        .insert(&queued(6, JobKind::LessonCompletion, Some(two.id)))
        .await
        .unwrap();
}

#[tokio::test]
async fn lesson_job_results_survive_the_round_trip() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let mut generation = Job::queued(
        JobId::new(uid(0x700)),
        env.topic,
        JobKind::LessonGeneration,
        T0,
    );
    env.jobs.insert(&generation).await.unwrap();
    generation.start(T0).unwrap();
    env.jobs
        .update(&generation, JobStatus::Queued)
        .await
        .unwrap();
    generation
        .succeed(
            JobResult::LessonGeneration {
                lesson_id: one.id,
                lesson_number: 1,
            },
            T0,
        )
        .unwrap();
    env.jobs
        .update(&generation, JobStatus::Running)
        .await
        .unwrap();
    assert_eq!(
        env.jobs
            .get(domain::UserId::OWNER, env.topic, generation.id)
            .await
            .unwrap(),
        Some(generation)
    );

    let mut comp = Job::queued_for_lesson(
        JobId::new(uid(0x701)),
        env.topic,
        one.id,
        JobKind::LessonCompletion,
        T0,
    );
    env.jobs.insert(&comp).await.unwrap();
    comp.start(T0).unwrap();
    env.jobs.update(&comp, JobStatus::Queued).await.unwrap();
    comp.succeed(
        JobResult::LessonCompletion {
            lesson_id: one.id,
            record_count: 2,
            term_count: 3,
            review_card_count: 4,
        },
        T0,
    )
    .unwrap();
    env.jobs.update(&comp, JobStatus::Running).await.unwrap();
    assert_eq!(
        env.jobs
            .get(domain::UserId::OWNER, env.topic, comp.id)
            .await
            .unwrap(),
        Some(comp)
    );
}

#[tokio::test]
async fn the_latest_job_is_found_however_many_jobs_the_topic_has() {
    let env = env().await;
    let one = lesson(&env, 1).await;
    let fail = |mut j: Job| {
        j.start(T0).unwrap();
        j.fail(
            domain::JobErrorCode::Timeout,
            domain::ContentLanguage::It,
            T0,
        )
        .unwrap();
        j
    };
    let completion = Job::queued_for_lesson(
        JobId::new(uid(0x800)),
        env.topic,
        one.id,
        JobKind::LessonCompletion,
        T0,
    );
    env.jobs.insert(&completion).await.unwrap();
    env.jobs
        .update(&fail(completion.clone()), JobStatus::Queued)
        .await
        .unwrap();
    let mut newest = None;
    for n in 1..=120_u32 {
        let job = Job::queued(
            JobId::new(uid(0x900 + u128::from(n))),
            env.topic,
            JobKind::Research,
            T0 + time::Duration::seconds(i64::from(n)),
        );
        env.jobs.insert(&job).await.unwrap();
        let failed = fail(job);
        env.jobs.update(&failed, JobStatus::Queued).await.unwrap();
        newest = Some(failed);
    }
    let found = env
        .jobs
        .latest(env.topic, JobKind::LessonCompletion, Some(one.id))
        .await
        .unwrap();
    assert_eq!(found.map(|j| j.id), Some(completion.id));
    let found = env
        .jobs
        .latest(env.topic, JobKind::Research, None)
        .await
        .unwrap();
    assert_eq!(found.map(|j| j.id), newest.map(|j| j.id));
    // A topic-level query never returns a lesson's job, nor the reverse.
    assert!(
        env.jobs
            .latest(env.topic, JobKind::LessonCompletion, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        env.jobs
            .latest(env.topic, JobKind::Research, Some(one.id))
            .await
            .unwrap()
            .is_none()
    );
}
