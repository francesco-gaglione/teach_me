//! Researcher: the real `RunResearch` job, with the real web search.

use domain::UserId;
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::anyhow;
use application::ResearcherPrompts;
use application::research_output::{SeenUrls, parse_and_validate};
use application::structured::{OutputSchema, check_schema, parse_object};
use domain::JobKind;
use domain::llm::LlmRole;
use serde_json::{Map, Value, json};

use super::{Env, job_failure, subject_calls, wait_for_job};
use crate::checks::{Check, in_range, patterns, range, regex, run_spec, words};
use crate::report::{RoleRun, TurnReport};
use crate::seed::new_app;
use crate::suite::{Case, Suite, criteria};

pub async fn run(env: &Env, suite: &Suite, case: &Case) -> anyhow::Result<RoleRun> {
    let title = case
        .get("topic")
        .and_then(|t| t.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Argomento");
    let app = new_app(
        env.llm.clone(),
        env.tuning,
        title,
        "researching",
        case.content_language(),
    )
    .await?;
    let mission = case.get("mission").cloned().unwrap_or(Value::Null);
    app.mission("confirmed", &mission).await?;
    app.preferences(&case.array("preferences")).await?;
    let existing = case.array("existing_resources");
    app.resources(&existing).await?;

    let runner = Arc::new(
        app.state
            .job_runner(vec![(JobKind::Research, app.state.research_handler())]),
    );
    let task = tokio::spawn(runner.run());
    let mark = env.ledger.mark();
    app.state
        .jobs
        .enqueue(UserId::OWNER, app.topic, JobKind::Research)
        .await
        .map_err(|e| anyhow!("{e:?}"))?;
    let job = wait_for_job(
        &app,
        JobKind::Research,
        None,
        std::time::Duration::from_secs(30 * 60),
    )
    .await;
    task.abort();
    let job = job?;
    let calls = subject_calls(env, mark, LlmRole::Researcher);
    let last = calls.last();
    let raw = last.map_or_default(|c| c.text.clone());
    let searches: u32 = calls.iter().map(|c| c.usage.web_searches).sum();
    let mut seen: HashSet<String> = calls
        .iter()
        .flat_map(|c| c.search_urls.iter().cloned())
        .collect();
    seen.extend(
        existing
            .iter()
            .filter_map(|r| r.get("url").and_then(Value::as_str).map(str::to_owned)),
    );
    let queries: Vec<String> = calls
        .iter()
        .flat_map(|c| c.blocks.iter())
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("server_tool_use"))
        .filter_map(|b| b.pointer("/input/query").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let mut sorted_urls: Vec<String> = seen.iter().cloned().collect();
    sorted_urls.sort();
    let budget = suite
        .default_u32("search_budget")
        .unwrap_or(env.tuning.max_searches)
        .min(env.tuning.max_searches.max(1));
    let prompts = ResearcherPrompts::embedded();
    let schema = OutputSchema::parse(prompts.output_schema, "research").ok();
    let output = parse_object(&raw).ok();

    let mut checks = Vec::new();
    if job.status != domain::JobStatus::Succeeded {
        checks.push(Check::of("job_succeeded", false, || job_failure(&job)));
    }
    checks.push({
        let mut urls = SeenUrls::default();
        for u in &seen {
            urls.add(u);
        }
        let verdict = parse_and_validate(&raw, &urls).err().unwrap_or_default();
        Check::of("backend_validator", verdict.is_empty(), || {
            verdict.join(" | ")
        })
    });
    let spec = suite.deterministic(case);
    let ctx = Ctx {
        output: output.as_ref(),
        schema: schema.as_ref().map(|s| &s.schema),
        seen: &seen,
        existing: &existing,
        searches,
        budget,
        search_available: !seen.is_empty(),
        language: case.content_language(),
    };
    checks.extend(researcher_checks(&spec, &ctx));

    let material = format!(
        "Topic: {title}\nMission: {mission}\nPreferences: {}\nExisting sources (saved before this research): {}\n\nResearcher output:\n{raw}",
        Value::Array(case.array("preferences")),
        Value::Array(existing.clone()),
    );
    let judge = env
        .judge
        .judge(&material, &criteria(case.get("rubric")))
        .await;
    let mut run = RoleRun {
        turns: vec![TurnReport {
            label: "research".into(),
            input: String::new(),
            output: raw,
            checks,
            judge,
            extras: json!({"web_searches": searches, "search_result_urls": seen.len(), "model_calls": calls.len(), "queries": queries, "urls": sorted_urls}),
        }],
        ..RoleRun::default()
    };
    run.metrics
        .insert("web_searches".into(), f64::from(searches));
    Ok(run)
}

pub struct Ctx<'a> {
    pub output: Option<&'a Value>,
    pub schema: Option<&'a Value>,
    /// URLs of the search results and of the existing sources.
    pub seen: &'a HashSet<String>,
    pub existing: &'a [Value],
    pub searches: u32,
    pub budget: u32,
    /// Whether the provider returned search results at all.
    pub search_available: bool,
    /// Language of the case's topic: what the annotations must be written in.
    pub language: domain::ContentLanguage,
}

fn items(out: &Value, key: &str) -> Vec<Value> {
    out.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or_default()
}

fn norm_url(u: &str) -> String {
    u.trim()
        .trim_end_matches('/')
        .replace("://www.", "://")
        .to_lowercase()
}

/// Every key of the Researcher suites, on the output JSON.
pub fn researcher_checks(spec: &Map<String, Value>, ctx: &Ctx<'_>) -> Vec<Check> {
    let Some(out) = ctx.output else {
        return vec![Check::of("output_is_json", false, || {
            "the final text has no JSON object".into()
        })];
    };
    let resources = items(out, "resources");
    let gaps = items(out, "gaps");
    let of_kind = |k: &str| resources.iter().filter(|r| s(r, "kind") == k).count();
    let communities_skipped = out
        .get("communities_skipped")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    run_spec(spec, "", |key, v| {
        let each = |field: &str, f: &dyn Fn(&str) -> Option<String>| -> Check {
            let bad: Vec<String> = resources.iter().filter_map(|r| f(s(r, field))).collect();
            Check::of(key, bad.is_empty(), || bad.join(" | "))
        };
        Some(match key {
            "schema_valid" => {
                let p = ctx.schema.map_or_default(|sc| check_schema(out, sc));
                Check::of(key, p.is_empty(), || p.join("; "))
            }
            "resources_count" => {
                let r = range(v)?;
                Check::of(key, in_range(resources.len(), r), || {
                    format!("{} resources, expected {r:?}", resources.len())
                })
            }
            "min_per_kind" | "max_per_kind" => {
                let min = key == "min_per_kind";
                let bad: Vec<String> = v
                    .as_object()?
                    .iter()
                    .filter_map(|(k, n)| {
                        let n = n.as_u64()?;
                        let have = of_kind(k) as u64;
                        ((min && have < n) || (!min && have > n))
                            .then(|| format!("{k}: {have}, limit {n}"))
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || bad.join("; "))
            }
            "urls_must_be_https" => each("url", &|u| {
                (!u.is_empty() && !u.starts_with("https://")).then(|| u.to_owned())
            }),
            "no_duplicate_urls" => {
                let mut seen = HashSet::new();
                let dup: Vec<String> = resources
                    .iter()
                    .map(|r| norm_url(s(r, "url")))
                    .filter(|u| !u.is_empty() && !seen.insert(u.clone()))
                    .collect();
                Check::of(key, dup.is_empty(), || dup.join(", "))
            }
            "urls_must_be_in_search_results" => {
                if !ctx.search_available {
                    return Some(Check::skipped(
                        key,
                        "the provider returned no search results to compare with",
                    ));
                }
                let bad: Vec<String> = resources
                    .iter()
                    .map(|r| s(r, "url"))
                    .filter(|u| {
                        !u.is_empty() && !ctx.seen.iter().any(|x| norm_url(x) == norm_url(u))
                    })
                    .map(str::to_owned)
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("not seen in the search results: {bad:?}")
                })
            }
            "max_web_searches" => Check::of(key, ctx.searches <= ctx.budget, || {
                format!("{} searches, budget {}", ctx.searches, ctx.budget)
            }),
            "annotation_language" => {
                // On by any value but `false`: the language is the case's.
                if v.as_bool() == Some(false) {
                    return None;
                }
                each("annotation", &|t| {
                    (!crate::checks::looks_like(ctx.language, t))
                        .then(|| t.chars().take(60).collect())
                })
            }
            "annotation_words" | "use_for_words" => {
                let r = range(v)?;
                let field = if key == "annotation_words" {
                    "annotation"
                } else {
                    "use_for"
                };
                each(field, &|t| {
                    (!in_range(words(t), r)).then(|| {
                        format!(
                            "{} words in `{}`",
                            words(t),
                            t.chars().take(40).collect::<String>()
                        )
                    })
                })
            }
            "annotation_must_not_match" | "use_for_must_not_match" | "gap_note_must_not_match" => {
                let pats = patterns(v);
                let (field, rows): (&str, Vec<&Value>) = match key {
                    "annotation_must_not_match" => ("annotation", resources.iter().collect()),
                    "use_for_must_not_match" => ("use_for", resources.iter().collect()),
                    _ => ("note", gaps.iter().collect()),
                };
                let bad: Vec<String> = rows
                    .iter()
                    .filter_map(|r| {
                        crate::checks::first_match(&pats, s(r, field))
                            .ok()
                            .flatten()
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || format!("matched {bad:?}"))
            }
            "community_url_must_not_match" => {
                let pats = patterns(v);
                let bad: Vec<String> = resources
                    .iter()
                    .filter(|r| s(r, "kind") == "community")
                    .filter_map(|r| {
                        crate::checks::first_match(&pats, s(r, "url"))
                            .ok()
                            .flatten()
                            .map(|m| format!("{} ({m})", s(r, "url")))
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || bad.join(", "))
            }
            "communities_skipped" => {
                Check::of(key, Some(communities_skipped) == v.as_bool(), || {
                    format!("communities_skipped is {communities_skipped}")
                })
            }
            "no_community_when_skipped" => Check::of(
                key,
                !(v.as_bool().unwrap_or(true) && communities_skipped && of_kind("community") > 0),
                || "communities_skipped is true but a community was delivered".into(),
            ),
            "knowledge_before_community" => {
                let first_community = resources.iter().position(|r| s(r, "kind") == "community");
                let last_knowledge = resources.iter().rposition(|r| s(r, "kind") == "knowledge");
                Check::of(
                    key,
                    match (first_community, last_knowledge) {
                        (Some(c), Some(k)) => k < c,
                        _ => true,
                    },
                    || "a community comes before a knowledge source".into(),
                )
            }
            "gap_note_chars" => {
                let r = range(v)?;
                let bad: Vec<usize> = gaps
                    .iter()
                    .map(|g| s(g, "note").trim().chars().count())
                    .filter(|n| !in_range(*n, r))
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("note lengths {bad:?}, expected {r:?}")
                })
            }
            "gap_note_max_words" => {
                let max = v.as_u64()?;
                let bad: Vec<usize> = gaps
                    .iter()
                    .map(|g| words(s(g, "note")))
                    .filter(|n| *n as u64 > max)
                    .collect();
                Check::of(key, bad.is_empty(), || {
                    format!("notes of {bad:?} words, max {max}")
                })
            }
            "must_include_url_matching" | "must_not_include_url_matching" => {
                let must = key == "must_include_url_matching";
                let mut failures = Vec::new();
                for p in patterns(v) {
                    match regex(&p) {
                        Ok(re) => {
                            let any = resources.iter().any(|r| re.is_match(s(r, "url")));
                            if any != must {
                                failures.push(p);
                            }
                        }
                        Err(e) => failures.push(e),
                    }
                }
                Check::of(key, failures.is_empty(), || format!("{failures:?}"))
            }
            // The kept sources must come back with their own URL: same title,
            // different URL means the research swapped it for a mirror.
            "existing_urls_kept_must_be_identical" | "no_equivalent_mirror_of_existing" => {
                let bad: Vec<String> = ctx
                    .existing
                    .iter()
                    .filter_map(|e| {
                        let title = s(e, "title").to_lowercase();
                        let url = norm_url(s(e, "url"));
                        resources
                            .iter()
                            .find(|r| {
                                s(r, "title").to_lowercase() == title
                                    && norm_url(s(r, "url")) != url
                            })
                            .map(|r| format!("`{}` came back as {}", s(e, "title"), s(r, "url")))
                    })
                    .collect();
                Check::of(key, bad.is_empty(), || bad.join("; "))
            }
            _ => return None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{Status, status_of};

    fn run(spec: &Value, out: &Value, seen: &[&str], existing: &[Value]) -> Vec<Check> {
        let seen: HashSet<String> = seen.iter().map(|s| (*s).to_owned()).collect();
        let ctx = Ctx {
            output: Some(out),
            schema: None,
            seen: &seen,
            existing,
            searches: 2,
            budget: 4,
            search_available: true,
            language: domain::ContentLanguage::It,
        };
        researcher_checks(spec.as_object().unwrap(), &ctx)
    }

    fn res(kind: &str, url: &str) -> Value {
        json!({"kind": kind, "title": "T", "url": url, "annotation": "Fonte ufficiale con esempi chiari sul tema.", "use_for": "capire le basi"})
    }

    #[test]
    fn urls_must_come_from_the_search_results() {
        let out = json!({"resources": [res("knowledge", "https://a.org/x"), res("knowledge", "https://invented.org")], "gaps": [], "communities_skipped": false});
        let c = run(
            &json!({"urls_must_be_in_search_results": true, "urls_must_be_https": true}),
            &out,
            &["https://a.org/x/"],
            &[],
        );
        assert_eq!(
            status_of(&c, "urls_must_be_in_search_results"),
            Status::Fail
        );
        assert!(c.iter().any(|c| c.detail.contains("invented")));
        assert_eq!(status_of(&c, "urls_must_be_https"), Status::Pass);
    }

    #[test]
    fn annotation_language_follows_the_language_of_the_case() {
        let out = json!({"resources": [res("knowledge", "https://a.org")], "gaps": [], "communities_skipped": false});
        let english = json!({"resources": [{"kind": "knowledge", "title": "T", "url": "https://a.org",
            "annotation": "What you can do with it and how the examples are written.", "use_for": "basics"}],
            "gaps": [], "communities_skipped": false});
        let check = |language, out: &Value| {
            let seen = HashSet::new();
            let ctx = Ctx {
                output: Some(out),
                schema: None,
                seen: &seen,
                existing: &[],
                searches: 0,
                budget: 4,
                search_available: true,
                language,
            };
            let spec = json!({"annotation_language": "it"});
            status_of(
                &researcher_checks(spec.as_object().unwrap(), &ctx),
                "annotation_language",
            )
        };
        use domain::ContentLanguage::{En, It};
        assert_eq!(check(It, &out), Status::Pass);
        assert_eq!(check(It, &english), Status::Fail);
        assert_eq!(check(En, &english), Status::Pass);
        assert_eq!(check(En, &out), Status::Fail);
    }

    #[test]
    fn kinds_counts_and_opt_out() {
        let out = json!({"resources": [res("knowledge", "https://a.org"), res("community", "https://b.org/forum")], "gaps": [], "communities_skipped": true});
        let c = run(
            &json!({"min_per_kind": {"knowledge": 1}, "max_per_kind": {"community": 0}, "no_community_when_skipped": true, "communities_skipped": true}),
            &out,
            &[],
            &[],
        );
        assert_eq!(status_of(&c, "min_per_kind"), Status::Pass);
        assert_eq!(status_of(&c, "max_per_kind"), Status::Fail);
        assert_eq!(status_of(&c, "no_community_when_skipped"), Status::Fail);
        assert_eq!(status_of(&c, "communities_skipped"), Status::Pass);
    }

    #[test]
    fn search_budget_and_mirrors() {
        let existing = vec![json!({"title": "Book", "url": "https://doc.rust-lang.org/book/"})];
        let mirror = json!({"resources": [{"kind": "knowledge", "title": "Book", "url": "https://mirror.org/book", "annotation": "x", "use_for": "y"}], "gaps": [], "communities_skipped": false});
        let c = run(
            &json!({"max_web_searches": "search_budget", "existing_urls_kept_must_be_identical": true}),
            &mirror,
            &[],
            &existing,
        );
        assert_eq!(status_of(&c, "max_web_searches"), Status::Pass);
        assert_eq!(
            status_of(&c, "existing_urls_kept_must_be_identical"),
            Status::Fail
        );
    }
}
