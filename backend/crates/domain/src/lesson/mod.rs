//! SDUI lesson document (catalog v1): validation model, inline markdown
//! scanner, `Validator` and client view.
//!
//! Contract: `docs/contracts/sdui.md`. Everything here is pure logic over
//! `serde_json::Value`: no I/O, no framework.
//!
//! Why `Value` instead of `#[derive(Deserialize)]`: the Validator must report
//! *every* problem with an exact JSON Pointer and a message the Generator can
//! use to repair the document. Serde stops at the first error and its paths
//! are not JSON Pointers, so the parser walks the value by hand and builds a
//! lean *validation model* (only what the rules need), not a full content
//! model.

mod client_view;
mod duration;
mod highlight;
mod inline;
mod model;
mod parse;
mod quality;
mod spoken;
mod stroke;
mod validator;

pub use client_view::client_view;
pub use duration::{
    MAX_LESSON_MINUTES, MIN_LESSON_MINUTES, document_minutes, lesson_minutes, set_computed_minutes,
};
pub use highlight::{
    Highlight, HighlightColor, HighlightDraft, HighlightIssue, HighlightTargets, MAX_HIGHLIGHTS,
};
pub use inline::{InlineScan, MarkdownProblem, count_words, placeholders, scan_inline};
pub use model::{
    Block, BlockKind, DiagramEdge, DiagramLayout, GlossaryEntry, LessonDocument, LessonErrorCode,
    LessonIssue, MdField, QuizOption, SourceEntry,
};
pub use parse::{BLOCK_TYPES, parse_document};
pub use quality::{QualityWarning, QualityWarningCode, warning_score};
pub use spoken::{ReadableBlock, readable_blocks};
pub use stroke::{
    MAX_POINTS_PER_STROKE, MAX_POINTS_TOTAL, MAX_STROKES, Stroke, StrokeColor, StrokeDraft,
    StrokeIssue, StrokeTargets, StrokeWidth,
};
pub use validator::{LessonValidator, ValidationContext, ValidatorConfig};

#[cfg(test)]
mod tests;
