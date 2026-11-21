use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde_json::{Value, json};
use time::OffsetDateTime;

use super::UnknownLearningValue;
use super::exercise::ExerciseKind;
use crate::ids::{AttemptId, LessonId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Correct,
    Partial,
    Incorrect,
    /// An `open_answer` the Grader has not judged yet.
    Pending,
}

impl Outcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Partial => "partial",
            Self::Incorrect => "incorrect",
            Self::Pending => "pending",
        }
    }

    /// An outcome the app may report for a locally corrected exercise.
    #[must_use]
    pub fn parse_reported(kind: ExerciseKind, raw: &str) -> Option<Self> {
        match raw {
            "correct" => Some(Self::Correct),
            "incorrect" => Some(Self::Incorrect),
            "partial" if kind.allows_partial() => Some(Self::Partial),
            _ => None,
        }
    }
}

impl FromStr for Outcome {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "correct" => Ok(Self::Correct),
            "partial" => Ok(Self::Partial),
            "incorrect" => Ok(Self::Incorrect),
            "pending" => Ok(Self::Pending),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// What the Grader decided: an [`Outcome`] that cannot be `pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Correct,
    Partial,
    Incorrect,
}

impl Verdict {
    #[must_use]
    pub const fn outcome(self) -> Outcome {
        match self {
            Self::Correct => Outcome::Correct,
            Self::Partial => Outcome::Partial,
            Self::Incorrect => Outcome::Incorrect,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.outcome().as_str()
    }
}

impl FromStr for Verdict {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "correct" => Ok(Self::Correct),
            "partial" => Ok(Self::Partial),
            "incorrect" => Ok(Self::Incorrect),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

pub const FEEDBACK_MAX_CHARS: usize = 800;
pub const MISCONCEPTION_MAX_CHARS: usize = 300;

/// The Grader's judgement of an `open_answer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grade {
    pub verdict: Verdict,
    /// Inline markdown, addressed to the learner.
    pub feedback: String,
    /// The wrong belief the answer reveals, in the third person.
    pub misconception: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

impl Rating {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Hard => "hard",
            Self::Good => "good",
            Self::Easy => "easy",
        }
    }
}

impl FromStr for Rating {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "again" => Ok(Self::Again),
            "hard" => Ok(Self::Hard),
            "good" => Ok(Self::Good),
            "easy" => Ok(Self::Easy),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// A flashcard has no correction: its outcome is the learner's own rating.
#[must_use]
pub const fn derive_flashcard_outcome(rating: Rating) -> Outcome {
    match rating {
        Rating::Again => Outcome::Incorrect,
        Rating::Hard => Outcome::Partial,
        Rating::Good | Rating::Easy => Outcome::Correct,
    }
}

/// An answer, in the shape of its exercise (`lessons.md` §2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Option(String),
    Options(Vec<String>),
    Blanks(BTreeMap<String, String>),
    Steps(Vec<String>),
    Rating(Rating),
    Text(String),
}

impl Answer {
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Option(o) => json!({ "option": o }),
            Self::Options(o) => json!({ "options": o }),
            Self::Blanks(b) => json!({ "blanks": b }),
            Self::Steps(s) => json!({ "steps": s }),
            Self::Rating(r) => json!({ "rating": r.as_str() }),
            Self::Text(t) => json!({ "text": t }),
        }
    }

    /// Reads a stored answer back. Stored answers were checked on the way in,
    /// so only the shape is verified here.
    #[must_use]
    pub fn from_json(kind: ExerciseKind, value: &Value) -> Option<Self> {
        let strings = |v: &Value| -> Option<Vec<String>> {
            v.as_array()?
                .iter()
                .map(|s| s.as_str().map(str::to_owned))
                .collect()
        };
        match kind {
            ExerciseKind::QuizSingle => Some(Self::Option(value.get("option")?.as_str()?.into())),
            ExerciseKind::QuizMulti => Some(Self::Options(strings(value.get("options")?)?)),
            ExerciseKind::FillBlank => Some(Self::Blanks(
                value
                    .get("blanks")?
                    .as_object()?
                    .iter()
                    .map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                    .collect::<Option<_>>()?,
            )),
            ExerciseKind::OrderSteps => Some(Self::Steps(strings(value.get("steps")?)?)),
            ExerciseKind::Flashcard => value
                .get("rating")?
                .as_str()?
                .parse()
                .ok()
                .map(Self::Rating),
            ExerciseKind::OpenAnswer => Some(Self::Text(value.get("text")?.as_str()?.into())),
        }
    }
}

/// Why an attempt number is not acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttemptRuleError {
    #[error("this exercise has no second attempt")]
    NoSecondAttempt,
    #[error("a second attempt needs a first one that was not correct")]
    FirstMissingOrCorrect,
    #[error("attempt numbers are 1 or 2")]
    OutOfRange,
}

/// The answer to one exercise of a lesson.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub id: AttemptId,
    pub lesson_id: LessonId,
    pub block_id: String,
    pub kind: ExerciseKind,
    /// 1, or 2 for the optional retry.
    pub attempt_number: u8,
    pub answer: Answer,
    pub outcome: Outcome,
    /// `open_answer` only, once graded.
    pub grade: Option<Grade>,
    /// When the learner answered (the app's clock).
    pub answered_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

impl Attempt {
    /// A passed attempt counts as evidence of learning (`lessons.md` §6.3).
    #[must_use]
    pub fn is_passed(&self) -> bool {
        self.outcome == Outcome::Correct
    }

    /// Checks `number` against the rules of `kind`, given the outcome of the
    /// first attempt at the same block if there is one.
    pub fn check_number(
        kind: ExerciseKind,
        number: u8,
        first: Option<Outcome>,
    ) -> Result<(), AttemptRuleError> {
        match number {
            1 => Ok(()),
            2 if !kind.allows_second_attempt() => Err(AttemptRuleError::NoSecondAttempt),
            2 => match first {
                Some(o) if o != Outcome::Correct => Ok(()),
                _ => Err(AttemptRuleError::FirstMissingOrCorrect),
            },
            _ => Err(AttemptRuleError::OutOfRange),
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;

    fn attempt(kind: ExerciseKind, n: u8, outcome: Outcome) -> Attempt {
        Attempt {
            id: AttemptId::new(Uuid::from_u128(1)),
            lesson_id: LessonId::new(Uuid::from_u128(2)),
            block_id: "b".into(),
            kind,
            attempt_number: n,
            answer: Answer::Option("a".into()),
            outcome,
            grade: None,
            answered_at: datetime!(2026-11-09 10:00 UTC),
            created_at: datetime!(2026-11-09 10:00 UTC),
        }
    }

    #[test]
    fn flashcard_ratings_map_to_outcomes() {
        assert_eq!(derive_flashcard_outcome(Rating::Again), Outcome::Incorrect);
        assert_eq!(derive_flashcard_outcome(Rating::Hard), Outcome::Partial);
        assert_eq!(derive_flashcard_outcome(Rating::Good), Outcome::Correct);
        assert_eq!(derive_flashcard_outcome(Rating::Easy), Outcome::Correct);
    }

    #[test]
    fn a_second_attempt_needs_a_retryable_kind_and_a_failed_first() {
        let first_wrong = attempt(ExerciseKind::QuizMulti, 1, Outcome::Incorrect);
        let first_ok = attempt(ExerciseKind::QuizMulti, 1, Outcome::Correct);
        let check = Attempt::check_number;
        assert_eq!(check(ExerciseKind::QuizMulti, 1, None), Ok(()));
        assert_eq!(
            check(ExerciseKind::QuizMulti, 2, Some(first_wrong.outcome)),
            Ok(())
        );
        assert_eq!(
            check(ExerciseKind::QuizMulti, 2, Some(first_ok.outcome)),
            Err(AttemptRuleError::FirstMissingOrCorrect)
        );
        assert_eq!(
            check(ExerciseKind::QuizMulti, 2, None),
            Err(AttemptRuleError::FirstMissingOrCorrect)
        );
        assert_eq!(
            check(ExerciseKind::QuizSingle, 2, Some(first_wrong.outcome)),
            Err(AttemptRuleError::NoSecondAttempt)
        );
        assert_eq!(
            check(ExerciseKind::OpenAnswer, 2, Some(first_wrong.outcome)),
            Err(AttemptRuleError::NoSecondAttempt)
        );
        assert_eq!(
            check(ExerciseKind::QuizMulti, 3, None),
            Err(AttemptRuleError::OutOfRange)
        );
    }

    #[test]
    fn reported_outcomes_respect_the_kind() {
        use Outcome::{Correct, Incorrect, Partial};
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::QuizMulti, "partial"),
            Some(Partial)
        );
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::QuizSingle, "partial"),
            None
        );
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::OrderSteps, "partial"),
            None
        );
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::FillBlank, "correct"),
            Some(Correct)
        );
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::FillBlank, "incorrect"),
            Some(Incorrect)
        );
        assert_eq!(
            Outcome::parse_reported(ExerciseKind::FillBlank, "pending"),
            None
        );
    }

    #[test]
    fn answers_round_trip_through_json() {
        let cases = [
            (ExerciseKind::QuizSingle, Answer::Option("b".into())),
            (
                ExerciseKind::QuizMulti,
                Answer::Options(vec!["a".into(), "c".into()]),
            ),
            (
                ExerciseKind::FillBlank,
                Answer::Blanks([("b1".to_owned(), "borrow".to_owned())].into()),
            ),
            (
                ExerciseKind::OrderSteps,
                Answer::Steps(vec!["s2".into(), "s1".into()]),
            ),
            (ExerciseKind::Flashcard, Answer::Rating(Rating::Hard)),
            (ExerciseKind::OpenAnswer, Answer::Text("perché".into())),
        ];
        for (kind, answer) in cases {
            assert_eq!(Answer::from_json(kind, &answer.to_json()), Some(answer));
        }
    }
}
