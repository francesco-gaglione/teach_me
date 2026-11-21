//! How long a lesson takes: the one formula, used for the plan and for the
//! saved document. The model's own figure is never trusted (measured: both the
//! Planner and the Generator get the arithmetic wrong).

use serde_json::Value;

use super::model::Block;
use super::parse::parse_document;

/// `estimated_minutes` limits of the contract (`sdui.md` §2: 1-20).
pub const MIN_LESSON_MINUTES: u32 = 1;
pub const MAX_LESSON_MINUTES: u32 = 20;

/// Reading speed behind the formula: words of prose per minute.
const WORDS_PER_MINUTE: usize = 250;

/// `round(prose_words / 250 + 0.5 * exercises + 2 * open_answers)`, clamped to
/// the contract's 1-20. `exercises` counts every exercise, `open_answers`
/// only adds the extra time they take on top of that.
///
/// Integer arithmetic on the numerator over 250 (words of prose + 125 per
/// exercise + 500 per open answer), rounding halves up, so the result never
/// depends on floating point.
#[must_use]
pub fn lesson_minutes(prose_words: usize, exercises: usize, open_answers: usize) -> u32 {
    let scaled = prose_words
        .saturating_add(exercises.saturating_mul(WORDS_PER_MINUTE / 2))
        .saturating_add(open_answers.saturating_mul(WORDS_PER_MINUTE * 2));
    let minutes = scaled.saturating_add(WORDS_PER_MINUTE / 2) / WORDS_PER_MINUTE;
    u32::try_from(minutes)
        .unwrap_or(MAX_LESSON_MINUTES)
        .clamp(MIN_LESSON_MINUTES, MAX_LESSON_MINUTES)
}

/// The duration of a lesson document, from the prose words the Validator
/// counts (`sdui.md` §9.4) and its exercises (blocks inside a `fallback` do
/// not count, §9.6). `None` when the document does not parse.
#[must_use]
pub fn document_minutes(document: &Value) -> Option<u32> {
    let (doc, issues) = parse_document(document);
    if !issues.is_empty() {
        return None;
    }
    let exercises = doc.blocks.iter().filter(|b| b.is_exercise()).count();
    let open_answers = doc.blocks.iter().filter(|b| b.is_open_answer()).count();
    Some(lesson_minutes(doc.prose_words(), exercises, open_answers))
}

/// Puts the computed `estimated_minutes` into a model-written document so the
/// Validator, which still demands a figure in 1-20, never rejects the
/// document for a field that is replaced anyway. While the rest of the
/// document does not parse the figure is the lowest valid one: the other
/// problems are reported and the figure is recomputed on the next attempt.
/// The parser stays strict for everything else (saved documents, fixtures).
pub fn set_computed_minutes(document: &mut Value) {
    let Some(object) = document.as_object_mut() else {
        return;
    };
    object.insert(
        "estimated_minutes".to_owned(),
        Value::from(MIN_LESSON_MINUTES),
    );
    if let Some(minutes) = document_minutes(document) {
        document["estimated_minutes"] = Value::from(minutes);
    }
}

impl Block {
    /// Any exercise block, required or not.
    #[must_use]
    pub const fn is_exercise(&self) -> bool {
        use super::model::BlockKind as K;
        matches!(
            self.kind,
            K::QuizSingle { .. }
                | K::QuizMulti { .. }
                | K::FillBlank { .. }
                | K::OrderSteps { .. }
                | K::Flashcard { .. }
                | K::OpenAnswer { .. }
        )
    }

    #[must_use]
    pub const fn is_open_answer(&self) -> bool {
        matches!(self.kind, super::model::BlockKind::OpenAnswer { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_formula_rounds_and_stays_within_the_contract() {
        assert_eq!(lesson_minutes(0, 0, 0), 1, "never below 1");
        assert_eq!(lesson_minutes(1000, 4, 0), 6); // 4 + 2
        assert_eq!(lesson_minutes(1000, 4, 1), 8); // 4 + 2 + 2
        assert_eq!(lesson_minutes(125, 0, 0), 1); // 0.5 rounds up
        assert_eq!(lesson_minutes(124, 0, 0), 1);
        assert_eq!(lesson_minutes(375, 0, 0), 2); // 1.5 rounds up
        assert_eq!(lesson_minutes(374, 0, 0), 1);
        assert_eq!(lesson_minutes(100_000, 50, 50), 20, "never above 20");
        assert_eq!(lesson_minutes(usize::MAX, usize::MAX, usize::MAX), 20);
    }
}
