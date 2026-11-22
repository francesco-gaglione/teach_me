//! Saving and comparing baselines (ADR 0021, format and thresholds in
//! `backend/evals/baselines/README.md`, which is the spec of this module).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::checks::Status;
use crate::report::{Report, SuiteReport};

/// Exit code of `run-evals` when a baseline comparison finds a regression.
pub const REGRESSION_EXIT_CODE: i32 = 3;
/// Exit code when `--baseline` was given but no case could be compared at all
/// (an exit 0 would then say nothing).
pub const NOTHING_COMPARED_EXIT_CODE: i32 = 4;

/// Absolute drop of the rubric mean that counts as a regression.
pub const RUBRIC_MEAN_DROP: f64 = 0.10;
/// Absolute drop of the worst run's rubric score that counts as a regression.
pub const RUBRIC_MIN_DROP: f64 = 0.20;
/// Relative increase of the cost per run that counts as a regression...
pub const COST_INCREASE: f64 = 0.25;
/// ...unless the rubric mean rose by at least this much (the README leaves
/// "a quality improvement that justifies it" open: we use the same size as the
/// mean's noise threshold).
pub const COST_JUSTIFYING_GAIN: f64 = 0.10;

/// Cases whose "never before turn N" rule must hold in every run (README rule 4).
const MUST_PASS_EVERY_RUN: &[(&str, &str)] = &[
    ("tutor/tools", "vague_pure"),
    ("tutor/tools", "skip_interview"),
];

const EPS: f64 = 1e-9;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaseBaseline {
    /// Runs of the case (the README's example omits it: it is needed to turn
    /// `runs_checks_ok` into a rate when the run counts differ).
    pub runs: usize,
    pub runs_checks_ok: usize,
    pub rubric_mean: Option<f64>,
    pub rubric_min: Option<f64>,
    /// Subject plus judge cost divided by the runs (the README compares the
    /// cost per run; the suite's `cost_usd` alone cannot give it per case).
    pub cost_per_run_usd: f64,
    pub failed_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Baseline {
    pub suite: String,
    pub prompt_version: String,
    pub model: String,
    pub judge_model: String,
    pub date: String,
    pub runs_per_case: usize,
    pub cost_usd: f64,
    pub cases: BTreeMap<String, CaseBaseline>,
}

impl Baseline {
    /// The reduced form of one suite of a run; `None` for a suite that was skipped.
    #[must_use]
    pub fn from_suite(report: &Report, suite: &SuiteReport) -> Option<Self> {
        if suite.skipped.is_some() {
            return None;
        }
        let mut models = BTreeSet::new();
        let mut cases = BTreeMap::new();
        for c in &suite.cases {
            let mut failed = BTreeSet::new();
            for r in &c.runs {
                models.extend(r.models.iter().cloned());
                for t in &r.turns {
                    failed.extend(
                        t.checks
                            .iter()
                            .filter(|k| k.status == Status::Fail)
                            .map(|k| k.name.clone()),
                    );
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let per_run = c.cost_usd / c.runs.len().max(1) as f64;
            cases.insert(
                c.id.clone(),
                CaseBaseline {
                    runs: c.runs.len(),
                    runs_checks_ok: c.runs_checks_ok,
                    rubric_mean: c.rubric_mean,
                    rubric_min: c.rubric_min,
                    cost_per_run_usd: per_run,
                    failed_checks: failed.into_iter().collect(),
                },
            );
        }
        Some(Self {
            suite: suite.id.clone(),
            prompt_version: suite.prompt_version.trim().to_owned(),
            model: models.into_iter().collect::<Vec<_>>().join(", "),
            judge_model: report
                .options
                .get("judge_model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned(),
            date: report.started_at.chars().take(10).collect(),
            runs_per_case: suite.cases.iter().map(|c| c.runs.len()).max().unwrap_or(0),
            cost_usd: suite.cost_usd,
            cases,
        })
    }

    /// Every non-skipped suite of the run.
    #[must_use]
    pub fn from_report(report: &Report) -> Vec<Self> {
        report
            .suites
            .iter()
            .filter_map(|s| Self::from_suite(report, s))
            .collect()
    }

    /// `<role>__<suite>__<prompt_version>.json`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!(
            "{}__{}.json",
            self.suite.replace('/', "__"),
            self.prompt_version
        )
    }
}

/// Checks, before the run is paid for, that `save` will be able to write:
/// `runnable_suites` are the suites that will actually run (not skipped).
pub fn check_destination(dest: &Path, runnable_suites: usize) -> anyhow::Result<()> {
    if runnable_suites == 0 {
        bail!("--save-baseline: no suite will run, there would be nothing to save");
    }
    if dest.is_dir() {
        return Ok(());
    }
    if runnable_suites > 1 {
        bail!(
            "--save-baseline: {runnable_suites} suites will run: pass an existing directory, or select one suite with --suite"
        );
    }
    let parent = match dest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if !parent.is_dir() {
        bail!(
            "--save-baseline: the directory {} does not exist",
            parent.display()
        );
    }
    Ok(())
}

/// Writes the baselines of a run. `dest` is a file (the run must have exactly
/// one suite) or an existing directory (one file per suite, named as the README
/// says). Returns the files written.
pub fn save(report: &Report, dest: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let baselines = Baseline::from_report(report);
    if baselines.is_empty() {
        bail!("nothing to save: the run has no suite that ran");
    }
    let targets: Vec<(PathBuf, &Baseline)> = if dest.is_dir() {
        baselines
            .iter()
            .map(|b| (dest.join(b.file_name()), b))
            .collect()
    } else if let [only] = baselines.as_slice() {
        vec![(dest.to_owned(), only)]
    } else {
        bail!(
            "the run has {} suites: pass an existing directory to --save-baseline, or select one suite with --suite",
            baselines.len()
        );
    };
    let mut written = Vec::new();
    for (path, b) in targets {
        let mut text = serde_json::to_string_pretty(b)?;
        text.push('\n');
        std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

/// Reads a baseline file, or every `*.json` of a directory.
pub fn load(src: &Path) -> anyhow::Result<Vec<Baseline>> {
    let read = |p: &Path| -> anyhow::Result<Baseline> {
        let text =
            std::fs::read_to_string(p).with_context(|| format!("cannot read {}", p.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} is not a baseline", p.display()))
    };
    if src.is_dir() {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(src)
            .with_context(|| format!("cannot read {}", src.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        paths.iter().map(|p| read(p)).collect()
    } else {
        Ok(vec![read(src)?])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseComparison {
    pub suite: String,
    pub case: String,
    pub regressions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Comparison {
    pub cases: Vec<CaseComparison>,
    /// Not fatal: missing suites or cases, duplicate baselines.
    pub notes: Vec<String>,
    /// Suites skipped because model or judge differ from the baseline.
    pub not_compared: Vec<String>,
}

impl Comparison {
    #[must_use]
    pub fn has_regression(&self) -> bool {
        self.cases.iter().any(|c| !c.regressions.is_empty())
    }

    /// How many cases were actually compared.
    #[must_use]
    pub fn compared(&self) -> usize {
        self.cases.len()
    }

    /// A short markdown table plus the notes.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from("## Baseline comparison\n\n");
        if !self.cases.is_empty() {
            out.push_str("| suite | case | result | detail |\n|---|---|---|---|\n");
            for c in &self.cases {
                let (result, detail) = if c.regressions.is_empty() {
                    ("ok", String::new())
                } else {
                    ("REGRESSION", c.regressions.join("; "))
                };
                let _ = writeln!(out, "| {} | {} | {result} | {detail} |", c.suite, c.case);
            }
            out.push('\n');
        }
        for n in &self.not_compared {
            let _ = writeln!(out, "**NOT COMPARED: {n}**\n");
        }
        for n in &self.notes {
            let _ = writeln!(out, "- {n}");
        }
        let _ = writeln!(
            out,
            "\n{}",
            if self.has_regression() {
                "Result: REGRESSION"
            } else if self.compared() == 0 {
                "Result: NOTHING COMPARED (not a pass)"
            } else {
                "Result: no regression"
            }
        );
        out
    }
}

fn rate(ok: usize, runs: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let r = if runs == 0 {
        0.0
    } else {
        ok as f64 / runs as f64
    };
    r
}

fn compare_case(suite: &str, id: &str, base: &CaseBaseline, now: &CaseBaseline) -> Vec<String> {
    let mut out = Vec::new();
    if rate(now.runs_checks_ok, now.runs) + EPS < rate(base.runs_checks_ok, base.runs) {
        out.push(format!(
            "checks ok {}/{} (baseline {}/{})",
            now.runs_checks_ok, now.runs, base.runs_checks_ok, base.runs
        ));
    }
    let newly: Vec<&String> = now
        .failed_checks
        .iter()
        .filter(|k| !base.failed_checks.contains(k))
        .collect();
    if !newly.is_empty() {
        out.push(format!(
            "new failed checks: {}",
            newly
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let (Some(b), None) = (base.rubric_mean, now.rubric_mean) {
        // No score at all means the judge (or the run) failed: not an improvement.
        out.push(format!("no rubric score (baseline {b:.2}): judge failed?"));
    }
    if let (Some(b), Some(n)) = (base.rubric_mean, now.rubric_mean)
        && b - n > RUBRIC_MEAN_DROP + EPS
    {
        out.push(format!("rubric mean {n:.2} (baseline {b:.2})"));
    }
    if let (Some(b), Some(n)) = (base.rubric_min, now.rubric_min)
        && b - n > RUBRIC_MIN_DROP + EPS
    {
        out.push(format!("rubric min {n:.2} (baseline {b:.2})"));
    }
    if MUST_PASS_EVERY_RUN.contains(&(suite, id)) && now.runs_checks_ok < now.runs {
        out.push(format!(
            "must pass in every run: {}/{}",
            now.runs_checks_ok, now.runs
        ));
    }
    if base.cost_per_run_usd > 0.0 {
        let growth = now.cost_per_run_usd / base.cost_per_run_usd - 1.0;
        let gain = match (base.rubric_mean, now.rubric_mean) {
            (Some(b), Some(n)) => n - b,
            _ => 0.0,
        };
        if growth > COST_INCREASE + EPS && gain + EPS < COST_JUSTIFYING_GAIN {
            out.push(format!(
                "cost per run +{:.0}% ({:.4} vs {:.4} USD)",
                growth * 100.0,
                now.cost_per_run_usd,
                base.cost_per_run_usd
            ));
        }
    }
    out
}

/// Compares the suites of a run with the saved baselines, per case.
#[must_use]
pub fn compare(current: &[Baseline], saved: &[Baseline]) -> Comparison {
    let mut result = Comparison::default();
    let mut seen = BTreeSet::new();
    for b in saved {
        if !seen.insert(b.suite.as_str()) {
            result.notes.push(format!(
                "WARNING: several baseline files for {}: the first one is used",
                b.suite
            ));
        }
    }
    for now in current {
        let Some(base) = saved.iter().find(|b| b.suite == now.suite) else {
            result
                .notes
                .push(format!("{}: no baseline for this suite", now.suite));
            continue;
        };
        if base.model != now.model || base.judge_model != now.judge_model {
            result.not_compared.push(format!(
                "{}: not comparable (model `{}` / judge `{}` vs baseline `{}` / `{}`)",
                now.suite, now.model, now.judge_model, base.model, base.judge_model
            ));
            continue;
        }
        if base.prompt_version != now.prompt_version {
            result.notes.push(format!(
                "{}: prompt `{}` against baseline `{}`",
                now.suite, now.prompt_version, base.prompt_version
            ));
        }
        for (id, n) in &now.cases {
            match base.cases.get(id) {
                Some(b) => result.cases.push(CaseComparison {
                    suite: now.suite.clone(),
                    case: id.clone(),
                    regressions: compare_case(&now.suite, id, b, n),
                }),
                None => result
                    .notes
                    .push(format!("{}/{id}: not in the baseline", now.suite)),
            }
        }
        for id in base.cases.keys().filter(|id| !now.cases.contains_key(*id)) {
            result.notes.push(format!(
                "{}/{id}: in the baseline, not in this run",
                now.suite
            ));
        }
    }
    for base in saved {
        if !current.iter().any(|c| c.suite == base.suite) {
            result
                .notes
                .push(format!("{}: in the baseline, not in this run", base.suite));
        }
    }
    result
}
