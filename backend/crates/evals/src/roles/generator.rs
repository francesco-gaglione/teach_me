//! Generator: the real `GenerateLesson` job with the plan fixed by the case
//! (the Planner is answered locally), then the real Validator on every
//! document the model wrote.

use domain::UserId;
use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use anyhow::anyhow;
use application::structured::parse_object;
use domain::llm::LlmRole;
use domain::{JobKind, LessonIssue, LessonValidator, ValidationContext, ValidatorConfig};
use regex::Regex;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::fetch;
use super::sdui::{self, EXERCISES, correct_ids, is_exercise, kind, options, prose, text};
use super::shape;
use super::{Env, job_failure, subject_calls, wait_for_job};
use crate::checks::{
    Check, Status, first_match, in_range, patterns, range, regex, run_spec, words,
};
use crate::llm::InjectedPage;
use crate::report::{RoleRun, TurnReport};
use crate::seed::{Tuning, new_app};
use crate::suite::{Case, Suite, criteria};

/// A document the model wrote, and what the Validator said about it.
#[derive(Default)]
pub struct DocAttempt {
    /// The call was the backend's quality improvement (`generator.improve`).
    pub improve: bool,
    /// The Validator's quality warnings (only for a valid document).
    pub warnings: Vec<domain::QualityWarning>,
    pub document: Option<Value>,
    pub issues: Vec<LessonIssue>,
    pub parse_errors: Vec<String>,
    pub output_tokens: u32,
}

impl DocAttempt {
    fn valid(&self) -> bool {
        self.document.is_some() && self.issues.is_empty()
    }

    fn score(&self) -> u32 {
        domain::warning_score(&self.warnings)
    }
}

/// The document the backend saves: the last valid one of the repair chain,
/// replaced by the improvement only when that is valid and its weighted
/// warning score is strictly lower (`generation.rs`, `write`); else the last
/// attempt, for the reports of a failed run.
fn kept_index(docs: &[DocAttempt]) -> Option<usize> {
    let last = |improve: bool| docs.iter().rposition(|d| d.improve == improve && d.valid());
    match (last(false), last(true)) {
        (Some(b), Some(i)) if docs[i].score() < docs[b].score() => Some(i),
        (Some(b), _) => Some(b),
        (None, Some(i)) => Some(i),
        (None, None) => docs.len().checked_sub(1),
    }
}

/// The pages of the case's `fixture_pages`, with the title of the matching
/// source, to show the Generator as already read.
fn injected_pages(case: &Case, sources: &[Value]) -> Vec<InjectedPage> {
    case.get("fixture_pages")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(url, text)| InjectedPage {
            url: url.clone(),
            title: sources
                .iter()
                .find(|s| s["url"] == url.as_str())
                .map_or("", |s| text_of_title(s))
                .to_owned(),
            text: text.as_str().unwrap_or_default().to_owned(),
        })
        .collect()
}

fn text_of_title(source: &Value) -> &str {
    source["title"].as_str().unwrap_or_default()
}

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    let title = case
        .get("topic")
        .and_then(|t| t.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Argomento");
    // The web fetch tool is on only for the suites and cases that ask for it
    // (`requires.web_fetch`), unless the case says `"web_fetch": "off"`: the
    // other Generator suites measure the Generator as it is without the tool,
    // and their sources are made-up addresses it could not read anyway.
    let fetch_on = suite.requires_for(case).web_fetch
        && !suite.web_fetch_off(case)
        && env.tuning.web_fetch_available();
    let tuning = Tuning {
        max_fetches: if fetch_on { env.tuning.max_fetches } else { 0 },
        ..env.tuning
    };
    let mut app = new_app(
        env.llm.clone(),
        tuning,
        title,
        "ready",
        case.content_language(),
    )
    .await?;
    let mission = case.get("mission").cloned().unwrap_or(Value::Null);
    app.mission("confirmed", &mission).await?;
    app.preferences(&case.array("preferences")).await?;
    let sources = case.array("sources");
    app.resources(&sources).await?;
    // Uploaded PDFs (Phase 10): their text is what the Generator must follow.
    let documents = case.array("documents");
    app.documents(&documents).await?;

    // Lessons the plan may link to.
    let lessons = case.array("lessons");
    let empty_doc = |t: &str| json!({"catalog_version": 1, "title": t, "objective": "Lezione precedente.", "estimated_minutes": 5, "blocks": []});
    let mut anchor = None;
    for l in &lessons {
        let t = text(l, "title");
        let number = l
            .get("number")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(1);
        let id = app
            .lesson(
                l.get("id").and_then(Value::as_str),
                number,
                t,
                "Lezione precedente.",
                &empty_doc(t),
                &json!({}),
            )
            .await?;
        anchor.get_or_insert((id, number));
    }
    // Glossary terms and records must hang from a lesson: the first one of
    // the case, or a placeholder when the case has none.
    let (anchor, anchor_number) = match anchor {
        Some(a) => a,
        None => (app.stub_lesson(1).await?, 1),
    };
    let topic_glossary = case.array("topic_glossary");
    app.glossary(&topic_glossary, anchor).await?;
    // The records the case gives are written as they are: the Assembler shows
    // them to the Generator (`{{records_section}}`). The plan may also cite
    // numbers the case does not list (older suites): placeholders make the
    // real plan validation accept them.
    let plan = case.get("plan").cloned().unwrap_or(Value::Null);
    let mut records = case.array("records");
    let listed: HashSet<u64> = records
        .iter()
        .filter_map(|r| r.get("number").and_then(Value::as_u64))
        .collect();
    let cited = plan
        .get("prerequisites")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|p| {
            p.get("record_numbers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|n| n.as_u64())
        .filter(|n| !listed.contains(n));
    records.extend(cited.map(|n| json!({"number": n, "kind": "demonstrated", "title": "Record di prova", "body": "Segnaposto per la validazione del piano."})));
    app.records(&records, anchor_number).await?;

    env.planned.set_plan(Some(plan.to_string()));
    // `fixture_pages`: the provider cannot read a page nobody publishes, so
    // the conversation carries it as already read.
    let injected = injected_pages(case, &sources);
    env.injector.set_pages(injected.clone());
    let runner = Arc::new(app.state.job_runner(vec![(
        JobKind::LessonGeneration,
        app.state.generation_handler(),
    )]));
    let task = tokio::spawn(runner.run());
    let mark = env.ledger.mark();
    let started = app
        .state
        .lessons
        .request_generation(UserId::OWNER, app.topic)
        .await
        .map_err(|e| anyhow!("{e:?}"));
    let job = match started {
        Ok(_) => {
            wait_for_job(
                &app,
                JobKind::LessonGeneration,
                None,
                std::time::Duration::from_secs(30 * 60),
            )
            .await
        }
        Err(e) => Err(e),
    };
    task.abort();
    env.planned.set_plan(None);
    env.injector.set_pages(Vec::new());
    let job = job?;
    let all_calls = subject_calls(env, mark, LlmRole::Generator);
    // A turn paused by the provider (`pause_turn`) is continued by another
    // call: only the calls that ended their turn wrote a document.
    let calls: Vec<_> = all_calls
        .iter()
        .filter(|c| c.stop_reason != "PauseTurn")
        .cloned()
        .collect();

    // The Validator, with the Topic's real context, on each document.
    let document_resources: HashSet<Uuid> = documents
        .iter()
        .filter_map(|d| Uuid::parse_str(text(d, "resource_id")).ok())
        .collect();
    let resource_ids: HashSet<Uuid> = sources
        .iter()
        .filter_map(|s| Uuid::parse_str(text(s, "resource_id")).ok())
        .chain(document_resources.iter().copied())
        .collect();
    let lesson_ids: HashSet<Uuid> = lessons
        .iter()
        .filter_map(|l| Uuid::parse_str(text(l, "id")).ok())
        .collect();
    let validator = LessonValidator::new(
        ValidatorConfig::default(),
        ValidationContext::production(resource_ids, lesson_ids)
            .with_document_resources(document_resources),
    );
    // What the model was given as excerpts (the first call: repairs read the same).
    let excerpt = calls
        .first()
        .map_or_default(|c| Excerpt::from_request(&c.request));
    let mut docs: Vec<DocAttempt> = calls
        .iter()
        .map(|c| match parse_object(&c.text) {
            Ok(document) => {
                let issues = validator.validate(&document);
                let warnings = if issues.is_empty() {
                    validator.quality_warnings(&document, case.content_language())
                } else {
                    Vec::new()
                };
                DocAttempt {
                    improve: c.label == "generator.improve",
                    warnings,
                    issues,
                    document: Some(document),
                    parse_errors: vec![],
                    output_tokens: c.usage.output_tokens,
                }
            }
            Err(e) => DocAttempt {
                improve: c.label == "generator.improve",
                document: None,
                issues: vec![],
                parse_errors: e,
                output_tokens: c.usage.output_tokens,
                ..DocAttempt::default()
            },
        })
        .collect();

    // The backend replaces the model's `estimated_minutes` before saving: the
    // checks (`estimated_minutes_*`) and the judge read the saved value.
    if let Some(domain::JobResult::LessonGeneration { lesson_id, .. }) = &job.result
        && let Ok(saved) = app.state.lessons.get(UserId::OWNER, *lesson_id).await
        && let Some(doc) = kept_index(&docs).and_then(|i| docs[i].document.as_mut())
    {
        doc["estimated_minutes"] = json!(saved.estimated_minutes);
    }

    let mut checks = Vec::new();
    if job.status != domain::JobStatus::Succeeded {
        checks.push(Check::of("job_succeeded", false, || job_failure(&job)));
    }
    let mut spec = suite.deterministic(case);
    // The fetch checks may sit in `fetch_checks` or, once moved, in
    // `deterministic`: both are read.
    let mut fetch_spec = suite.fetch_checks(case);
    for key in fetch::KEYS {
        if let Some(v) = spec.remove(*key) {
            fetch_spec.insert((*key).to_owned(), v);
        }
    }
    let facts = Facts {
        case,
        docs: &docs,
        sources: &sources,
        lessons: &lessons,
        topic_glossary: &topic_glossary,
        plan: &plan,
        documents: &documents,
        excerpt: &excerpt,
    };
    checks.extend(generator_checks(&spec, &facts));

    let final_doc = kept_index(&docs).and_then(|i| docs[i].document.clone());
    let doc_text = final_doc.as_ref().map_or_default(Value::to_string);
    // What the provider's tool read, from the recorded answers.
    let read_pages = fetch::pages_with_injected(&all_calls, &injected);
    let plan_urls = plan_source_urls(&plan, &sources);
    let fixture_pages: Map<String, Value> = case
        .get("fixture_pages")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let lesson_text = final_doc
        .as_ref()
        .map_or_default(|d| sdui::visible_strings(d).join("\n"));
    checks.extend(fetch::checks(
        &fetch_spec,
        &fetch::FetchFacts {
            calls: &all_calls,
            plan_urls: &plan_urls,
            lesson_text: &lesson_text,
            document: final_doc.as_ref(),
            fixture_pages: &fixture_pages,
            injected: &injected,
        },
    ));
    let excerpt_note = if excerpt.raw.is_empty() {
        String::new()
    } else {
        format!(
            "Excerpts of the user's PDF given to the Generator (the only text it may rely on for the PDF; pages shown: {:?}, pages omitted: {:?}):\n{}\n\n",
            excerpt.shown, excerpt.omitted, excerpt.raw
        )
    };
    let pages_note = fetch::judge_material(&read_pages);
    let pages_note = if pages_note.is_empty() {
        String::new()
    } else {
        format!(
            "Web pages the Generator read with its web fetch tool (the only text of these sources it saw; a source without a page here was not read):\n{pages_note}"
        )
    };
    // The sources as the Generator saw them in its context (title, URL,
    // annotation): without them the judge cannot tell what an unread
    // source's annotation already says.
    let sources_note = if sources.is_empty() {
        String::new()
    } else {
        let listed: Vec<Value> = sources
            .iter()
            .map(|s| {
                serde_json::json!({
                    "title": s.get("title"),
                    "url": s.get("url"),
                    "annotation": s.get("annotation"),
                    "use_for": s.get("use_for"),
                })
            })
            .collect();
        format!(
            "Sources given to the Generator with their annotations (what it may say about a source it did not read):\n{}\n\n",
            Value::Array(listed)
        )
    };
    let material = format!(
        "Topic: {}\nMission: {mission}\nPlan given to the Generator: {plan}\n\n{sources_note}{excerpt_note}{pages_note}Lesson document (JSON, SDUI catalog v1) written by the Generator:\n{doc_text}",
        title
    );
    let judge = env
        .judge
        .judge(&material, &criteria(case.get("rubric")))
        .await;
    let issue_codes: Vec<Vec<&str>> = docs
        .iter()
        .map(|d| d.issues.iter().map(|i| i.code.as_str()).collect())
        .collect();
    let attempts = attempts_report(&calls, &docs);
    let mut run = RoleRun {
        turns: vec![TurnReport {
            label: "lesson".into(),
            input: String::new(),
            output: doc_text,
            checks,
            judge,
            extras: json!({"documents": docs.len(), "repairs": docs.iter().filter(|d| !d.improve).count().saturating_sub(1),
                "quality": quality_report(&docs), "validator_codes_per_document": issue_codes,
                "attempts": attempts,
                "web_fetch": {
                    "tool_on": fetch_on,
                    "injected_pages": injected.iter().map(|p| p.url.clone()).collect::<Vec<_>>(),
                    "requests": all_calls.iter().map(fetch::call_web_fetches).sum::<u32>(),
                    "model_calls": all_calls.len(),
                    "block_diagnostics": fetch::block_diagnostics(&all_calls),
                    "pause_turn_continuations": all_calls.len() - calls.len(),
                    "pages": read_pages.iter().map(|p| json!({"url": p.url, "error": p.error, "chars": p.text.as_deref().map(|t| t.chars().count())})).collect::<Vec<_>>(),
                    "fetched_tokens_estimate": fetch::fetched_tokens_estimate(&read_pages),
                },
                "excerpt_pages_shown": excerpt.shown, "excerpt_pages_omitted": excerpt.omitted,
                "parse_errors": docs.iter().map(|d| d.parse_errors.clone()).collect::<Vec<_>>()}),
        }],
        ..RoleRun::default()
    };
    let wanted: Vec<String> = case
        .get("report_metrics")
        .and_then(Value::as_array)
        .map_or_default(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
    let wanted: Vec<&str> = wanted.iter().map(String::as_str).collect();
    let unsupported = final_doc
        .as_ref()
        .map_or(0, |d| fetch::unsupported_citations(d, &read_pages).len());
    run.metrics
        .extend(fetch::metrics(&wanted, &all_calls, docs.len(), unsupported));
    run.metrics.insert("documents".into(), docs.len() as f64);
    run.metrics.extend(quality_metrics(&docs));
    run.metrics.insert(
        "output_tokens".into(),
        f64::from(docs.iter().map(|d| d.output_tokens).max().unwrap_or(0)),
    );
    Ok(run)
}

/// Quality warnings before and after the improvement, and whether it was
/// tried and kept (the score is the domain's `warning_score`).
fn quality_metrics(docs: &[DocAttempt]) -> Vec<(String, f64)> {
    let base = docs.iter().rposition(|d| !d.improve && d.valid());
    let Some(b) = base else { return Vec::new() };
    let tried = docs.iter().any(|d| d.improve);
    let kept = kept_index(docs).is_some_and(|i| docs[i].improve);
    let after = kept_index(docs).map_or(0, |i| docs[i].warnings.len());
    vec![
        ("warnings_before".into(), docs[b].warnings.len() as f64),
        ("warnings_after".into(), after as f64),
        ("score_before".into(), f64::from(docs[b].score())),
        ("improve_tried".into(), f64::from(u8::from(tried))),
        ("improve_kept".into(), f64::from(u8::from(kept))),
    ]
}

fn quality_report(docs: &[DocAttempt]) -> Value {
    let codes =
        |d: &DocAttempt| -> Vec<&str> { d.warnings.iter().map(|w| w.code.as_str()).collect() };
    let base = docs.iter().rposition(|d| !d.improve && d.valid());
    let improved = docs.iter().rposition(|d| d.improve);
    json!({
        "before": base.map(|i| codes(&docs[i])),
        "improve_valid": improved.map(|i| docs[i].valid()),
        "improve_codes": improved.map(|i| codes(&docs[i])),
        "kept_improvement": kept_index(docs).is_some_and(|i| docs[i].improve),
    })
}

/// Every attempt of the Generator, not only the last: what the Validator said
/// (code and path), and the head of the answer when it is not a valid
/// document. `thinking_tokens` (0 when the provider does not report it) is part
/// of `output_tokens`.
fn attempts_report(calls: &[crate::llm::CallRecord], docs: &[DocAttempt]) -> Value {
    Value::Array(
        calls
            .iter()
            .zip(docs)
            .enumerate()
            .map(|(i, (call, doc))| {
                let mut issues: Vec<Value> = doc
                    .issues
                    .iter()
                    .map(|x| json!({"code": x.code.as_str(), "path": x.path, "message": x.message}))
                    .collect();
                // An answer that is not a JSON object is a `schema` problem at
                // the root, as the backend itself reports it.
                issues.extend(
                    doc.parse_errors
                        .iter()
                        .map(|m| json!({"code": "schema", "path": "", "message": m})),
                );
                json!({
                    "attempt": i + 1,
                    "output_tokens": call.usage.output_tokens,
                    "thinking_tokens": call.usage.thinking_tokens,
                    "stop_reason": call.stop_reason,
                    "valid": doc.valid(),
                    "issues": issues,
                    "response_head": (!doc.valid()).then(|| super::response_head(&call.text)),
                })
            })
            .collect(),
    )
}

/// Addresses of the sources the plan chose (primary and others), as the case
/// gives them: the only ones the Generator may be asked to read.
fn plan_source_urls(plan: &Value, sources: &[Value]) -> Vec<String> {
    let chosen: Vec<&str> = plan
        .get("primary_source_id")
        .and_then(Value::as_str)
        .into_iter()
        .chain(
            plan.get("other_source_ids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .collect();
    sources
        .iter()
        .filter(|s| chosen.contains(&text(s, "resource_id")))
        .filter_map(|s| s.get("url").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

/// Every key of `deterministic` the Generator's checks compute; a suite
/// naming any other is refused at load time.
pub const KNOWN_KEYS: &[&str] = &[
    "validator_pass_first_try",
    "validator_pass_within_3",
    "output_tokens",
    "quality_warnings_max",
    "language",
    "sources_copied_exactly",
    "topic_glossary_copied_exactly",
    "document_sources",
    "pdf_anchor_opening",
    "pages_mentioned_within_excerpt",
    "no_page_exercises",
    "code_indentation_rebuilt",
    "code_lines_in_excerpt",
    "lesson_links_must_be_in",
    "no_exclamation_in_feedback",
    "sections",
    "forbidden_phrases",
    "must_not_match_in_text",
    "objective_not_repeated_in_paragraph",
    "open_answer_rubric_words",
    "quiz_correct_position_varies",
    "no_nearly_all_or_none_of_the_above",
    "exercise_types_subset_of_plan",
    "quiz_option_char_ratio_max",
    "quiz_correct_not_longest_in_chars",
    "fill_blank_answer_not_in_adjacent_text",
    "closing_active",
    "consecutive_code_blocks_need_missing_component",
    "glossary_entries_max",
    "numbers_need_citation",
    "min_exercises",
    "exercises_with_code",
    "must_include_block_types",
    "block_type_count",
    "must_include_missing_component",
    "missing_component_max",
    "max_same_type_consecutive",
];

/// Whether the Generator's suites may name `key` in `deterministic`.
#[must_use]
pub fn knows_check(key: &str) -> bool {
    KNOWN_KEYS.contains(&key) || shape::KEYS.contains(&key) || fetch::KEYS.contains(&key)
}

/// Why the value of a Generator key cannot be read, or `None` (see
/// `planner::value_problem`).
#[must_use]
pub fn value_problem(key: &str, v: &Value) -> Option<String> {
    use crate::checks::{Want, value_problem as top};
    match key {
        "sections" | "open_answer_rubric_words" | "exercises_with_code" => top(key, v, Want::Range),
        "quiz_option_char_ratio_max" => v
            .as_f64()
            .is_none()
            .then(|| format!("`{key}` must be a number, found {v}")),
        "glossary_entries_max"
        | "quality_warnings_max"
        | "min_exercises"
        | "missing_component_max"
        | "max_same_type_consecutive" => top(key, v, Want::UInt),
        "must_include_block_types" => top(key, v, Want::StrList),
        "block_type_count" => block_type_count_problem(key, v),
        "must_include_missing_component" => {
            (!v.is_string()).then(|| format!("`{key}` must be a string, found {v}"))
        }
        _ if fetch::KEYS.contains(&key) => fetch::value_problem(key, v),
        _ => shape::value_problem(key, v),
    }
}

/// `block_type_count` is an object `{"<block type>": [min, max]}`.
fn block_type_count_problem(key: &str, v: &Value) -> Option<String> {
    let Some(o) = v.as_object() else {
        return Some(format!("`{key}` must be an object of ranges, found {v}"));
    };
    o.iter().find_map(|(t, r)| {
        if !domain::lesson::BLOCK_TYPES.contains(&t.as_str()) {
            return Some(format!(
                "`{key}` names `{t}`, which is not a block type of the catalog"
            ));
        }
        crate::checks::value_problem(&format!("{key}.{t}"), r, crate::checks::Want::Range)
    })
}

pub struct Facts<'a> {
    pub case: &'a Case,
    pub docs: &'a [DocAttempt],
    pub sources: &'a [Value],
    pub lessons: &'a [Value],
    pub topic_glossary: &'a [Value],
    pub plan: &'a Value,
    /// Uploaded PDFs of the case (`documents`), possibly none.
    pub documents: &'a [Value],
    pub excerpt: &'a Excerpt,
}

/// The excerpts of the user's PDFs as the Generator received them, read back
/// from the request: the pages it saw (`[p. N]`), the pages the backend left
/// out, and the text without whitespace (to compare code with).
#[derive(Debug, Default)]
pub struct Excerpt {
    pub raw: String,
    pub shown: BTreeSet<u64>,
    pub omitted: BTreeSet<u64>,
    squashed: String,
}

impl Excerpt {
    pub fn from_request(req: &domain::llm::LlmRequest) -> Self {
        let all: String = req
            .system
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Self::from_context(&all)
    }

    pub fn from_context(context: &str) -> Self {
        let Some(start) = context.find("## Estratti delle fonti") else {
            return Self::default();
        };
        let body = &context[start..];
        let body = body
            .get("## Estratti delle fonti".len()..)
            .map_or(body, |b| b.split("\n## ").next().unwrap_or(b))
            .trim();
        let marker = Regex::new(r"\[p\. (\d+)\]").expect("static regex");
        let shown = marker
            .captures_iter(body)
            .filter_map(|c| c[1].parse().ok())
            .collect();
        let mut omitted = BTreeSet::new();
        if let Some(note) = body.split("Pagine omesse").nth(1) {
            let note = note.split("Non dire").next().unwrap_or(note);
            for part in note.split("pagine ").skip(1) {
                let pages = part.split(['"', ';']).next().unwrap_or(part);
                for piece in pages.split(',') {
                    let piece = piece.trim().trim_end_matches('.');
                    let (a, b) = piece.split_once('-').unwrap_or((piece, piece));
                    if let (Ok(a), Ok(b)) = (a.trim().parse::<u64>(), b.trim().parse::<u64>()) {
                        omitted.extend(a..=b);
                    }
                }
            }
        }
        Self {
            squashed: body.chars().filter(|c| !c.is_whitespace()).collect(),
            raw: body.to_owned(),
            shown,
            omitted,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty() || self.raw == "Nessuno."
    }
}

fn count_in(spec: &Value, n: usize) -> Option<bool> {
    range(spec).map(|r| in_range(n, r))
}

#[allow(clippy::too_many_lines)] // one arm per key of the suite
pub fn generator_checks(spec: &Map<String, Value>, f: &Facts<'_>) -> Vec<Check> {
    let doc_ref = kept_index(f.docs).and_then(|i| f.docs[i].document.as_ref());
    // `validator_*` keys are about the sequence of documents, not one.
    let mut out: Vec<Check> = Vec::new();
    let sequence = [
        "validator_pass_first_try",
        "validator_pass_within_3",
        "output_tokens",
        "quality_warnings_max",
    ];
    let mut rest = spec.clone();
    for key in sequence {
        let Some(v) = rest.remove(key) else { continue };
        out.push(match key {
            "validator_pass_first_try" => {
                let ok = f.docs.first().is_some_and(DocAttempt::valid);
                let codes: Vec<&str> = f
                    .docs
                    .first()
                    .map_or_default(|d| d.issues.iter().map(|i| i.code.as_str()).collect());
                // The suite wants >= 60% over runs: a miss is information, not a failed run.
                Check::soft(key, ok, || {
                    format!("the first document was not valid: {codes:?}")
                })
            }
            "validator_pass_within_3" => {
                let ok = f.docs.iter().take(3).any(DocAttempt::valid);
                Check::of(key, ok, || {
                    format!("no valid document in {} attempts", f.docs.len().min(3))
                })
            }
            "quality_warnings_max" => {
                // The warnings of the document the backend keeps: computed by
                // the Validator of the domain (one rule, not a copy here).
                let max = v.as_u64().unwrap_or(0);
                let kept = kept_index(f.docs).map(|i| &f.docs[i]);
                let n = kept.map_or(0, |d| d.warnings.len() as u64);
                let codes: Vec<&str> =
                    kept.map_or_default(|d| d.warnings.iter().map(|w| w.code.as_str()).collect());
                Check::soft(key, n <= max, || {
                    format!("{n} quality warnings in the kept document (max {max}): {codes:?}")
                })
            }
            _ => {
                let max = f.docs.iter().map(|d| d.output_tokens).max().unwrap_or(0);
                let warn = v
                    .get("warn_above")
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX);
                Check::soft(key, u64::from(max) <= warn, || {
                    format!("{max} output tokens in a document, warn above {warn}")
                })
            }
        });
    }
    let Some(doc) = doc_ref else {
        out.push(Check::of("document", false, || {
            "the model produced no readable document".into()
        }));
        return out;
    };
    let blocks = sdui::top(doc);
    let flat = sdui::all(doc);
    let prose_texts: Vec<String> = flat.iter().filter_map(|b| prose(b)).collect();
    let all_text = sdui::visible_strings(doc).join("\n");
    let exercises: Vec<&Value> = flat.iter().copied().filter(|b| is_exercise(b)).collect();
    let quizzes: Vec<&Value> = flat
        .iter()
        .copied()
        .filter(|b| matches!(kind(b), "quiz_single" | "quiz_multi"))
        .collect();
    let level2 =
        |b: &Value| kind(b) == "heading" && b.get("level").and_then(Value::as_u64) == Some(2);

    // The shape of a Phase 12 lesson (and the prose count) lives in `shape`.
    let (shape_spec, rest): (Map<String, Value>, Map<String, Value>) = rest
        .into_iter()
        .partition(|(k, _)| shape::KEYS.contains(&k.as_str()));
    out.extend(shape::checks(
        &shape_spec,
        &shape::Shape {
            doc,
            plan: f.plan,
            blocks: blocks.clone(),
            all_text: &all_text,
            language: f.case.content_language(),
        },
    ));

    // The text checks of `run_spec` read an empty text here, which makes
    // `language` pass whatever the lesson says: it gets the lesson's prose.
    let mut rest = rest;
    if let Some(spec) = rest.remove("language") {
        out.push(super::language::check("language", &spec, doc));
    }

    out.extend(run_spec(&rest, "", |key, v| {
        let sev = |c: Check| c.with_severity(v);
        Some(match key {
            "sources_copied_exactly" => {
                let bad: Vec<String> = doc.get("sources").and_then(Value::as_array).into_iter().flatten()
                    .filter(|s| {
                        let id = text(s, "resource_id");
                        // Sources of uploaded PDFs are checked by `document_sources`.
                        !is_document_source(f.documents, id) && !f.sources.iter().any(|c| text(c, "resource_id") == id && text(c, "title") == text(s, "title") && text(c, "url") == text(s, "url"))
                    })
                    .map(|s| text(s, "key").to_owned()).collect();
                Check::of(key, bad.is_empty(), || format!("sources not copied exactly: {bad:?}"))
            }
            "topic_glossary_copied_exactly" => {
                let bad: Vec<String> = doc.get("glossary").and_then(Value::as_array).into_iter().flatten()
                    .filter_map(|g| {
                        let t = f.topic_glossary.iter().find(|t| text(t, "key") == text(g, "key"))?;
                        let same = ["term", "definition"].iter().all(|k| text(t, k) == text(g, k))
                            && t.get("avoid").cloned().unwrap_or(json!([])) == g.get("avoid").cloned().unwrap_or(json!([]));
                        (!same).then(|| text(g, "key").to_owned())
                    }).collect();
                Check::of(key, bad.is_empty(), || format!("entries not copied exactly: {bad:?}"))
            }
            "document_sources" => {
                let problems = document_source_problems(doc, f);
                Check::of(key, problems.is_empty(), || problems.join("; "))
            }
            "pdf_anchor_opening" => {
                let problems = anchor_problems(doc, &blocks, f);
                Check::of(key, problems.is_empty(), || problems.join("; "))
            }
            "pages_mentioned_within_excerpt" => {
                let bad: Vec<u64> = pages_in(&all_text).into_iter()
                    .filter(|p| !f.excerpt.shown.contains(p) || f.excerpt.omitted.contains(p))
                    .collect();
                Check::of(key, bad.is_empty() && !f.excerpt.shown.is_empty(), || format!("page numbers in the text that the excerpt does not show: {bad:?}"))
            }
            "no_page_exercises" => {
                let own = patterns(v);
                let re = regex(&if own.is_empty() { PAGE_WORDS.to_owned() } else { own.join("|") }).ok()?;
                let bad: Vec<&str> = exercises.iter().filter(|b| {
                    let s = sdui::visible_strings(&json!({"blocks": [b]})).join("\n");
                                        re.is_match(&s)
                }).map(|b| text(b, "id")).collect();
                Check::of(key, bad.is_empty(), || format!("exercises about pages, chapters or listings: {bad:?}"))
            }
            "code_indentation_rebuilt" => {
                let langs = patterns(v);
                let bad: Vec<&str> = flat.iter().filter(|b| kind(b) == "code" && langs.iter().any(|l| l == text(b, "language")))
                    .filter(|b| flat_indentation(text(b, "code"))).map(|b| text(b, "id")).collect();
                Check::of(key, bad.is_empty(), || format!("code blocks with unindented bodies (the PDF text loses indentation): {bad:?}"))
            }
            "code_lines_in_excerpt" => {
                let min = v.get("min_share").and_then(Value::as_f64).unwrap_or(0.5);
                let langs = patterns(v.get("languages").unwrap_or(&Value::Null));
                let lines: Vec<String> = flat.iter().filter(|b| kind(b) == "code" && (langs.is_empty() || langs.iter().any(|l| l == text(b, "language"))))
                    .flat_map(|b| text(b, "code").lines().map(|l| l.chars().filter(|c| !c.is_whitespace()).collect::<String>()).collect::<Vec<_>>())
                    .filter(|l| l.chars().count() >= 6).collect();
                let found = lines.iter().filter(|l| f.excerpt.squashed.contains(l.as_str())).count();
                #[allow(clippy::cast_precision_loss)]
                let share = if lines.is_empty() { 1.0 } else { found as f64 / lines.len() as f64 };
                sev(Check::soft(key, share >= min, || format!("{found} of {} code lines come from the excerpt ({share:.2}), expected {min}", lines.len())))
            }
            "lesson_links_must_be_in" => {
                let allowed: Vec<String> = match v {
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
                    _ => f.lessons.iter().map(|l| text(l, "id").to_owned()).collect(),
                };
                let bad: Vec<&str> = flat.iter().filter(|b| kind(b) == "lesson_link").map(|b| text(b, "lesson_id")).filter(|id| !allowed.iter().any(|a| a == id)).collect();
                Check::of(key, bad.is_empty(), || format!("links to lessons outside the context: {bad:?}"))
            }
            "no_exclamation_in_feedback" => {
                let bad = sdui::feedback_strings(doc).into_iter().filter(|s| crate::checks::strip_inline_code(s).contains('!')).count();
                Check::of(key, bad == 0, || format!("{bad} feedback text(s) with `!`"))
            }
            "sections" => {
                let n = blocks.iter().filter(|b| level2(b)).count();
                let r = range(v)?;
                Check::of(key, in_range(n, r), || format!("{n} sections, expected {r:?}"))
            }
            "forbidden_phrases" | "must_not_match_in_text" => crate::checks::must_not_match(key, v, &all_text),
            "objective_not_repeated_in_paragraph" => {
                let objective = crate::checks::plain(text(doc, "objective")).to_lowercase();
                let hit = !objective.is_empty() && prose_texts.iter().any(|t| t.to_lowercase().contains(&objective));
                Check::of(key, !hit, || "a paragraph repeats the objective verbatim".into())
            }
            "open_answer_rubric_words" => {
                let r = range(v)?;
                let bad: Vec<usize> = flat.iter().filter(|b| kind(b) == "open_answer").map(|b| words(text(b, "rubric"))).filter(|n| !in_range(*n, r)).collect();
                Check::of(key, bad.is_empty(), || format!("rubric lengths {bad:?}, expected {r:?}"))
            }
            "quiz_correct_position_varies" => {
                let singles: Vec<&&Value> = quizzes.iter().filter(|b| kind(b) == "quiz_single").collect();
                let positions: HashSet<usize> = singles.iter().filter_map(|b| options(b).iter().position(|(id, _)| correct_ids(b).contains(id))).collect();
                Check::of(key, singles.len() < 3 || positions.len() > 1, || "the correct option is always in the same position".into())
            }
            "no_nearly_all_or_none_of_the_above" => {
                let pats = patterns(v);
                let bad: Vec<String> = quizzes.iter().flat_map(|b| options(b)).filter_map(|(_, t)| first_match(&pats, &t).ok().flatten()).collect();
                Check::of(key, bad.is_empty(), || format!("{bad:?}"))
            }
            "exercise_types_subset_of_plan" => {
                let planned: HashSet<&str> = f.plan.get("exercises").and_then(Value::as_array).into_iter().flatten().map(|e| text(e, "type")).collect();
                let extra: Vec<&str> = exercises.iter().map(|b| kind(b)).filter(|k| !planned.contains(k)).collect();
                sev(Check::of(key, extra.is_empty(), || format!("types outside the plan: {extra:?}")))
            }
            "quiz_option_char_ratio_max" => {
                let max = v.as_f64()?;
                let bad: Vec<String> = quizzes.iter().filter_map(|b| {
                    let lens: Vec<usize> = options(b).iter().map(|(_, t)| t.chars().count()).collect();
                    let (lo, hi) = (*lens.iter().min()?, *lens.iter().max()?);
                    #[allow(clippy::cast_precision_loss)]
                    let ratio = hi as f64 / lo.max(1) as f64;
                    (ratio > max).then(|| format!("{} {ratio:.2}", text(b, "id")))
                }).collect();
                Check::of(key, bad.is_empty(), || format!("char ratio above {max}: {bad:?}"))
            }
            "quiz_correct_not_longest_in_chars" => {
                let singles: Vec<&&Value> = quizzes.iter().filter(|b| kind(b) == "quiz_single").collect();
                let longest = singles.iter().filter(|b| {
                    let o = options(b);
                    let c = correct_ids(b);
                    let len = |id: &str| o.iter().find(|(i, _)| i == id).map_or(0, |(_, t)| t.chars().count());
                    let correct = c.first().map_or(0, |id| len(id));
                    o.iter().filter(|(i, _)| !c.contains(i)).all(|(i, _)| len(i) < correct)
                }).count();
                let ok = if singles.len() < 3 { longest == 0 } else { longest * 3 <= singles.len() };
                Check::soft(key, ok, || format!("{longest} of {} quizzes have the longest option as the correct one", singles.len()))
            }
            "fill_blank_answer_not_in_adjacent_text" => sev(fill_blank_leak(key, &blocks).0),
            "closing_active" => closing_active(key, v, &blocks),
            "consecutive_code_blocks_need_missing_component" => {
                let cue = regex(r"(?i)\b(invece|rispetto|confronto|mentre|contro|vs|Go|prima|dopo)\b").ok()?;
                let has_compare = flat.iter().any(|b| kind(b) == "missing_component" && text(b, "wanted") == "code_compare");
                let triggered = blocks.windows(2).any(|w| {
                    kind(w[0]) == "code" && kind(w[1]) == "code"
                        && (cue.is_match(text(w[1], "caption")) || cue.is_match(text(w[1], "code")))
                });
                sev(Check::of(key, !triggered || has_compare, || "two adjacent code blocks compare and no `code_compare` missing_component was written".into()))
            }
            "glossary_entries_max" => {
                let n = doc.get("glossary").and_then(Value::as_array).map_or(0, Vec::len);
                let max = v.as_u64()?;
                Check::of(key, n as u64 <= max, || format!("{n} glossary entries, max {max}"))
            }
            "numbers_need_citation" => {
                let digit = regex(r"\d").ok()?;
                let tick = regex(r"`[^`]*`").ok()?;
                let mut texts: Vec<String> = flat.iter().filter(|b| matches!(kind(b), "paragraph" | "callout")).map(|b| format!("{} {}", text(b, "title"), text(b, "text"))).collect();
                texts.extend(sdui::feedback_strings(doc));
                let bad = texts.iter().filter(|t| digit.is_match(&tick.replace_all(t, "")) && !t.contains("[^")).count();
                Check::soft(key, bad == 0, || format!("{bad} text(s) with a number and no citation (heuristic)"))
            }
            "exercises_with_code" => {
                let r = range(v)?;
                let n = exercises.iter().filter(|b| b.get("code").is_some_and(|c| !c.is_null())).count();
                Check::of(key, in_range(n, r), || format!("{n} exercises with a `code` field, expected {r:?}"))
            }
            "min_exercises" => {
                let min = v.as_u64()?;
                Check::of(key, exercises.len() as u64 >= min, || format!("{} exercises, min {min}", exercises.len()))
            }
            "must_include_block_types" => {
                let missing: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).filter(|t| !flat.iter().any(|b| kind(b) == *t)).collect();
                Check::of(key, missing.is_empty(), || format!("missing block types: {missing:?}"))
            }
            "block_type_count" => {
                // Over `flat` like `must_include_block_types`: the blocks inside
                // a `missing_component` fallback count too.
                let mut bad = Vec::new();
                for (t, r) in v.as_object()? {
                    let r = range(r)?;
                    let n = flat.iter().filter(|b| kind(b) == t).count();
                    if !in_range(n, r) {
                        bad.push(format!("{n} `{t}` blocks, expected {}-{}", r.0, r.1));
                    }
                }
                Check::of(key, bad.is_empty(), || bad.join("; "))
            }
            "must_include_missing_component" => {
                let want = v.as_str()?;
                Check::of(key, flat.iter().any(|b| kind(b) == "missing_component" && text(b, "wanted") == want), || format!("no missing_component `{want}`"))
            }
            "missing_component_max" => {
                let n = flat.iter().filter(|b| kind(b) == "missing_component").count();
                let max = v.as_u64()?;
                Check::of(key, n as u64 <= max, || format!("{n} missing_component blocks, max {max}"))
            }
            "max_same_type_consecutive" => {
                let max = v.as_u64()?;
                let mut run = 0usize;
                let mut worst = 0usize;
                let mut prev = "";
                for b in &exercises {
                    run = if kind(b) == prev { run + 1 } else { 1 };
                    prev = kind(b);
                    worst = worst.max(run);
                }
                Check::of(key, worst as u64 <= max, || format!("{worst} exercises of the same type in a row, max {max}"))
            }
            _ => return None,
        })
    }));
    if rest.contains_key("fill_blank_answer_not_in_adjacent_text") {
        let (_, short) = fill_blank_leak("fill_blank_answer_not_in_adjacent_text", &blocks);
        if !short.is_empty() {
            out.push(Check::skipped(
                "fill_blank_short_answers",
                &format!(
                    "answers under {MIN_ANSWER_CHARS} characters are not compared with the text: {short:?}"
                ),
            ));
        }
    }
    let _ = (EXERCISES, count_in(&Value::Null, 0), Status::Pass);
    out
}

/// Words and symbols of layout: an exercise must not ask about them.
const PAGE_WORDS: &str = r"(?i)\bpagin|\bpp?\.\s*\d|\bcapitol[oi]\s+\d|\bcap\.\s*\d|\blistat|\blisting|\bparagraf[oi]\s+\d";

fn is_document_source(documents: &[Value], resource_id: &str) -> bool {
    documents
        .iter()
        .any(|d| text(d, "resource_id") == resource_id)
}

/// Page numbers a text names (`p. 5`, `pp. 101-118`, `pagine 3 e 4`), ranges expanded.
fn pages_in(text: &str) -> BTreeSet<u64> {
    let re = Regex::new(r"(?i)\b(?:pp?\.|pagin[ae])\s*(\d+)(?:\s*(?:-|–|e)\s*(\d+))?")
        .expect("static regex");
    let mut out = BTreeSet::new();
    for c in re.captures_iter(text) {
        let a: u64 = c[1].parse().unwrap_or(0);
        let b: u64 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(a);
        if b >= a && b - a < 2000 {
            out.extend(a..=b);
        }
    }
    out
}

/// Source entries of the document whose pages must be in the title, and the
/// rest of `document_sources`.
fn document_source_problems(doc: &Value, f: &Facts<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    let entries: Vec<&Value> = doc
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    let pdf: Vec<&&Value> = entries
        .iter()
        .filter(|s| is_document_source(f.documents, text(s, "resource_id")))
        .collect();
    if !f.documents.is_empty() && pdf.is_empty() {
        problems.push("the PDF is not among the sources".to_owned());
    }
    if pdf.len() > 3 {
        problems.push(format!("{} sources for the PDF, max 3", pdf.len()));
    }
    for s in &entries {
        let is_pdf = is_document_source(f.documents, text(s, "resource_id"));
        let url_null = s.get("url").is_none_or(Value::is_null);
        if is_pdf != url_null {
            problems.push(format!("`{}`: url null iff PDF", text(s, "key")));
        }
    }
    for s in &pdf {
        let title = text(s, "title");
        let key = text(s, "key");
        if title.chars().count() > 200 {
            problems.push(format!("`{key}`: title over 200 characters"));
        }
        let pages = pages_in(title);
        if pages.is_empty() {
            problems.push(format!("`{key}`: no pages in the title `{title}`"));
        }
        let outside: Vec<&u64> = pages
            .iter()
            .filter(|p| !f.excerpt.shown.contains(p) || f.excerpt.omitted.contains(p))
            .collect();
        if !outside.is_empty() {
            problems.push(format!(
                "`{key}`: pages {outside:?} are not in the excerpt shown"
            ));
        }
    }
    problems
}

/// The first paragraph after the title says where the lesson is in the PDF.
fn anchor_problems(doc: &Value, blocks: &[&Value], f: &Facts<'_>) -> Vec<String> {
    let Some(b) = blocks.get(1).filter(|b| kind(b) == "paragraph") else {
        return vec!["the block after the title is not a paragraph".to_owned()];
    };
    let t = text(b, "text");
    let mut problems = Vec::new();
    let keys: Vec<&str> = doc
        .get("sources")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| is_document_source(f.documents, text(s, "resource_id")))
        .map(|s| text(s, "key"))
        .collect();
    if !keys.iter().any(|k| t.contains(&format!("[^{k}]"))) {
        problems.push("the opening does not cite the PDF".to_owned());
    }
    if pages_in(t).is_empty() {
        problems.push("the opening names no pages".to_owned());
    }
    let n = words(&crate::checks::plain(t));
    if n > 45 {
        problems.push(format!("the opening has {n} words, max 45"));
    }
    problems
}

/// A code block where a line ending in `{` is followed by a line at column 0
/// (the PDF text loses indentation and the model did not rebuild it).
fn flat_indentation(code: &str) -> bool {
    let lines: Vec<&str> = code.lines().collect();
    lines.windows(2).any(|w| {
        w[0].trim_end().ends_with('{')
            && !w[1].trim().is_empty()
            && !w[1].trim_start().starts_with('}')
            && !w[1].starts_with([' ', '\t'])
    })
}

/// Shortest answer that is compared with the text: shorter ones (`si`, `r`)
/// are common words and would flag nearly every lesson.
const MIN_ANSWER_CHARS: usize = 3;

/// No answer of a blank may appear, as a whole word, in the prose the learner
/// read just before (section heading, caption of the code block above, the two
/// previous non-exercise blocks) or just after. Code is not looked at. The
/// second value lists the answers too short to be checked.
fn fill_blank_leak(name: &str, blocks: &[&Value]) -> (Check, Vec<String>) {
    let mut leaks = Vec::new();
    let mut short = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if kind(b) != "fill_blank" {
            continue;
        }
        let answers: Vec<String> = b
            .get("blanks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|bl| {
                bl.get("answers")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|a| a.as_str().map(str::to_owned))
            .collect();
        let mut context: Vec<String> = Vec::new();
        if let Some(h) = blocks[..i].iter().rev().find(|x| kind(x) == "heading") {
            context.push(crate::checks::plain(text(h, "text")));
        }
        if let Some(c) = blocks[..i].iter().rev().find(|x| kind(x) == "code") {
            context.push(crate::checks::plain(&crate::checks::strip_inline_code(
                text(c, "caption"),
            )));
        }
        context.extend(
            blocks[..i]
                .iter()
                .rev()
                .filter(|x| !is_exercise(x))
                .filter_map(|x| sdui::prose_without_code(x))
                .take(2),
        );
        if let Some(p) = blocks.get(i + 1).and_then(|x| sdui::prose_without_code(x)) {
            context.push(p);
        }
        for a in &answers {
            if a.trim().chars().count() < MIN_ANSWER_CHARS {
                short.push(format!("{}: `{}`", text(b, "id"), a.trim()));
            } else if context.iter().any(|t| contains_answer(t, a)) {
                leaks.push(format!("{}: `{a}`", text(b, "id")));
            }
        }
    }
    let check = Check::of(name, leaks.is_empty(), || {
        format!("answers visible in the adjacent text: {leaks:?}")
    });
    (check, short)
}

/// Whole-word, case-insensitive.
pub(super) fn contains_answer(text: &str, answer: &str) -> bool {
    let a = answer.trim().to_lowercase();
    if a.is_empty() {
        return false;
    }
    let t = text.to_lowercase();
    let mut from = 0;
    while let Some(i) = t[from..].find(&a) {
        let start = from + i;
        let end = start + a.len();
        let before = t[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after = t[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        if before && after {
            return true;
        }
        from = start + a.chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn closing_active(name: &str, spec: &Value, blocks: &[&Value]) -> Check {
    let last = blocks
        .iter()
        .rposition(|b| kind(b) == "heading" && b.get("level").and_then(Value::as_u64) == Some(2));
    let section: Vec<&&Value> = blocks
        .get(last.unwrap_or(0)..)
        .unwrap_or_default()
        .iter()
        .collect();
    let mut problems = Vec::new();
    let checklist = section.iter().find(|b| kind(b) == "checklist");
    if spec
        .get("last_section_has_checklist")
        .and_then(Value::as_bool)
        == Some(true)
        && checklist.is_none()
    {
        problems.push("the last section has no checklist".to_owned());
    }
    if let (Some(c), Some(r)) = (checklist, spec.get("checklist_items").and_then(range)) {
        let n = c.get("items").and_then(Value::as_array).map_or(0, Vec::len);
        if !in_range(n, r) {
            problems.push(format!("checklist has {n} items, expected {r:?}"));
        }
    }
    if let Some(max) = spec
        .get("closing_summary_words_max")
        .and_then(Value::as_u64)
    {
        let n: usize = section
            .iter()
            .filter_map(|b| prose(b))
            .map(|t| words(&t))
            .sum();
        if n as u64 > max {
            problems.push(format!("closing prose has {n} words, max {max}"));
        }
    }
    Check::of(name, problems.is_empty(), || problems.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::status_of;

    fn case() -> Case {
        Case {
            id: "c".into(),
            raw: json!({}),
        }
    }

    fn check(spec: &Value, doc: &Value) -> Vec<Check> {
        let docs = vec![DocAttempt {
            document: Some(doc.clone()),
            issues: vec![],
            parse_errors: vec![],
            output_tokens: 100,
            ..DocAttempt::default()
        }];
        let c = case();
        let sources = vec![json!({"resource_id": "r1", "title": "T", "url": "https://a.org"})];
        let f = Facts {
            case: &c,
            docs: &docs,
            sources: &sources,
            lessons: &[],
            topic_glossary: &[],
            plan: &json!({"exercises": [{"type": "quiz_single"}]}),
            documents: &[],
            excerpt: &Excerpt::default(),
        };
        generator_checks(spec.as_object().unwrap(), &f)
    }

    fn quiz(id: &str, a: &str, b: &str, c: &str, correct: &str) -> Value {
        json!({"id": id, "type": "quiz_single", "question": "Q?", "correct": correct,
            "options": [{"id": "a", "text": a}, {"id": "b", "text": b}, {"id": "c", "text": c}]})
    }

    #[test]
    fn block_type_count_bounds_the_blocks_of_a_type() {
        let fig = |id: &str| json!({"id": id, "type": "diagram"});
        let spec = |r: Value| json!({"block_type_count": {"diagram": r}});
        let st = |doc: &Value, r: Value| status_of(&check(&spec(r), doc), "block_type_count");
        let two = json!({"blocks": [fig("a"), quiz("q", "x", "y", "z", "a"), fig("b")]});
        assert_eq!(st(&two, json!([1, 2])), Status::Pass);
        assert_eq!(st(&two, json!([3, 4])), Status::Fail);
        assert_eq!(st(&two, json!([0, 1])), Status::Fail);
        let none = json!({"blocks": [quiz("q", "x", "y", "z", "a")]});
        assert_eq!(st(&none, json!([0, 0])), Status::Pass);
        assert_eq!(st(&none, json!([1, 2])), Status::Fail);
        // Like `must_include_block_types`, a block inside a fallback counts.
        let nested = json!({"blocks": [{"id": "m", "type": "missing_component", "wanted": "x", "fallback": [fig("a")]}]});
        assert_eq!(st(&nested, json!([1, 1])), Status::Pass);
        // Every listed type is checked, and the detail names the failing one.
        let c = check(
            &json!({"block_type_count": {"diagram": [0, 5], "quiz_single": [2, 3]}}),
            &none,
        );
        let bad = c.iter().find(|c| c.name == "block_type_count").unwrap();
        assert_eq!(bad.status, Status::Fail);
        assert!(bad.detail.contains("quiz_single") && !bad.detail.contains("diagram"));
    }

    #[test]
    fn block_type_count_value_must_be_a_map_of_ranges() {
        assert!(value_problem("block_type_count", &json!({"diagram": [1, 2]})).is_none());
        assert!(value_problem("block_type_count", &json!({"diagram": 1})).is_some());
        assert!(value_problem("block_type_count", &json!([1, 2])).is_some());
        // A typo in the type would otherwise match no block and pass silently.
        let typo = value_problem("block_type_count", &json!({"diagrm": [0, 0]}));
        assert!(typo.is_some_and(|p| p.contains("diagrm")));
        assert!(knows_check("block_type_count"));
    }

    #[test]
    fn diagram_text_is_visible_to_the_text_checks() {
        let fig = json!({"id": "f", "type": "diagram", "layout": "flow", "title": "Il ciclo di vita",
            "nodes": [{"id": "n1", "label": "Parsing", "detail": "legge il testo"}, {"id": "n2", "label": "Validazione"}],
            "edges": [{"from": "n1", "to": "n2", "label": "se valido"}],
            "caption": "Le due fasi in ordine.", "alt": "Due caselle collegate da una freccia."});
        let doc = json!({"blocks": [fig]});
        let c = check(&json!({"forbidden_phrases": ["se valido"]}), &doc);
        assert!(c.iter().any(|c| c.status == Status::Fail), "{c:?}");
        for needle in [
            "Il ciclo di vita",
            "Parsing",
            "legge il testo",
            "se valido",
            "Le due fasi",
            "Due caselle",
        ] {
            let c = check(&json!({"must_not_match_in_text": [needle]}), &doc);
            assert!(
                c.iter().any(|c| c.status == Status::Fail),
                "{needle}: {c:?}"
            );
        }
        // Structure is not text: layout word and node ids are not visible.
        let all = sdui::visible_strings(&doc);
        assert!(!all.iter().any(|s| s == "flow" || s == "n1"), "{all:?}");
    }

    #[test]
    fn option_length_checks() {
        let doc = json!({"blocks": [quiz("q1", "uno", "due", "tre tre tre tre tre tre", "c")]});
        let c = check(
            &json!({"quiz_option_char_ratio_max": 1.3, "quiz_correct_not_longest_in_chars": true}),
            &doc,
        );
        assert_eq!(status_of(&c, "quiz_option_char_ratio_max"), Status::Fail);
        assert_eq!(
            status_of(&c, "quiz_correct_not_longest_in_chars"),
            Status::Warn
        );
        let even = json!({"blocks": [quiz("q1", "uno uno", "due due", "tre tre", "a")]});
        assert!(
            check(&json!({"quiz_option_char_ratio_max": 1.3}), &even)
                .iter()
                .all(|c| c.status == Status::Pass)
        );
    }

    #[test]
    fn a_blank_answer_in_the_adjacent_text_is_a_leak() {
        let doc = json!({"blocks": [
            {"id": "h", "type": "heading", "level": 2, "text": "Usare &mut"},
            {"id": "p", "type": "paragraph", "text": "Per modificare si usa &mut davanti al valore."},
            {"id": "f", "type": "fill_blank", "text": "Passa {{x}} s", "blanks": [{"id": "x", "answers": ["&mut"]}]}]});
        assert_eq!(
            check(
                &json!({"fill_blank_answer_not_in_adjacent_text": {"severity": "error"}}),
                &doc
            )[0]
            .status,
            Status::Fail
        );
        let clean = json!({"blocks": [
            {"id": "h", "type": "heading", "level": 2, "text": "Prestiti"},
            {"id": "p", "type": "paragraph", "text": "Un riferimento presta il valore."},
            {"id": "f", "type": "fill_blank", "text": "Passa {{x}} s", "blanks": [{"id": "x", "answers": ["&mut"]}]}]});
        assert_eq!(
            check(
                &json!({"fill_blank_answer_not_in_adjacent_text": {"severity": "error"}}),
                &clean
            )[0]
            .status,
            Status::Pass
        );
    }

    #[test]
    fn short_answers_and_code_are_not_compared() {
        let doc = json!({"blocks": [
            {"id": "h", "type": "heading", "level": 2, "text": "Prestiti"},
            {"id": "p", "type": "paragraph", "text": "Il compilatore si lamenta; usa `&mut` e `push_str` qui."},
            {"id": "f", "type": "fill_blank", "text": "Passa {{x}} e {{y}}", "blanks": [
                {"id": "x", "answers": ["si"]}, {"id": "y", "answers": ["&mut", "push_str"]}]}]});
        let c = check(
            &json!({"fill_blank_answer_not_in_adjacent_text": {"severity": "error"}}),
            &doc,
        );
        assert_eq!(
            status_of(&c, "fill_blank_answer_not_in_adjacent_text"),
            Status::Pass
        );
        assert_eq!(status_of(&c, "fill_blank_short_answers"), Status::Skipped);
    }

    #[test]
    fn exclamations_in_inline_code_of_feedback_are_ignored() {
        let doc = json!({"blocks": [{"id": "q", "type": "quiz_single", "question": "Q?", "correct": "a",
            "options": [{"id": "a", "text": "x", "feedback": "Usa `println!` per stampare."},
                        {"id": "b", "text": "y", "feedback": "Ecco!"}]}]});
        let c = check(&json!({"no_exclamation_in_feedback": true}), &doc);
        assert_eq!(status_of(&c, "no_exclamation_in_feedback"), Status::Fail);
        let ok = json!({"blocks": [{"id": "q", "type": "quiz_single", "question": "Q?", "correct": "a",
            "options": [{"id": "a", "text": "x", "feedback": "Usa `println!` per stampare."}]}]});
        assert_eq!(
            status_of(
                &check(&json!({"no_exclamation_in_feedback": true}), &ok),
                "no_exclamation_in_feedback"
            ),
            Status::Pass
        );
    }

    const CONTEXT: &str = "## Piano\nx\n\n## Estratti delle fonti\n### Documento \"Libro\"\n\n#### Cap (pp. 3-5)\n[p. 3]\nfn main() {\nlet x = 1;\n}\n\n[p. 4]\ntesto\n\n(Pagine omesse perché il testo supera il limite: \"Libro\": pagine 5, 7-8. Non dire cosa contengono.)\n\n## Glossario\nNessuno.";

    fn pdf_check(spec: &Value, doc: &Value) -> Vec<Check> {
        let docs = vec![DocAttempt {
            document: Some(doc.clone()),
            issues: vec![],
            parse_errors: vec![],
            output_tokens: 100,
            ..DocAttempt::default()
        }];
        let c = case();
        let documents = vec![json!({"resource_id": "r-pdf", "title": "Libro"})];
        let excerpt = Excerpt::from_context(CONTEXT);
        let f = Facts {
            case: &c,
            docs: &docs,
            sources: &[],
            lessons: &[],
            topic_glossary: &[],
            plan: &Value::Null,
            documents: &documents,
            excerpt: &excerpt,
        };
        generator_checks(spec.as_object().unwrap(), &f)
    }

    #[test]
    fn the_excerpt_is_read_back_from_the_context() {
        let e = Excerpt::from_context(CONTEXT);
        assert_eq!(e.shown, BTreeSet::from([3, 4]));
        assert_eq!(e.omitted, BTreeSet::from([5, 7, 8]));
        assert!(e.squashed.contains("fnmain(){letx=1;}"));
        assert!(Excerpt::from_context("niente").is_empty());
    }

    #[test]
    fn pdf_sources_need_a_null_url_and_pages_that_were_shown() {
        let good = json!({"sources": [
            {"key": "libro", "resource_id": "r-pdf", "title": "Libro, cap. 1 (pp. 3-4)", "url": null},
            {"key": "web", "resource_id": "r-web", "title": "Web", "url": "https://a.org"}],
            "blocks": [{"id": "h", "type": "heading", "level": 1, "text": "T"},
                {"id": "a", "type": "paragraph", "text": "Oggi seguiamo il capitolo 1 di *Libro*, pagine 3-4[^libro]."}]});
        let spec = json!({"document_sources": true, "pdf_anchor_opening": true, "pages_mentioned_within_excerpt": true});
        assert!(
            pdf_check(&spec, &good)
                .iter()
                .all(|c| c.status == Status::Pass),
            "{:?}",
            pdf_check(&spec, &good)
        );
        let bad = json!({"sources": [
            {"key": "libro", "resource_id": "r-pdf", "title": "Libro (pp. 3-5)", "url": "https://x.org"}],
            "blocks": [{"id": "h", "type": "heading", "level": 1, "text": "T"},
                {"id": "a", "type": "paragraph", "text": "Il capitolo spiega tutto."}]});
        let c = pdf_check(&spec, &bad);
        assert_eq!(status_of(&c, "document_sources"), Status::Fail);
        assert_eq!(status_of(&c, "pdf_anchor_opening"), Status::Fail);
        let invented = json!({"sources": [], "blocks": [{"id": "p", "type": "paragraph", "text": "Vedi p. 9."}]});
        assert_eq!(
            status_of(
                &pdf_check(&spec, &invented),
                "pages_mentioned_within_excerpt"
            ),
            Status::Fail
        );
    }

    #[test]
    fn exercises_about_layout_and_flat_code_are_found() {
        let doc = json!({"blocks": [
            {"id": "q", "type": "quiz_single", "question": "Che cosa dice il capitolo 4?", "correct": "a",
             "options": [{"id": "a", "text": "x", "feedback": "y"}, {"id": "b", "text": "z", "feedback": "w"}]},
            {"id": "ok", "type": "quiz_single", "question": "Dopo `let b = a;` si può usare `a`?", "correct": "a",
             "options": [{"id": "a", "text": "No", "feedback": "y"}, {"id": "b", "text": "Si", "feedback": "w"}]},
            {"id": "c", "type": "code", "language": "rust", "code": "fn main() {\nlet x = 1;\n}"},
            {"id": "d", "type": "code", "language": "rust", "code": "fn main() {\n    let x = 1;\n}"}]});
        let c = pdf_check(
            &json!({"no_page_exercises": true, "code_indentation_rebuilt": ["rust"]}),
            &doc,
        );
        let layout = c.iter().find(|c| c.name == "no_page_exercises").unwrap();
        assert_eq!(layout.status, Status::Fail);
        assert!(
            layout.detail.contains('q') && !layout.detail.contains("ok"),
            "{}",
            layout.detail
        );
        let flat = c
            .iter()
            .find(|c| c.name == "code_indentation_rebuilt")
            .unwrap();
        assert_eq!(flat.status, Status::Fail);
        assert!(flat.detail.contains('c') && !flat.detail.contains("\"d\""));
    }

    #[test]
    fn answers_match_on_word_boundaries() {
        assert!(contains_answer("il borrow checker", "borrow"));
        assert!(!contains_answer("borrowing", "borrow"));
    }

    #[test]
    fn the_closing_needs_a_short_checklist() {
        let doc = json!({"blocks": [
            {"id": "h", "type": "heading", "level": 2, "text": "Chiusura"},
            {"id": "c", "type": "checklist", "items": [{"id": "1", "text": "a"}, {"id": "2", "text": "b"}]}]});
        let spec = json!({"closing_active": {"last_section_has_checklist": true, "checklist_items": [2, 3], "closing_summary_words_max": 45, "summary_not_repeating_answers": "rubrica"}});
        assert_eq!(check(&spec, &doc)[0].status, Status::Pass);
        let none = json!({"blocks": [{"id": "h", "type": "heading", "level": 2, "text": "Chiusura"}, {"id": "p", "type": "paragraph", "text": "x"}]});
        assert_eq!(check(&spec, &none)[0].status, Status::Fail);
    }

    #[test]
    fn counts_prose_exercises_and_block_types() {
        let doc = json!({"blocks": [
            {"id": "p", "type": "paragraph", "text": "uno due tre"},
            quiz("q1", "a a", "b b", "c c", "a"), quiz("q2", "a a", "b b", "c c", "b"),
            {"id": "ask", "type": "ask_agent", "prompt_hint": "x"}]});
        let c = check(
            &json!({"prose_words": [17, 17], "min_exercises": 2, "must_include_block_types": ["ask_agent", "code"], "max_same_type_consecutive": 1}),
            &doc,
        );
        assert_eq!(status_of(&c, "prose_words"), Status::Pass);
        assert_eq!(status_of(&c, "min_exercises"), Status::Pass);
        assert_eq!(status_of(&c, "must_include_block_types"), Status::Fail);
        assert_eq!(status_of(&c, "max_same_type_consecutive"), Status::Fail);
    }

    #[test]
    fn exercises_with_code_counts_the_code_field() {
        let mut with = quiz("q1", "a a", "b b", "c c", "a");
        with["code"] = json!({"language": "rust", "code": "let a = 1;"});
        let doc = json!({"blocks": [with, quiz("q2", "a a", "b b", "c c", "b")]});
        let ok = check(&json!({"exercises_with_code": [1, 4]}), &doc);
        assert_eq!(status_of(&ok, "exercises_with_code"), Status::Pass);
        let none = check(&json!({"exercises_with_code": [2, 4]}), &doc);
        assert_eq!(status_of(&none, "exercises_with_code"), Status::Fail);
        assert!(value_problem("exercises_with_code", &json!(1)).is_some());
    }

    #[test]
    fn the_first_document_is_a_soft_check_and_three_are_the_hard_one() {
        let bad = || DocAttempt {
            document: None,
            issues: vec![],
            parse_errors: vec!["x".into()],
            output_tokens: 10,
            ..DocAttempt::default()
        };
        let c = case();
        let f = Facts {
            case: &c,
            docs: &[bad(), bad(), bad()],
            sources: &[],
            lessons: &[],
            topic_glossary: &[],
            plan: &Value::Null,
            documents: &[],
            excerpt: &Excerpt::default(),
        };
        let checks = generator_checks(
            json!({"validator_pass_first_try": true, "validator_pass_within_3": true})
                .as_object()
                .unwrap(),
            &f,
        );
        assert_eq!(status_of(&checks, "validator_pass_first_try"), Status::Warn);
        assert_eq!(status_of(&checks, "validator_pass_within_3"), Status::Fail);
    }

    #[test]
    fn every_check_of_the_generator_suites_is_computed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let dir = root.join("../../evals");
        let fixture = root.join("../../../docs/contracts/fixtures/lesson-rust-ownership.json");
        let document: Value =
            serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
        let suites = crate::suite::load(
            &dir,
            &[
                "generator/phase6".into(),
                "generator/phase10".into(),
                "generator/phase12".into(),
            ],
        )
        .unwrap();
        for suite in &suites {
            for case in &suite.cases {
                let docs = vec![DocAttempt {
                    document: Some(document.clone()),
                    issues: vec![],
                    parse_errors: vec![],
                    output_tokens: 100,
                    ..DocAttempt::default()
                }];
                let (sources, lessons, glossary, documents) = (
                    case.array("sources"),
                    case.array("lessons"),
                    case.array("topic_glossary"),
                    case.array("documents"),
                );
                let plan = case.get("plan").cloned().unwrap_or(Value::Null);
                let facts = Facts {
                    case,
                    docs: &docs,
                    sources: &sources,
                    lessons: &lessons,
                    topic_glossary: &glossary,
                    plan: &plan,
                    documents: &documents,
                    excerpt: &Excerpt::default(),
                };
                let spec = suite.deterministic(case);
                let checks = generator_checks(&spec, &facts);
                let skipped: Vec<&str> = checks
                    .iter()
                    .filter(|c| c.status == Status::Skipped && c.name != "fill_blank_short_answers")
                    .map(|c| c.name.as_str())
                    .collect();
                assert!(skipped.is_empty(), "{}/{}: {skipped:?}", suite.id, case.id);
            }
        }
    }
}
