use super::*;

fn plan_with_flow() -> Value {
    let mut p = plan_json();
    p["concepts"][0]["visual"] = json!("flow");
    p
}

fn doc_with_flow() -> Value {
    let mut doc = lesson_doc();
    let flow = json!({
        "id": "fig", "type": "diagram", "layout": "flow", "title": "Ordine",
        "nodes": [{"id": "a", "label": "Apri"}, {"id": "b", "label": "Scatta"}],
        "edges": [{"from": "a", "to": "b"}],
        "caption": "Guarda **l'ordine**.", "alt": "Due passi in fila."
    });
    doc["blocks"].as_array_mut().unwrap().insert(2, flow);
    doc
}

fn has_figure(env: &LessonEnv) -> bool {
    env.f.repo.0.lock().unwrap().lessons[0].document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["type"] == "diagram")
}

#[tokio::test]
async fn a_missing_planned_figure_triggers_an_improvement_that_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&lesson_doc().to_string()),
        say(&doc_with_flow().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[2].trace_label, "generator.improve");
        let message = format!("{:?}", calls[2].messages);
        assert!(
            message.contains("Il piano chiede per il concetto"),
            "{message}"
        );
    }
    assert!(has_figure(&env));
    assert!(warning_codes(&env).is_empty(), "never stored");
}

#[tokio::test]
async fn an_improvement_that_loses_the_planned_figure_keeps_the_original() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&doc_with_flow().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    // The figure is there: no improvement at all.
    assert_eq!(env.calls(), 2);
    assert!(has_figure(&env));
}

#[tokio::test]
async fn an_improvement_without_the_figure_keeps_the_first_draft() {
    // The first draft has the figure and a quality warning: improve runs, and
    // the answer drops the figure although it has fewer warnings.
    let mut first = doc_with_flow();
    let at = first["blocks"].as_array().unwrap().len() / 2;
    let code = |id: &str, caption: &str| json!({"id": id, "type": "code", "language": "rust", "code": "let x = 1;", "caption": caption});
    let blocks = first["blocks"].as_array_mut().unwrap();
    blocks.insert(at, code("w2", "Versione invece corretta"));
    blocks.insert(at, code("w1", "Esempio"));
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say(&first.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
    assert_eq!(warning_codes(&env), ["consecutive_code_blocks"]);
}

#[tokio::test]
async fn two_flows_requested_one_present_keeps_the_draft_when_improve_drops_it() {
    let mut plan = plan_with_flow();
    plan["concepts"][1]["visual"] = json!("flow");
    let env = LessonEnv::new(vec![
        say(&plan.to_string()),
        say(&doc_with_flow().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
}

#[tokio::test]
async fn no_improvement_runs_when_the_repairs_are_exhausted() {
    let env = LessonEnv::new(vec![
        say(&plan_with_flow().to_string()),
        say("not json"),
        say("still not json"),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 4);
    let calls = env.f.llm.requests.lock().unwrap();
    assert!(calls.iter().all(|c| c.trace_label != "generator.improve"));
}

/// The fixture with a fourth concept section, so its concept sections line
/// up with the four concepts of `plan_json`; `warning` puts the typical
/// mistake callout in the first one.
fn doc_aligned(warning: bool, flow: bool) -> Value {
    let mut doc = if flow { doc_with_flow() } else { lesson_doc() };
    let blocks = doc["blocks"].as_array_mut().unwrap();
    let at = blocks
        .iter()
        .position(|b| b["type"] == "heading" && b["text"] == "Ripasso e pratica")
        .unwrap();
    blocks.insert(
        at,
        json!({"id": "p4", "type": "paragraph", "text": "Il diaframma regola la luce."}),
    );
    blocks.insert(
        at,
        json!({"id": "h4", "type": "heading", "level": 2, "text": "Diaframma e luce"}),
    );
    if warning {
        // The fixture's only callout of the first section becomes the
        // typical mistake (a second callout would trip `too_many_callouts`).
        let callout = blocks.iter_mut().find(|b| b["type"] == "callout").unwrap();
        callout["variant"] = json!("warning");
    }
    doc
}

fn has_warning_callout(env: &LessonEnv) -> bool {
    env.f.repo.0.lock().unwrap().lessons[0].document["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["type"] == "callout" && b["variant"] == "warning")
}

#[tokio::test]
async fn a_missing_typical_mistake_triggers_an_improvement_that_is_kept() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&doc_aligned(false, false).to_string()),
        say(&doc_aligned(true, false).to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    {
        let calls = env.f.llm.requests.lock().unwrap();
        assert_eq!(calls[2].trace_label, "generator.improve");
        let message = format!("{:?}", calls[2].messages);
        assert!(
            message.contains("Il piano dà per il concetto «Stop» un errore tipico"),
            "{message}"
        );
    }
    assert!(has_warning_callout(&env));
    assert!(warning_codes(&env).is_empty(), "never stored");
}

#[tokio::test]
async fn an_improvement_that_drops_the_typical_mistake_keeps_the_original() {
    // The draft has the callout and a quality warning: improve runs and
    // answers with fewer warnings but without the callout. Better on the
    // warnings alone, so only the lost callout can reject it.
    let mut first = doc_aligned(true, false);
    let at = first["blocks"].as_array().unwrap().len() / 2;
    let code = |id: &str, caption: &str| json!({"id": id, "type": "code", "language": "rust", "code": "let x = 1;", "caption": caption});
    let blocks = first["blocks"].as_array_mut().unwrap();
    blocks.insert(at, code("w2", "Versione invece corretta"));
    blocks.insert(at, code("w1", "Esempio"));
    let candidate = doc_aligned(false, false);
    let score = |doc: &Value| {
        domain::warning_score(
            &domain::LessonValidator::new(
                domain::ValidatorConfig::default(),
                domain::ValidationContext::fixture(),
            )
            .quality_warnings(doc, domain::ContentLanguage::It),
        )
    };
    assert!(
        score(&candidate) < score(&first),
        "the candidate must improve the warnings"
    );
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&first.to_string()),
        say(&candidate.to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_warning_callout(&env));
    assert!(
        !warning_codes(&env).is_empty(),
        "the original, warnings included, is kept"
    );
}

#[tokio::test]
async fn an_improvement_that_adds_the_callout_but_loses_the_figure_is_discarded() {
    let mut plan = plan_with_flow();
    plan["concepts"][0]["pitfall"] = json!("Pensare che uno stop sommi luce.");
    let env = LessonEnv::new(vec![
        say(&plan.to_string()),
        say(&doc_aligned(false, true).to_string()),
        say(&doc_aligned(true, false).to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.calls(), 3);
    assert!(has_figure(&env));
    assert!(!has_warning_callout(&env));
}
