use super::*;

// ----------------------------------------------------------- documents

use crate::document_tests::{section, seed_ready_document};

/// A ready 12-page PDF with two sections, 5000 characters per page.
fn with_pdf(env: &LessonEnv) -> (domain::DocumentId, ResourceId) {
    let pages: Vec<String> = (1..=12)
        .map(|p| format!("Testo della pagina {p}. {}", "x".repeat(5000)))
        .collect();
    seed_ready_document(
        &env.f,
        1,
        "Fotografia pratica",
        pages,
        vec![
            section("s1", "1 Esposizione", 1, 1, 6),
            section("s2", "2 Stop", 1, 7, 12),
        ],
    )
}

fn plan_with_pdf(doc: domain::DocumentId, resource: ResourceId, sections: &[&str]) -> Value {
    let mut plan = plan_json();
    plan["primary_source_id"] = json!(resource.to_string());
    plan["other_source_ids"] = json!([Uuid::from_u128(RESOURCE).to_string()]);
    plan["document_sections"] = json!([{"document_id": doc.to_string(), "section_ids": sections}]);
    plan
}

fn lesson_with_pdf(resource: ResourceId) -> Value {
    let mut doc = lesson_doc();
    doc["sources"][0]["resource_id"] = json!(resource.to_string());
    doc["sources"][0]["url"] = Value::Null;
    doc
}

#[tokio::test]
async fn a_lesson_follows_the_chosen_pdf_sections_and_cites_it_without_a_url() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    assert_eq!(with_pdf(&env), (DOC_ID, RES_ID));
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    // The Planner sees the index with ids, the document among the sources.
    let planner = &calls[0].system[1].text;
    assert!(planner.contains("## Documenti caricati"), "{planner}");
    assert!(planner.contains(&DOC_ID.to_string()));
    assert!(planner.contains("[s2] 2 Stop (pp. 7-12)"));
    assert!(planner.contains(&format!("{RES_ID} · Fotografia pratica")));
    // The Generator gets the text of section s2 only, cut at the limit.
    let generator = &calls[1].system[1].text;
    assert!(generator.contains("#### 2 Stop (pp. 7-12)"), "{generator}");
    assert!(generator.contains("[p. 7]") && generator.contains("Testo della pagina 7."));
    assert!(!generator.contains("Testo della pagina 3."));
    // 24,000 characters hold four pages of about 5,000; the rest is noted.
    assert!(generator.contains("[p. 10]") && !generator.contains("[p. 11]"));
    assert!(generator.contains(
        "Pagine omesse perché il testo supera il limite: \"Fotografia pratica\": pagine 11-12"
    ));
    let excerpt_len = generator
        .split("## Estratti delle fonti")
        .nth(1)
        .unwrap()
        .split("## Glossario")
        .next()
        .unwrap()
        .chars()
        .count();
    assert!(excerpt_len <= 24_000 + 60, "{excerpt_len}");
    drop(calls);

    let s = env.f.repo.0.lock().unwrap();
    let saved = &s.lessons[0].document;
    assert_eq!(
        saved["sources"][0]["url"],
        Value::Null,
        "a PDF has no address"
    );
    assert_eq!(saved["sources"][1]["url"], "https://b.example/stops");
}

#[tokio::test]
async fn a_pdf_source_with_a_url_or_a_web_source_without_one_is_repaired() {
    let mut wrong = lesson_with_pdf(RES_ID);
    wrong["sources"][0]["url"] = json!("https://invented.example/pdf");
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s1"]).to_string()),
        say(&wrong.to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].trace_label, "generator.repair");
    let message = format!("{:?}", calls[2].messages[0].parts);
    assert!(message.contains("/sources/0/url"), "{message}");
    assert!(message.contains("deve essere null"), "{message}");
}

#[tokio::test]
async fn a_plan_that_invents_a_section_is_repaired_by_the_planner() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s9"]).to_string()),
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s1"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].trace_label, "planner.repair");
    let message = format!("{:?}", calls[1].messages[0].parts);
    assert!(
        message.contains("non è una sezione del documento"),
        "{message}"
    );
}

#[tokio::test]
async fn a_plan_from_web_sources_only_has_no_excerpts_and_the_old_context() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let generator = &calls[1].system[1].text;
    assert!(
        generator.contains("## Estratti delle fonti\nNessuno."),
        "{generator}"
    );
}

#[tokio::test]
async fn the_planner_sees_document_ids_and_which_sections_earlier_lessons_used() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    // An earlier lesson followed section s1 of the PDF; another used web only.
    env.store_lesson_with_plan(
        lesson_with_pdf(RES_ID),
        plan_with_pdf(DOC_ID, RES_ID, &["s1"]),
    )
    .await;
    env.store_lesson(lesson_doc()).await;

    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let planner = &calls[0].system[1].text;
    assert!(
        planner.contains(&format!("{RES_ID} · Fotografia pratica — nessun sito: PDF caricato dalla persona · documento {DOC_ID}")),
        "{planner}"
    );
    let lessons: Vec<&str> = planner
        .lines()
        .filter(|l| l.contains("· lezione "))
        .collect();
    assert_eq!(lessons.len(), 2);
    assert!(
        lessons[0]
            .ends_with(" · sezioni del PDF usate: \"Fotografia pratica\" 1 Esposizione (pp. 1-6)"),
        "{}",
        lessons[0]
    );
    assert!(!lessons[1].contains("sezioni del PDF"), "{}", lessons[1]);
    // The Generator's source line names the document too.
    assert!(
        calls[1].system[1]
            .text
            .contains(&format!("documento {DOC_ID}"))
    );
}

#[tokio::test]
async fn a_plan_with_document_sections_has_a_reading_step_and_shows_the_sources() {
    let env = LessonEnv::new(vec![
        say(&plan_with_pdf(DOC_ID, RES_ID, &["s2", "s1"]).to_string()),
        say(&lesson_with_pdf(RES_ID).to_string()),
    ])
    .await;
    with_pdf(&env);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.steps(),
        ["planning", "reading", "writing", "checking", "saving"]
    );

    let owner = domain::UserId::OWNER;
    let lesson = env.f.repo.0.lock().unwrap().lessons[0].lesson.clone();
    let expected = vec![crate::DocumentSource {
        document_id: DOC_ID,
        title: "Fotografia pratica".into(),
        page_start: 1,
        page_end: 12,
    }];
    let view = env.lessons().view(owner, lesson.id).await.unwrap();
    assert_eq!(view.document_sources, expected);
    let page = env
        .lessons()
        .list(
            owner,
            env.f.topic,
            domain::PageRequest {
                after: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].document_sources, expected);

    // Deleting the document removes it from the sources.
    env.f.repo.0.lock().unwrap().documents.clear();
    let view = env.lessons().view(owner, lesson.id).await.unwrap();
    assert!(view.document_sources.is_empty());
}
