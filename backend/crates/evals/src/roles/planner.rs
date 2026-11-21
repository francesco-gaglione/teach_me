//! Planner: the real `LessonGeneration` job up to the plan. The Generator is
//! refused, so a run costs one Planner call (two with a repair) and the
//! judge. The plan is checked here against the case's world with the
//! backend's own `validate_plan`, and the choices of sections on top.

use domain::UserId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::anyhow;
use application::structured::parse_object;
use application::{PlanDocument, PlanFacts, validate_plan};
use domain::JobKind;
use domain::llm::LlmRole;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::{Env, subject_calls, wait_for_job};
use crate::checks::{Check, in_range, lesson_minutes, patterns, range, regex, run_spec};
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
    let mission = case.get("mission").cloned().unwrap_or(Value::Null);
    app.mission("confirmed", &mission).await?;
    app.preferences(&case.array("preferences")).await?;
    let sources = case.array("sources");
    app.resources(&sources).await?;
    let documents = case.array("documents");
    app.documents(&documents).await?;

    let lessons = case.array("lessons");
    let mut anchor = None;
    for l in &lessons {
        let t = l.get("title").and_then(Value::as_str).unwrap_or_default();
        let number = l
            .get("number")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(1);
        let objective = l
            .get("objective")
            .and_then(Value::as_str)
            .unwrap_or("Lezione precedente.");
        let doc = json!({"catalog_version": 1, "title": t, "objective": objective, "estimated_minutes": 5, "blocks": []});
        let id = app
            .lesson(
                l.get("id").and_then(Value::as_str),
                number,
                t,
                objective,
                &doc,
                &json!({}),
            )
            .await?;
        anchor.get_or_insert((id, number));
    }
    // Glossary terms and records hang from a lesson. A case without lessons
    // and without them gets no placeholder: the Planner would see it as a
    // lesson already given.
    let (glossary, records) = (case.array("topic_glossary"), case.array("records"));
    if !glossary.is_empty() || !records.is_empty() {
        let (anchor, anchor_number) = match anchor {
            Some(a) => a,
            None => (app.stub_lesson(1).await?, 1),
        };
        app.glossary(&glossary, anchor).await?;
        app.records(&records, anchor_number).await?;
    }

    env.planned.set_refuse_generator(true);
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
                std::time::Duration::from_secs(10 * 60),
            )
            .await
        }
        Err(e) => Err(e),
    };
    task.abort();
    env.planned.set_refuse_generator(false);
    let job = job?;
    let calls = subject_calls(env, mark, LlmRole::Planner);
    // The Generator is refused, so the job always ends failed; but one that
    // never reached the Planner failed on the case's world (a fixture the
    // domain cannot read, ...). That is an error of the case, said as such,
    // not a Planner that "gave no plan".
    if calls.is_empty() {
        anyhow::bail!(
            "the job ended before calling the Planner, so the case's world is broken: {}",
            super::job_failure(&job)
        );
    }

    let world = World::new(&sources, &documents, &lessons, case);
    let mut spec = suite.deterministic(case);
    let first = calls.first().and_then(|c| parse_object(&c.text).ok());
    // The plan the backend keeps has its `estimated_minutes` computed by the
    // backend, not the model's: the minutes checks must read that one.
    let last = calls
        .last()
        .and_then(|c| parse_object(&c.text).ok())
        .map(|plan| world.finalized(&plan).unwrap_or(plan));
    // Every attempt, not only the last: what was wrong with each, and the head
    // of the answer when it is not a valid plan. `usage` has no thinking count.
    let attempts: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(i, call)| {
            let errors = world.text_errors(&call.text);
            json!({
                "attempt": i + 1,
                "output_tokens": call.usage.output_tokens,
                "stop_reason": call.stop_reason,
                "valid": errors.is_empty(),
                "errors": errors,
                "response_head": (!errors.is_empty()).then(|| super::response_head(&call.text)),
            })
        })
        .collect();

    let mut checks = Vec::new();
    // Computed below from the plan itself, not by `plan_checks`: they are
    // always reported (`plan_valid_first_try` as a warning, as the suite says).
    spec.remove("plan_valid");
    spec.remove("plan_valid_first_try");
    if let Some(max) = spec.remove("planner_calls_max").and_then(|v| v.as_u64()) {
        checks.push(Check::of(
            "planner_calls_max",
            calls.len() as u64 <= max,
            || {
                format!(
                    "{} Planner calls (a repair is a second one), max {max}",
                    calls.len()
                )
            },
        ));
    }
    match (&first, &last) {
        (Some(first), Some(last)) => {
            let errors = world.reference_errors(first);
            checks.push(Check::soft(
                "plan_valid_first_try",
                errors.is_empty(),
                || format!("{errors:?}"),
            ));
            let errors = world.reference_errors(last);
            checks.push(Check::of("plan_valid", errors.is_empty(), || {
                format!("{errors:?}")
            }));
            checks.extend(plan_checks(&spec, last, &world));
        }
        _ => checks.push(Check::of("plan_produced", false, || {
            "the Planner gave no readable plan".into()
        })),
    }
    let plan_text = last.as_ref().map_or_default(Value::to_string);
    let material = format!(
        "Topic: {title}\nMission: {mission}\nDocuments (index given to the Planner; `resource_id` is the id the plan must use as a source for that document): {}\nWeb sources: {}\nPrevious lessons: {}\nActive learning records: {}\nPlan written by the Planner (JSON):\n{plan_text}",
        Value::Array(documents.iter().map(index_of).collect()),
        Value::Array(sources.clone()),
        Value::Array(lessons.clone()),
        Value::Array(case.array("records")),
    );
    let judge = env
        .judge
        .judge(&material, &criteria(case.get("rubric")))
        .await;
    let mut run = RoleRun {
        turns: vec![TurnReport {
            label: "plan".into(),
            input: String::new(),
            output: plan_text,
            checks,
            judge,
            extras: json!({"planner_calls": calls.len(), "repairs": calls.len().saturating_sub(1), "attempts": attempts}),
        }],
        ..RoleRun::default()
    };
    run.metrics
        .insert("planner_calls".into(), calls.len() as f64);
    if let Some(plan) = &last {
        // Share of `general` prerequisites over all of them, in percent (the
        // report prints metrics without decimals): beginner profiles should
        // produce them, profiles with records should not.
        run.metrics
            .insert("general_share_pct".into(), general_share(plan) * 100.0);
        run.metrics
            .insert("concepts".into(), list(plan, "concepts").len() as f64);
    }
    Ok(run)
}

/// A document as the Planner sees it: no text, only title and sections.
fn index_of(d: &Value) -> Value {
    json!({"document_id": d.get("document_id"), "resource_id": d.get("resource_id"), "title": d.get("title"), "sections": d.get("sections")})
}

/// What exists in the case, to check the plan's references against.
pub struct World {
    /// What `validate_plan` checks the references against.
    resource_ids: HashSet<Uuid>,
    lesson_ids: HashSet<Uuid>,
    record_numbers: HashSet<u32>,
    glossary_keys: HashSet<String>,
    plan_documents: Vec<PlanDocument>,
    /// Resource id of each document, by document id.
    documents: HashMap<String, DocInfo>,
}

struct DocInfo {
    resource_id: String,
    /// `(id, level, first page, last page)` in index order.
    sections: Vec<(String, u64, u64, u64)>,
}

impl World {
    pub fn new(sources: &[Value], documents: &[Value], lessons: &[Value], case: &Case) -> Self {
        let s = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let uuid = |v: &Value, k: &str| domain::parse_uuid(&s(v, k));
        let mut resource_ids: HashSet<Uuid> = sources
            .iter()
            .filter(|r| s(r, "kind") != "community")
            .filter_map(|r| uuid(r, "resource_id"))
            .collect();
        let mut docs = HashMap::new();
        let mut plan_documents = Vec::new();
        for d in documents {
            let sections: Vec<(String, u64, u64, u64)> = d
                .get("sections")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|x| {
                    let n = |k: &str| x.get(k).and_then(Value::as_u64).unwrap_or(0);
                    (s(x, "id"), n("level"), n("page_start"), n("page_end"))
                })
                .collect();
            if let (Some(document_id), Some(resource_id)) =
                (uuid(d, "document_id"), uuid(d, "resource_id"))
            {
                resource_ids.insert(resource_id);
                plan_documents.push(PlanDocument {
                    document_id,
                    resource_id,
                    section_ids: sections.iter().map(|x| x.0.clone()).collect(),
                });
            }
            docs.insert(
                s(d, "document_id"),
                DocInfo {
                    resource_id: s(d, "resource_id"),
                    sections,
                },
            );
        }
        Self {
            resource_ids,
            lesson_ids: lessons.iter().filter_map(|l| uuid(l, "id")).collect(),
            record_numbers: case
                .array("records")
                .iter()
                .filter_map(|r| r.get("number").and_then(Value::as_u64))
                .filter_map(|n| u32::try_from(n).ok())
                .collect(),
            glossary_keys: case
                .array("topic_glossary")
                .iter()
                .map(|t| s(t, "key"))
                .collect(),
            plan_documents,
            documents: docs,
        }
    }

    /// The plan's problems, from the backend's own `validate_plan` (no schema:
    /// the real run already sent it to the model): the ones that would send it
    /// to repair.
    pub fn reference_errors(&self, plan: &Value) -> Vec<String> {
        self.text_errors(&plan.to_string())
    }

    /// The same, for the model's answer as it came.
    pub fn text_errors(&self, text: &str) -> Vec<String> {
        let facts = PlanFacts {
            resource_ids: &self.resource_ids,
            lesson_ids: &self.lesson_ids,
            active_record_numbers: &self.record_numbers,
            glossary_keys: &self.glossary_keys,
            documents: &self.plan_documents,
        };
        validate_plan(text, None, &facts).err().unwrap_or_default()
    }

    /// The plan as the backend would keep it: validated, with its minutes
    /// computed by the backend. `None` if it does not validate.
    pub fn finalized(&self, plan: &Value) -> Option<Value> {
        let facts = PlanFacts {
            resource_ids: &self.resource_ids,
            lesson_ids: &self.lesson_ids,
            active_record_numbers: &self.record_numbers,
            glossary_keys: &self.glossary_keys,
            documents: &self.plan_documents,
        };
        validate_plan(&plan.to_string(), None, &facts)
            .ok()
            .map(|valid| valid.raw)
    }

    /// The chosen sections as `(id, level, first page, last page)`, in plan order.
    fn chosen(&self, plan: &Value) -> Vec<(String, u64, u64, u64)> {
        plan.get("document_sections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|e| {
                let doc = e
                    .get("document_id")
                    .and_then(Value::as_str)
                    .and_then(|id| self.documents.get(id));
                e.get("section_ids")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(move |sid| doc?.sections.iter().find(|(x, ..)| x == sid).cloned())
            })
            .collect()
    }

    fn document_resources(&self) -> HashSet<&str> {
        self.documents
            .values()
            .map(|d| d.resource_id.as_str())
            .collect()
    }
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn basis_count(plan: &Value, basis: &str) -> usize {
    list(plan, "prerequisites")
        .iter()
        .filter(|p| field(p, "basis") == basis)
        .count()
}

/// `general` prerequisites over all prerequisites; 0 when there are none.
fn general_share(plan: &Value) -> f64 {
    let total = list(plan, "prerequisites").len();
    if total == 0 {
        return 0.0;
    }
    basis_count(plan, "general") as f64 / total as f64
}

/// Lowercase alphanumeric words: what a 4-word overlap is made of.
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn grams(text: &str, n: usize) -> HashSet<String> {
    tokens(text).windows(n).map(|w| w.join(" ")).collect()
}

/// The first run of `n` words that an example and an exercise purpose share.
fn shared_gram(plan: &Value, n: usize) -> Option<String> {
    let examples: Vec<HashSet<String>> = list(plan, "concepts")
        .iter()
        .map(|c| grams(field(c, "example"), n))
        .collect();
    list(plan, "exercises")
        .iter()
        .map(|e| grams(field(e, "purpose"), n))
        .find_map(|purpose| {
            examples
                .iter()
                .find_map(|ex| ex.intersection(&purpose).next().cloned())
        })
}

/// `estimated_minutes` the formula gives for this plan
/// (`evals/README.md`): `P = 220 + 450 per concept + 200 with a
/// `general` prerequisite`, the recall adding one exercise.
fn planned_minutes(plan: &Value) -> i64 {
    let general = usize::from(basis_count(plan, "general") > 0);
    let prose = 220 + 450 * list(plan, "concepts").len() + 200 * general;
    let exercises = list(plan, "exercises");
    let open = exercises
        .iter()
        .filter(|e| field(e, "type") == "open_answer")
        .count();
    lesson_minutes(prose, exercises.len() + general, open)
}

/// Checks of the concept fields (`evals/README.md`).
#[allow(clippy::too_many_lines)] // one arm per key of the suite
fn richness_check(key: &str, v: &Value, plan: &Value) -> Option<Check> {
    let concepts = list(plan, "concepts");
    let general = basis_count(plan, "general");
    Some(match key {
        "concepts_count" => {
            let r = range(v)?;
            Check::of(key, in_range(concepts.len(), r), || {
                format!("{} concepts, expected {r:?}", concepts.len())
            })
        }
        "concepts_max_when_general" => {
            let max = usize::try_from(v.as_u64()?).ok()?;
            Check::of(key, general == 0 || concepts.len() <= max, || {
                format!(
                    "{} concepts with a `general` prerequisite, max {max}",
                    concepts.len()
                )
            })
        }
        "concept_fields" => {
            let example = v.get("example_chars").and_then(range);
            let steps = v.get("steps_count").and_then(range);
            let step = v.get("step_chars").and_then(range);
            let pitfall_max = v.get("pitfall_chars_max").and_then(Value::as_u64);
            let mut problems = Vec::new();
            for (i, c) in concepts.iter().enumerate() {
                match (c.get("example").and_then(Value::as_str), example) {
                    (None, _) => problems.push(format!("concepts[{i}].example is missing")),
                    (Some(e), Some(r)) if !in_range(e.trim().chars().count(), r) => {
                        problems.push(format!(
                            "concepts[{i}].example has {} characters, expected {r:?}",
                            e.trim().chars().count()
                        ));
                    }
                    _ => {}
                }
                let Some(list) = c.get("steps").and_then(Value::as_array) else {
                    problems.push(format!("concepts[{i}].steps is not a list"));
                    continue;
                };
                if let Some(r) = steps.filter(|r| !in_range(list.len(), *r)) {
                    problems.push(format!(
                        "concepts[{i}].steps has {}, expected {r:?}",
                        list.len()
                    ));
                }
                for (j, st) in list.iter().enumerate() {
                    match (st.as_str(), step) {
                        (None, _) => {
                            problems.push(format!("concepts[{i}].steps[{j}] is not a string"));
                        }
                        (Some(t), Some(r)) if !in_range(t.trim().chars().count(), r) => {
                            problems.push(format!(
                                "concepts[{i}].steps[{j}] has {} characters, expected {r:?}",
                                t.trim().chars().count()
                            ));
                        }
                        _ => {}
                    }
                }
                match (c.get("pitfall").and_then(Value::as_str), pitfall_max) {
                    (None, _) => problems.push(format!("concepts[{i}].pitfall is missing")),
                    (Some(t), Some(max)) if t.trim().chars().count() as u64 > max => {
                        problems.push(format!(
                            "concepts[{i}].pitfall has {} characters, max {max}",
                            t.trim().chars().count()
                        ));
                    }
                    _ => {}
                }
            }
            Check::of(key, problems.is_empty(), || problems.join("; "))
        }
        "pitfall_form" => {
            let re = regex(v.as_str()?).ok()?;
            let bad: Vec<String> = concepts
                .iter()
                .map(|c| field(c, "pitfall").trim())
                .filter(|p| !re.is_match(p))
                .map(str::to_owned)
                .collect();
            Check::of(key, bad.is_empty(), || {
                format!("pitfalls that do not fit the form: {bad:?}")
            })
        }
        "new_terms_max" => {
            let introduce = plan.get("terms").map_or(0, |t| list(t, "introduce").len());
            let total = usize::try_from(v.get("total")?.as_u64()?).ok()?;
            let per = usize::try_from(v.get("per_concept")?.as_u64()?).ok()?;
            Check::of(
                key,
                introduce <= total && introduce <= per * concepts.len(),
                || {
                    format!(
                        "{introduce} new terms for {} concepts, max {total} in total and {per} per concept",
                        concepts.len()
                    )
                },
            )
        }
        "general_prerequisites_max" => {
            let max = usize::try_from(v.as_u64()?).ok()?;
            Check::of(key, general <= max, || {
                format!("{general} `general` prerequisites, max {max}")
            })
        }
        "general_count" => {
            let r = range(v)?;
            Check::of(key, in_range(general, r), || {
                format!("{general} `general` prerequisites, expected {r:?}")
            })
        }
        "prerequisite_basis_max" | "prerequisite_basis_min" => {
            let most = key == "prerequisite_basis_max";
            let bad: Vec<String> = v
                .as_object()?
                .iter()
                .filter_map(|(basis, n)| {
                    let n = usize::try_from(n.as_u64()?).ok()?;
                    let have = basis_count(plan, basis);
                    let ok = if most { have <= n } else { have >= n };
                    (!ok).then(|| {
                        format!("{have} `{basis}`, {} {n}", if most { "max" } else { "min" })
                    })
                })
                .collect();
            Check::of(key, bad.is_empty(), || bad.join("; "))
        }
        "record_numbers_subset_of" => {
            let allowed: HashSet<u64> = v.as_array()?.iter().filter_map(Value::as_u64).collect();
            let cited: Vec<u64> = list(plan, "prerequisites")
                .iter()
                .flat_map(|p| list(p, "record_numbers"))
                .filter_map(Value::as_u64)
                .filter(|n| !allowed.contains(n))
                .collect();
            Check::of(key, cited.is_empty(), || {
                format!("records outside {allowed:?}: {cited:?}")
            })
        }
        "example_not_in_exercises" => {
            let shared = shared_gram(plan, 4);
            Check::of(key, shared.is_none(), || {
                format!(
                    "an example and an exercise purpose share `{}`",
                    shared.clone().unwrap_or_default()
                )
            })
            .with_severity(v)
        }
        "exercises_at_least_one_per_concept" => {
            let n = list(plan, "exercises").len();
            Check::of(key, n >= concepts.len(), || {
                format!("{n} exercises for {} concepts", concepts.len())
            })
            .with_severity(v)
        }
        "estimated_minutes_formula" => {
            let tolerance = v.get("tolerance").and_then(Value::as_i64).unwrap_or(0);
            let expected = planned_minutes(plan);
            let given = plan.get("estimated_minutes").and_then(Value::as_i64);
            Check::of(
                key,
                given.is_some_and(|g| (g - expected).abs() <= tolerance),
                || {
                    format!(
                        "estimated_minutes is {given:?}, the formula gives {expected} (tolerance {tolerance})"
                    )
                },
            )
        }
        "estimated_minutes_range" => {
            let r = range(v.get("range").unwrap_or(v))?;
            let given = plan.get("estimated_minutes").and_then(Value::as_u64);
            Check::of(
                key,
                given.is_some_and(|g| in_range(usize::try_from(g).unwrap_or(usize::MAX), r)),
                || format!("estimated_minutes is {given:?}, expected {r:?}"),
            )
            .with_severity(v)
        }
        _ => return None,
    })
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

/// Checks of the section choices (keys of the suite's `deterministic`).
#[allow(clippy::too_many_lines)] // one arm per key of the suite
pub fn plan_checks(spec: &Map<String, Value>, plan: &Value, world: &World) -> Vec<Check> {
    let chosen = world.chosen(plan);
    let chosen_ids: Vec<&str> = chosen.iter().map(|(id, ..)| id.as_str()).collect();
    run_spec(spec, "", |key, v| {
        Some(match key {
            "document_sections_include_one_of" => {
                let want = ids(v);
                Check::of(
                    key,
                    chosen_ids.iter().any(|c| want.iter().any(|w| w == c)),
                    || format!("chose {chosen_ids:?}, expected at least one of {want:?}"),
                )
            }
            "document_sections_exclude" => {
                let banned = ids(v);
                let hit: Vec<&&str> = chosen_ids
                    .iter()
                    .filter(|c| banned.iter().any(|b| b == **c))
                    .collect();
                Check::of(key, hit.is_empty(), || {
                    format!("chose sections outside the perimeter or already covered: {hit:?}")
                })
            }
            "document_sections_only_from" => {
                let allowed = ids(v);
                let bad: Vec<&&str> = chosen_ids
                    .iter()
                    .filter(|c| !allowed.iter().any(|a| a == **c))
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("sections outside the allowed set {allowed:?}: {bad:?}")
                })
            }
            "document_sections_empty" => {
                let want = v.as_bool()?;
                Check::of(key, chosen.is_empty() == want, || {
                    format!("chose {chosen_ids:?}")
                })
            }
            "document_sections_max" => {
                let max = usize::try_from(v.as_u64()?).ok()?;
                Check::of(key, chosen.len() <= max, || {
                    format!("{} sections, max {max}", chosen.len())
                })
            }
            "document_sections_max_pages" => {
                // Distinct pages of the chosen sections: what the Generator would receive.
                let mut pages = HashSet::new();
                for (_, _, a, b) in &chosen {
                    pages.extend(*a..=*b);
                }
                let max = usize::try_from(v.as_u64()?).ok()?;
                Check::of(key, pages.len() <= max, || {
                    format!("{} pages, max {max}", pages.len())
                })
            }
            "no_parent_and_child" => {
                // A section with another one inside its pages that is also chosen.
                let nested = chosen.iter().any(|(_, la, a, b)| {
                    chosen
                        .iter()
                        .any(|(_, lb, c, d)| lb > la && c >= a && d <= b)
                });
                Check::of(key, !nested, || {
                    format!("a section and one of its subsections are both chosen: {chosen_ids:?}")
                })
            }
            "document_sections_in_book_order" => {
                let ordered = chosen.windows(2).all(|w| w[0].2 <= w[1].2);
                Check::of(key, ordered, || {
                    format!("sections not in page order: {chosen:?}")
                })
            }
            "primary_is_document" => {
                let want = v.as_bool()?;
                let primary = plan
                    .get("primary_source_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let is_doc = world.document_resources().contains(primary);
                Check::of(key, is_doc == want, || {
                    format!("primary source is a document: {is_doc}, expected {want}")
                })
            }
            "document_resource_in_sources_when_chosen" => {
                // The backend rejects it; here it is a named, visible check.
                Check::of(
                    key,
                    world
                        .reference_errors(plan)
                        .iter()
                        .all(|e| !e.contains("resource is not among")),
                    || "the document's resource is missing from the plan's sources".into(),
                )
            }
            "title_not_a_chapter_number" => {
                let re = regex(r"(?i)^\s*(capitolo|cap\.?|parte|chapter|sezione)\s*\d").ok()?;
                let t = plan
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                Check::of(key, !re.is_match(t), || {
                    format!("title `{t}` is a number, not a concept")
                })
            }
            "exercises_not_about_layout" => {
                let pats = patterns(v);
                let re = regex(&pats.join("|")).ok()?;
                let bad: Vec<String> = plan
                    .get("exercises")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.get("purpose").and_then(Value::as_str))
                    .filter(|p| re.is_match(p))
                    .map(str::to_owned)
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("exercise purposes about pages or titles: {bad:?}")
                })
            }
            _ => return richness_check(key, v, plan),
        })
    })
}

/// Why the value of a Planner key cannot be read, or `None` when it can (or
/// the key is not one with a typed value). A malformed value would make its
/// check vanish, so the suite is refused at load time.
#[must_use]
pub fn value_problem(key: &str, v: &Value) -> Option<String> {
    use crate::checks::{Want, object_problem, value_problem as top};
    match key {
        "concepts_count" | "general_count" => top(key, v, Want::Range),
        "concepts_max_when_general"
        | "general_prerequisites_max"
        | "document_sections_max"
        | "document_sections_max_pages" => top(key, v, Want::UInt),
        "document_sections_empty" | "primary_is_document" => top(key, v, Want::Bool),
        "document_sections_include_one_of"
        | "document_sections_exclude"
        | "document_sections_only_from" => top(key, v, Want::StrList),
        "concept_fields" => object_problem(
            key,
            v,
            &[
                ("example_chars", Want::Range, false),
                ("steps_count", Want::Range, false),
                ("step_chars", Want::Range, false),
                ("pitfall_chars_max", Want::UInt, false),
            ],
        ),
        "pitfall_form" => top(key, v, Want::Regex),
        "new_terms_max" => object_problem(
            key,
            v,
            &[
                ("total", Want::UInt, true),
                ("per_concept", Want::UInt, true),
            ],
        ),
        "prerequisite_basis_max" | "prerequisite_basis_min" => top(key, v, Want::UIntMap),
        "record_numbers_subset_of" => top(key, v, Want::UIntList),
        "estimated_minutes_formula" => object_problem(key, v, &[("tolerance", Want::Int, false)]),
        "estimated_minutes_range" => top(key, v, Want::RangeOrWrapped),
        _ => None,
    }
}

/// Every key of `deterministic` that `plan_checks` and the run itself
/// compute; a suite naming any other is refused at load time.
pub const KNOWN_KEYS: &[&str] = &[
    "planner_calls_max",
    "plan_valid",
    "plan_valid_first_try",
    "document_sections_include_one_of",
    "document_sections_exclude",
    "document_sections_only_from",
    "document_sections_empty",
    "document_sections_max",
    "document_sections_max_pages",
    "no_parent_and_child",
    "document_sections_in_book_order",
    "primary_is_document",
    "document_resource_in_sources_when_chosen",
    "title_not_a_chapter_number",
    "exercises_not_about_layout",
    "concepts_count",
    "concepts_max_when_general",
    "concept_fields",
    "pitfall_form",
    "new_terms_max",
    "general_prerequisites_max",
    "general_count",
    "prerequisite_basis_max",
    "prerequisite_basis_min",
    "record_numbers_subset_of",
    "example_not_in_exercises",
    "exercises_at_least_one_per_concept",
    "estimated_minutes_formula",
    "estimated_minutes_range",
];

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use super::*;
    use crate::checks::{Status, status_of};

    const D: &str = "0193d000-0000-7000-8000-000000000001";
    const R: &str = "0193e000-0000-7000-8000-000000000001";
    const W: &str = "0193e000-0000-7000-8000-000000000002";

    fn world() -> World {
        let docs = vec![
            json!({"document_id": D, "resource_id": R, "title": "Libro", "sections": [
            {"id": "s1", "title": "1", "level": 1, "page_start": 1, "page_end": 20},
            {"id": "s2", "title": "1.1", "level": 2, "page_start": 3, "page_end": 9},
            {"id": "s3", "title": "2", "level": 1, "page_start": 21, "page_end": 40}]}),
        ];
        let sources = vec![json!({"resource_id": W, "kind": "knowledge", "url": "https://a.org"})];
        World::new(
            &sources,
            &docs,
            &[],
            &Case {
                id: "c".into(),
                raw: json!({}),
            },
        )
    }

    fn plan(sections: &[&str], primary: &str) -> Value {
        json!({"title": "Capitolo 2", "objective": "Saper citare il capitolo 2.",
            "why_now": "Segue il libro.", "estimated_minutes": 10, "primary_source_id": primary, "other_source_ids": [W],
            "concepts": [
                {"name": "a", "key_point": "b", "example": "un caso", "steps": ["uno", "due"], "pitfall": ""},
                {"name": "c", "key_point": "d", "example": "un altro caso", "steps": ["uno", "due"], "pitfall": ""},
                {"name": "e", "key_point": "f", "example": "un terzo caso", "steps": ["uno", "due"], "pitfall": ""},
                {"name": "g", "key_point": "h", "example": "un quarto caso", "steps": ["uno", "due"], "pitfall": ""}
            ],
            "exercises": [{"type": "open_answer", "purpose": "Citare a pagina 21", "verifies_objective": true},
                          {"type": "fill_blank", "purpose": "Usare l'idea", "verifies_objective": false}],
            "document_sections": [{"document_id": D, "section_ids": sections}]})
    }

    #[test]
    fn references_are_checked_like_the_backend() {
        let w = world();
        assert!(w.reference_errors(&plan(&["s3"], R)).is_empty());
        assert!(!w.reference_errors(&plan(&["s9"], R)).is_empty());
        assert!(!w.reference_errors(&plan(&["s1", "s1"], R)).is_empty());
        // The document's resource must be among the plan's sources.
        let mut p = plan(&["s3"], W);
        p["other_source_ids"] = json!([]);
        assert!(
            w.reference_errors(&p)
                .iter()
                .any(|e| e.contains("deve essere tra"))
        );
    }

    #[test]
    fn the_plan_the_checks_read_has_the_backends_minutes() {
        let w = world();
        let mut p = plan(&["s3"], R);
        p["estimated_minutes"] = json!(99);
        // 4 concepts, 2 exercises, 1 open answer: 8.08 + 1 + 2 = 11.08 -> 11.
        let kept = w.finalized(&p).expect("a valid plan");
        assert_eq!(kept["estimated_minutes"], 11);
        // A plan that does not validate is not finalized.
        assert!(w.finalized(&plan(&["s9"], R)).is_none());
        // A text that is no plan is described, not panicked on.
        assert!(!w.text_errors("non e un piano").is_empty());
    }

    #[test]
    fn section_choices_are_scored() {
        let w = world();
        let spec = json!({"document_sections_include_one_of": ["s3"], "document_sections_exclude": ["s1"],
            "no_parent_and_child": true, "primary_is_document": true, "document_sections_max_pages": 25,
            "title_not_a_chapter_number": true, "exercises_not_about_layout": "(?i)pagin"});
        let ok = plan_checks(spec.as_object().unwrap(), &plan(&["s3"], R), &w);
        assert_eq!(
            status_of(&ok, "document_sections_include_one_of"),
            Status::Pass
        );
        assert_eq!(status_of(&ok, "primary_is_document"), Status::Pass);
        assert_eq!(status_of(&ok, "title_not_a_chapter_number"), Status::Fail);
        assert_eq!(status_of(&ok, "exercises_not_about_layout"), Status::Fail);
        let nested = plan_checks(spec.as_object().unwrap(), &plan(&["s1", "s2"], R), &w);
        assert_eq!(status_of(&nested, "no_parent_and_child"), Status::Fail);
        assert_eq!(
            status_of(&nested, "document_sections_exclude"),
            Status::Fail
        );
        assert_eq!(
            status_of(&nested, "document_sections_include_one_of"),
            Status::Fail
        );
    }

    // ----- Concept fields -----

    fn good_plan() -> Value {
        let step = |t: &str| json!(t);
        json!({
            "title": "Perché una formula cambia", "objective": "Saper prevedere.", "why_now": "Serve.",
            "estimated_minutes": 10,
            "prerequisites": [
                {"statement": "Sa cos'è una cella", "basis": "general", "record_numbers": []},
                {"statement": "Prevede il move", "basis": "record", "record_numbers": [1]}],
            "concepts": [
                {"name": "relativo", "key_point": "Si sposta.", "example": "In C2 c'è A2 per B2 con 12 e 3.",
                 "steps": [step("la distanza resta"), step("la riga cambia")], "pitfall": "Pensare che sia identica."},
                {"name": "assoluto", "key_point": "Resta fermo.", "example": "L'IVA sta in E1 e vale 22.",
                 "steps": [step("il dollaro fissa"), step("la copia lo lascia")], "pitfall": ""}],
            "terms": {"introduce": [{"key": "a", "term": "a", "hint": "h"}, {"key": "b", "term": "b", "hint": "h"}], "reuse": []},
            "exercises": [
                {"type": "fill_blank", "purpose": "completare una formula copiata", "verifies_objective": true},
                {"type": "open_answer", "purpose": "spiegare cosa si sposta", "verifies_objective": false},
                {"type": "quiz_single", "purpose": "scegliere la formula giusta", "verifies_objective": false}]
        })
    }

    fn run_checks(spec: &Value, plan: &Value) -> Vec<Check> {
        plan_checks(spec.as_object().unwrap(), plan, &world())
    }

    #[test]
    fn concept_and_term_limits_are_checked_on_the_plan() {
        let spec = json!({
            "concepts_count": [2, 4], "concepts_max_when_general": 3,
            "concept_fields": {"example_chars": [1, 300], "steps_count": [2, 6], "step_chars": [1, 150], "pitfall_chars_max": 200},
            "pitfall_form": "^(Pensare che .+)?$",
            "new_terms_max": {"per_concept": 2, "total": 8},
            "general_prerequisites_max": 3});
        let ok = run_checks(&spec, &good_plan());
        assert!(ok.iter().all(|c| c.status == Status::Pass), "{ok:?}");

        let mut p = good_plan();
        p["concepts"][0]["pitfall"] = json!("Copiare è sbagliato.");
        p["concepts"][1]["steps"] = json!(["uno"]);
        p["concepts"][1]["example"] = json!("x".repeat(301));
        p["terms"]["introduce"] = json!(
            (0..5)
                .map(|i| json!({"key": format!("k{i}"), "term": "t", "hint": "h"}))
                .collect::<Vec<_>>()
        );
        let c = run_checks(&spec, &p);
        for name in ["pitfall_form", "concept_fields", "new_terms_max"] {
            assert_eq!(status_of(&c, name), Status::Fail, "{name}");
        }
        // Three concepts with a `general` prerequisite exceed the cap of 3.
        let mut four = good_plan();
        let third = four["concepts"][0].clone();
        four["concepts"]
            .as_array_mut()
            .unwrap()
            .extend([third.clone(), third]);
        assert_eq!(
            status_of(&run_checks(&spec, &four), "concepts_max_when_general"),
            Status::Fail
        );
        assert_eq!(
            status_of(&run_checks(&spec, &four), "concepts_count"),
            Status::Pass
        );
    }

    #[test]
    fn prerequisites_are_counted_by_basis() {
        let spec = json!({
            "general_count": [1, 3], "prerequisite_basis_max": {"record": 0, "mission": 0},
            "prerequisite_basis_min": {"record": 1}, "record_numbers_subset_of": [1, 2],
            "general_prerequisites_max": 1});
        let c = run_checks(&spec, &good_plan());
        assert_eq!(status_of(&c, "general_count"), Status::Pass);
        assert_eq!(status_of(&c, "prerequisite_basis_max"), Status::Fail);
        assert_eq!(status_of(&c, "prerequisite_basis_min"), Status::Pass);
        assert_eq!(status_of(&c, "record_numbers_subset_of"), Status::Pass);
        let mut p = good_plan();
        p["prerequisites"][1]["record_numbers"] = json!([1, 7]);
        assert_eq!(
            status_of(&run_checks(&spec, &p), "record_numbers_subset_of"),
            Status::Fail
        );
        p["prerequisites"] = json!([]);
        assert_eq!(
            status_of(&run_checks(&spec, &p), "general_count"),
            Status::Fail
        );
        assert_eq!(general_share(&p), 0.0);
        assert!((general_share(&good_plan()) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn an_example_shared_with_an_exercise_is_a_warning() {
        let spec = json!({"example_not_in_exercises": {"severity": "warn"}, "exercises_at_least_one_per_concept": "warn"});
        let c = run_checks(&spec, &good_plan());
        assert_eq!(status_of(&c, "example_not_in_exercises"), Status::Pass);
        assert_eq!(
            status_of(&c, "exercises_at_least_one_per_concept"),
            Status::Pass
        );
        let mut p = good_plan();
        // Three consecutive words are not enough, four are.
        p["exercises"][0]["purpose"] = json!("scrivere A2 per B2");
        assert_eq!(
            status_of(&run_checks(&spec, &p), "example_not_in_exercises"),
            Status::Pass
        );
        p["exercises"][0]["purpose"] = json!("Calcolare: in C2 c'è A2 per B2, con altri valori");
        assert_eq!(
            status_of(&run_checks(&spec, &p), "example_not_in_exercises"),
            Status::Warn
        );
        p["exercises"] = json!([p["exercises"][0].clone()]);
        assert_eq!(
            status_of(&run_checks(&spec, &p), "exercises_at_least_one_per_concept"),
            Status::Warn
        );
    }

    #[test]
    fn the_minutes_follow_the_formula() {
        // P = 220 + 450*2 + 200 = 1320; E = 3 + 1; O = 1: 5.28 + 2 + 2 = 9.28 -> 9.
        assert_eq!(planned_minutes(&good_plan()), 9);
        let spec = json!({"estimated_minutes_formula": {"tolerance": 1},
            "estimated_minutes_range": {"range": [10, 15], "severity": "warn"}});
        let c = run_checks(&spec, &good_plan());
        assert_eq!(status_of(&c, "estimated_minutes_formula"), Status::Pass);
        assert_eq!(status_of(&c, "estimated_minutes_range"), Status::Pass);
        let mut p = good_plan();
        p["estimated_minutes"] = json!(14);
        let c = run_checks(&spec, &p);
        assert_eq!(status_of(&c, "estimated_minutes_formula"), Status::Fail);
        p["estimated_minutes"] = json!(5);
        assert_eq!(
            status_of(&run_checks(&spec, &p), "estimated_minutes_range"),
            Status::Warn
        );
        // Without a `general` prerequisite P and E shrink.
        p["prerequisites"] = json!([]);
        assert_eq!(planned_minutes(&p), 8);
    }

    /// The suites of the repository: which checks the runner can compute, and
    /// whether the fixed plans of the Generator cases are accepted by the
    /// backend's own validation.
    fn shipped(suites: &[&str]) -> Vec<crate::suite::Suite> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals");
        crate::suite::load(
            &dir,
            &suites.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn every_check_of_the_planner_suites_is_computed() {
        for suite in shipped(&["planner/pdf", "planner/concepts"]) {
            for case in &suite.cases {
                let mut spec = suite.deterministic(case);
                for k in ["plan_valid", "plan_valid_first_try", "planner_calls_max"] {
                    spec.remove(k);
                }
                let world = World::new(
                    &case.array("sources"),
                    &case.array("documents"),
                    &case.array("lessons"),
                    case,
                );
                let checks = plan_checks(&spec, &good_plan(), &world);
                assert_eq!(checks.len(), spec.len(), "{}/{}", suite.id, case.id);
                let skipped: Vec<&Check> = checks
                    .iter()
                    .filter(|c| c.status == Status::Skipped)
                    .collect();
                assert!(skipped.is_empty(), "{}/{}: {skipped:?}", suite.id, case.id);
            }
        }
    }

    #[test]
    fn the_fixed_plans_of_every_generator_suite_pass_the_backend_validation() {
        let suites = shipped(&[
            "generator/lessons",
            "generator/pdf",
            "generator/rich-lessons",
            "generator/web-fetch",
        ]);
        // Suites load sorted by id: lessons, pdf, rich-lessons, web-fetch.
        // Cases and fixed plans are counted so that a case added to a suite is
        // noticed here and its plan checked, not skipped by a stale loop.
        let counts: Vec<(usize, usize)> = suites
            .iter()
            .map(|s| {
                (
                    s.cases.len(),
                    s.cases.iter().filter(|c| c.get("plan").is_some()).count(),
                )
            })
            .collect();
        assert_eq!(counts, [(7, 7), (2, 2), (5, 5), (5, 5)], "{counts:?}");
        let mut failures = Vec::new();
        for suite in &suites {
            for case in &suite.cases {
                let Some(plan) = case.get("plan") else {
                    continue;
                };
                let world = World::new(
                    &case.array("sources"),
                    &case.array("documents"),
                    &case.array("lessons"),
                    case,
                );
                let errors = world.reference_errors(plan);
                if !errors.is_empty() {
                    failures.push(format!("{}/{}: {errors:?}", suite.id, case.id));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
