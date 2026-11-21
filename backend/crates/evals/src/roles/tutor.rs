//! Tutor: the real chat use case (Context Assembler, tool loop and tool
//! validation included) over a database seeded from the case.

use domain::UserId;
use std::fmt::Write as _;
use std::time::Duration;

use anyhow::{Context, anyhow};
use domain::llm::{LlmRole, Part};
use domain::{MessagePart, MessageStatus, PageRequest};
use serde_json::{Map, Value, json};

use super::{Env, subject_calls};
use crate::checks::{Check, in_range, range, regex, run_spec, words};
use crate::llm::CallRecord;
use crate::report::{RoleRun, TurnReport};
use crate::seed::new_app;
use crate::suite::{Case, Suite, criteria};

const REPLY_TIMEOUT: Duration = Duration::from_secs(300);
const MISSION_TOOLS: [&str; 2] = ["save_mission_draft", "propose_mission_change"];

enum Step {
    User { text: String, expect: Value },
    Event(String),
}

/// Multi-step cases have `steps`; conversation cases have `messages`, whose last
/// entry is the user turn under test and the rest the history.
fn plan(case: &Case) -> (Vec<Value>, Vec<Step>) {
    if let Some(steps) = case.get("steps").and_then(Value::as_array) {
        let steps = steps
            .iter()
            .filter_map(|s| {
                if let Some(e) = s.get("event").and_then(Value::as_str) {
                    Some(Step::Event(e.to_owned()))
                } else {
                    Some(Step::User {
                        text: s.get("user")?.as_str()?.to_owned(),
                        expect: s.get("expect").cloned().unwrap_or(Value::Null),
                    })
                }
            })
            .collect();
        return (case.array("history"), steps);
    }
    let mut messages = case.array("messages");
    let last = messages.pop();
    let text = last
        .as_ref()
        .and_then(|m| m.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    (
        messages,
        vec![Step::User {
            text,
            // Conversation cases keep the rubric at case level.
            expect: json!({"rubric": case.get("rubric").cloned().unwrap_or(Value::Null)}),
        }],
    )
}

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    let topic = case.get("topic").cloned().unwrap_or(Value::Null);
    let title = topic
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Argomento");
    let state = topic
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("interviewing");
    let app = new_app(
        env.llm.clone(),
        env.tuning,
        title,
        state,
        case.content_language(),
    )
    .await?;
    if let Some(m) = topic.get("mission") {
        if let Some(current) = m.get("current") {
            app.mission("confirmed", current).await?;
        }
        if let Some(draft) = m.get("draft") {
            app.mission("draft", draft).await?;
        }
    }
    if let Some(prefs) = topic.get("preferences").and_then(Value::as_array) {
        app.preferences(prefs).await?;
    }
    let (history, steps) = plan(case);
    app.history(&history).await?;

    let base = suite.deterministic(case);
    let tool_defaults = suite.tool_input_defaults();
    let mut conversation: Vec<(String, String)> = history
        .iter()
        .map(|m| {
            (
                m.get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_owned(),
                m.get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect();
    let mut turns = Vec::new();
    let mut events = Vec::new();
    let mut tool_rounds = 0usize;
    let mut validation_errors = 0usize;

    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Event(name) => {
                if name != "confirm_mission" {
                    return Err(anyhow!("unknown event `{name}`"));
                }
                let mission = app
                    .state
                    .missions
                    .get(UserId::OWNER, app.topic)
                    .await
                    .map_err(|e| anyhow!("{e:?}"))?;
                let draft = mission
                    .draft
                    .context("confirm_mission: there is no open draft")?;
                app.state
                    .missions
                    .confirm(UserId::OWNER, app.topic, Some(draft.version))
                    .await
                    .map_err(|e| anyhow!("confirm_mission: {e:?}"))?;
                events.push(json!({"step": index, "event": name, "version": draft.version}));
            }
            Step::User { text, expect } => {
                // As the app does: a proposal the person answered in words
                // is not open any more.
                let current: String = sqlx::query_scalar("SELECT state FROM topics WHERE id = ?")
                    .bind(app.topic.to_string())
                    .fetch_one(&app.pool)
                    .await?;
                if current == "mission_proposed" {
                    app.set_state("interviewing").await?;
                }
                let mark = env.ledger.mark();
                let sent = app
                    .state
                    .chat
                    .send(UserId::OWNER, app.topic, Some(text))
                    .await
                    .map_err(|e| anyhow!("send: {e:?}"))?;
                let reply = wait_for_reply(&app, sent.assistant.id).await?;
                let reply_text = reply.text();
                let calls = subject_calls(env, mark, LlmRole::Tutor);
                let tool_calls: Vec<(String, Value)> =
                    calls.iter().flat_map(|c| c.tool_calls.clone()).collect();
                let errors = validation_errors_in(&calls);
                tool_rounds += calls.len().saturating_sub(1);
                validation_errors += errors;

                let (mut checks, rubric) = (Vec::new(), expect.get("rubric"));
                if reply.status == MessageStatus::Failed {
                    checks.push(Check::of("reply_completed", false, || {
                        reply
                            .error
                            .as_ref()
                            .map_or_else(|| "failed".to_owned(), |e| format!("{e:?}"))
                    }));
                }
                let mut spec: Map<String, Value> = base.clone();
                if let Some(over) = expect.get("deterministic").and_then(Value::as_object) {
                    spec.extend(over.clone());
                }
                checks.extend(text_checks(&spec, &reply_text, text, &tool_calls));
                if let Some(t) = expect.get("tools") {
                    checks.extend(tool_checks(t, &tool_calls));
                }
                checks.extend(tool_input_defaults(&tool_defaults, &tool_calls, errors));

                conversation.push(("user".into(), text.clone()));
                let material = material(&conversation, &tool_calls, &reply_text);
                let judge = env.judge.judge(&material, &criteria(rubric)).await;
                conversation.push(("assistant".into(), reply_text.clone()));
                turns.push(TurnReport {
                    label: format!("step {index}"),
                    input: text.clone(),
                    output: reply_text,
                    checks,
                    judge,
                    extras: json!({
                        "tool_calls": tool_calls.iter().map(|(n, i)| json!({"name": n, "input": i})).collect::<Vec<_>>(),
                        "model_calls": calls.len(),
                        "tool_validation_errors": errors,
                        "mission_proposal": reply.parts.iter().any(|p| matches!(p, MessagePart::MissionProposal { .. })),
                    }),
                });
            }
        }
    }
    let mut run = RoleRun {
        turns,
        extras: json!({"events": events}),
        ..RoleRun::default()
    };
    run.metrics.insert("tool_rounds".into(), tool_rounds as f64);
    run.metrics
        .insert("tool_validation_errors".into(), validation_errors as f64);
    Ok(run)
}

async fn wait_for_reply(
    app: &crate::seed::App,
    id: domain::MessageId,
) -> anyhow::Result<domain::ChatMessage> {
    let deadline = tokio::time::Instant::now() + REPLY_TIMEOUT;
    loop {
        let page = app
            .state
            .chat
            .list(
                UserId::OWNER,
                app.topic,
                PageRequest {
                    after: None,
                    limit: 10,
                },
            )
            .await
            .map_err(|e| anyhow!("{e:?}"))?;
        if let Some(m) = page.page.items.into_iter().find(|m| m.id == id)
            && m.status != MessageStatus::Streaming
        {
            return Ok(m);
        }
        if tokio::time::Instant::now() > deadline {
            return Err(anyhow!("the reply did not finish in {REPLY_TIMEOUT:?}"));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Tool results that came back as errors: the model's input was refused by
/// the real validation (a sign that a schema or description is unclear).
fn validation_errors_in(calls: &[CallRecord]) -> usize {
    calls
        .iter()
        .filter_map(|c| c.request.messages.last())
        .flat_map(|m| m.parts.iter())
        .filter(|p| matches!(p, Part::ToolResult { is_error: true, .. }))
        .count()
}

fn material(conversation: &[(String, String)], tools: &[(String, Value)], reply: &str) -> String {
    let mut out = String::from("CONVERSATION (so far)\n");
    for (role, text) in conversation {
        let _ = writeln!(out, "{role}: {text}");
    }
    out.push_str("\nTOOL CALLS MADE BY THE TUTOR IN ITS LAST TURN\n");
    if tools.is_empty() {
        out.push_str("(none)\n");
    }
    for (name, input) in tools {
        let _ = writeln!(out, "{name}: {input}");
    }
    let _ = write!(out, "\nTUTOR REPLY TO JUDGE\n{reply}\n");
    out
}

/// `user` is the message of the turn: words the person wrote may come back in
/// the reply (the name of the topic) without counting as copying the card.
fn text_checks(
    spec: &Map<String, Value>,
    text: &str,
    user: &str,
    tools: &[(String, Value)],
) -> Vec<Check> {
    let called = !tools.is_empty();
    run_spec(spec, text, |key, v| {
        Some(match key {
            "max_words_when_tool_called" => {
                if !called {
                    return Some(Check::pass(key));
                }
                let max = v.as_u64()?;
                let n = words(text);
                Check::of(key, n as u64 <= max, || {
                    format!("{n} words with a tool call, max {max}")
                })
            }
            "text_must_not_repeat_tool_fields" => {
                let hit = tools
                    .iter()
                    .find_map(|(_, input)| repeated_window(text, user, input));
                Check::of(key, hit.is_none(), || {
                    format!(
                        "the text repeats {REPEAT_WINDOW} words of a tool field: `{}`",
                        hit.unwrap_or_default()
                    )
                })
            }
            "no_list_when_tool_called" => {
                let list = regex(r"(?m)^\s*([-*+]|\d+[.)])\s+").ok()?;
                Check::of(key, !(called && list.is_match(text)), || {
                    "a list next to a tool call".into()
                })
            }
            _ => return None,
        })
    })
}

/// Consecutive words of a tool field that the text may not repeat. Five, not
/// four: with four, the name of the topic in a one-line intro ("servizio di
/// pagamenti che") already counted as copying the card.
const REPEAT_WINDOW: usize = 5;

/// [`REPEAT_WINDOW`] consecutive words of `text` found in a string of the tool
/// input and not written by the person in `user`.
fn repeated_window(text: &str, user: &str, input: &Value) -> Option<String> {
    fn norm(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    }
    fn strings(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
            Value::Object(o) => o.values().for_each(|x| strings(x, out)),
            _ => {}
        }
    }
    let mut fields = Vec::new();
    strings(input, &mut fields);
    let text_words = norm(text);
    let user_words = norm(user);
    for f in fields {
        let fw = norm(&f);
        for window in fw.windows(REPEAT_WINDOW) {
            if text_words.windows(REPEAT_WINDOW).any(|t| t == window)
                && !user_words.windows(REPEAT_WINDOW).any(|u| u == window)
            {
                return Some(window.join(" "));
            }
        }
    }
    None
}

fn tool_checks(expect: &Value, calls: &[(String, Value)]) -> Vec<Check> {
    let names = |key: &str| -> Vec<String> {
        expect
            .get(key)
            .and_then(Value::as_array)
            .map_or_default(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
    };
    let mut out = Vec::new();
    let called = names("called");
    if expect.get("called").is_some() {
        let missing: Vec<_> = called
            .iter()
            .filter(|n| !calls.iter().any(|(c, _)| c == *n))
            .collect();
        out.push(Check::of("tools.called", missing.is_empty(), || {
            format!("not called: {missing:?}")
        }));
    }
    if expect.get("not_called").is_some() {
        let wrong: Vec<_> = names("not_called")
            .into_iter()
            .filter(|n| calls.iter().any(|(c, _)| c == n))
            .collect();
        out.push(Check::of("tools.not_called", wrong.is_empty(), || {
            format!("called: {wrong:?}")
        }));
    }
    if let Some(max) = expect.get("max_calls").and_then(Value::as_u64) {
        out.push(Check::of(
            "tools.max_calls",
            calls.len() as u64 <= max,
            || format!("{} tool calls, max {max}", calls.len()),
        ));
    }
    if let Some(checks) = expect.get("input_checks") {
        // The last call of a tool the case expects (else the last call).
        let target = calls
            .iter()
            .rev()
            .find(|(n, _)| called.contains(n))
            .or_else(|| calls.last());
        match target {
            Some((_, input)) => out.extend(input_checks(checks, input)),
            None => out.push(Check::of("tools.input_checks", false, || {
                "no tool call to check".into()
            })),
        }
    }
    out
}

fn entries(input: &Value, field: &str) -> Vec<String> {
    match input.get(field) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn input_checks(spec: &Value, input: &Value) -> Vec<Check> {
    let mut out = Vec::new();
    let Some(spec) = spec.as_object() else {
        return out;
    };
    for (key, v) in spec {
        let name = format!("input.{key}");
        match key.as_str() {
            "success_criteria_count" => {
                if let Some(r) = range(v) {
                    let n = entries(input, "success_criteria").len();
                    out.push(Check::of(&name, in_range(n, r), || {
                        format!("{n} criteria, expected {r:?}")
                    }));
                }
            }
            "fields_any_match" => {
                for (field, pattern) in v.as_object().into_iter().flatten() {
                    let Some(pattern) = pattern.as_str() else {
                        continue;
                    };
                    let items = entries(input, field);
                    let ok = match regex(pattern) {
                        Ok(re) => items.iter().any(|i| re.is_match(i)),
                        Err(e) => {
                            out.push(Check::of(&name, false, || e));
                            continue;
                        }
                    };
                    out.push(Check::of(&format!("{name}.{field}"), ok, || {
                        format!("no entry matches `{pattern}`: {items:?}")
                    }));
                }
            }
            "fields_must_match" | "fields_must_not_match" => {
                let must = key == "fields_must_match";
                for (field, pattern) in v.as_object().into_iter().flatten() {
                    let Some(pattern) = pattern.as_str() else {
                        continue;
                    };
                    let items = entries(input, field);
                    let re = match regex(pattern) {
                        Ok(re) => re,
                        Err(e) => {
                            out.push(Check::of(&name, false, || e));
                            continue;
                        }
                    };
                    let bad: Vec<_> = items.iter().filter(|i| re.is_match(i) != must).collect();
                    let empty_must = must && items.is_empty();
                    out.push(Check::of(
                        &format!("{name}.{field}"),
                        bad.is_empty() && !empty_must,
                        || {
                            if empty_must {
                                "the field is empty".to_owned()
                            } else if must {
                                format!("does not match `{pattern}`: {bad:?}")
                            } else {
                                format!("matches `{pattern}`: {bad:?}")
                            }
                        },
                    ));
                }
            }
            "preference_scope" => {
                let got = input
                    .get("scope")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                out.push(Check::of(&name, Some(got) == v.as_str(), || {
                    format!("scope `{got}`")
                }));
            }
            _ => out.push(Check::skipped(&name, "not computed by this runner")),
        }
    }
    out
}

/// `defaults.tool_input` of the suite, applied to every tool call of the turn.
fn tool_input_defaults(
    spec: &Map<String, Value>,
    calls: &[(String, Value)],
    errors: usize,
) -> Vec<Check> {
    let mut out = Vec::new();
    for (key, v) in spec {
        let name = format!("tool_input.{key}");
        let mission_calls = || {
            calls
                .iter()
                .filter(|(n, _)| MISSION_TOOLS.contains(&n.as_str()))
        };
        match key.as_str() {
            "criteria_must_not_match" => {
                let pats: Vec<String> = crate::checks::patterns(v)
                    .iter()
                    .map(|p| format!("(?i){p}"))
                    .collect();
                for (_, input) in mission_calls() {
                    for c in entries(input, "success_criteria") {
                        if let Ok(Some(m)) = crate::checks::first_match(&pats, &c) {
                            out.push(Check::of(&name, false, || {
                                format!("vague criterion `{c}` ({m})")
                            }));
                        }
                    }
                }
            }
            "why_min_words" => {
                let min = v.as_u64().unwrap_or(0);
                for (_, input) in mission_calls() {
                    let n = words(input.get("why").and_then(Value::as_str).unwrap_or_default());
                    out.push(Check::of(&name, n as u64 >= min, || {
                        format!("`why` has {n} words, min {min}")
                    }));
                }
            }
            "preference_text_must_not_match" => {
                if let Some(p) = v.as_str() {
                    for (_, input) in calls.iter().filter(|(n, _)| n == "save_preference") {
                        let text = input
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        out.push(crate::checks::must_not_match(&name, &json!(p), text));
                    }
                }
            }
            "fields_within_schema_limits" => {
                out.push(Check::of(&name, errors == 0, || {
                    format!("{errors} tool input(s) refused by the real validation")
                }));
            }
            _ => out.push(Check::skipped(&name, "not computed by this runner")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::Status;

    #[test]
    fn five_word_windows_of_tool_fields_are_detected() {
        let input = json!({"why": "Mantenere il servizio di pagamenti in Rust"});
        let user = "Voglio cambiare lavoro.";
        assert!(repeated_window("Ok: mantenere il servizio di pagamenti.", user, &input).is_some());
        assert!(repeated_window("Il servizio di pagamenti che usi.", user, &input).is_none());
        assert!(repeated_window("Ti riassumo cosa ho capito.", user, &input).is_none());
    }

    #[test]
    fn words_the_person_wrote_are_not_copying() {
        let input = json!({"success_criteria": ["Scrivo un driver I2C in Rust senza std"]});
        let user = "Voglio scrivere driver I2C in Rust senza std.";
        assert!(
            repeated_window("Passiamo ai driver I2C in Rust senza std.", user, &input).is_none()
        );
        assert!(repeated_window("Scrivo un driver I2C in Rust: ecco.", "Sì.", &input).is_some());
    }

    #[test]
    fn any_match_needs_one_entry() {
        let calls = |c: Value| vec![("save_mission_draft".to_owned(), json!({"constraints": c}))];
        let expect =
            json!({"input_checks": {"fields_any_match": {"constraints": "(?i)tre settimane"}}});
        let pass = |c| {
            tool_checks(&expect, &calls(c))
                .iter()
                .filter(|c| c.name == "input.fields_any_match.constraints")
                .all(|c| c.status == Status::Pass)
        };
        assert!(pass(json!(["Mezz'ora al giorno", "Entro tre settimane"])));
        assert!(!pass(json!(["Mezz'ora al giorno"])));
        assert!(!pass(json!([])));
    }

    #[test]
    fn tool_expectations() {
        let calls = vec![(
            "save_mission_draft".to_owned(),
            json!({"success_criteria": ["Leggo axum", "Scrivo test"], "out_of_scope": ["Kubernetes"]}),
        )];
        let expect = json!({"called": ["save_mission_draft"], "not_called": ["save_preference"], "max_calls": 1,
            "input_checks": {"success_criteria_count": [1, 4], "fields_must_match": {"out_of_scope": "(?i)kubernetes"},
                             "fields_must_not_match": {"success_criteria": "(?i)tokio"}}});
        assert!(
            tool_checks(&expect, &calls)
                .iter()
                .all(|c| c.status == Status::Pass)
        );
        let wrong = json!({"called": ["save_preference"], "max_calls": 0});
        assert!(
            tool_checks(&wrong, &calls)
                .iter()
                .all(|c| c.status == Status::Fail)
        );
    }

    #[test]
    fn plain_chat_cases_use_the_last_message_as_the_turn() {
        let case = Case {
            id: "x".into(),
            raw: json!({"id": "x", "messages": [{"role": "user", "text": "a"}, {"role": "assistant", "text": "b"}, {"role": "user", "text": "c"}]}),
        };
        let (history, steps) = plan(&case);
        assert_eq!(history.len(), 2);
        assert!(matches!(&steps[0], Step::User { text, .. } if text == "c"));
    }

    #[test]
    fn vague_criteria_and_short_whys_are_caught() {
        let defaults =
            json!({"criteria_must_not_match": ["^(capire|imparare)\\b"], "why_min_words": 6})
                .as_object()
                .unwrap()
                .clone();
        let calls = vec![(
            "save_mission_draft".to_owned(),
            json!({"why": "Per lavoro", "success_criteria": ["Capire Rust"]}),
        )];
        let checks = tool_input_defaults(&defaults, &calls, 0);
        assert!(
            checks.iter().all(|c| c.status == Status::Fail),
            "{checks:?}"
        );
    }
}
