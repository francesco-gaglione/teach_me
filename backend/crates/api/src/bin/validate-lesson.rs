//! Validates a lesson JSON file against the SDUI catalog v1 contract.
//!
//! Usage: `cargo run -p api --bin validate-lesson -- <file.json>`
//!
//! Prints the issues as a JSON array of `{path, code, message}` on stdout.
//! Exit code: 0 valid, 1 invalid, 2 usage or I/O error. Runs in fixture mode
//! (no Topic context): `resource_id: null` is accepted.

use std::process::ExitCode;

use anyhow::Context;
use domain::{LessonIssue, LessonValidator, ValidationContext, ValidatorConfig};
use serde_json::Value;

fn run() -> anyhow::Result<bool> {
    let path = std::env::args()
        .nth(1)
        .context("usage: validate-lesson <file.json>")?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("cannot read {path}"))?;
    let document: Value =
        serde_json::from_str(&text).with_context(|| format!("{path} is not valid JSON"))?;
    let validator = LessonValidator::new(ValidatorConfig::default(), ValidationContext::fixture());
    let issues = validator.validate(&document);
    let json: Vec<Value> = issues.iter().map(LessonIssue::to_json).collect();
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(issues.is_empty())
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
