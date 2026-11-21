//! Running suites: one fresh world per run, a cost budget over all of them.

use std::collections::HashMap;
use std::sync::Arc;

use domain::llm::{LlmProvider, LlmRole};
use serde_json::{Value, json};

use crate::judge::{JUDGE_ROLE, Judge};
use crate::llm::{BUDGET_MESSAGE, InjectingLlm, Ledger, PlannedLlm, RecordingLlm, Source};
use crate::pricing::typical_cost;
use crate::report::{
    CallSummary, Report, RoleRun, RunReport, RunStatus, SkippedCase, SuiteReport, summarize_case,
    tally,
};
use crate::roles::{self, Env, prompt_version};
use crate::seed::Tuning;
use crate::suite::{Case, Role, Suite};

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// `--runs`: overrides every suite's `runs_per_case`.
    pub runs: Option<u32>,
    pub max_cost_usd: Option<f64>,
    /// Case ids to run (empty: all).
    pub cases: Vec<String>,
    pub tuning: Tuning,
}

/// The model each role is configured with, and the judge's.
#[derive(Debug, Clone)]
pub struct Models {
    pub roles: HashMap<LlmRole, String>,
    pub judge: String,
}

impl Models {
    fn of(&self, role: LlmRole) -> &str {
        self.roles
            .get(&role)
            .map_or("claude-opus-5-5", String::as_str)
    }
}

fn user_steps(case: &Case) -> usize {
    match case.get("steps").and_then(Value::as_array) {
        Some(steps) => steps.iter().filter(|s| s.get("user").is_some()).count(),
        None => 1,
    }
}

fn judged_steps(case: &Case) -> usize {
    match case.get("steps").and_then(Value::as_array) {
        Some(steps) => steps
            .iter()
            .filter(|s| s.get("expect").and_then(|e| e.get("rubric")).is_some())
            .count(),
        None => usize::from(case.get("rubric").is_some()),
    }
}

/// A typical cost of one run, for `--dry-run` and for the check before a run.
/// From `llm-port.md` §13.6 and `evals/README.md`: deliberately on the
/// cautious side, since it decides whether a run may start.
#[must_use]
pub fn estimate_run(role: Role, case: &Case, models: &Models, max_searches: u32) -> f64 {
    let judge = |n: usize, input: u32, output: u32| {
        n as f64 * typical_cost(&models.judge, input, output, 0)
    };
    match role {
        Role::Tutor => {
            let turns = user_steps(case) as f64;
            turns * typical_cost(models.of(LlmRole::Tutor), 3_500, 700, 0) * 1.3
                + judge(judged_steps(case), 3_500, 700)
        }
        Role::Researcher => {
            typical_cost(
                models.of(LlmRole::Researcher),
                22_000 * max_searches + 4_000,
                3_000,
                max_searches,
            ) + judge(1, 6_000, 900)
        }
        Role::Planner => {
            // Planner input is small; the Generator is refused, so only the plan and the judge.
            typical_cost(models.of(LlmRole::Planner), 4_000, 4_000, 0) * 1.3 + judge(1, 4_000, 700)
        }
        Role::Generator => {
            typical_cost(models.of(LlmRole::Generator), 10_000, 14_000, 0) * 1.4
                + judge(1, 9_000, 1_000)
        }
        Role::Grader => {
            typical_cost(models.of(LlmRole::Grader), 2_100, 500, 0) + judge(1, 2_000, 400)
        }
        Role::Recorder => {
            let open = case
                .array("attempts")
                .iter()
                .filter(|a| a.get("outcome").and_then(Value::as_str) == Some("pending"))
                .count();
            typical_cost(models.of(LlmRole::Recorder), 5_500, 3_500, 0) * 1.2
                + open as f64 * typical_cost(models.of(LlmRole::Grader), 2_100, 500, 0)
                + judge(1, 6_000, 900)
        }
    }
}

/// Why a whole suite is not run: it needs something this run lacks.
#[must_use]
pub fn suite_skip_reason(suite: &Suite, tuning: &Tuning) -> Option<String> {
    if suite.requires.web_fetch {
        return tuning
            .web_fetch_unavailable_reason()
            .map(|why| format!("requires the Generator's web fetch tool, but {why}"));
    }
    None
}

/// Why a case is not run, or `None`.
#[must_use]
pub fn case_skip_reason(suite: &Suite, case: &Case, tuning: &Tuning) -> Option<String> {
    let needs = suite.requires_for(case);
    if needs.web_fetch
        && let Some(why) = tuning.web_fetch_unavailable_reason()
    {
        return Some(format!(
            "requires the Generator's web fetch tool, but {why}"
        ));
    }
    None
}

/// Whether `--suite` named a suite the run cannot do: an error for the owner,
/// who asked for it by name, where a suite picked up by default is only skipped.
pub fn check_explicit_suites(
    suites: &[Suite],
    wanted: &[String],
    tuning: &Tuning,
) -> anyhow::Result<()> {
    for suite in suites.iter().filter(|s| wanted.contains(&s.id)) {
        if let Some(why) = suite_skip_reason(suite, tuning) {
            anyhow::bail!("suite `{}` cannot run: {why}", suite.id);
        }
    }
    Ok(())
}

/// The estimated cost of one run of `case`: the role's typical cost, plus the
/// pages the Generator's web fetch tool would read when the case runs with it.
#[must_use]
pub fn estimate_case(suite: &Suite, case: &Case, models: &Models, tuning: &Tuning) -> f64 {
    let base = estimate_run(suite.role, case, models, tuning.max_searches);
    let fetches = suite.requires_for(case).web_fetch
        && !suite.web_fetch_off(case)
        && tuning.web_fetch_available();
    if suite.role != Role::Generator || !fetches {
        return base;
    }
    let plan = case.get("plan");
    let sources = plan
        .and_then(|p| p.get("other_source_ids"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
        + usize::from(plan.and_then(|p| p.get("primary_source_id")).is_some());
    let pages = u32::try_from(sources.min(3))
        .unwrap_or(3)
        .min(tuning.max_fetches);
    // Up to 8000 tokens a page, read again by a repair in four runs of ten.
    base + typical_cost(models.of(LlmRole::Generator), pages * 8_000 * 14 / 10, 0, 0)
}

/// The `--dry-run` listing: nothing is called.
#[must_use]
pub fn dry_run(suites: &[Suite], models: &Models, opts: &RunOptions) -> String {
    use std::fmt::Write as _;
    let mut out =
        String::from("| suite | case | runs | est. $/run | est. $ |\n|---|---|---|---|---|\n");
    let mut total = 0.0;
    for s in suites {
        let runs = s.runs_per_case(opts.runs);
        if let Some(reason) = suite_skip_reason(s, &opts.tuning) {
            let _ = writeln!(out, "| {} | (suite skipped: {reason}) | - | - | - |", s.id);
            continue;
        }
        for c in s.cases.iter().filter(|c| wanted(opts, s, &c.id)) {
            if let Some(reason) = case_skip_reason(s, c, &opts.tuning) {
                let _ = writeln!(
                    out,
                    "| {} | {} (skipped: {reason}) | - | - | - |",
                    s.id, c.id
                );
                continue;
            }
            let one = estimate_case(s, c, models, &opts.tuning);
            total += one * f64::from(runs);
            let _ = writeln!(
                out,
                "| {} | {} | {runs} | {one:.4} | {:.4} |",
                s.id,
                c.id,
                one * f64::from(runs)
            );
        }
    }
    let _ = writeln!(
        out,
        "\nEstimated total: ${total:.4} (judge: `{}`)",
        models.judge
    );
    if let Some(max) = opts.max_cost_usd {
        let _ = writeln!(
            out,
            "Cost budget: ${max:.2}{}",
            if total > max {
                " (the estimate exceeds it: the run would stop early)"
            } else {
                ""
            }
        );
    }
    out
}

/// `--case` is applied per suite. `suite:id` names a case of one suite;
/// a bare `id` filters only the suites that have a case with that id. A suite
/// no `--case` points at runs whole.
#[must_use]
pub fn wanted(opts: &RunOptions, suite: &Suite, id: &str) -> bool {
    let mut pointed_at = false;
    let mut chosen = false;
    for entry in &opts.cases {
        match entry.split_once(':') {
            Some((s, case)) if s == suite.id => {
                pointed_at = true;
                chosen |= case == id;
            }
            Some(_) => {}
            None => {
                if suite.cases.iter().any(|c| c.id == *entry) {
                    pointed_at = true;
                    chosen |= entry == id;
                }
            }
        }
    }
    !pointed_at || chosen
}

/// Runs the suites against `subject` (the models under evaluation) and
/// `judge_llm`. Calls of both go through one ledger, so one budget covers all.
pub async fn run_suites(
    subject: Arc<dyn LlmProvider>,
    judge_llm: Arc<dyn LlmProvider>,
    models: &Models,
    suites: &[Suite],
    opts: &RunOptions,
    started_at: &str,
) -> Report {
    let ledger = Ledger::new(opts.max_cost_usd);
    let recording: Arc<dyn LlmProvider> = Arc::new(
        RecordingLlm::new(subject, ledger.clone(), Source::Subject)
            .with_models(models.roles.clone()),
    );
    let planned = Arc::new(PlannedLlm::new(recording));
    // Above the recorder: the recorded request (and its price) is what the
    // provider receives, the simulated read included.
    let injector = Arc::new(InjectingLlm::new(planned.clone()));
    let judge_recording: Arc<dyn LlmProvider> = Arc::new(
        RecordingLlm::new(judge_llm, ledger.clone(), Source::Judge)
            .with_models(HashMap::from([(JUDGE_ROLE, models.judge.clone())])),
    );
    let env = Env {
        llm: injector.clone(),
        planned,
        injector,
        judge: Arc::new(Judge::new(judge_recording)),
        ledger: ledger.clone(),
        tuning: opts.tuning,
    };
    let mut reports = Vec::new();
    let mut stopped = false;
    for suite in suites {
        let before = ledger.spent();
        let mut cases = Vec::new();
        let mut not_run = Vec::new();
        let mut skipped = Vec::new();
        if let Some(reason) = suite_skip_reason(suite, &opts.tuning) {
            reports.push(SuiteReport {
                id: suite.id.clone(),
                prompt_version: prompt_version(suite.role),
                cases,
                pass_criteria: suite.pass_criteria.clone(),
                cost_usd: 0.0,
                cases_not_run: not_run,
                skipped: Some(reason),
                cases_skipped: skipped,
            });
            continue;
        }
        for case in suite.cases.iter().filter(|c| wanted(opts, suite, &c.id)) {
            if let Some(reason) = case_skip_reason(suite, case, &opts.tuning) {
                skipped.push(SkippedCase {
                    id: case.id.clone(),
                    reason,
                });
                continue;
            }
            if stopped {
                not_run.push(case.id.clone());
                continue;
            }
            let mut runs = Vec::new();
            for n in 1..=suite.runs_per_case(opts.runs) {
                let estimate = estimate_case(suite, case, models, &opts.tuning);
                if let Some(max) = ledger.max_cost()
                    && ledger.spent() + estimate > max
                {
                    stopped = true;
                    break;
                }
                let refused = ledger.refused();
                let run = one_run(&env, suite, case, n).await;
                if ledger.refused() > refused {
                    stopped = true;
                    break;
                }
                runs.push(run);
                if ledger.exhausted() {
                    stopped = true;
                    break;
                }
            }
            if runs.is_empty() {
                not_run.push(case.id.clone());
            } else {
                cases.push(summarize_case(&case.id, runs));
            }
        }
        reports.push(SuiteReport {
            id: suite.id.clone(),
            prompt_version: prompt_version(suite.role),
            cases,
            pass_criteria: suite.pass_criteria.clone(),
            cost_usd: ledger.spent() - before,
            cases_not_run: not_run,
            skipped: None,
            cases_skipped: skipped,
        });
    }
    Report {
        started_at: started_at.to_owned(),
        options: json!({
            "suites": suites.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            "runs": opts.runs, "max_cost_usd": opts.max_cost_usd, "judge_model": models.judge,
            "cases": opts.cases, "max_searches": opts.tuning.max_searches,
            "max_fetches": opts.tuning.max_fetches,
            "web_fetch_capable": opts.tuning.web_fetch_capable,
        }),
        suites: reports,
        total_cost_usd: ledger.spent(),
        budget_exhausted: stopped,
    }
}

async fn one_run(env: &Env, suite: &Suite, case: &Case, run: u32) -> RunReport {
    let mark = env.ledger.mark();
    let result = roles::run(env, suite, case).await;
    let calls = env.ledger.since(mark);
    let over_budget = calls.iter().any(|c| {
        c.error
            .as_deref()
            .is_some_and(|e| e.contains(BUDGET_MESSAGE))
    });
    let (role_run, status, error) = match result {
        Ok(r) if over_budget => (r, RunStatus::BudgetStopped, Some(BUDGET_MESSAGE.to_owned())),
        Ok(r) => (r, RunStatus::Completed, None),
        Err(e) if over_budget || env.ledger.exhausted() => (
            RoleRun::default(),
            RunStatus::BudgetStopped,
            Some(format!("{BUDGET_MESSAGE}: {e:#}")),
        ),
        Err(e) => (
            RoleRun::default(),
            RunStatus::Failed,
            Some(format!("{e:#}")),
        ),
    };
    let (failed, warned, skipped, score) = tally(&role_run.turns);
    let subject: Vec<_> = calls
        .iter()
        .filter(|c| c.source == Source::Subject)
        .collect();
    let mut models: Vec<String> = subject
        .iter()
        .map(|c| c.model.clone())
        .filter(|m| !m.is_empty())
        .collect();
    models.sort();
    models.dedup();
    RunReport {
        run,
        status,
        error,
        turns: role_run.turns,
        extras: role_run.extras,
        metrics: role_run.metrics,
        calls: calls.iter().map(CallSummary::from).collect(),
        models,
        prompt_version: prompt_version(suite.role),
        cost_usd: subject.iter().map(|c| c.cost_usd).sum(),
        judge_cost_usd: calls
            .iter()
            .filter(|c| c.source == Source::Judge)
            .map(|c| c.cost_usd)
            .sum(),
        latency_ms: subject
            .iter()
            .map(|c| u64::try_from(c.latency.as_millis()).unwrap_or(0))
            .sum(),
        checks_failed: failed,
        checks_warned: warned,
        checks_skipped: skipped,
        rubric_score: score,
    }
}

#[must_use]
pub fn judge_role() -> LlmRole {
    JUDGE_ROLE
}
