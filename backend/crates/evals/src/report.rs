//! What a run leaves behind: JSON for machines, markdown for the owner.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;
use serde_json::Value;

use crate::checks::{Check, Status};
use crate::judge::JudgeReport;
use crate::llm::{CallRecord, Source};

#[derive(Debug, Clone, Serialize)]
pub struct CallSummary {
    pub source: Source,
    pub role: &'static str,
    pub label: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub thinking_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    pub web_searches: u32,
    pub web_fetches: u32,
    pub latency_ms: u64,
    pub cost_usd: f64,
    pub price_assumed: bool,
    pub stop_reason: String,
    pub error: Option<String>,
}

impl From<&CallRecord> for CallSummary {
    fn from(c: &CallRecord) -> Self {
        Self {
            source: c.source,
            role: c.role.as_str(),
            label: c.label.clone(),
            model: c.model.clone(),
            input_tokens: c.usage.input_tokens,
            output_tokens: c.usage.output_tokens,
            thinking_tokens: c.usage.thinking_tokens,
            cache_read_tokens: c.usage.cache_read_tokens,
            cache_write_tokens: c.usage.cache_write_tokens,
            web_searches: c.usage.web_searches,
            web_fetches: c.usage.web_fetches,
            latency_ms: u64::try_from(c.latency.as_millis()).unwrap_or(u64::MAX),
            cost_usd: c.cost_usd,
            price_assumed: c.price_assumed,
            stop_reason: c.stop_reason.clone(),
            error: c.error.clone(),
        }
    }
}

/// One scored unit of a run: a Tutor turn, or the whole output of another role.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TurnReport {
    pub label: String,
    pub input: String,
    pub output: String,
    pub checks: Vec<Check>,
    pub judge: Option<JudgeReport>,
    /// Role-specific facts: tool calls, validator codes, repairs...
    pub extras: Value,
}

/// What a role's run produces, before the runner adds usage and cost.
#[derive(Debug, Clone, Default)]
pub struct RoleRun {
    pub turns: Vec<TurnReport>,
    pub extras: Value,
    /// Numbers to aggregate per case (median and max in the summary).
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Completed,
    /// The use case or the provider failed.
    Failed,
    /// The cost budget stopped the run.
    BudgetStopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub run: u32,
    pub status: RunStatus,
    pub error: Option<String>,
    pub turns: Vec<TurnReport>,
    pub extras: Value,
    pub metrics: BTreeMap<String, f64>,
    pub calls: Vec<CallSummary>,
    pub models: Vec<String>,
    pub prompt_version: String,
    pub cost_usd: f64,
    pub judge_cost_usd: f64,
    pub latency_ms: u64,
    pub checks_failed: usize,
    pub checks_warned: usize,
    pub checks_skipped: usize,
    /// Weighted share of the criteria met, over every judged turn.
    pub rubric_score: Option<f64>,
}

impl RunReport {
    #[must_use]
    pub fn checks_ok(&self) -> bool {
        self.status == RunStatus::Completed && self.checks_failed == 0
    }
}

pub fn tally(turns: &[TurnReport]) -> (usize, usize, usize, Option<f64>) {
    let mut failed = 0;
    let mut warned = 0;
    let mut skipped = 0;
    let (mut met, mut total) = (0.0, 0.0);
    for t in turns {
        for c in &t.checks {
            match c.status {
                Status::Fail => failed += 1,
                Status::Warn => warned += 1,
                Status::Skipped => skipped += 1,
                Status::Pass => {}
            }
        }
        if let Some(j) = &t.judge {
            for c in &j.criteria {
                total += c.weight;
                if c.met {
                    met += c.weight;
                }
            }
        }
    }
    (failed, warned, skipped, (total > 0.0).then(|| met / total))
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseReport {
    pub id: String,
    pub runs: Vec<RunReport>,
    pub runs_checks_ok: usize,
    pub rubric_mean: Option<f64>,
    pub rubric_min: Option<f64>,
    /// Per metric: (median, max) over the runs.
    pub metrics: BTreeMap<String, (f64, f64)>,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuiteReport {
    pub id: String,
    pub prompt_version: String,
    pub cases: Vec<CaseReport>,
    pub pass_criteria: Value,
    pub cost_usd: f64,
    /// Cases not started because the budget ran out.
    pub cases_not_run: Vec<String>,
    /// Why the whole suite was skipped (it needs something this run does not
    /// have), if it was.
    pub skipped: Option<String>,
    /// Cases skipped for the same reason, with it.
    pub cases_skipped: Vec<SkippedCase>,
}

/// A case that was not run because the run lacks what it requires.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedCase {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub started_at: String,
    pub options: Value,
    pub suites: Vec<SuiteReport>,
    pub total_cost_usd: f64,
    pub budget_exhausted: bool,
}

#[must_use]
pub fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    match values.len() {
        0 => 0.0,
        n if n % 2 == 1 => values[n / 2],
        n => f64::midpoint(values[n / 2 - 1], values[n / 2]),
    }
}

#[must_use]
pub fn summarize_case(id: &str, runs: Vec<RunReport>) -> CaseReport {
    let scores: Vec<f64> = runs.iter().filter_map(|r| r.rubric_score).collect();
    #[allow(clippy::cast_precision_loss)]
    let mean = (!scores.is_empty()).then(|| scores.iter().sum::<f64>() / scores.len() as f64);
    let min = scores.iter().copied().reduce(f64::min);
    let mut by_metric: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for r in &runs {
        for (k, v) in &r.metrics {
            by_metric.entry(k.clone()).or_default().push(*v);
        }
    }
    let metrics = by_metric
        .into_iter()
        .map(|(k, mut v)| {
            let max = v.iter().copied().fold(f64::MIN, f64::max);
            (k, (median(&mut v), max))
        })
        .collect();
    CaseReport {
        id: id.to_owned(),
        runs_checks_ok: runs.iter().filter(|r| r.checks_ok()).count(),
        rubric_mean: mean,
        rubric_min: min,
        metrics,
        cost_usd: runs.iter().map(|r| r.cost_usd + r.judge_cost_usd).sum(),
        runs,
    }
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_owned(), |v| format!("{:.2}", v))
}

/// The summary the owner reads: one table per suite, then every failure.
#[must_use]
pub fn markdown(report: &Report) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Eval run {}\n", report.started_at);
    let _ = writeln!(
        out,
        "Total cost: **${:.4}**{}\n",
        report.total_cost_usd,
        if report.budget_exhausted {
            " (stopped by the cost budget)"
        } else {
            ""
        }
    );
    let _ = writeln!(out, "Options: `{}`\n", report.options);
    for s in &report.suites {
        let _ = writeln!(
            out,
            "## {} (prompt `{}`) - ${:.4}\n",
            s.id,
            s.prompt_version.trim(),
            s.cost_usd
        );
        if let Some(reason) = &s.skipped {
            let _ = writeln!(out, "**Skipped:** {reason}\n");
            continue;
        }
        let _ = writeln!(
            out,
            "| case | runs | checks ok | rubric mean | rubric min | cost $ | notes |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|");
        for c in &s.cases {
            let notes: Vec<String> = c
                .metrics
                .iter()
                .map(|(k, (m, x))| format!("{k}: median {m:.0}, max {x:.0}"))
                .collect();
            let _ = writeln!(
                out,
                "| {} | {} | {}/{} | {} | {} | {:.4} | {} |",
                c.id,
                c.runs.len(),
                c.runs_checks_ok,
                c.runs.len(),
                pct(c.rubric_mean),
                pct(c.rubric_min),
                c.cost_usd,
                notes.join("; ")
            );
        }
        for c in &s.cases_skipped {
            let _ = writeln!(out, "\nSkipped `{}`: {}", c.id, c.reason);
        }
        if !s.cases_not_run.is_empty() {
            let _ = writeln!(out, "\nNot run (budget): {}", s.cases_not_run.join(", "));
        }
        let _ = writeln!(
            out,
            "\nPass criteria of the suite (to read by hand): `{}`\n",
            s.pass_criteria
        );
        for c in &s.cases {
            for r in &c.runs {
                let problems: Vec<String> = r
                    .turns
                    .iter()
                    .flat_map(|t| {
                        let label = t.label.clone();
                        t.checks
                            .iter()
                            .filter(|k| matches!(k.status, Status::Fail | Status::Warn))
                            .map(move |k| {
                                format!("[{label}] {:?} {}: {}", k.status, k.name, k.detail)
                            })
                    })
                    .collect();
                let unmet: Vec<String> = r
                    .turns
                    .iter()
                    .filter_map(|t| t.judge.as_ref().map(|j| (t.label.clone(), j)))
                    .flat_map(|(label, j)| {
                        j.criteria
                            .iter()
                            .filter(|k| !k.met)
                            .map(move |k| format!("[{label}] rubric `{}`: {}", k.id, k.reason))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                if r.status != RunStatus::Completed || !problems.is_empty() || !unmet.is_empty() {
                    let _ = writeln!(
                        out,
                        "### {} / {} run {} ({:?})\n",
                        s.id, c.id, r.run, r.status
                    );
                    if let Some(e) = &r.error {
                        let _ = writeln!(out, "- error: {e}");
                    }
                    for p in problems.iter().chain(&unmet) {
                        let _ = writeln!(out, "- {p}");
                    }
                    out.push('\n');
                }
            }
        }
    }
    out
}
