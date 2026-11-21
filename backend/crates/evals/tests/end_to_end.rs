#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::must_use_candidate,
    clippy::duration_suboptimal_units,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::similar_names,
    clippy::items_after_statements,
    clippy::unnecessary_wraps,
    clippy::implicit_hasher
)] // a developer tool, not the server: readability over pedantry
//! The runner end to end with a scripted provider: real use cases, real
//! seeding, real checks and report. Nothing here calls a real model.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmRole, Part,
    StopReason, StructuredOutput, Usage,
};
use evals::report::{RunStatus, markdown};
use evals::runner::{Models, RunOptions, dry_run, run_suites};
use evals::seed::Tuning;
use evals::suite::Suite;
use serde_json::{Value, json};

/// Answers by role; the judge (role `Interviewer`) says every criterion is met.
struct Fake {
    tutor_text: String,
    recorder_json: String,
    /// Whether the Tutor calls `save_mission_draft` when it has tools.
    use_tools: bool,
    /// The variable system block (the rendered `context.md`) of every request, by role.
    seen: std::sync::Mutex<Vec<(LlmRole, String)>>,
    /// Every request in full, for what the tests need beyond the context.
    requests: std::sync::Mutex<Vec<LlmRequest>>,
    /// Whether the provider declares the web fetch capability.
    web_fetch: bool,
}

struct Events(std::collections::VecDeque<LlmEvent>);

#[async_trait]
impl LlmEventStream for Events {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        self.0.pop_front().map(Ok)
    }
}

fn criteria_ids(req: &LlmRequest) -> Vec<String> {
    let text: String = req
        .messages
        .iter()
        .flat_map(|m| m.parts.iter())
        .filter_map(|p| {
            if let Part::Text(t) = p {
                Some(t.clone())
            } else {
                None
            }
        })
        .collect();
    text.split("CRITERIA\n")
        .nth(1)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("- ")?.split(':').next().map(str::to_owned))
        .collect()
}

#[async_trait]
impl LlmProvider for Fake {
    fn capabilities(&self, _role: LlmRole) -> LlmCapabilities {
        LlmCapabilities {
            tools: true,
            parallel_tool_calls: false,
            forced_tool_choice: false,
            structured_output: StructuredOutput::None,
            streaming_tool_input: false,
            prompt_caching: false,
            max_cache_breakpoints: 0,
            context_window_tokens: 100_000,
            max_output_tokens: 4096,
            effort: None,
            server_web_search: true,
            server_web_fetch: self.web_fetch,
        }
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        // The second system block is the rendered `context.md`.
        let context = req.system.last().map_or_default(|b| b.text.clone());
        self.seen.lock().unwrap().push((req.role, context));
        self.requests.lock().unwrap().push(req.clone());
        if self.use_tools
            && req.role == LlmRole::Tutor
            && !req.tools.is_empty()
            && !req
                .messages
                .last()
                .is_some_and(|m| m.parts.iter().any(|p| matches!(p, Part::ToolResult { .. })))
        {
            let input = json!({"why": "Mantenere un servizio di pagamenti scritto in Rust nel nuovo lavoro.",
                "success_criteria": ["Leggo e modifico il servizio axum senza chiedere aiuto."],
                "starting_point": "Programmo in Kotlin; Rust mai scritto.", "constraints": [], "out_of_scope": []});
            return Ok(Box::new(Events(
                [
                    LlmEvent::Started {
                        model: "claude-sonnet-5-5".into(),
                        request_id: None,
                    },
                    LlmEvent::ToolCallFinished {
                        block: 1,
                        id: "t1".into(),
                        name: "save_mission_draft".into(),
                        input,
                    },
                    LlmEvent::Finished {
                        stop_reason: StopReason::ToolUse,
                        usage: Usage {
                            input_tokens: 800,
                            output_tokens: 100,
                            ..Usage::default()
                        },
                    },
                ]
                .into(),
            )));
        }
        let text = match req.role {
            LlmRole::Interviewer => {
                let verdict: serde_json::Map<String, Value> = criteria_ids(&req)
                    .into_iter()
                    .map(|id| (id, json!({"met": true, "reason": "ok"})))
                    .collect();
                Value::Object(verdict).to_string()
            }
            LlmRole::Grader => json!({"verdict": "incorrect", "feedback": "Manca il perché. Prova a dire cosa succede dopo.", "misconception": null}).to_string(),
            LlmRole::Recorder => self.recorder_json.clone(),
            _ => self.tutor_text.clone(),
        };
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 2500,
            ..Usage::default()
        };
        Ok(Box::new(Events(
            [
                LlmEvent::Started {
                    model: "claude-sonnet-5-5".into(),
                    request_id: None,
                },
                LlmEvent::TextDelta { block: 0, text },
                LlmEvent::Finished {
                    stop_reason: StopReason::EndTurn,
                    usage,
                },
            ]
            .into(),
        )))
    }
}

fn models() -> Models {
    Models {
        roles: HashMap::from([(LlmRole::Tutor, "claude-sonnet-5-5".to_owned())]),
        judge: "claude-sonnet-5-5".into(),
    }
}

fn options(max_cost: Option<f64>) -> RunOptions {
    RunOptions {
        runs: Some(2),
        max_cost_usd: max_cost,
        cases: vec![],
        tuning: Tuning {
            max_searches: 2,
            history_token_budget: 8000,
            max_fetches: 0,
            web_fetch_capable: false,
        },
    }
}

fn fake(tutor: &str) -> Arc<Fake> {
    Arc::new(Fake {
        tutor_text: tutor.to_owned(),
        recorder_json: json!({"records": [], "terms": [], "cards": []}).to_string(),
        use_tools: false,
        seen: std::sync::Mutex::default(),
        requests: std::sync::Mutex::default(),
        web_fetch: false,
    })
}

fn fake_with_web_fetch() -> Arc<Fake> {
    Arc::new(Fake {
        tutor_text: "non e un documento".to_owned(),
        recorder_json: String::new(),
        use_tools: false,
        seen: std::sync::Mutex::default(),
        requests: std::sync::Mutex::default(),
        web_fetch: true,
    })
}

fn fetch_options() -> RunOptions {
    let mut opts = options(None);
    opts.runs = Some(1);
    opts.tuning.max_fetches = 3;
    opts.tuning.web_fetch_capable = true;
    opts
}

const TUTOR_SUITE: &str = r#"{
  "defaults": {"runs_per_case": 3, "deterministic": {"max_words": 12, "language": "it", "no_exclamation_marks": true}},
  "pass_criteria": {"rubric_score": ">= 0.8"},
  "cases": [
    {"id": "short", "topic": {"title": "Rust", "state": "interviewing"},
     "messages": [{"role": "user", "text": "Voglio imparare Rust"}],
     "rubric": [{"id": "asks", "weight": 2, "criterion": "Fa una domanda."}]},
    {"id": "too_long", "topic": {"title": "Rust", "state": "interviewing"},
     "messages": [{"role": "user", "text": "Ciao"}, {"role": "assistant", "text": "Ciao"}, {"role": "user", "text": "Dimmi"}],
     "deterministic": {"max_words": 3}}
  ]
}"#;

#[tokio::test]
async fn a_tutor_suite_runs_checks_and_judges_and_reports() {
    let suite = Suite::parse("tutor/test", TUTOR_SUITE).unwrap();
    let provider = fake("Cosa vorresti riuscire a fare con Rust?");
    let report = run_suites(
        provider.clone(),
        provider,
        &models(),
        &[suite],
        &options(None),
        "2026-11-09T10:00:00.000Z",
    )
    .await;

    let suite = &report.suites[0];
    assert_eq!(suite.cases.len(), 2);
    let short = &suite.cases[0];
    assert_eq!(short.runs.len(), 2, "--runs wins over runs_per_case");
    assert!(
        short
            .runs
            .iter()
            .all(|r| r.status == RunStatus::Completed && r.checks_ok()),
        "{short:?}"
    );
    assert_eq!(short.rubric_mean, Some(1.0));
    let run = &short.runs[0];
    assert_eq!(
        run.prompt_version,
        evals::roles::prompt_version(evals::suite::Role::Tutor)
    );
    assert!(run.models.contains(&"claude-sonnet-5-5".to_owned()));
    // One Tutor call and one judge call, both priced.
    assert_eq!(
        run.calls.len(),
        2,
        "{:?}",
        run.calls
            .iter()
            .map(|c| (&c.label, &c.role))
            .collect::<Vec<_>>()
    );
    assert!(run.cost_usd > 0.0 && run.judge_cost_usd > 0.0);
    assert!(run.turns[0].output.contains("Rust"));

    // The case-level limit (3 words) fails the deterministic check.
    let long = &suite.cases[1];
    assert_eq!(long.runs_checks_ok, 0);
    let failed: Vec<_> = long.runs[0].turns[0]
        .checks
        .iter()
        .filter(|c| c.name == "max_words")
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].detail.contains("words"));

    assert!((report.total_cost_usd - suite.cost_usd).abs() < 1e-9);
    let md = markdown(&report);
    assert!(md.contains("tutor/test") && md.contains("max_words"));
    serde_json::to_string(&report).unwrap();
}

const ENGLISH_SUITE: &str = r#"{
  "defaults": {"runs_per_case": 1, "deterministic": {"language": "en"}},
  "cases": [
    {"id": "english", "content_language": "en", "topic": {"title": "Rust", "state": "interviewing"},
     "messages": [{"role": "user", "text": "I want to learn Rust"}],
     "rubric": [{"id": "asks", "weight": 1, "criterion": "Asks a question."}]}
  ]
}"#;

#[tokio::test]
async fn an_english_case_seeds_the_topic_language_and_is_checked_in_english() {
    let suite = Suite::parse("tutor/test", ENGLISH_SUITE).unwrap();
    let provider = fake("What do you want to be able to do with it?");
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &[suite],
        &options(None),
        "2026-11-09T10:00:00.000Z",
    )
    .await;
    let run = &report.suites[0].cases[0].runs[0];
    assert!(run.checks_ok(), "{run:?}");
    let lang = run.turns[0].checks.iter().find(|c| c.name == "language");
    assert!(lang.is_some_and(|c| c.status == evals::checks::Status::Pass));
    // {{content_language}} was rendered for the Tutor from the topic's language.
    let seen = provider.seen.lock().unwrap();
    assert!(
        seen.iter()
            .any(|(role, ctx)| *role == LlmRole::Tutor && ctx.contains("inglese")),
        "{seen:?}"
    );
}

#[tokio::test]
async fn the_cost_budget_stops_the_runner_before_it_is_exceeded() {
    let suite = Suite::parse("tutor/test", TUTOR_SUITE).unwrap();
    let provider = fake("Cosa vorresti riuscire a fare con Rust?");
    // A run costs about 0.1 $ for real; each call books about 0.09 $ first.
    let opts = options(Some(0.09));
    let report = run_suites(provider.clone(), provider, &models(), &[suite], &opts, "t").await;
    assert!(
        report.budget_exhausted,
        "spent {} runs {:?}",
        report.total_cost_usd,
        report.suites[0]
            .cases
            .iter()
            .map(|c| (
                c.runs.len(),
                c.runs.iter().map(|r| r.status).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
    let runs: usize = report.suites[0].cases.iter().map(|c| c.runs.len()).sum();
    assert!((1..4).contains(&runs), "{runs} runs were started");
    assert!(
        report.total_cost_usd <= 0.09,
        "spent {}",
        report.total_cost_usd
    );
}

#[tokio::test]
async fn a_call_is_refused_when_its_estimate_does_not_fit() {
    let suite = Suite::parse("tutor/test", TUTOR_SUITE).unwrap();
    let provider = fake("Cosa vorresti riuscire a fare con Rust?");
    // Real calls cost 0.03 $, but one books far more: nothing may start.
    let opts = options(Some(0.01));
    let report = run_suites(provider.clone(), provider, &models(), &[suite], &opts, "t").await;
    assert!(
        report.total_cost_usd.abs() < 1e-12,
        "spent {}",
        report.total_cost_usd
    );
    assert!(report.budget_exhausted);
    // A refused call is no result: the case is listed as not run, not as a
    // run whose job failed with an internal error.
    assert_eq!(report.suites[0].cases.len(), 0);
    assert_eq!(report.suites[0].cases_not_run.len(), 2);
}

#[tokio::test]
async fn a_dry_run_lists_cases_and_estimates_without_calling() {
    let suite = Suite::parse("tutor/test", TUTOR_SUITE).unwrap();
    let listing = dry_run(&[suite], &models(), &options(Some(0.001)));
    assert!(listing.contains("short") && listing.contains("too_long"));
    assert!(listing.contains("Estimated total"));
    assert!(listing.contains("exceeds"));
}

#[tokio::test]
async fn the_grader_and_recorder_suites_run_through_the_real_use_cases() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites =
        evals::suite::load(&dir, &["grader/grading".into(), "recorder/records".into()]).unwrap();
    // One quick case of each.
    for s in &mut suites {
        s.cases.retain(|c| {
            matches!(
                c.id.as_str(),
                "correct_own_words" | "misconception_corrected" | "explained_only_zero_records"
            )
        });
    }
    let provider = fake("non usato");
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(provider.clone(), provider, &models(), &suites, &opts, "t").await;

    let grader = &report.suites[0].cases[0].runs[0];
    assert_eq!(grader.status, RunStatus::Completed, "{:?}", grader.error);
    assert!(
        grader.turns[0]
            .checks
            .iter()
            .any(|c| c.name == "expect.verdict")
    );
    // The scripted Grader says `incorrect`; the case accepts only `correct`.
    assert!(
        grader.turns[0]
            .checks
            .iter()
            .any(|c| c.name == "expect.verdict" && c.status == evals::checks::Status::Fail)
    );
    assert!(
        grader.turns[0]
            .checks
            .iter()
            .any(|c| c.name == "schema_valid" && c.status == evals::checks::Status::Pass)
    );

    // Nothing passed: the Recorder is not even called (the backend skips it).
    let zero = report.suites[1]
        .cases
        .iter()
        .find(|c| c.id == "explained_only_zero_records")
        .unwrap();
    assert_eq!(
        zero.runs[0].status,
        RunStatus::Completed,
        "{:?}",
        zero.runs[0].error
    );
    assert!(
        zero.runs[0].checks_ok(),
        "{:?}",
        zero.runs[0].turns[0].checks
    );
    assert_eq!(zero.runs[0].turns[0].extras["recorder_called"], false);

    // The seeded attempts reach the Recorder, whose empty answer misses the expectations.
    let mc = report.suites[1]
        .cases
        .iter()
        .find(|c| c.id == "misconception_corrected")
        .unwrap();
    assert_eq!(
        mc.runs[0].status,
        RunStatus::Completed,
        "{:?}",
        mc.runs[0].error
    );
    assert_eq!(mc.runs[0].turns[0].extras["recorder_called"], true);
    assert!(
        mc.runs[0].turns[0]
            .checks
            .iter()
            .any(|c| c.name == "expect.records" && c.status == evals::checks::Status::Fail)
    );
}

#[tokio::test]
async fn a_tool_conversation_runs_the_real_tools_and_the_confirm_event() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["tutor/tools".into()]).unwrap();
    suites[0].cases.retain(|c| c.id == "vague_becomes_precise");
    let provider = Arc::new(Fake {
        tutor_text: "Quanto tempo hai ogni giorno per studiare?".into(),
        recorder_json: String::new(),
        use_tools: true,
        seen: std::sync::Mutex::default(),
        requests: std::sync::Mutex::default(),
        web_fetch: false,
    });
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(provider.clone(), provider, &models(), &suites, &opts, "t").await;
    let run = &report.suites[0].cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
    // Three user turns; the confirmation between them is an event, not a turn.
    assert_eq!(run.turns.len(), 4);
    assert_eq!(run.extras["events"][0]["event"], "confirm_mission");
    // The scripted model calls the tool on every turn: the real tool stored a draft each time.
    assert_eq!(
        run.turns[2].extras["tool_calls"][0]["name"],
        "save_mission_draft"
    );
    assert!(
        run.turns[2]
            .checks
            .iter()
            .any(|c| c.name == "tools.called" && c.status == evals::checks::Status::Pass)
    );
    assert_eq!(run.turns[2].extras["tool_validation_errors"], 0);
    // ... and the first step's `not_called` expectation catches it.
    assert!(
        run.turns[0]
            .checks
            .iter()
            .any(|c| c.name == "tools.not_called" && c.status == evals::checks::Status::Fail)
    );
}

#[tokio::test]
async fn the_generator_case_reaches_the_model_through_the_fixed_plan() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["generator/lessons".into()]).unwrap();
    suites[0]
        .cases
        .retain(|c| c.id == "rust_second_lesson_recall_and_link");
    // The scripted Generator writes text that is no document: three attempts, then the job fails.
    let provider = fake("non e un documento");
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(provider.clone(), provider, &models(), &suites, &opts, "t").await;
    let run = &report.suites[0].cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
    // No Planner call reached the provider (the case's plan was used and accepted) ...
    assert!(
        run.calls.iter().all(|c| c.role != "planner"),
        "{:?}",
        run.calls
    );
    // ... and the Generator was asked three times (1 + 2 repairs).
    assert_eq!(run.turns[0].extras["documents"], 3);
    let checks = &run.turns[0].checks;
    assert!(checks.iter().any(|c| c.name == "validator_pass_within_3" && c.status == evals::checks::Status::Fail));
    assert!(
        checks
            .iter()
            .any(|c| c.name == "job_succeeded" && c.status == evals::checks::Status::Fail)
    );
    // Every attempt is in the report with its Validator problems (a text that
    // is no JSON object is a `schema` problem at the root) and the head of
    // the answer, which is what lets a bad run be diagnosed.
    let attempts = run.turns[0].extras["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 3);
    for (i, attempt) in attempts.iter().enumerate() {
        assert_eq!(attempt["attempt"], i + 1);
        assert_eq!(attempt["valid"], false);
        assert_eq!(attempt["issues"][0]["code"], "schema");
        assert_eq!(attempt["issues"][0]["path"], "");
        assert_eq!(attempt["response_head"], "non e un documento");
        assert_eq!(attempt["output_tokens"], 2500);
        assert_eq!(attempt["stop_reason"], "EndTurn");
    }
}

#[test]
fn case_filters_apply_per_suite() {
    let a = Suite::parse("tutor/a", r#"{"cases": [{"id": "one"}, {"id": "two"}]}"#).unwrap();
    let b = Suite::parse(
        "grader/b",
        r#"{"cases": [{"id": "three"}, {"id": "four"}]}"#,
    )
    .unwrap();
    let mut opts = options(None);
    opts.cases = vec!["one".into()];
    assert!(evals::runner::wanted(&opts, &a, "one"));
    assert!(!evals::runner::wanted(&opts, &a, "two"));
    // The other suite has no such case: it runs whole.
    assert!(evals::runner::wanted(&opts, &b, "three") && evals::runner::wanted(&opts, &b, "four"));
    opts.cases = vec!["grader/b:four".into()];
    assert!(evals::runner::wanted(&opts, &a, "two"));
    assert!(evals::runner::wanted(&opts, &b, "four") && !evals::runner::wanted(&opts, &b, "three"));
    opts.cases = vec!["one".into(), "grader/b:three".into()];
    assert!(!evals::runner::wanted(&opts, &a, "two") && !evals::runner::wanted(&opts, &b, "four"));
}

#[tokio::test]
async fn the_pdf_suites_seed_documents_and_read_back_the_excerpts() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites =
        evals::suite::load(&dir, &["planner/pdf".into(), "generator/pdf".into()]).unwrap();
    for s in &mut suites {
        s.cases.retain(|c| {
            matches!(
                c.id.as_str(),
                "follow_book_in_order_after_two_lessons" | "pdf_excerpt_cut_by_the_limit"
            )
        });
    }
    // The Planner and the Generator both answer with the scripted text: a
    // valid plan for the first case. The Generator's answer is no lesson, so
    // its run ends in validation errors, which is what this test lets happen.
    let plan = || -> String {
        let suite = suites.iter().find(|s| s.id == "planner/pdf").unwrap();
        let c = &suite.cases[0];
        let doc = &c.array("documents")[0];
        json!({
            "title": "Copiare o spostare", "objective": "Saper scegliere tra copia e move.",
            "why_now": "Segue il libro.", "estimated_minutes": 8,
            "prerequisites": [], "concepts": [{"name": "Copy", "key_point": "x", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Move", "key_point": "y", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Clone", "key_point": "z", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}, {"name": "Borrow", "key_point": "w", "example": "Un caso con valori.", "steps": ["primo passaggio", "secondo passaggio"], "pitfall": ""}],
            "terms": {"introduce": [], "reuse": []},
            "primary_source_id": doc["resource_id"],
            "other_source_ids": [c.array("sources")[0]["resource_id"]],
            "document_sections": [{"document_id": doc["document_id"], "section_ids": ["s6"]}],
            "exercises": [{"type": "open_answer", "purpose": "a", "verifies_objective": true},
                          {"type": "fill_blank", "purpose": "b", "verifies_objective": false}],
            "lesson_links": []
        })
        .to_string()
    };
    let provider = fake(&plan());
    let mut models = models();
    models
        .roles
        .insert(LlmRole::Planner, "claude-sonnet-5-5".into());
    models
        .roles
        .insert(LlmRole::Generator, "claude-sonnet-5-5".into());
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(provider.clone(), provider, &models, &suites, &opts, "t").await;

    let planner = report
        .suites
        .iter()
        .find(|s| s.id == "planner/pdf")
        .unwrap();
    let run = &planner.cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
    let checks = &run.turns[0].checks;
    let status = |name: &str| {
        checks
            .iter()
            .find(|c| c.name == name)
            .map(|c| (c.status, c.detail.clone()))
    };
    assert_eq!(
        status("plan_valid").map(|s| s.0),
        Some(evals::checks::Status::Pass),
        "{checks:?}"
    );
    assert_eq!(
        status("document_sections_include_one_of").map(|s| s.0),
        Some(evals::checks::Status::Pass)
    );
    // The Generator is never called in a Planner suite.
    assert!(run.calls.iter().all(|c| c.role != "generator"));

    let generator = report
        .suites
        .iter()
        .find(|s| s.id == "generator/pdf")
        .unwrap();
    let run = &generator.cases[0].runs[0];
    assert_ne!(run.status, RunStatus::Failed, "{:?}", run.error);
    let extras = &run.turns[0].extras;
    assert_eq!(extras["excerpt_pages_shown"], json!([161, 162, 163]));
    assert_eq!(extras["excerpt_pages_omitted"], json!([164, 165, 166]));
}

#[tokio::test]
async fn a_rich_generator_case_gives_the_model_its_records_and_the_new_plan_format() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["generator/rich-lessons".into()]).unwrap();
    suites[0]
        .cases
        .retain(|c| c.id == "rust_second_lesson_records_no_recall");
    let provider = fake("non e un documento");
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &opts,
        "t",
    )
    .await;
    let run = &report.suites[0].cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
    // The plan of the case was accepted as it is: the Planner was never called.
    assert!(
        run.calls.iter().all(|c| c.role != "planner"),
        "{:?}",
        run.calls
    );
    let seen = provider.seen.lock().unwrap();
    let (_, context) = seen
        .iter()
        .find(|(role, _)| *role == LlmRole::Generator)
        .expect("a Generator request");
    for line in [
        "Prerequisiti dimostrati (non si ripassano):\n- Sa prevedere che dopo un move la variabile di partenza non è utilizzabile [record 1]",
        "Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):\n- nessuno",
        "   Passaggi:\n   1) ",
        "(record attivi)\n- [1] (demonstrated, lezione 1) Prevede l'uso dopo un move: ",
        "- [2] (misconception_corrected, lezione 1) I tipi semplici si copiano: ",
    ] {
        assert!(context.contains(line), "{line}\n---\n{context}");
    }
    assert!(!context.contains("{{"), "{context}");
}

#[test]
fn the_concept_suites_are_listed_and_estimated_in_a_dry_run() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let suites = evals::suite::load(
        &dir,
        &["planner/concepts".into(), "generator/rich-lessons".into()],
    )
    .unwrap();
    let table = dry_run(&suites, &models(), &options(None));
    for case in [
        "beginner_zero_records_general_prereqs",
        "convention_pitfall_not_invented",
        "narrow_plan_four_concepts_no_filler",
    ] {
        assert!(table.contains(case), "{table}");
    }
}

#[test]
fn a_suite_naming_a_check_the_runner_does_not_know_does_not_load() {
    let suite = |role: &str, defaults: &str, case: &str| {
        Suite::parse(
            &format!("{role}/x"),
            &format!(
                r#"{{"defaults": {{"deterministic": {defaults}}}, "cases": [{{"id": "a", "deterministic": {case}}}]}}"#
            ),
        )
    };
    assert!(suite("planner", r#"{"concepts_count": [4, 5]}"#, "{}").is_ok());
    assert!(suite("generator", r#"{"prose_words": [1, 2]}"#, "{}").is_ok());
    for (role, defaults, case) in [
        ("planner", r#"{"concept_count": [4, 5]}"#, "{}"),
        ("planner", "{}", r#"{"mystery": true}"#),
        ("generator", r#"{"prose_wordz": [1, 2]}"#, "{}"),
        ("generator", "{}", r#"{"mystery": true}"#),
    ] {
        let err = suite(role, defaults, case).unwrap_err().to_string();
        assert!(err.contains("unknown check"), "{err}");
    }
}

// ------------------------------------------------ web fetch (ADR 0009)

#[test]
fn a_dry_run_names_the_fetch_suite_as_skipped_while_the_tool_is_off() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let suites = evals::suite::load(&dir, &[]).unwrap();
    let table = dry_run(&suites, &models(), &options(None));
    let row = table
        .lines()
        .find(|l| l.contains("generator/web-fetch"))
        .unwrap_or_else(|| panic!("{table}"));
    assert!(row.contains("suite skipped"), "{row}");
    assert!(!table.contains("rust_book_page_facts"), "{table}");
    // With the tool on, the cases are listed, the one that needs a local page
    // says why it will not run.
    let on = dry_run(&suites, &models(), &fetch_options());
    assert!(on.contains("rust_book_page_facts |"), "{on}");
    assert!(on.contains("injection_in_fetched_page |"), "{on}");
    assert!(!on.contains("non è ancora pubblicata"), "{on}");
    // The other suites run exactly as they did.
    assert!(table.contains("generator/rich-lessons |"), "{table}");
}

#[test]
fn naming_the_fetch_suite_with_the_tool_off_is_an_error_but_the_default_is_a_skip() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let wanted = vec!["generator/web-fetch".to_owned()];
    let suites = evals::suite::load(&dir, &wanted).unwrap();
    let err = evals::runner::check_explicit_suites(&suites, &wanted, &options(None).tuning)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("generator/web-fetch") && err.contains("max_fetches"),
        "{err}"
    );
    // A provider that cannot run the tool says so, not the setting.
    let mut tuning = fetch_options().tuning;
    tuning.web_fetch_capable = false;
    let err = evals::runner::check_explicit_suites(&suites, &wanted, &tuning)
        .unwrap_err()
        .to_string();
    assert!(err.contains("server_web_fetch"), "{err}");
    // Not named: nothing to complain about.
    assert!(evals::runner::check_explicit_suites(&suites, &[], &options(None).tuning).is_ok());
    assert!(
        evals::runner::check_explicit_suites(&suites, &wanted, &fetch_options().tuning).is_ok()
    );
}

#[tokio::test]
async fn the_fetch_suite_is_skipped_with_the_tool_off_and_calls_nothing() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let suites = evals::suite::load(&dir, &["generator/web-fetch".into()]).unwrap();
    let provider = fake_with_web_fetch();
    let opts = options(None);
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &opts,
        "t",
    )
    .await;
    let suite = &report.suites[0];
    assert!(
        suite
            .skipped
            .as_deref()
            .is_some_and(|r| r.contains("max_fetches")),
        "{:?}",
        suite.skipped
    );
    assert!(suite.cases.is_empty());
    assert!(provider.requests.lock().unwrap().is_empty());
    assert!(markdown(&report).contains("Skipped"));
    assert!(report.total_cost_usd.abs() < f64::EPSILON);
}

#[tokio::test]
async fn with_the_tool_on_the_fetch_cases_run_the_twin_without_it_and_runs_too() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["generator/web-fetch".into()]).unwrap();
    suites[0].cases.retain(|c| {
        matches!(
            c.id.as_str(),
            "rust_book_page_facts" | "three_pages_baseline_no_fetch" | "injection_in_fetched_page"
        )
    });
    let provider = fake_with_web_fetch();
    let opts = fetch_options();
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &opts,
        "t",
    )
    .await;
    let suite = &report.suites[0];

    // The fixture case runs too: its page is put in the conversation.
    assert!(suite.cases_skipped.is_empty());
    assert_eq!(suite.cases.len(), 3);

    let generator_calls = |label: &str| -> Vec<LlmRequest> {
        provider
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.role == LlmRole::Generator && r.trace_label == label)
            .cloned()
            .collect()
    };
    let writes = generator_calls("generator.write");
    assert_eq!(writes.len(), 3, "one first call per case");
    // The fixture case: kickoff, the simulated read, "Continua.", no tool.
    let injected = writes
        .iter()
        .find(|r| r.messages.len() == 3)
        .expect("the injected case");
    assert_eq!(injected.tools.len(), 0);
    assert_eq!(
        injected.messages[1]
            .parts
            .iter()
            .filter(|p| matches!(p, Part::ProviderBlock { .. }))
            .count(),
        2
    );
    // The case with the tool: tool, hosts of the plan's source, the list.
    let with = writes
        .iter()
        .find(|r| !r.tools.is_empty())
        .expect("a request with the tool");
    assert_eq!(
        with.tools,
        [domain::llm::ToolDef::ServerWebFetch {
            max_uses: 1,
            allowed_domains: vec!["doc.rust-lang.org".into()],
            max_content_tokens: 8000
        }]
    );
    let user_text: String = with.messages[0]
        .parts
        .iter()
        .filter_map(|p| {
            if let Part::Text(t) = p {
                Some(t.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        user_text.starts_with("Procedi.\n\nPrima di scrivere"),
        "{user_text}"
    );
    assert!(
        user_text.contains("https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html")
    );
    // The twin (`"web_fetch": "off"`) runs with the tool off in the same configuration.
    let twin = writes
        .iter()
        .find(|r| r.tools.is_empty() && r.messages.len() == 1)
        .expect("the twin");
    assert_eq!(twin.messages.len(), 1);
    assert!(matches!(&twin.messages[0].parts[..], [Part::Text(t)] if t == "Procedi."));

    // The fetch checks ran: the fake read nothing, so the minimum is missed
    // (information) and the twin's check on its own request passes.
    let checks_of = |id: &str| {
        suite.cases.iter().find(|c| c.id == id).unwrap().runs[0].turns[0]
            .checks
            .clone()
    };
    let status = |checks: &[evals::checks::Check], name: &str| {
        checks.iter().find(|c| c.name == name).map(|c| c.status)
    };
    let facts = checks_of("rust_book_page_facts");
    assert_eq!(
        status(&facts, "web_fetch_requests"),
        Some(evals::checks::Status::Warn)
    );
    assert_eq!(
        status(&facts, "fetch_urls_only_from_sources"),
        Some(evals::checks::Status::Pass)
    );
    assert_eq!(
        status(&facts, "no_refetch_in_repair"),
        Some(evals::checks::Status::Pass)
    );
    let baseline = checks_of("three_pages_baseline_no_fetch");
    assert_eq!(
        status(&baseline, "user_message_has_no_sources_list"),
        Some(evals::checks::Status::Pass)
    );
    assert_eq!(
        status(&baseline, "web_fetch_requests"),
        Some(evals::checks::Status::Pass)
    );
    // The metrics the case asked for are reported.
    let metrics = &suite
        .cases
        .iter()
        .find(|c| c.id == "three_pages_baseline_no_fetch")
        .unwrap()
        .metrics;
    for name in [
        "cost_usd",
        "input_tokens",
        "output_tokens",
        "web_fetch_requests",
        "fetched_tokens",
        "model_calls",
        "documents",
    ] {
        assert!(metrics.contains_key(name), "{name} in {metrics:?}");
    }
}

#[tokio::test]
async fn the_other_generator_suites_never_get_the_tool_even_when_it_is_on() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["generator/rich-lessons".into()]).unwrap();
    suites[0]
        .cases
        .retain(|c| c.id == "rust_second_lesson_records_no_recall");
    let provider = fake_with_web_fetch();
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &fetch_options(),
        "t",
    )
    .await;
    assert_eq!(report.suites[0].cases.len(), 1);
    let requests = provider.requests.lock().unwrap();
    let writes: Vec<_> = requests
        .iter()
        .filter(|r| r.role == LlmRole::Generator)
        .collect();
    assert_ne!(writes.len(), 0, "the Generator was called");
    assert!(writes.iter().all(|r| r.tools.is_empty()));
}

fn planner_suite(case: &str) -> Vec<evals::suite::Suite> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
    let mut suites = evals::suite::load(&dir, &["planner/concepts".into()]).unwrap();
    suites[0].cases.retain(|c| c.id == case);
    assert_eq!(suites[0].cases.len(), 1, "{case}");
    suites
}

#[tokio::test]
async fn a_seed_the_domain_cannot_read_is_an_error_of_the_case_with_its_reason() {
    let mut suites = planner_suite("records_cover_prerequisites");
    suites[0].cases[0].raw["sources"][0]["source_type"] = json!("tutorial");
    let provider = fake("non e un piano");
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &opts,
        "t",
    )
    .await;
    let run = &report.suites[0].cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Failed);
    let error = run.error.as_deref().unwrap();
    assert!(
        error.contains("source_type") && error.contains("tutorial"),
        "{error}"
    );
    // Nothing was called: the world was refused before the job.
    assert!(run.calls.is_empty(), "{:?}", run.calls);
    assert!(provider.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn every_planner_attempt_is_reported_with_its_errors_and_the_head_of_the_answer() {
    let suites = planner_suite("records_cover_prerequisites");
    // The scripted Planner answers prose, twice (the first try and the repair).
    let provider = fake(&"non e un piano ".repeat(300));
    let mut opts = options(None);
    opts.runs = Some(1);
    let report = run_suites(
        provider.clone(),
        provider.clone(),
        &models(),
        &suites,
        &opts,
        "t",
    )
    .await;
    let run = &report.suites[0].cases[0].runs[0];
    assert_eq!(run.status, RunStatus::Completed, "{:?}", run.error);
    let attempts = run.turns[0].extras["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 2, "{attempts:?}");
    for attempt in attempts {
        assert_eq!(attempt["valid"], false);
        assert!(attempt["errors"][0].is_string(), "{attempt}");
        // Cut at 2000 characters.
        assert_eq!(
            attempt["response_head"].as_str().unwrap().chars().count(),
            2000
        );
        assert_eq!(attempt["output_tokens"], 2500);
    }
}

#[test]
fn a_fixture_case_on_an_invalid_host_is_not_skipped() {
    let url = "https://guide.esempio.invalid/p";
    let case = format!(
        r#"{{"cases": [{{"id": "a", "requires": {{"fixture_pages": true}},
          "sources": [{{"url": "{url}"}}], "fixture_pages": {{"{url}": "t"}}}}]}}"#
    );
    let tuning = fetch_options().tuning;
    let suite = Suite::parse("generator/x", &case).unwrap();
    assert_eq!(
        evals::runner::case_skip_reason(&suite, &suite.cases[0], &tuning),
        None
    );
}
