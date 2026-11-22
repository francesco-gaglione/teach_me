use super::*;

// ----------------------------------------------------------------- chat

#[tokio::test]
async fn a_chat_message_from_a_lesson_tells_the_tutor_which_one() {
    let env = LessonEnv::new(vec![say("Certo.")]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    let chat = crate::ChatService::new(
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.repo.clone(),
        env.f.events.clone(),
        env.f.llm.clone(),
        crate::TutorPrompts::embedded(),
        env.f.clock.clone(),
        env.ids.clone(),
        crate::fakes::owner_keys(),
        crate::ChatSettings {
            flush_interval: SLOW,
            history_token_budget: 8000,
            max_tool_iterations: 4,
        },
    )
    .with_lessons(env.f.repo.clone());

    let unknown = chat
        .send_about(
            domain::UserId::OWNER,
            env.f.topic,
            Some("ciao"),
            Some(LessonId::new(Uuid::from_u128(5))),
        )
        .await;
    assert!(
        matches!(unknown, Err(AppError::Validation(v)) if v[0].field == "lesson_id" && v[0].issue.as_str() == "not_found")
    );

    chat.send_about(
        domain::UserId::OWNER,
        env.f.topic,
        Some("Non capisco lo stop"),
        Some(lesson.id),
    )
    .await
    .unwrap();
    for _ in 0..200 {
        if env.calls() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let calls = env.f.llm.requests.lock().unwrap();
    let context = &calls[0].system[1].text;
    assert!(
        context.contains("Lezione 1: \"Esposizione: diaframma, tempo e ISO\""),
        "{context}"
    );
    assert!(context.contains("Obiettivo: "), "{context}");
    let _ = (StopReason::EndTurn, LlmEvent::Usage(Usage::default()));
}
