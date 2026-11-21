//! Grader: one call of the real `Grader` on one open answer.

use application::LessonPrompts;
use application::structured::{OutputSchema, check_schema, parse_object};
use domain::llm::LlmRole;
use domain::{Lesson, LessonExercises, LessonId, LessonStatus, TopicId};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Env, subject_calls};
use crate::checks::{self, Check, in_range, range, run_spec, sentences};
use crate::report::{RoleRun, TurnReport};
use crate::suite::{Case, Suite, criteria};

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    let input = case.get("input").cloned().unwrap_or(Value::Null);
    let text = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let doc = json!({"blocks": [{"id": "q1", "type": "open_answer", "question": text("question"),
        "max_chars": input.get("max_chars").and_then(Value::as_u64).unwrap_or(1000),
        "rubric": text("rubric"), "required": true}]});
    let exercises = LessonExercises::from_document(&doc);
    let exercise = exercises
        .get("q1")
        .ok_or_else(|| anyhow::anyhow!("the case does not describe a valid open answer"))?;
    let lesson = Lesson {
        quality_warnings: Vec::new(),
        id: LessonId::new(Uuid::nil()),
        topic_id: TopicId::new(Uuid::nil()),
        number: 1,
        title: text("lesson_title"),
        objective: text("lesson_objective"),
        estimated_minutes: 5,
        status: LessonStatus::NotStarted,
        catalog_version: 1,
        mission_version: 1,
        created_at: OffsetDateTime::UNIX_EPOCH,
        completed_at: None,
    };
    let prompts = LessonPrompts::embedded().grader;
    let grader = application::Grader::new(env.llm.clone(), prompts, 1500);
    let mark = env.ledger.mark();
    let outcome = grader
        .grade(&lesson, exercise, &text("answer"), case.content_language())
        .await;
    let calls = subject_calls(env, mark, LlmRole::Grader);
    let raw = calls.last().map_or_default(|c| c.text.clone());
    let schema = prompts
        .output_schema
        .and_then(|s| OutputSchema::parse(s, "grade").ok());

    let spec = suite.deterministic(case);
    let expect = case.get("expect").cloned().unwrap_or(Value::Null);
    let parsed = parse_object(&raw).ok();
    let mut checks = grader_checks(&spec, parsed.as_ref(), schema.as_ref().map(|s| &s.schema));
    checks.extend(expect_checks(&expect, parsed.as_ref()));
    if let Err(e) = &outcome {
        checks.push(Check::of("grade_call", false, || e.to_string()));
    }

    let material = format!(
        "Lesson: {}\nObjective: {}\nQuestion: {}\nGrading rubric (hidden from the learner): {}\nLearner answer:\n{}\n\nGrader output:\n{}",
        text("lesson_title"),
        text("lesson_objective"),
        text("question"),
        text("rubric"),
        text("answer"),
        raw
    );
    let judge = env
        .judge
        .judge(&material, &criteria(case.get("rubric")))
        .await;
    Ok(RoleRun {
        turns: vec![TurnReport {
            label: "grade".into(),
            input: text("answer"),
            output: raw,
            checks,
            judge,
            extras: json!({"latency_ms": calls.last().map(|c| c.latency.as_millis())}),
        }],
        ..RoleRun::default()
    })
}

fn field<'a>(out: &'a Value, key: &str) -> &'a str {
    out.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// The suite's deterministic keys, on the Grader's JSON.
pub fn grader_checks(
    spec: &serde_json::Map<String, Value>,
    out: Option<&Value>,
    schema: Option<&Value>,
) -> Vec<Check> {
    let Some(out) = out else {
        return vec![Check::of("output_is_json", false, || {
            "the answer is not a JSON object".into()
        })];
    };
    let feedback = field(out, "feedback");
    let misconception = out.get("misconception").and_then(Value::as_str);
    let both = format!("{feedback}\n{}", misconception.unwrap_or_default());
    let verdict = field(out, "verdict").to_owned();
    run_spec(spec, &both, |key, v| {
        Some(match key {
            "schema_valid" => {
                let problems = schema.map_or_default(|s| check_schema(out, s));
                Check::of(key, problems.is_empty(), || problems.join("; "))
            }
            "feedback_chars" => {
                let r = range(v)?;
                let n = feedback.trim().chars().count();
                Check::of(key, in_range(n, r), || {
                    format!("{n} characters, expected {r:?}")
                })
            }
            "misconception_chars" => {
                let r = range(v)?;
                match misconception {
                    None => Check::pass(key),
                    Some(m) => {
                        let n = m.trim().chars().count();
                        Check::of(key, in_range(n, r), || {
                            format!("{n} characters, expected {r:?}")
                        })
                    }
                }
            }
            "no_newline_in_feedback" => Check::of(key, !feedback.contains('\n'), || {
                "feedback has a line break".into()
            }),
            "no_numeric_grade" => {
                let numeric = out
                    .as_object()
                    .is_some_and(|o| o.values().any(Value::is_number));
                Check::of(key, !numeric, || "the output carries a number".into())
            }
            "misconception_third_person" => match (misconception, v.as_str()) {
                (Some(m), Some(p)) => match checks::regex(p) {
                    Ok(re) => Check::of(key, re.is_match(m.trim()), || {
                        "the misconception is not in the third person".into()
                    }),
                    Err(e) => Check::of(key, false, || e),
                },
                _ => Check::pass(key),
            },
            "feedback_sentences" => {
                let r = range(v)?;
                let n = sentences(feedback);
                Check::soft(key, in_range(n, r), || {
                    format!("{n} sentences, typical {r:?}")
                })
            }
            "feedback_chars_typical" => {
                let r = range(v)?;
                let n = feedback.trim().chars().count();
                Check::soft(key, in_range(n, r), || {
                    format!("{n} characters, typical {r:?}")
                })
            }
            "feedback_chars_max_by_verdict" => {
                let n = feedback.trim().chars().count();
                let max = v.get(&verdict).and_then(Value::as_u64).unwrap_or(350);
                let hard = 450;
                if n as u64 > hard {
                    Check::of(key, false, || {
                        format!("{n} characters for `{verdict}`, hard limit {hard}")
                    })
                } else {
                    Check::soft(key, n as u64 <= max, || {
                        format!("{n} characters for `{verdict}`, indicative limit {max}")
                    })
                }
            }
            "partial_single_gap" => Check::skipped(key, "judged by the rubric"),
            _ => return None,
        })
    })
}

fn expect_checks(expect: &Value, out: Option<&Value>) -> Vec<Check> {
    let mut checks = Vec::new();
    let Some(out) = out else { return checks };
    if let Some(accepted) = expect.get("verdict").and_then(Value::as_array) {
        let verdict = field(out, "verdict");
        checks.push(Check::of(
            "expect.verdict",
            accepted.iter().any(|a| a.as_str() == Some(verdict)),
            || format!("verdict `{verdict}`, accepted {accepted:?}"),
        ));
    }
    match expect.get("misconception").and_then(Value::as_str) {
        Some("null") => checks.push(Check::of(
            "expect.misconception",
            out.get("misconception").is_none_or(Value::is_null),
            || "the misconception should be null".into(),
        )),
        Some("present") => checks.push(Check::of(
            "expect.misconception",
            out.get("misconception")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty()),
            || "a misconception was expected".into(),
        )),
        _ => {}
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Status, status_of};

    fn spec(v: &Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn verdict_dependent_length_is_soft_until_the_hard_limit() {
        let s = spec(
            &json!({"feedback_chars_max_by_verdict": {"correct": 300, "partial": 350, "incorrect": 350}}),
        );
        let out = |n: usize| json!({"verdict": "correct", "feedback": "a".repeat(n), "misconception": null});
        assert_eq!(
            grader_checks(&s, Some(&out(250)), None)[0].status,
            Status::Pass
        );
        assert_eq!(
            grader_checks(&s, Some(&out(400)), None)[0].status,
            Status::Warn
        );
        assert_eq!(
            grader_checks(&s, Some(&out(500)), None)[0].status,
            Status::Fail
        );
    }

    #[test]
    fn typical_range_and_third_person() {
        let s = spec(
            &json!({"feedback_chars_typical": [100, 350], "misconception_third_person": "(?i)^(pensa|crede)\\b"}),
        );
        let good = json!({"verdict": "incorrect", "feedback": "x".repeat(120), "misconception": "Pensa che sia vero."});
        assert!(
            grader_checks(&s, Some(&good), None)
                .iter()
                .all(|c| c.status == Status::Pass)
        );
        let bad =
            json!({"verdict": "incorrect", "feedback": "short", "misconception": "Tu pensi male."});
        let c = grader_checks(&s, Some(&bad), None);
        assert_eq!(status_of(&c, "feedback_chars_typical"), Status::Warn);
        assert_eq!(status_of(&c, "misconception_third_person"), Status::Fail);
    }

    #[test]
    fn expectations_on_verdict_and_misconception() {
        let out = json!({"verdict": "partial", "feedback": "f", "misconception": null});
        let ok = expect_checks(
            &json!({"verdict": ["partial"], "misconception": "null"}),
            Some(&out),
        );
        assert!(ok.iter().all(|c| c.status == Status::Pass));
        let bad = expect_checks(
            &json!({"verdict": ["correct"], "misconception": "present"}),
            Some(&out),
        );
        assert!(bad.iter().all(|c| c.status == Status::Fail));
    }
}
