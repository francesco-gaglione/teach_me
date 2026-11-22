use super::*;

// ----------------------------------------------------------- attempts

#[tokio::test]
async fn the_first_attempt_stored_wins_and_moves_the_lesson_forward() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let first = attempt(&l, 1, "q1", 1, Outcome::Incorrect);
    let stored = env
        .attempts
        .record(l.id, std::slice::from_ref(&first))
        .await
        .unwrap();
    assert_eq!(stored, vec![first.clone()]);
    assert_eq!(
        env.lessons
            .get(domain::UserId::OWNER, l.id)
            .await
            .unwrap()
            .unwrap()
            .lesson
            .status,
        LessonStatus::InProgress
    );

    // Same pair, different content and id: the stored one is returned.
    let mut other = attempt(&l, 2, "q1", 1, Outcome::Incorrect);
    other.answer = Answer::Option("b".into());
    let mixed = env
        .attempts
        .record(l.id, &[other, attempt(&l, 3, "q1", 2, Outcome::Correct)])
        .await
        .unwrap();
    assert_eq!(mixed[0], first);
    assert_eq!(mixed[1].attempt_number, 2);
    assert_eq!(env.attempts.list(l.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_second_attempt_is_checked_against_the_first_inside_the_write() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let second = attempt(&l, 2, "q1", 2, Outcome::Correct);
    // No first attempt stored.
    assert!(matches!(
        env.attempts
            .record(l.id, std::slice::from_ref(&second))
            .await,
        Err(RepositoryError::RuleViolated)
    ));
    // A first attempt that was already correct.
    env.attempts
        .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
        .await
        .unwrap();
    let batch = [attempt(&l, 3, "fc", 1, Outcome::Correct), second.clone()];
    assert!(matches!(
        env.attempts.record(l.id, &batch).await,
        Err(RepositoryError::RuleViolated)
    ));
    assert_eq!(
        env.attempts.list(l.id).await.unwrap().len(),
        1,
        "the batch stored nothing"
    );
    // A wrong first attempt, even one sent in the same batch, allows it.
    let wrong = [
        attempt(&l, 4, "qm", 1, Outcome::Incorrect),
        attempt(&l, 5, "qm", 2, Outcome::Correct),
    ];
    assert_eq!(env.attempts.record(l.id, &wrong).await.unwrap().len(), 2);
}

#[tokio::test]
async fn attempts_round_trip_every_answer_shape_and_a_grade() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let mut all = Vec::new();
    for (i, (kind, answer)) in [
        (ExerciseKind::QuizSingle, Answer::Option("b".into())),
        (
            ExerciseKind::QuizMulti,
            Answer::Options(vec!["a".into(), "c".into()]),
        ),
        (
            ExerciseKind::FillBlank,
            Answer::Blanks([("b1".to_owned(), "borrow".to_owned())].into()),
        ),
        (
            ExerciseKind::OrderSteps,
            Answer::Steps(vec!["s2".into(), "s1".into()]),
        ),
        (ExerciseKind::Flashcard, Answer::Rating(Rating::Easy)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut a = attempt(&l, i as u128 + 10, &format!("b{i}"), 1, Outcome::Correct);
        a.kind = kind;
        a.answer = answer;
        all.push(a);
    }
    let mut open = attempt(&l, 20, "oa", 1, Outcome::Pending);
    all.push(open.clone());
    env.attempts.record(l.id, &all).await.unwrap();
    assert_eq!(env.attempts.list(l.id).await.unwrap(), all);

    let grade = Grade {
        verdict: Verdict::Partial,
        feedback: "Manca il perché.".into(),
        misconception: Some("Pensa che...".into()),
    };
    let graded = env.attempts.set_grade(open.id, &grade).await.unwrap();
    open.outcome = Outcome::Partial;
    open.grade = Some(grade.clone());
    assert_eq!(graded, open);
    // Already graded: a second grade does not overwrite it.
    let other = Grade {
        verdict: Verdict::Correct,
        feedback: "ok".into(),
        misconception: None,
    };
    assert_eq!(env.attempts.set_grade(open.id, &other).await.unwrap(), open);
    assert!(matches!(
        env.attempts
            .set_grade(AttemptId::new(uid(0x1)), &grade)
            .await,
        Err(RepositoryError::NotFound)
    ));
}

#[tokio::test]
async fn a_completed_lesson_takes_no_attempts() {
    let env = env().await;
    let l = lesson(&env, 1).await;
    let job = Job::queued_for_lesson(
        JobId::new(uid(0x500)),
        env.topic,
        l.id,
        JobKind::LessonCompletion,
        T0,
    );
    env.lessons.complete(l.id, T0, &job).await.unwrap();
    assert!(matches!(
        env.attempts
            .record(l.id, &[attempt(&l, 1, "q1", 1, Outcome::Correct)])
            .await,
        Err(RepositoryError::Conflict)
    ));
    assert!(matches!(
        env.attempts
            .record(
                LessonId::new(uid(0x9)),
                &[attempt(&l, 2, "q1", 1, Outcome::Correct)]
            )
            .await,
        Err(RepositoryError::NotFound)
    ));
    assert_eq!(
        env.attempts.list(l.id).await.unwrap(),
        Vec::<Attempt>::new()
    );
}
