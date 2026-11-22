use super::*;

// ------------------------------------------------- web fetch (ADR 0009)

fn fetch_use(url: &str, id: &str) -> Value {
    json!({ "type": "server_tool_use", "id": id, "name": "web_fetch", "input": { "url": url } })
}

fn fetch_page(url: &str, id: &str, data: &str) -> Value {
    json!({ "type": "web_fetch_tool_result", "tool_use_id": id, "content": {
        "type": "web_fetch_result", "url": url, "retrieved_at": "2026-11-11T10:00:00Z",
        "content": { "type": "document", "title": "T",
                     "source": { "type": "text", "media_type": "text/plain", "data": data } } } })
}

fn fetch_failure(id: &str, code: &str) -> Value {
    json!({ "type": "web_fetch_tool_result", "tool_use_id": id,
            "content": { "type": "web_fetch_tool_result_error", "error_code": code } })
}

/// The provider read `url` (result block `result`), then the model wrote
/// `text`; `stop` is how the answer ended.
fn fetched(url: &str, id: &str, result: Value, text: Option<&str>, stop: StopReason) -> Vec<Step> {
    fetched_reporting(url, id, result, text, stop, 1)
}

/// Like [`fetched`], with the read count the provider reports in the usage
/// (0 is what `OpenRouter` does).
fn fetched_reporting(
    url: &str,
    id: &str,
    result: Value,
    text: Option<&str>,
    stop: StopReason,
    reported: u32,
) -> Vec<Step> {
    let usage = Usage {
        web_fetches: reported,
        ..Usage::default()
    };
    let mut steps = vec![
        Step::Event(LlmEvent::ServerToolUse {
            block: 0,
            name: "web_fetch".into(),
            input: json!({ "url": url }),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 0,
            provider: "anthropic".into(),
            payload: fetch_use(url, id),
        }),
        Step::Event(LlmEvent::ProviderBlock {
            block: 1,
            provider: "anthropic".into(),
            payload: result.clone(),
        }),
    ];
    if result["content"]["type"] == "web_fetch_tool_result_error" {
        steps.push(Step::Event(LlmEvent::ServerFetchError {
            block: 1,
            error_code: result["content"]["error_code"].as_str().unwrap().to_owned(),
        }));
    }
    if let Some(text) = text {
        steps.push(Step::Event(LlmEvent::TextDelta {
            block: 2,
            text: text.to_owned(),
        }));
    }
    steps.push(Step::Event(LlmEvent::Finished {
        stop_reason: stop,
        usage,
    }));
    steps
}

fn user_text(m: &domain::llm::LlmMessage) -> String {
    m.parts
        .iter()
        .filter_map(|p| match p {
            domain::llm::Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

fn fetch_tool(max_uses: u32, domains: &[&str]) -> domain::llm::ToolDef {
    domain::llm::ToolDef::ServerWebFetch {
        max_uses,
        allowed_domains: domains.iter().map(|d| (*d).to_owned()).collect(),
        max_content_tokens: 8_000,
    }
}

#[tokio::test]
async fn with_the_tool_off_the_generator_request_is_what_it_always_was() {
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].tools, []);
    assert_eq!(calls[1].messages.len(), 1);
    assert_eq!(user_text(&calls[1].messages[0]), "Procedi.");
    assert_eq!(calls[1].output, OutputFormat::Text);
    assert_eq!(calls[1].trace_label, "generator.write");
}

#[tokio::test]
async fn with_the_tool_on_the_generator_gets_it_and_the_sources_to_read() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(
        env.steps(),
        ["planning", "reading", "writing", "checking", "saving"]
    );

    let calls = env.f.llm.requests.lock().unwrap();
    // The Planner is not touched.
    assert_eq!(calls[0].tools, []);
    assert_eq!(user_text(&calls[0].messages[0]), "Procedi.");
    let generator = &calls[1];
    assert_eq!(
        generator.tools,
        [fetch_tool(2, &["a.example", "b.example"])]
    );
    let message = user_text(&generator.messages[0]);
    let list = format!(
        "- {} · Fonte 81 — https://a.example/exposure\n- {} · Fonte 82 — https://b.example/stops",
        Uuid::from_u128(RESOURCE),
        Uuid::from_u128(RESOURCE_2)
    );
    assert!(message.starts_with("Procedi.\n\n"), "{message}");
    assert!(message.trim_end().ends_with(&list), "{message}");
    assert!(!message.contains("{{"), "{message}");
    // The URLs of the list are the ones of the sources section.
    assert!(
        generator.system[1]
            .text
            .contains("https://a.example/exposure")
    );
    assert!(generator.system[1].text.contains("https://b.example/stops"));
}

#[tokio::test]
async fn the_ceiling_is_the_smaller_of_the_setting_and_the_web_sources() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    env.generate().await;
    let calls = env.f.llm.requests.lock().unwrap();
    // Only the primary source is listed and allowed: a second listed page
    // would fail with `max_uses_exceeded`.
    assert_eq!(calls[1].tools, [fetch_tool(1, &["a.example"])]);
}

#[tokio::test]
async fn a_provider_without_web_fetch_gets_neither_the_tool_nor_the_list() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    env.f
        .llm
        .without_web_fetch
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].tools, []);
    assert_eq!(user_text(&calls[1].messages[0]), "Procedi.");
}

#[tokio::test]
async fn a_paused_fetch_turn_is_continued_with_its_blocks_untouched() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "Gli stop raddoppiano la luce.");
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(url, "srv_1", page.clone(), None, StopReason::PauseTurn),
        fetched(
            "https://b.example/stops",
            "srv_2",
            fetch_page("https://b.example/stops", "srv_2", "Compensazione."),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let second = &calls[2];
    // One read was spent: one is left, the hosts do not change.
    assert_eq!(second.tools, [fetch_tool(1, &["a.example", "b.example"])]);
    assert_eq!(second.messages.len(), 2);
    assert_eq!(second.messages[1].role, R::Assistant, "nothing after it");
    let blocks: Vec<&Value> = second.messages[1]
        .parts
        .iter()
        .filter_map(|p| match p {
            domain::llm::Part::ProviderBlock { payload, .. } => Some(payload),
            _ => None,
        })
        .collect();
    assert_eq!(blocks, [&fetch_use(url, "srv_1"), &page]);
}

#[tokio::test]
async fn the_repair_goes_on_with_the_same_conversation_and_the_same_tool() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "Gli stop raddoppiano la luce.");
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let broken_text = format!("```json\n{broken}\n```");
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            page.clone(),
            Some(&broken_text),
            StopReason::EndTurn,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;

    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let (first, repair) = (&calls[1], &calls[2]);
    assert_eq!(repair.trace_label, "generator.repair");
    assert_eq!(repair.system, first.system);
    // The same tool and hosts, to keep the history valid, with what is left
    // of the job's reads: one of the two was spent.
    assert_eq!(first.tools, [fetch_tool(2, &["a.example", "b.example"])]);
    assert_eq!(repair.tools, [fetch_tool(1, &["a.example", "b.example"])]);
    // [kickoff] [everything the model answered, pages included] [the repair]
    assert_eq!(repair.messages.len(), 3);
    assert_eq!(repair.messages[0], first.messages[0]);
    assert_eq!(repair.messages[1].role, R::Assistant);
    assert_eq!(
        repair.messages[1].parts,
        [
            domain::llm::Part::ProviderBlock {
                provider: "anthropic".into(),
                payload: fetch_use(url, "srv_1")
            },
            domain::llm::Part::ProviderBlock {
                provider: "anthropic".into(),
                payload: page
            },
            domain::llm::Part::Text(broken_text),
        ]
    );
    assert_eq!(repair.messages[2].role, R::User);
    assert!(user_text(&repair.messages[2]).contains("unknown_resource"));
    // The document is the assistant message above: not quoted a second time.
    assert!(user_text(&repair.messages[2]).contains("ultima risposta in questa conversazione"));
}

#[tokio::test]
async fn without_the_tool_the_repair_is_a_new_conversation_as_before() {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say(&broken.to_string()),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.generate().await;
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].messages.len(), 1);
    assert_eq!(calls[2].tools, []);
}

#[tokio::test]
async fn a_failed_read_is_something_the_model_sees_not_a_failure_of_the_job() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            fetch_failure("srv_1", "url_not_accessible"),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(env.lesson_rows(), 1);
    assert_eq!(env.calls(), 2, "no repair, no second try");
}

#[tokio::test]
async fn a_rejection_of_the_request_with_the_tool_fails_the_job_without_a_fallback() {
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        vec![Step::Err(domain::llm::LlmError::InvalidRequest(
            "tools.0: unknown tool type web_fetch_20250910".into(),
        ))],
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Internal);
    assert_eq!(env.calls(), 2, "no silent second request without the tool");
    assert_eq!(env.lesson_rows(), 0);
}

#[tokio::test]
async fn too_many_paused_turns_time_the_generator_out() {
    let url = "https://a.example/exposure";
    let page = fetch_page(url, "srv_1", "x");
    let mut scripts = vec![say(&plan_json().to_string())];
    for _ in 0..3 {
        scripts.push(fetched(
            url,
            "srv_1",
            page.clone(),
            None,
            StopReason::PauseTurn,
        ));
    }
    let mut env = LessonEnv::new(scripts).await;
    env.settings.max_web_fetches = 3;
    env.settings.max_continuations = 1;
    let job = env.generate().await;
    // The second pause exceeds the single continuation allowed.
    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
}

fn broken_text() -> String {
    let mut broken = lesson_doc();
    broken["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x999).to_string());
    format!("```json\n{broken}\n```")
}

#[tokio::test]
async fn the_read_budget_is_the_jobs_across_two_repairs() {
    let (a, b) = ("https://a.example/exposure", "https://b.example/stops");
    let broken = broken_text();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        // 1 read of 2.
        fetched(
            a,
            "srv_1",
            fetch_page(a, "srv_1", "x"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The first repair reads the second: the budget is now all spent.
        fetched(
            b,
            "srv_2",
            fetch_page(b, "srv_2", "y"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The second repair has nothing left to read.
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let limits: Vec<Option<u32>> = calls[1..]
        .iter()
        .map(|c| match &c.tools[..] {
            [domain::llm::ToolDef::ServerWebFetch { max_uses, .. }] => Some(*max_uses),
            [] => None,
            other => panic!("{other:?}"),
        })
        .collect();
    // 2 at the start, 1 left for the first repair, none left for the second:
    // the tool goes (the API rejects `max_uses: 0`), the pages read stay.
    assert_eq!(limits, [Some(2), Some(1), None]);
    assert_eq!(calls[3].messages.len(), 5);
    assert_eq!(calls[3].trace_label, "generator.repair");
}

#[tokio::test]
async fn a_repair_without_the_tool_still_answers_the_latest_document() {
    let url = "https://a.example/exposure";
    let broken = broken_text();
    let mut second = lesson_doc();
    second["sources"][0]["resource_id"] = json!(Uuid::from_u128(0x998).to_string());
    let second = second.to_string();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        // The only read of the job goes in the first call.
        fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        // The first repair has no tool and is still wrong.
        say(&second),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    let last = &calls[3];
    assert!(last.tools.is_empty());
    let roles: Vec<_> = last.messages.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        [R::User, R::Assistant, R::User, R::Assistant, R::User]
    );
    // The document the last repair points to is the one that just failed.
    assert_eq!(user_text(&last.messages[3]), second);
}

#[tokio::test]
async fn a_failed_read_spends_the_budget_too() {
    let (a, b) = ("https://a.example/exposure", "https://b.example/stops");
    let broken = broken_text();
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            a,
            "srv_1",
            fetch_failure("srv_1", "url_not_accessible"),
            Some(&broken),
            StopReason::EndTurn,
        ),
        fetched(
            b,
            "srv_2",
            fetch_page(b, "srv_2", "y"),
            Some(&lesson_doc().to_string()),
            StopReason::EndTurn,
        ),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example", "b.example"])]);
}

#[tokio::test]
async fn a_generator_paused_with_the_budget_spent_goes_on_instead_of_returning_the_pause() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            None,
            StopReason::PauseTurn,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 1;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example"])]);
    // The pause is sent back whole, with nothing after it.
    assert_eq!(calls[2].messages.len(), 2);
    assert_eq!(calls[2].messages[1].role, R::Assistant);
}

#[tokio::test]
async fn a_generator_that_stays_paused_with_the_budget_spent_times_out() {
    let url = "https://a.example/exposure";
    let mut scripts = vec![say(&plan_json().to_string())];
    for _ in 0..3 {
        scripts.push(fetched(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            Some("Ecco quasi"),
            StopReason::PauseTurn,
        ));
    }
    let mut env = LessonEnv::new(scripts).await;
    env.settings.max_web_fetches = 1;
    env.settings.max_continuations = 2;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error.unwrap().code, JobErrorCode::Timeout);
    assert_eq!(env.lesson_rows(), 0, "a pause is never taken as a document");
}

#[tokio::test]
async fn reads_the_provider_does_not_report_still_spend_the_budget() {
    let url = "https://a.example/exposure";
    let mut env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        fetched_reporting(
            url,
            "srv_1",
            fetch_page(url, "srv_1", "x"),
            None,
            StopReason::PauseTurn,
            0,
        ),
        say(&lesson_doc().to_string()),
    ])
    .await;
    env.settings.max_web_fetches = 3;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    // The plan allows 2 reads (two sources); one seen in the blocks while the
    // usage said 0: 1 left.
    assert_eq!(calls[2].tools, [fetch_tool(1, &["a.example", "b.example"])]);
}

#[tokio::test]
async fn the_saved_minutes_are_computed_by_the_backend_not_taken_from_the_model() {
    let mut plan = plan_json();
    plan["estimated_minutes"] = json!(19);
    let mut doc = lesson_doc();
    doc["estimated_minutes"] = json!(17);
    let env = LessonEnv::new(vec![say(&plan.to_string()), say(&doc.to_string())]).await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    let (document, stored_plan, minutes) = {
        let s = env.f.repo.0.lock().unwrap();
        let row = &s.lessons[0];
        (
            row.document.clone(),
            row.plan.clone(),
            row.lesson.estimated_minutes,
        )
    };
    // The plan: 4 concepts, 2 exercises, 1 open answer, no recall.
    assert_eq!(stored_plan["estimated_minutes"], 11);
    // The document: the same formula over the prose the Validator counts.
    let expected = domain::document_minutes(&document).unwrap();
    assert_eq!(document["estimated_minutes"], expected);
    assert_eq!(minutes, expected);
    assert_ne!(expected, 17);
    assert!((1..=20).contains(&expected));
}

#[tokio::test]
async fn a_wrong_or_missing_model_minutes_figure_costs_no_repair() {
    for bad in [Some(json!(0)), Some(json!(99)), Some(json!("molti")), None] {
        let mut doc = lesson_doc();
        match &bad {
            Some(v) => doc["estimated_minutes"] = v.clone(),
            None => {
                doc.as_object_mut().unwrap().remove("estimated_minutes");
            }
        }
        let env = LessonEnv::new(vec![say(&plan_json().to_string()), say(&doc.to_string())]).await;
        let job = env.generate().await;
        assert_eq!(job.status, JobStatus::Succeeded, "{bad:?}: {:?}", job.error);
        assert_eq!(env.calls(), 2, "{bad:?}: no repair");
        let document = env.f.repo.0.lock().unwrap().lessons[0].document.clone();
        assert_eq!(
            document["estimated_minutes"],
            domain::document_minutes(&document).unwrap()
        );
    }
}

#[tokio::test]
async fn the_generator_cap_is_the_roles_not_the_requests() {
    // `[llm.roles.generator] max_output_tokens` is the only source: the use
    // case sends no cap of its own, for the first document or for its repair.
    let env = LessonEnv::new(vec![
        say(&plan_json().to_string()),
        say("{\"blocks\": []}"),
        say(&lesson_doc().to_string()),
    ])
    .await;
    let job = env.generate().await;
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let calls = env.f.llm.requests.lock().unwrap();
    assert_eq!(calls[1].trace_label, "generator.write");
    assert_eq!(calls[2].trace_label, "generator.repair");
    assert_eq!(calls[1].max_output_tokens, None);
    assert_eq!(calls[2].max_output_tokens, None);
}

thread_local! {
    /// What the test thread has logged (a `#[tokio::test]` runs on one thread).
    static LOG: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Writes to [`LOG`].
struct LogWriter;

impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        LOG.with(|log| log.borrow_mut().extend_from_slice(bytes));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A global subscriber, installed once: a per-test one would race with the
/// other tests over `tracing`'s cache of which callsites are enabled.
fn capture_logs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(|| LogWriter)
                .with_ansi(false)
                .finish(),
        );
    });
}

#[tokio::test]
async fn running_out_of_tokens_is_logged_with_its_role_cap_effort_and_remedy() {
    capture_logs();
    // Generator, then Planner: both name themselves.
    for (steps, role) in [
        (
            vec![
                say(&plan_json().to_string()),
                cut_short("{\"title\": \"a metà", StopReason::MaxTokens),
            ],
            "generator",
        ),
        (
            vec![cut_short("{\"title\"", StopReason::MaxTokens)],
            "planner",
        ),
    ] {
        let env = LessonEnv::new(steps).await;
        let job = env.generate().await;
        // No new error code and no retry: the same `upstream_failed` as before.
        assert_eq!(job.error.unwrap().code, JobErrorCode::UpstreamFailed);
        let text = LOG.with(|log| String::from_utf8(log.borrow().clone()).unwrap());
        let line = text
            .lines()
            .find(|l| {
                l.contains("ran out of output tokens") && l.contains(&format!("role=\"{role}\""))
            })
            .unwrap_or_else(|| panic!("no warning for {role}: {text}"));
        for needle in [
            "WARN",
            "output_tokens=0",
            "max_output_tokens=",
            "effort=\"low\"",
            "lower `effort` or raise `max_output_tokens` in [llm.roles.",
        ] {
            assert!(line.contains(needle), "{needle}: {line}");
        }
    }
}
