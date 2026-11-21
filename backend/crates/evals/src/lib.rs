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
//! Eval runner. For each case of a suite it drives the **real** use case of
//! the role (Tutor chat, `RunResearch`, `GenerateLesson`, `Grader`,
//! `RunLessonCompletion`) over an in-memory database, against the provider
//! of the server's own configuration, then applies the suite's deterministic
//! checks and an LLM judge on the rubric. See `backend/evals/README.md`.

pub mod checks;
pub mod judge;
pub mod llm;
pub mod pricing;
pub mod report;
pub mod roles;
pub mod runner;
pub mod seed;
pub mod suite;
