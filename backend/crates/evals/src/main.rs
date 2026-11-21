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
//! `run-evals`: see `backend/evals/README.md`.
//!
//! ```text
//! cargo run -p evals --bin run-evals -- [--suite <role/suite>]... [--case <id|suite:id>]...
//!     [--runs N] [--judge-model <model>] [--max-cost-usd X | --no-budget] [--dry-run]
//!     [--evals-dir DIR] [--out-dir DIR]
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use api::config::Config;
use application::{RetryPolicy, RetryingLlm, SystemRetryEnv};
use domain::llm::{LlmProvider, LlmRole};
use evals::report::markdown;
use evals::runner::{Models, RunOptions, check_explicit_suites, dry_run, run_suites};
use evals::seed::{Tuning, stamp};
use evals::suite;
use infrastructure::anthropic::{OPENROUTER_BASE_URL, openrouter_model_id};
use infrastructure::{AnthropicProvider, RoleModel, SystemClock};

/// The cost limit when none is given: a first pass must never be a surprise.
const DEFAULT_MAX_COST_USD: f64 = 2.0;

const USAGE: &str = "usage: run-evals [--suite <role/suite>]... [--case <id|suite:id>]... [--runs N] \
[--judge-model <model>] [--max-cost-usd X | --no-budget] [--dry-run] [--evals-dir DIR] [--out-dir DIR]";

struct Args {
    suites: Vec<String>,
    cases: Vec<String>,
    runs: Option<u32>,
    judge_model: String,
    max_cost_usd: Option<f64>,
    dry_run: bool,
    evals_dir: Option<PathBuf>,
    out_dir: Option<PathBuf>,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> anyhow::Result<Args> {
    let mut out = Args {
        suites: Vec::new(),
        cases: Vec::new(),
        runs: None,
        judge_model: "claude-sonnet-5-5".to_owned(),
        max_cost_usd: Some(DEFAULT_MAX_COST_USD),
        dry_run: false,
        evals_dir: None,
        out_dir: None,
    };
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().with_context(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--suite" => out.suites.push(value("--suite")?),
            "--case" => out.cases.push(value("--case")?),
            "--runs" => {
                let n: u32 = value("--runs")?
                    .parse()
                    .context("--runs must be a number")?;
                out.runs = Some(n);
            }
            "--judge-model" => out.judge_model = value("--judge-model")?,
            "--max-cost-usd" => {
                let v: f64 = value("--max-cost-usd")?
                    .parse()
                    .context("--max-cost-usd must be a number")?;
                if v < 0.0 {
                    bail!("--max-cost-usd must not be negative");
                }
                // 0 lifts the limit, like --no-budget.
                out.max_cost_usd = (v > 0.0).then_some(v);
            }
            "--no-budget" => out.max_cost_usd = None,
            "--dry-run" => out.dry_run = true,
            "--evals-dir" => out.evals_dir = Some(value("--evals-dir")?.into()),
            "--out-dir" => out.out_dir = Some(value("--out-dir")?.into()),
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument `{other}`\n{USAGE}"),
        }
    }
    if out.runs == Some(0) {
        bail!("--runs must be at least 1");
    }
    Ok(out)
}

/// `backend/evals` from the repository root or `evals` from `backend/`.
fn find_evals_dir() -> anyhow::Result<PathBuf> {
    for candidate in ["evals", "backend/evals"] {
        let p = Path::new(candidate);
        if p.join("README.md").exists() {
            return Ok(p.to_owned());
        }
    }
    bail!(
        "cannot find the evals directory: run from `backend/` or the repository root, or pass --evals-dir"
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args(std::env::args().skip(1))?;
    // The use cases explain their failures only in the log: shown with RUST_LOG.
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = api::telemetry::init(api::config::LogFormat::Text, None);
    }
    let evals_dir = match &args.evals_dir {
        Some(d) => d.clone(),
        None => find_evals_dir()?,
    };
    let suites = suite::load(&evals_dir, &args.suites)?;

    // The server's own configuration. A token is required by it and useless here.
    let config_path = std::env::var("TEACH_ME_CONFIG")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| {
            Path::new("config/teach_me.toml")
                .exists()
                .then(|| "config/teach_me.toml".to_owned())
        });
    let config = Config::load(
        config_path.as_deref().map(Path::new),
        |k| match std::env::var(k).ok() {
            None if k == "TEACH_ME_API_TOKEN" => {
                Some("eval-runner-token-0123456789-abcdefghij".to_owned())
            }
            other => other,
        },
    )
    .context("invalid configuration")?;

    let mut judge_cfg = config.llm.anthropic.clone();
    let judge_model = if judge_cfg.base_url == OPENROUTER_BASE_URL {
        openrouter_model_id(&args.judge_model)
    } else {
        args.judge_model.clone()
    };
    judge_cfg.roles.insert(
        LlmRole::Interviewer,
        RoleModel {
            model: judge_model,
            effort: Some("low".to_owned()),
            max_output_tokens: 4096,
        },
    );
    let models = Models {
        roles: config
            .llm
            .anthropic
            .roles
            .iter()
            .map(|(r, m)| (*r, m.model.clone()))
            .collect(),
        judge: args.judge_model.clone(),
    };
    let opts = RunOptions {
        runs: args.runs,
        max_cost_usd: args.max_cost_usd,
        cases: args.cases.clone(),
        tuning: Tuning {
            max_searches: config.llm.researcher_max_searches,
            history_token_budget: config.llm.tutor_history_token_budget,
            max_fetches: config.llm.generator_max_fetches,
            web_fetch_capable: config.llm.anthropic.server_web_fetch,
        },
    };

    // A suite named with `--suite` that needs the web fetch tool is an error
    // when the tool is off; the others are skipped in the report.
    check_explicit_suites(&suites, &args.suites, &opts.tuning)?;

    if args.dry_run {
        print!("{}", dry_run(&suites, &models, &opts));
        return Ok(());
    }
    if config.llm.anthropic.api_key.is_none() {
        bail!(
            "no API key: set TEACH_ME_ANTHROPIC_API_KEY, or TEACH_ME_OPENROUTER_API_KEY with TEACH_ME_LLM_PROVIDER=openrouter"
        );
    }
    let retrying = |cfg| -> anyhow::Result<Arc<dyn LlmProvider>> {
        Ok(Arc::new(RetryingLlm::new(
            Arc::new(AnthropicProvider::new(cfg).context("LLM provider")?),
            RetryPolicy::default(),
            Arc::new(SystemRetryEnv),
        )))
    };
    let subject = retrying(config.llm.anthropic.clone())?;
    let judge = retrying(judge_cfg)?;

    let now = domain::Clock::now(&SystemClock);
    let started_at = stamp(now);
    let dir_name = started_at
        .replace([':', '.'], "-")
        .replace('T', "_")
        .trim_end_matches('Z')
        .to_owned();
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| evals_dir.join("results"))
        .join(dir_name);
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("cannot create {}", out_dir.display()))?;

    eprintln!(
        "running {} suite(s); results in {}",
        suites.len(),
        out_dir.display()
    );
    let report = run_suites(subject, judge, &models, &suites, &opts, &started_at).await;
    std::fs::write(
        out_dir.join("report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    std::fs::write(out_dir.join("summary.md"), markdown(&report))?;
    println!("{}", markdown(&report));
    println!("report: {}", out_dir.join("report.json").display());
    Ok(())
}
