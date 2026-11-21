//! Recorder: the real `lesson_completion` job (grading what is pending, the
//! Recorder call, the backend's own rules) over a lesson seeded with the
//! case's exercises and attempts.

use domain::UserId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::anyhow;
use application::LessonPrompts;
use application::completion::parse_draft;
use application::structured::{OutputSchema, check_schema, parse_object};
use domain::llm::LlmRole;
use domain::{
    Attempt, AttemptRepository, JobKind, LessonExercises, Outcome, RecorderContext,
    validate_recorder,
};
use infrastructure::SqliteAttemptRepository;
use serde_json::{Map, Value, json};

use super::{Env, job_failure, subject_calls, wait_for_job};
use crate::checks::{Check, first_match, in_range, patterns, range, regex, run_spec, sentences};
use crate::report::{RoleRun, TurnReport};
use crate::seed::new_app;
use crate::suite::{Case, Suite, criteria};

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    let title = case
        .get("topic")
        .and_then(|t| t.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Argomento");
    let mut app = new_app(
        env.llm.clone(),
        env.tuning,
        title,
        "ready",
        case.content_language(),
    )
    .await?;
    app.mission("confirmed", &json!({"version": 1, "why": case.str_of("mission_why"), "success": ["Criterio di prova (non usato dal Recorder)."], "constraints": [], "out_of_scope": []})).await?;
    let lesson = case.get("lesson").cloned().unwrap_or(Value::Null);
    let number = lesson
        .get("number")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1);
    let lesson_glossary = case.array("lesson_glossary");
    let exercise_blocks = case.array("exercise_blocks");
    let document = json!({"catalog_version": 1, "title": case.get("lesson").and_then(|l| l.get("title")),
        "objective": case.get("lesson").and_then(|l| l.get("objective")), "estimated_minutes": 5,
        "glossary": lesson_glossary, "sources": [], "blocks": exercise_blocks});
    let plan = case.get("plan").cloned().unwrap_or(json!({}));
    let lesson_id = app
        .lesson(
            None,
            number,
            super::str_of(&lesson, "title"),
            super::str_of(&lesson, "objective"),
            &document,
            &plan,
        )
        .await?;
    let active = case.array("active_records");
    app.records(&active, number.saturating_sub(1).max(1))
        .await?;
    app.glossary(&case.array("topic_glossary"), lesson_id)
        .await?;
    let exercises = LessonExercises::from_document(&document);
    app.attempts(lesson_id, &exercises, &case.array("attempts"))
        .await?;

    let runner = Arc::new(app.state.job_runner(vec![(
        JobKind::LessonCompletion,
        app.state.completion_handler(),
    )]));
    let task = tokio::spawn(runner.run());
    let mark = env.ledger.mark();
    let started = app
        .state
        .lessons
        .complete(UserId::OWNER, lesson_id)
        .await
        .map_err(|e| anyhow!("{e:?}"));
    let job = match started {
        Ok(_) => {
            wait_for_job(
                &app,
                JobKind::LessonCompletion,
                Some(lesson_id),
                std::time::Duration::from_secs(20 * 60),
            )
            .await
        }
        Err(e) => Err(e),
    };
    task.abort();
    let job = job?;
    let calls = subject_calls(env, mark, LlmRole::Recorder);
    let attempts = SqliteAttemptRepository::new(app.pool.clone())
        .list(lesson_id)
        .await
        .map_err(|e| anyhow!("{e:?}"))?;

    // The backend's own rules on each answer, like the use case applies them.
    let prompts = LessonPrompts::embedded().recorder;
    let schema = prompts
        .output_schema
        .and_then(|s| OutputSchema::parse(s, "recorder_output").ok());
    let active_numbers: HashSet<u32> = active
        .iter()
        .filter_map(|r| {
            r.get("number")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
        })
        .collect();
    let errors_per_call: Vec<Vec<String>> = calls
        .iter()
        .map(
            |c| match parse_draft(&c.text, schema.as_ref().map(|s| &s.schema)) {
                Err(e) => e,
                Ok(draft) => validate_recorder(
                    &draft,
                    &RecorderContext {
                        attempts: &attempts,
                        exercises: &exercises,
                        active_record_numbers: &active_numbers,
                    },
                ),
            },
        )
        .collect();
    let raw = calls.last().map(|c| c.text.clone());
    let output = raw
        .as_deref()
        .and_then(|t| parse_object(t).ok())
        .unwrap_or_else(|| json!({"records": [], "terms": [], "cards": []}));
    let called = !calls.is_empty();

    let mut checks = Vec::new();
    if job.status != domain::JobStatus::Succeeded {
        checks.push(Check::of("job_succeeded", false, || job_failure(&job)));
    }
    let spec = suite.deterministic(case);
    let facts = Facts {
        output: &output,
        attempts: &attempts,
        schema: schema.as_ref().map(|s| &s.schema),
        errors_per_call: &errors_per_call,
        lesson_glossary: &lesson_glossary,
        active_numbers: &active_numbers,
        blocks: exercise_blocks
            .iter()
            .filter_map(|b| b.get("id").and_then(Value::as_str))
            .collect(),
    };
    checks.extend(recorder_checks(&spec, &facts));
    checks.extend(expect_checks(
        case.get("expect").unwrap_or(&Value::Null),
        &output,
    ));

    let material = format!(
        "Topic: {title}\nLesson: {lesson}\nExercises of the lesson: {}\nAttempts of the learner: {}\nActive records before this lesson: {}\nTopic glossary: {}\n\nThe Recorder was {}. Its output:\n{}",
        Value::Array(exercise_blocks.clone()),
        Value::Array(case.array("attempts")),
        Value::Array(active),
        Value::Array(case.array("topic_glossary")),
        if called {
            "called"
        } else {
            "not called (no exercise was passed)"
        },
        output
    );
    let judge = env
        .judge
        .judge(&material, &criteria(case.get("rubric")))
        .await;
    let mut run = RoleRun {
        turns: vec![TurnReport {
            label: "completion".into(),
            input: String::new(),
            output: output.to_string(),
            checks,
            judge,
            extras: json!({"recorder_called": called, "documents": calls.len(), "repairs": calls.len().saturating_sub(1), "backend_validator_errors_per_call": errors_per_call}),
        }],
        ..RoleRun::default()
    };
    run.metrics
        .insert("recorder_calls".into(), calls.len() as f64);
    Ok(run)
}

pub struct Facts<'a> {
    pub output: &'a Value,
    pub attempts: &'a [Attempt],
    pub schema: Option<&'a Value>,
    pub errors_per_call: &'a [Vec<String>],
    pub lesson_glossary: &'a [Value],
    pub active_numbers: &'a HashSet<u32>,
    /// Ids of the exercises of the lesson.
    pub blocks: HashSet<&'a str>,
}

fn list(v: &Value, key: &str) -> Vec<Value> {
    v.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn blocks_of(v: &Value) -> Vec<String> {
    list(v, "evidence_blocks")
        .iter()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

/// (any attempt correct, first attempt failed or misconception seen) per block.
fn evidence(attempts: &[Attempt]) -> HashMap<&str, (bool, bool)> {
    let mut map: HashMap<&str, (bool, bool)> = HashMap::new();
    for a in attempts {
        let e = map.entry(a.block_id.as_str()).or_default();
        e.0 |= a.outcome == Outcome::Correct;
        e.1 |= (a.attempt_number == 1
            && matches!(a.outcome, Outcome::Incorrect | Outcome::Partial))
            || a.grade.as_ref().is_some_and(|g| g.misconception.is_some());
    }
    map
}

#[allow(clippy::too_many_lines)] // one arm per key of the suite
pub fn recorder_checks(spec: &Map<String, Value>, f: &Facts<'_>) -> Vec<Check> {
    let records = list(f.output, "records");
    let terms = list(f.output, "terms");
    let cards = list(f.output, "cards");
    let ev = evidence(f.attempts);
    let demonstrated: Vec<&Value> = records
        .iter()
        .filter(|r| s(r, "kind") == "demonstrated")
        .collect();
    let visible =
        |v: &Value, keys: &[&str]| keys.iter().map(|k| s(v, k).to_owned()).collect::<Vec<_>>();
    let mut all_text: Vec<String> = Vec::new();
    for r in &records {
        all_text.extend(visible(r, &["title", "body", "evidence"]));
    }
    for t in &terms {
        all_text.extend(visible(t, &["term", "definition"]));
    }
    for c in &cards {
        all_text.extend(visible(c, &["front", "back"]));
    }
    let joined = all_text.join("\n");
    let passed = |b: &str| ev.get(b).is_some_and(|e| e.0);
    let per = |name: &str, rows: &[Value], f: &dyn Fn(&Value) -> Option<String>| -> Check {
        let bad: Vec<String> = rows.iter().filter_map(f).collect();
        Check::of(name, bad.is_empty(), || bad.join("; "))
    };
    let lengths = |name: &str, v: &Value| -> Check {
        let mut bad = Vec::new();
        fn chk(bad: &mut Vec<String>, v: &Value, what: &str, text: &str, key: &str) {
            if let Some(r) = v.get(key).and_then(range) {
                let n = text.trim().chars().count();
                if !in_range(n, r) {
                    bad.push(format!("{what} has {n} characters, expected {r:?}"));
                }
            }
        }
        for r in &records {
            chk(&mut bad, v, "record title", s(r, "title"), "title");
            chk(&mut bad, v, "record body", s(r, "body"), "body");
            chk(&mut bad, v, "record evidence", s(r, "evidence"), "evidence");
        }
        for t in &terms {
            chk(&mut bad, v, "term", s(t, "term"), "term");
            chk(&mut bad, v, "definition", s(t, "definition"), "definition");
            let avoid = list(t, "avoid");
            if let Some(r) = v.get("avoid_count").and_then(range)
                && !in_range(avoid.len(), r)
            {
                bad.push(format!(
                    "term `{}` has {} avoid entries",
                    s(t, "key"),
                    avoid.len()
                ));
            }
            for a in avoid.iter().filter_map(Value::as_str) {
                chk(&mut bad, v, "avoid entry", a, "avoid_each");
            }
        }
        for c in &cards {
            chk(&mut bad, v, "card front", s(c, "front"), "front");
            chk(&mut bad, v, "card back", s(c, "back"), "back");
        }
        Check::of(name, bad.is_empty(), || bad.join("; "))
    };

    run_spec(spec, &joined, |key, v| {
        Some(match key {
            "schema_valid" => {
                let p = f.schema.map_or_default(|sc| check_schema(f.output, sc));
                Check::of(key, p.is_empty(), || p.join("; "))
            }
            "backend_validator_pass_first_try" => match f.errors_per_call.first() {
                None => Check::pass(key),
                Some(e) => Check::soft(key, e.is_empty(), || e.join(" | ")),
            },
            "backend_validator_pass_within_2" => Check::of(
                key,
                f.errors_per_call.is_empty() || f.errors_per_call.iter().take(2).any(Vec::is_empty),
                || "no answer accepted by the backend validator in 2 attempts".into(),
            ),
            "max_records" | "max_terms" | "max_cards" => {
                let (n, what) = match key {
                    "max_records" => (records.len(), "records"),
                    "max_terms" => (terms.len(), "terms"),
                    _ => (cards.len(), "cards"),
                };
                let max = v.as_u64()?;
                Check::of(key, n as u64 <= max, || format!("{n} {what}, max {max}"))
            }
            "lengths" => lengths(key, v),
            "evidence_blocks_must_exist_with_attempts" => {
                let bad: Vec<String> = records
                    .iter()
                    .chain(&terms)
                    .flat_map(blocks_of)
                    .filter(|b| !ev.contains_key(b.as_str()))
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("evidence without attempts: {bad:?}")
                })
            }
            "demonstrated_needs_passed_attempt" => per(key, &records, &|r| {
                (s(r, "kind") == "demonstrated" && !blocks_of(r).iter().any(|b| passed(b)))
                    .then(|| format!("`{}` cites no passed exercise", s(r, "title")))
            }),
            "records_demonstrated_do_not_cite_failed_blocks_only" => per(key, &records, &|r| {
                let bad: Vec<String> = blocks_of(r)
                    .into_iter()
                    .filter(|b| s(r, "kind") == "demonstrated" && !passed(b))
                    .collect();
                (!bad.is_empty())
                    .then(|| format!("`{}` cites never-passed blocks {bad:?}", s(r, "title")))
            }),
            "misconception_needs_first_fail_and_a_pass" => per(key, &records, &|r| {
                (s(r, "kind") == "misconception_corrected"
                    && !blocks_of(r)
                        .iter()
                        .any(|b| passed(b) && ev.get(b.as_str()).is_some_and(|e| e.1)))
                .then(|| {
                    format!(
                        "`{}` has no block with a first failure and a later pass",
                        s(r, "title")
                    )
                })
            }),
            "terms_need_passed_attempt" => per(key, &terms, &|t| {
                (!blocks_of(t).iter().any(|b| passed(b)))
                    .then(|| format!("term `{}` cites no passed exercise", s(t, "key")))
            }),
            "supersedes_must_be_active_records" => per(key, &records, &|r| {
                let bad: Vec<u64> = list(r, "supersedes")
                    .iter()
                    .filter_map(Value::as_u64)
                    .filter(|n| !f.active_numbers.contains(&u32::try_from(*n).unwrap_or(0)))
                    .collect();
                (!bad.is_empty()).then(|| format!("supersedes {bad:?}, not active"))
            }),
            "each_card_links_exactly_one" => per(key, &cards, &|c| {
                let idx = c.get("record_index").is_some_and(|x| !x.is_null());
                let key_ = c.get("term_key").is_some_and(|x| !x.is_null());
                (idx == key_).then(|| {
                    format!(
                        "card `{}` links {} targets",
                        s(c, "front"),
                        if idx { "two" } else { "no" }
                    )
                })
            }),
            "term_keys_format" => {
                let re = regex(v.as_str()?).ok()?;
                per(key, &terms, &|t| {
                    (!re.is_match(s(t, "key"))).then(|| format!("key `{}`", s(t, "key")))
                })
            }
            "term_key_equals_lesson_glossary_key_when_same_term" => per(key, &terms, &|t| {
                let term = s(t, "term").to_lowercase();
                f.lesson_glossary
                    .iter()
                    .find(|g| s(g, "term").to_lowercase() == term && s(g, "key") != s(t, "key"))
                    .map(|g| {
                        format!(
                            "term `{}` has key `{}`, the lesson glossary says `{}`",
                            s(t, "term"),
                            s(t, "key"),
                            s(g, "key")
                        )
                    })
            }),
            "no_markdown_beyond_inline" => {
                let block = regex(r"(?m)^\s*(#|>|[-*+]\s|\d+[.)]\s)|```|\n|\]\(").ok()?;
                let bad: Vec<&String> = all_text.iter().filter(|t| block.is_match(t)).collect();
                Check::of(key, bad.is_empty(), || {
                    format!("block markdown in {} text(s)", bad.len())
                })
            }
            "no_double_brackets_in_definitions" => per(key, &terms, &|t| {
                s(t, "definition")
                    .contains("[[")
                    .then(|| format!("`{}`", s(t, "key")))
            }),
            "body_must_not_match" => {
                let pats = patterns(v);
                per(key, &records, &|r| {
                    first_match(&pats, s(r, "body")).ok().flatten()
                })
            }
            "evidence_must_not_match" => {
                let pats = patterns(v);
                per(key, &records, &|r| {
                    first_match(&pats, s(r, "evidence")).ok().flatten()
                })
            }
            "title_must_not_match" => {
                let pats = patterns(v);
                per(key, &records, &|r| {
                    first_match(&pats, s(r, "title")).ok().flatten()
                })
            }
            "card_text_must_not_match" => {
                let pats = patterns(v);
                per(key, &cards, &|c| {
                    first_match(&pats, &format!("{} {}", s(c, "front"), s(c, "back")))
                        .ok()
                        .flatten()
                })
            }
            "demonstrated_text_must_not_match" => {
                let pats = patterns(v);
                let bad: Vec<String> = demonstrated
                    .iter()
                    .filter_map(|r| {
                        first_match(
                            &pats,
                            &format!("{} {} {}", s(r, "title"), s(r, "body"), s(r, "evidence")),
                        )
                        .ok()
                        .flatten()
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || format!("matched {bad:?}"))
            }
            "demonstrated_evidence_blocks_subset_of" => {
                let allowed: HashSet<&str> =
                    v.as_array()?.iter().filter_map(Value::as_str).collect();
                let bad: Vec<String> = demonstrated
                    .iter()
                    .flat_map(|r| blocks_of(r))
                    .filter(|b| !allowed.contains(b.as_str()))
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("demonstrated records cite {bad:?}")
                })
            }
            "misconception_record_cites" => {
                let want: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).collect();
                let ok = records
                    .iter()
                    .filter(|r| s(r, "kind") == "misconception_corrected")
                    .any(|r| {
                        let b = blocks_of(r);
                        want.iter().all(|w| b.iter().any(|x| x == w))
                    });
                Check::of(key, ok, || {
                    format!("no misconception record cites {want:?}")
                })
            }
            "terms_must_not_include_keys" => {
                let banned: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).collect();
                let bad: Vec<&str> = terms
                    .iter()
                    .map(|t| s(t, "key"))
                    .filter(|k| banned.contains(k))
                    .collect();
                Check::of(key, bad.is_empty(), || format!("duplicated terms: {bad:?}"))
            }
            "body_and_evidence_second_person" => {
                let third = regex(r"(?i)\b(ha|sa|pensava|credeva|può|riesce|confonde)\b").ok()?;
                let second = regex(r"(?i)\b(hai|sai|pensavi|credevi|sei|ti|tuo|tua|tuoi|prevedi|vedi|puoi|riesci)\b").ok()?;
                let bad = records
                    .iter()
                    .filter(|r| {
                        let t = format!("{} {}", s(r, "body"), s(r, "evidence"));
                        third.is_match(&t) && !second.is_match(&t)
                    })
                    .count();
                Check::soft(key, bad == 0, || {
                    format!("{bad} record(s) in the third person (heuristic)")
                })
            }
            "no_ids_in_visible_text" => {
                let ids: HashSet<String> =
                    records.iter().chain(&terms).flat_map(blocks_of).collect();
                let bad: Vec<&String> = ids
                    .iter()
                    .filter(|id| {
                        regex(&format!(r"\b{}\b", regex::escape(id)))
                            .is_ok_and(|re| all_text.iter().any(|t| re.is_match(t)))
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("exercise ids in visible text: {bad:?}")
                })
            }
            "unpassed_block_only_on_never_passed" => per(key, &cards, &|c| {
                let id = c.get("unpassed_block").and_then(Value::as_str)?;
                match ev.get(id) {
                    None => Some(format!("`{id}` has no attempts")),
                    Some((true, _)) => Some(format!("`{id}` was passed, not to be consolidated")),
                    Some((false, _)) => None,
                }
            }),
            "unpassed_block_includes_all_of" => {
                let want: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).collect();
                let missing: Vec<&&str> = want
                    .iter()
                    .filter(|w| {
                        !cards
                            .iter()
                            .any(|c| c.get("unpassed_block").and_then(Value::as_str) == Some(**w))
                    })
                    .collect();
                Check::of(key, missing.is_empty(), || {
                    format!("no card has `unpassed_block` for {missing:?}")
                })
            }
            "unpassed_block_includes_one_of" => {
                let want: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).collect();
                let ok = cards.iter().any(|c| {
                    c.get("unpassed_block")
                        .and_then(Value::as_str)
                        .is_some_and(|b| want.contains(&b))
                });
                Check::of(key, ok, || {
                    format!("no card has `unpassed_block` in {want:?}")
                })
            }
            "unpassed_block_not_in_visible_text" => {
                let ids: HashSet<&str> = cards
                    .iter()
                    .filter_map(|c| c.get("unpassed_block").and_then(Value::as_str))
                    .collect();
                let bad: Vec<&&str> = ids
                    .iter()
                    .filter(|id| {
                        regex(&format!(r"\b{}\b", regex::escape(id)))
                            .is_ok_and(|re| all_text.iter().any(|t| re.is_match(t)))
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("unpassed_block ids in visible text: {bad:?}")
                })
            }
            "card_front_single_question" => per(key, &cards, &|c| {
                joins_two_questions(s(c, "front")).then(|| format!("`{}`", s(c, "front")))
            })
            .with_severity(v),
            "card_front_chars_max" | "card_back_chars_max" => {
                let front = key == "card_front_chars_max";
                let (field, hard_key) = if front {
                    ("front", "card_front_chars_hard_max")
                } else {
                    ("back", "card_back_chars_hard_max")
                };
                let soft = v.as_u64()?;
                let hard = spec
                    .get(hard_key)
                    .and_then(Value::as_u64)
                    .unwrap_or(u64::MAX);
                let longest = cards
                    .iter()
                    .map(|c| s(c, field).chars().count())
                    .max()
                    .unwrap_or(0) as u64;
                if longest > hard {
                    Check::of(key, false, || {
                        format!("a card {field} has {longest} characters, hard limit {hard}")
                    })
                } else {
                    Check::soft(key, longest <= soft, || {
                        format!("a card {field} has {longest} characters, limit {soft}")
                    })
                }
            }
            "card_front_chars_hard_max" | "card_back_chars_hard_max" => Check::pass(key),
            "card_back_max_sentences" => {
                let max = v.as_u64()?;
                let worst = cards
                    .iter()
                    .map(|c| sentences(s(c, "back")))
                    .max()
                    .unwrap_or(0) as u64;
                Check::soft(key, worst <= max, || {
                    format!("a card back has {worst} sentences, max {max}")
                })
            }
            _ => return None,
        })
    })
}

/// A front that joins two questions: "e" followed by an interrogative inside
/// one segment, or a second `?` whose segment is not just "Perché?" /
/// "Perché no?" / "Perché non?" ("Compila? Perché?" is one prediction with its
/// reason).
fn joins_two_questions(front: &str) -> bool {
    // A `?` inside inline code (the Rust operator) is not a question.
    let front = &crate::checks::strip_inline_code(front);
    let joined = regex::Regex::new(r"(?i)\b(e|and)\s+(chi|che cosa|cosa|quale|come|dove|perché)\b")
        .expect("static regex");
    let segments: Vec<&str> = front
        .split_inclusive('?')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .collect();
    if segments.iter().any(|seg| joined.is_match(seg)) {
        return true;
    }
    let why = ["perché?", "perché no?", "perché non?"];
    segments
        .iter()
        .skip(1)
        .any(|seg| front.matches('?').count() > 1 && !why.contains(&seg.to_lowercase().as_str()))
}

/// `expect` of a recorder case: counts, kinds, keys, supersessions. Checks that
/// need judgment (`cards_min_application`) are left to the rubric.
pub fn expect_checks(expect: &Value, output: &Value) -> Vec<Check> {
    let records = list(output, "records");
    let terms = list(output, "terms");
    let cards = list(output, "cards");
    let mut out = Vec::new();
    for (key, rows) in [("records", &records), ("terms", &terms), ("cards", &cards)] {
        if let Some(r) = expect.get(key).and_then(range) {
            out.push(Check::of(
                &format!("expect.{key}"),
                in_range(rows.len(), r),
                || format!("{} {key}, expected {r:?}", rows.len()),
            ));
        }
    }
    let kinds: Vec<&str> = records.iter().map(|r| s(r, "kind")).collect();
    for (key, include) in [
        ("record_kinds_include", true),
        ("record_kinds_exclude", false),
    ] {
        if let Some(want) = expect.get(key).and_then(Value::as_array) {
            let bad: Vec<&str> = want
                .iter()
                .filter_map(Value::as_str)
                .filter(|k| kinds.contains(k) != include)
                .collect();
            out.push(Check::of(&format!("expect.{key}"), bad.is_empty(), || {
                format!("{bad:?}")
            }));
        }
    }
    if let Some(allowed) = expect.get("term_keys_subset_of").and_then(Value::as_array) {
        let allowed: Vec<&str> = allowed.iter().filter_map(Value::as_str).collect();
        let bad: Vec<&str> = terms
            .iter()
            .map(|t| s(t, "key"))
            .filter(|k| !allowed.contains(k))
            .collect();
        out.push(Check::of(
            "expect.term_keys_subset_of",
            bad.is_empty(),
            || format!("unexpected keys {bad:?}"),
        ));
    }
    if let Some(want) = expect.get("supersedes_any").and_then(Value::as_array) {
        let got: Vec<u64> = records
            .iter()
            .flat_map(|r| list(r, "supersedes"))
            .filter_map(|n| n.as_u64())
            .collect();
        let ok = want
            .iter()
            .filter_map(Value::as_u64)
            .any(|n| got.contains(&n));
        out.push(Check::of("expect.supersedes_any", ok, || {
            format!("supersedes {got:?}, expected one of {want:?}")
        }));
    }
    if expect.get("cards_min_application").is_some() {
        out.push(Check::skipped(
            "expect.cards_min_application",
            "judged by the rubric",
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Status, status_of};
    use domain::{Answer, AttemptId, ExerciseKind, LessonId};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn attempt(block: &str, n: u8, outcome: Outcome) -> Attempt {
        Attempt {
            id: AttemptId::new(Uuid::new_v4_stub(block, n)),
            lesson_id: LessonId::new(Uuid::nil()),
            block_id: block.into(),
            kind: ExerciseKind::QuizSingle,
            attempt_number: n,
            answer: Answer::Option("a".into()),
            outcome,
            grade: None,
            answered_at: OffsetDateTime::UNIX_EPOCH,
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    trait Stub {
        fn new_v4_stub(block: &str, n: u8) -> Uuid;
    }
    impl Stub for Uuid {
        fn new_v4_stub(block: &str, n: u8) -> Uuid {
            Uuid::from_u128(u128::from(n) + block.bytes().map(u128::from).sum::<u128>() * 1000)
        }
    }

    fn run(spec: &Value, output: &Value, attempts: &[Attempt]) -> Vec<Check> {
        let active = HashSet::from([2u32]);
        let f = Facts {
            output,
            attempts,
            schema: None,
            errors_per_call: &[],
            lesson_glossary: &[json!({"key": "move", "term": "move"})],
            active_numbers: &active,
            blocks: HashSet::new(),
        };
        recorder_checks(spec.as_object().unwrap(), &f)
    }

    fn record(kind: &str, blocks: &[&str]) -> Value {
        json!({"kind": kind, "title": "Idea", "body": "Sai che x.", "evidence": "Hai fatto y.", "evidence_blocks": blocks, "supersedes": []})
    }

    #[test]
    fn evidence_rules() {
        let attempts = [
            attempt("q1", 1, Outcome::Incorrect),
            attempt("q1", 2, Outcome::Correct),
            attempt("q2", 1, Outcome::Incorrect),
        ];
        let ok = json!({"records": [record("misconception_corrected", &["q1"])], "terms": [], "cards": []});
        let c = run(
            &json!({"demonstrated_needs_passed_attempt": true, "misconception_needs_first_fail_and_a_pass": true, "evidence_blocks_must_exist_with_attempts": true}),
            &ok,
            &attempts,
        );
        assert!(c.iter().all(|c| c.status == Status::Pass), "{c:?}");
        let bad = json!({"records": [record("demonstrated", &["q2"])], "terms": [], "cards": []});
        let c = run(
            &json!({"demonstrated_needs_passed_attempt": true, "records_demonstrated_do_not_cite_failed_blocks_only": true}),
            &bad,
            &attempts,
        );
        assert!(c.iter().all(|c| c.status == Status::Fail));
    }

    #[test]
    fn card_question_allows_prediction_with_reason() {
        let ok = json!({"records": [], "terms": [], "cards": [
            {"front": "Dopo `let b = a;` la riga `f(a)?` compila? Perché?", "back": "No.", "record_index": 0}]});
        let c = run(
            &json!({"card_front_single_question": {"severity": "error"}}),
            &ok,
            &[],
        );
        assert_eq!(status_of(&c, "card_front_single_question"), Status::Pass);
        let bad = json!({"records": [], "terms": [], "cards": [
            {"front": "Dove avviene il move e come lo correggi?", "back": "Qui.", "record_index": 0}]});
        let c = run(
            &json!({"card_front_single_question": {"severity": "error"}}),
            &bad,
            &[],
        );
        assert_eq!(status_of(&c, "card_front_single_question"), Status::Fail);
    }

    #[test]
    fn card_rules() {
        let out = json!({"records": [], "terms": [], "cards": [
            {"front": "Cos'è un move? E cosa succede dopo?", "back": "Uno. Due. Tre. Quattro.", "record_index": 0, "term_key": "x"}]});
        let c = run(
            &json!({"card_front_single_question": {"severity": "error"}, "each_card_links_exactly_one": true,
            "card_front_chars_max": 10, "card_front_chars_hard_max": 300, "card_back_max_sentences": 3}),
            &out,
            &[],
        );
        assert_eq!(status_of(&c, "card_front_single_question"), Status::Fail);
        assert_eq!(status_of(&c, "each_card_links_exactly_one"), Status::Fail);
        assert_eq!(status_of(&c, "card_front_chars_max"), Status::Warn);
        assert_eq!(status_of(&c, "card_back_max_sentences"), Status::Warn);
    }

    #[test]
    fn a_prediction_with_its_reason_is_one_question() {
        assert!(!joins_two_questions("Cosa compila? Perché?"));
        assert!(!joins_two_questions("Compila questo codice? Perché no?"));
        assert!(joins_two_questions("Cos'è un move? E cosa succede dopo?"));
        assert!(joins_two_questions("Cos'è un borrow e come funziona?"));
        assert!(joins_two_questions("Compila? Perché? E poi?"));
        assert!(!joins_two_questions("Cos'è un move?"));
        assert!(!joins_two_questions("Completa: il valore passa a {{x}}"));
    }

    #[test]
    fn expectations_and_supersession() {
        let mut r = record("demonstrated", &["q1"]);
        r["supersedes"] = json!([2]);
        let out = json!({"records": [r], "terms": [{"key": "move"}], "cards": []});
        let ok = expect_checks(
            &json!({"records": [1, 1], "terms": [0, 1], "cards": [0, 0], "supersedes_any": [2], "term_keys_subset_of": ["move"], "cards_min_application": 1}),
            &out,
        );
        assert_eq!(ok.iter().filter(|c| c.status == Status::Fail).count(), 0);
        assert_eq!(ok.last().unwrap().status, Status::Skipped);
        let bad = expect_checks(&json!({"records": [2, 3]}), &out);
        assert_eq!(bad[0].status, Status::Fail);
    }

    #[test]
    fn demonstrated_text_and_ids() {
        let out = json!({"records": [{"kind": "demonstrated", "title": "clone", "body": "Dal q1 sai.", "evidence": "x", "evidence_blocks": ["q1"], "supersedes": []}], "terms": [], "cards": []});
        let attempts = [attempt("q1", 1, Outcome::Correct)];
        let c = run(
            &json!({"demonstrated_text_must_not_match": ["(?i)clone"], "no_ids_in_visible_text": true, "demonstrated_evidence_blocks_subset_of": ["q3"]}),
            &out,
            &attempts,
        );
        assert!(c.iter().all(|c| c.status == Status::Fail), "{c:?}");
    }
}
