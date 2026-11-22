use super::*;

// ----------------------------------------------------------- generation

#[tokio::test]
async fn a_valid_document_at_the_first_try_is_saved_with_its_extras() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let lesson_id = match job.result {
        Some(JobResult::LessonGeneration {
            lesson_id,
            lesson_number,
        }) => {
            assert_eq!(lesson_number, 1);
            lesson_id
        }
        other => panic!("unexpected result {other:?}"),
    };
    let lesson = env.lesson(lesson_id);
    assert_eq!(lesson.status, LessonStatus::NotStarted);
    assert_eq!(
        (
            lesson.number,
            lesson.mission_version,
            lesson.catalog_version
        ),
        (1, 1, 1)
    );
    assert_eq!(lesson.title, "Esposizione: diaframma, tempo e ISO");

    let (document, plan, requests): (Value, Value, Vec<ComponentRequest>) = {
        let s = env.f.repo.0.lock().unwrap();
        let row = &s.lessons[0];
        (row.document.clone(), row.plan.clone(), row.requests.clone())
    };
    // The model's URL never reaches the learner: the Resource's does.
    assert_eq!(document["sources"][0]["url"], "https://a.example/exposure");
    assert_eq!(document["sources"][1]["url"], "https://b.example/stops");
    assert!(
        document.to_string().contains("rubric"),
        "the stored document is complete"
    );
    assert_eq!(plan["objective"], plan_json()["objective"]);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        (requests[0].block_id.as_str(), requests[0].wanted.as_str()),
        ("mc_sim", "exposure_simulator")
    );
    assert_eq!(requests[0].fallback.as_array().unwrap().len(), 2);

    assert_eq!(env.steps(), ["planning", "writing", "checking", "saving"]);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].role, domain::llm::LlmRole::Planner);
    assert_eq!(calls[1].role, domain::llm::LlmRole::Generator);
    for call in calls.iter() {
        assert_eq!(call.system.len(), 2);
        assert!(call.system[0].cache_breakpoint);
        assert_eq!(call.messages.len(), 1);
        assert_eq!(call.messages[0].role, R::User);
    }
    assert!(matches!(calls[0].output, OutputFormat::JsonSchema { .. }));
    assert_eq!(
        calls[1].output,
        OutputFormat::Text,
        "the document is emulated JSON text"
    );
    // The community was not offered to the Planner; the knowledge sources were.
    let planner_context = &calls[0].system[1].text;
    assert!(planner_context.contains(&Uuid::from_u128(RESOURCE).to_string()));
    assert!(!planner_context.contains(&Uuid::from_u128(COMMUNITY).to_string()));
    // The Generator sees the plan and only the plan's sources.
    assert!(calls[1].system[1].text.contains("[PRIMARIA]"));
    // Both roles read the mission's starting point.
    for call in calls.iter() {
        assert!(
            call.system[1].text.contains(
                "Punto di partenza: Fotografo da due anni in automatico, mai con soggetti in movimento\n"
            ),
            "{}",
            call.system[1].text
        );
    }
    // No placeholder is left over, and a topic without records says so.
    for call in calls.iter() {
        assert!(
            !call.system[1].text.contains("{{"),
            "{}",
            call.system[1].text
        );
    }
    assert!(
        calls[1].system[1]
            .text
            .contains("(record attivi)\nNessuno.\n"),
        "{}",
        calls[1].system[1].text
    );
}

#[tokio::test]
async fn a_mission_without_starting_point_reaches_planner_and_generator_as_undeclared() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    {
        // A mission saved before contract v1.2.
        let mut s = env.f.repo.0.lock().unwrap();
        let m = &mut s.missions[0];
        m.content = MissionContent::parse_stored(
            m.content.why(),
            None,
            m.content.success_criteria(),
            &[],
            &[],
        )
        .unwrap();
    }
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 2);
    for call in calls.iter() {
        let context = &call.system[1].text;
        assert!(
            context.contains("Punto di partenza: non dichiarato\n"),
            "{context}"
        );
        assert!(!context.contains("{{"), "{context}");
    }
}

#[tokio::test]
async fn the_generator_sees_the_plan_by_basis_and_the_active_records() {
    let mut plan = plan_json();
    plan["prerequisites"] = json!([
        {"statement": "Sa cos'è la luce", "basis": "general", "record_numbers": []},
        {"statement": "Conosce il diaframma", "basis": "record", "record_numbers": [2]}
    ]);
    let env = LessonEnv::new(vec![say(&plan.to_string()), say(&lesson_doc().to_string())]).await;
    let earlier = env.store_lesson(lesson_doc()).await;
    {
        let topic = env.f.topic;
        let mut s = env.f.repo.0.lock().unwrap();
        for (n, status) in [(1, RecordStatus::Superseded), (2, RecordStatus::Active)] {
            s.records.push(LearningRecord {
                id: LearningRecordId::new(Uuid::from_u128(0x90 + u128::from(n))),
                topic_id: topic,
                number: n,
                kind: RecordKind::Demonstrated,
                title: format!("Record {n}"),
                body: "Spiega il diaframma.".into(),
                evidence: "e".into(),
                status,
                superseded_by: None,
                lesson_id: earlier.id,
                created_at: datetime!(2026-11-08 10:00 UTC),
            });
        }
    }
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let calls = env.f.llm.requests.lock().unwrap();
    let generator = &calls[1].system[1].text;
    assert!(!generator.contains("{{"), "{generator}");
    for line in [
        "Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):\n- Sa cos'è la luce\n",
        "Prerequisiti dichiarati dalla persona (nessuna sezione, richiamo di una clausola):\n- nessuno\n",
        "Prerequisiti dimostrati (non si ripassano):\n- Conosce il diaframma [record 2]\n",
        "1. Stop: Uno stop raddoppia o dimezza la luce.\n   Esempio: ",
        "(record attivi)\n- [2] (demonstrated, lezione 1) Record 2: Spiega il diaframma.\n",
    ] {
        assert!(generator.contains(line), "{line}\n---\n{generator}");
    }
    // Only active records are shown.
    assert!(!generator.contains("[1] (demonstrated"), "{generator}");
}

#[tokio::test]
async fn an_invalid_document_is_repaired_with_the_same_system_blocks() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&format!("Ecco la lezione:\n```json\n{broken}\n```")),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 3);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].trace_label, "generator.repair");
    assert_eq!(
        calls[2].system, calls[1].system,
        "same blocks: the cache is reused"
    );
    let message = match &calls[2].messages[..] {
        [m] => format!("{:?}", m.parts),
        other => panic!("one user message expected, got {other:?}"),
    };
    assert!(message.contains("unknown_resource"), "{message}");
    assert!(message.contains("/sources/0/resource_id"), "{message}");
    drop(calls);
    let repair_steps: Vec<_> = env
        .f
        .events
        .snapshot()
        .into_iter()
        .filter_map(|e| match e.payload {
            domain::TopicEventPayload::JobUpdated(j) => j.progress,
            _ => None,
        })
        .filter(|p| p.message.as_deref() == Some("Correggo la lezione"))
        .collect();
    assert_eq!(repair_steps[0].step, "writing");
    assert_eq!(repair_steps[0].done, Some(2));
}

#[tokio::test]
async fn a_valid_document_without_warnings_costs_no_extra_call() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 2);
    assert!(warning_codes(&env).is_empty());
}

#[tokio::test]
async fn a_better_improvement_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_with_warning().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert_eq!(
        env.f.llm.requests.lock().unwrap()[2].trace_label,
        "generator.improve"
    );
    assert!(warning_codes(&env).is_empty());
    let id = env.f.repo.0.lock().unwrap().lessons[0].lesson.id;
    let stored = env.f.repo.0.lock().unwrap().lessons[0].document.clone();
    assert!(
        stored["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["id"] != "w1"),
        "{id}"
    );
}

#[tokio::test]
async fn a_worse_or_invalid_improvement_keeps_the_first_document() {
    for second in [doc_with_warning().to_string(), "non è json".to_owned()] {
        let env = LessonEnv::new(vec![
            say(&plan_json().to_string()),
            say(&doc_with_warning().to_string()),
            say(&second),
        ])
        .await;
        let job = env.generate().await;
        assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
        assert_eq!(env.calls(), 3, "one improvement only");
        assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
    }
}

#[tokio::test]
async fn a_failed_improvement_call_does_not_fail_the_job() {
    // The script ends after the document: the next call errors.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_with_warning().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn warnings_do_not_take_attempts_from_the_errors() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    // Invalid, then valid with a warning: the improvement fits (3 documents).
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&doc_with_warning().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    assert!(warning_codes(&env).is_empty());

    // Two invalid, then valid with a warning: no attempt left, kept as it is.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&broken.to_string()),
        say(&doc_with_warning().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn a_source_that_is_not_usable_cannot_be_cited() {
    // The community resource exists in the topic but was never offered.
    let mut doc = lesson_doc();
    doc["sources"][0]["resource_id"] = json!(Uuid::from_u128(COMMUNITY).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(format!("{:?}", calls[2].messages).contains("unknown_resource"));
}

#[tokio::test]
async fn a_truncated_or_refused_answer_is_an_upstream_failure_not_a_repair() {
    // The Generator hits the token limit: the JSON is cut, repairing is useless.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        cut_short("{\"title\": \"a metà", StopReason::MaxTokens),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 2);
    assert_eq!(env.lesson_rows(), 0);

    // The Planner refuses.
    let env = LessonEnv::new(vec![cut_short("", StopReason::Refusal)]).await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 1);

    // A repair that is cut short fails the same way.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"blocks\": []}"),
        cut_short("{\"title\"", StopReason::MaxTokens),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 3);

    // The Recorder too: nothing is stored.
    let env = LessonEnv::new(vec![cut_short("{\"records\": [", StopReason::MaxTokens)]).await;
    let lesson = env.store_lesson(lesson_doc()).await;
    answer_q1_right(&env, &lesson).await;
    let job = completed(&env, &lesson).await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
    assert_eq!(env.calls(), 1);
    assert!(env.f.repo.0.lock().unwrap().completions.is_empty());
}

#[tokio::test]
async fn three_invalid_documents_fail_the_job_with_invalid_output() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"title\": 1}"),
        say("non è nemmeno json"),
        say("{\"blocks\": []}"),
    ])
    .await;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 4, "planner + 3 documents, no fourth");
    assert_eq!(env.lesson_rows(), 0);
    assert!(env.steps().contains(&"checking".to_owned()));
}

#[tokio::test]
async fn a_plan_with_invented_references_is_repaired_once_then_rejected() {
    let mut bad = plan_json();
    bad["primary_source_id"] = json!(Uuid::from_u128(COMMUNITY).to_string());
    let env = LessonEnv::new(vec![
        say(&bad.to_string()),
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[1].trace_label, "planner.repair");
        assert!(format!("{:?}", calls[1].messages).contains("primary_source_id"));
    }

    let env = LessonEnv::new(vec![say(&bad.to_string()), say(&bad.to_string())]).await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 2, "the Generator is never called");
}

#[tokio::test]
async fn without_a_usable_source_the_planner_is_not_called() {
    let env = LessonEnv::new(vec![]).await;
    env.f.repo.0.lock().unwrap().resources[0]
        .1
        .resources
        .clear();
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::InvalidOutput);
    assert_eq!(env.calls(), 0);
}

#[tokio::test]
async fn a_storage_failure_while_saving_leaves_no_lesson_and_fails_the_job() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.f.repo.0.lock().unwrap().fail_learning_writes = true;
    let job = env.generate().await;
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    assert_eq!(env.lesson_rows(), 0);
}

#[tokio::test]
async fn a_generation_run_again_after_a_crash_does_not_write_a_second_lesson() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded);
    // The backend died after saving but before the job was marked done.
    {
        let mut s = env.f.repo.0.lock().unwrap();
        let j = s.jobs.iter_mut().find(|j| j.id == job.id).unwrap();
        j.status = JobStatus::Queued;
        j.result = None;
        j.finished_at = None;
    }
    assert!(env.runner().run_next().await.unwrap());
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 2, "no new model calls");
    assert_eq!(env.job(job.id).status, JobStatus::Succeeded);
}

#[tokio::test]
async fn generation_needs_a_ready_topic_and_no_active_generation() {
    let env = LessonEnv::new(vec![]).await;
    env.lessons()
        .request_generation(domain::UserId::OWNER, env.f.topic)
        .await
        .unwrap();
    assert!(matches!(
        env.lessons()
            .request_generation(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::JobActive))
    ));
    env.f.repo.0.lock().unwrap().topics[0].state = TopicState::Researching;
    assert!(matches!(
        env.lessons()
            .request_generation(domain::UserId::OWNER, env.f.topic)
            .await,
        Err(AppError::Conflict(ConflictReason::TopicNotReady))
    ));
}
