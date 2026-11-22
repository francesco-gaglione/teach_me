use super::*;

// ----------------------------------------------------------- read aloud

fn readable_count(document: &Value) -> usize {
    domain::readable_blocks(document).len()
}

async fn queued_blocks(env: &LessonEnv, lesson: LessonId) -> usize {
    let a = env.audio.as_ref().unwrap();
    let voice = a.preparer.voice_of(domain::UserId::OWNER).await.unwrap();
    a.preparer.states(lesson, &voice).await.unwrap().len()
}

#[tokio::test]
async fn a_generated_lesson_has_its_audio_queued_and_made_in_the_background() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await
    .with_audio();
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let Some(JobResult::LessonGeneration { lesson_id, .. }) = job.result else {
        panic!("no result");
    };
    let readable = readable_count(&lesson_doc());
    assert!(readable > 0);
    assert_eq!(queued_blocks(&env, lesson_id).await, readable);

    let a = env.audio.as_ref().unwrap();
    for _ in 0..readable {
        assert_eq!(
            a.worker.step().await.unwrap(),
            crate::audio_queue::Step::Made
        );
    }
    assert_eq!(
        a.worker.step().await.unwrap(),
        crate::audio_queue::Step::Idle
    );
    assert_eq!(queued_blocks(&env, lesson_id).await, 0);
}

#[tokio::test]
async fn lessons_generate_and_open_with_read_aloud_off() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson = env.store_lesson(lesson_doc()).await;
    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
}

#[tokio::test]
async fn opening_a_lesson_queues_its_missing_audio_and_requeues_failed_blocks() {
    let env = LessonEnv::new(vec![]).await.with_audio();
    let lesson = env.store_lesson(lesson_doc()).await;
    let readable = readable_count(&lesson_doc());
    assert_eq!(queued_blocks(&env, lesson.id).await, 0);

    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(queued_blocks(&env, lesson.id).await, readable);

    // Every block fails three times.
    let a = env.audio.as_ref().unwrap();
    *a.tts.fail.lock().unwrap() = crate::audio::tests::FailMode::Rejects;
    for _ in 0..3 * readable {
        a.worker.step().await.unwrap();
    }
    let voice = a.preparer.voice_of(domain::UserId::OWNER).await.unwrap();
    let states = a.preparer.states(lesson.id, &voice).await.unwrap();
    assert!(
        states
            .values()
            .all(|s| *s == domain::BlockAudioState::Failed)
    );

    // Reopened: all queued again.
    env.lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    let states = a.preparer.states(lesson.id, &voice).await.unwrap();
    assert_eq!(states.len(), readable);
    assert!(
        states
            .values()
            .all(|s| *s == domain::BlockAudioState::Queued)
    );
}

#[tokio::test]
async fn opening_a_lesson_moves_it_ahead_of_the_others() {
    let env = LessonEnv::new(vec![]).await.with_audio();
    let first = env.store_lesson(lesson_doc()).await;
    let second = env.store_lesson(lesson_doc()).await;
    let a = env.audio.as_ref().unwrap();
    let doc = lesson_doc();
    let owner = domain::UserId::OWNER;
    a.preparer
        .prepare(owner, first.id, &doc, domain::ContentLanguage::It, false)
        .await
        .unwrap();
    a.preparer
        .prepare(owner, second.id, &doc, domain::ContentLanguage::It, false)
        .await
        .unwrap();

    env.lessons().view(owner, second.id).await.unwrap();
    let next = domain::AudioQueue::claim_next(a.queue.as_ref())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.request.lesson_id, second.id);
}

struct BrokenQueue;

#[async_trait::async_trait]
impl domain::AudioQueue for BrokenQueue {
    async fn enqueue(
        &self,
        _: LessonId,
        _: &str,
        _: &[domain::AudioRequest],
        _: bool,
    ) -> Result<(), domain::RepositoryError> {
        Err(domain::RepositoryError::Storage("down".into()))
    }
    async fn claim_next(&self) -> Result<Option<domain::QueuedAudio>, domain::RepositoryError> {
        Ok(None)
    }
    async fn complete(&self, _: i64) -> Result<(), domain::RepositoryError> {
        Ok(())
    }
    async fn release(&self, _: i64) -> Result<(), domain::RepositoryError> {
        Ok(())
    }
    async fn record_failure(
        &self,
        _: i64,
        _: u32,
    ) -> Result<domain::BlockAudioState, domain::RepositoryError> {
        Ok(domain::BlockAudioState::Failed)
    }
    async fn recover(&self) -> Result<u64, domain::RepositoryError> {
        Ok(0)
    }
    async fn states(
        &self,
        _: LessonId,
        _: &str,
    ) -> Result<std::collections::HashMap<String, domain::BlockAudioState>, domain::RepositoryError>
    {
        Ok(std::collections::HashMap::new())
    }
}

#[tokio::test]
async fn a_queue_that_fails_never_fails_the_opening_or_the_generation() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await
    .with_audio();
    // The same worker env, but its queue refuses every write.
    let a = env.audio.take().unwrap();
    let (preparer, worker) = crate::AudioPreparer::new(
        a.cache.clone(),
        Arc::new(BrokenQueue),
        crate::fakes::MemUsers::with_owner(),
        crate::AudioSettings::default(),
    );
    env.audio = Some(crate::audio_queue::tests::Env {
        preparer,
        worker,
        ..a
    });

    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson = env.store_lesson(lesson_doc()).await;
    let view = env
        .lessons()
        .view(domain::UserId::OWNER, lesson.id)
        .await
        .unwrap();
    assert_eq!(view.lesson.id, lesson.id);
}
