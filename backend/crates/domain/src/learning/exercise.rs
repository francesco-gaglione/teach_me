//! The exercise blocks of a lesson document, as far as attempts need them.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::str::FromStr;

use serde_json::Value;

use super::UnknownLearningValue;
use super::attempt::{Answer, Rating};
use crate::lesson::scan_inline;
use crate::validation::IssueKind;

/// Default `max_chars` of an `open_answer` (`sdui.md` §4).
const DEFAULT_MAX_CHARS: usize = 1000;
/// Longest value of a fill-in blank (`lessons.md` §2.1).
const BLANK_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExerciseKind {
    QuizSingle,
    QuizMulti,
    FillBlank,
    OrderSteps,
    Flashcard,
    OpenAnswer,
}

impl ExerciseKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QuizSingle => "quiz_single",
            Self::QuizMulti => "quiz_multi",
            Self::FillBlank => "fill_blank",
            Self::OrderSteps => "order_steps",
            Self::Flashcard => "flashcard",
            Self::OpenAnswer => "open_answer",
        }
    }

    /// The optional "Riprova" exists only for these (`lessons.md` §2).
    #[must_use]
    pub const fn allows_second_attempt(self) -> bool {
        matches!(self, Self::QuizMulti | Self::FillBlank | Self::OrderSteps)
    }

    /// The app corrects these locally and reports the outcome.
    #[must_use]
    pub const fn corrected_by_app(self) -> bool {
        matches!(
            self,
            Self::QuizSingle | Self::QuizMulti | Self::FillBlank | Self::OrderSteps
        )
    }

    /// `quiz_single` and `order_steps` are right or wrong, nothing between.
    #[must_use]
    pub const fn allows_partial(self) -> bool {
        !matches!(self, Self::QuizSingle | Self::OrderSteps)
    }
}

impl FromStr for ExerciseKind {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quiz_single" => Ok(Self::QuizSingle),
            "quiz_multi" => Ok(Self::QuizMulti),
            "fill_blank" => Ok(Self::FillBlank),
            "order_steps" => Ok(Self::OrderSteps),
            "flashcard" => Ok(Self::Flashcard),
            "open_answer" => Ok(Self::OpenAnswer),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

impl fmt::Display for ExerciseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What an exercise block offers, enough to check an answer to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exercise {
    pub id: String,
    pub kind: ExerciseKind,
    /// The question or prompt, as written in the document.
    pub prompt: String,
    /// The optional code the question asks to read (`sdui.md` §4.1), as
    /// `(language, code)`. Exercises with a question only.
    pub code: Option<(String, String)>,
    pub option_ids: Vec<String>,
    pub blank_ids: Vec<String>,
    pub step_ids: Vec<String>,
    /// `open_answer` only.
    pub max_chars: usize,
    /// `open_answer` only; never leaves the backend.
    pub rubric: Option<String>,
    pub required: bool,
    /// Visible text (markup removed) of each option and each step, by id.
    pub option_texts: Vec<(String, String)>,
    pub step_texts: Vec<(String, String)>,
    /// What a correct answer looks like, in plain text, for the Recorder.
    pub correct: String,
}

impl Exercise {
    /// Checks the shape of `raw` against this exercise (`lessons.md` §2.1).
    pub fn parse_answer(&self, raw: &Value) -> Result<Answer, IssueKind> {
        let object = raw.as_object().ok_or(IssueKind::Invalid)?;
        let only = |key: &str| {
            // Exactly the one field of the shape: anything else is a mistake.
            if object.len() == 1 {
                object.get(key).ok_or(IssueKind::Invalid)
            } else {
                Err(IssueKind::Invalid)
            }
        };
        match self.kind {
            ExerciseKind::QuizSingle => {
                let option = only("option")?.as_str().ok_or(IssueKind::Invalid)?;
                if self.option_ids.iter().any(|o| o == option) {
                    Ok(Answer::Option(option.to_owned()))
                } else {
                    Err(IssueKind::Invalid)
                }
            }
            ExerciseKind::QuizMulti => {
                let options = strings(only("options")?)?;
                let distinct: HashSet<_> = options.iter().collect();
                if options.is_empty()
                    || distinct.len() != options.len()
                    || !options.iter().all(|o| self.option_ids.contains(o))
                {
                    return Err(IssueKind::Invalid);
                }
                Ok(Answer::Options(options))
            }
            ExerciseKind::FillBlank => {
                let blanks = only("blanks")?.as_object().ok_or(IssueKind::Invalid)?;
                if blanks.len() != self.blank_ids.len()
                    || !self.blank_ids.iter().all(|id| blanks.contains_key(id))
                {
                    return Err(IssueKind::Invalid);
                }
                let mut out = BTreeMap::new();
                for (id, value) in blanks {
                    let value = value.as_str().ok_or(IssueKind::Invalid)?;
                    if value.chars().count() > BLANK_MAX_CHARS {
                        return Err(IssueKind::TooLong);
                    }
                    out.insert(id.clone(), value.to_owned());
                }
                Ok(Answer::Blanks(out))
            }
            ExerciseKind::OrderSteps => {
                let steps = strings(only("steps")?)?;
                let mut got = steps.clone();
                let mut want = self.step_ids.clone();
                got.sort();
                want.sort();
                if got == want {
                    Ok(Answer::Steps(steps))
                } else {
                    Err(IssueKind::Invalid)
                }
            }
            ExerciseKind::Flashcard => {
                let rating = only("rating")?.as_str().ok_or(IssueKind::Invalid)?;
                Rating::from_str(rating)
                    .map(Answer::Rating)
                    .map_err(|_| IssueKind::Invalid)
            }
            ExerciseKind::OpenAnswer => {
                let text = only("text")?.as_str().ok_or(IssueKind::Invalid)?.trim();
                if text.is_empty() {
                    return Err(IssueKind::Empty);
                }
                if text.chars().count() > self.max_chars {
                    return Err(IssueKind::TooLong);
                }
                Ok(Answer::Text(text.to_owned()))
            }
        }
    }
}

fn strings(value: &Value) -> Result<Vec<String>, IssueKind> {
    value
        .as_array()
        .ok_or(IssueKind::Invalid)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or(IssueKind::Invalid))
        .collect()
}

/// Every exercise of a document, including those inside a `fallback` (an app
/// that does not know a component still shows them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LessonExercises {
    exercises: Vec<Exercise>,
}

impl LessonExercises {
    /// Reads a document that already passed the Validator. Blocks that are not
    /// exercises, or are malformed, are skipped.
    #[must_use]
    pub fn from_document(document: &Value) -> Self {
        let mut exercises = Vec::new();
        if let Some(blocks) = document.get("blocks") {
            collect(blocks, &mut exercises);
        }
        Self { exercises }
    }

    #[must_use]
    pub fn get(&self, block_id: &str) -> Option<&Exercise> {
        self.exercises.iter().find(|e| e.id == block_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Exercise> {
        self.exercises.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.exercises.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exercises.is_empty()
    }
}

fn collect(blocks: &Value, out: &mut Vec<Exercise>) {
    let Some(list) = blocks.as_array() else {
        return;
    };
    for block in list {
        if let Some(fallback) = block.get("fallback") {
            collect(fallback, out);
        }
        if let Some(exercise) = exercise_from(block) {
            out.push(exercise);
        }
    }
}

#[allow(clippy::too_many_lines)] // one flat extraction per field of the block
fn exercise_from(block: &Value) -> Option<Exercise> {
    let kind = block.get("type")?.as_str()?.parse::<ExerciseKind>().ok()?;
    let id = block.get("id")?.as_str()?.to_owned();
    let ids = |key: &str| -> Vec<String> {
        block
            .get(key)
            .and_then(Value::as_array)
            .map_or_default(|items| {
                items
                    .iter()
                    .filter_map(|i| i.get("id")?.as_str().map(str::to_owned))
                    .collect()
            })
    };
    let text = |key: &str| block.get(key).and_then(Value::as_str).map(str::to_owned);
    let prompt = match kind {
        ExerciseKind::QuizSingle | ExerciseKind::QuizMulti | ExerciseKind::OpenAnswer => {
            text("question")
        }
        ExerciseKind::FillBlank => text("text"),
        ExerciseKind::OrderSteps => text("prompt"),
        ExerciseKind::Flashcard => text("front"),
    }
    .unwrap_or_default();
    let plain = |t: &str| scan_inline(t).plain;
    let pairs = |key: &str| -> Vec<(String, String)> {
        block
            .get(key)
            .and_then(Value::as_array)
            .map_or_default(|items| {
                items
                    .iter()
                    .filter_map(|i| {
                        Some((
                            i.get("id")?.as_str()?.to_owned(),
                            plain(i.get("text")?.as_str()?),
                        ))
                    })
                    .collect()
            })
    };
    let option_texts = pairs("options");
    let step_texts = pairs("steps");
    let text_of = |id: &str| {
        option_texts
            .iter()
            .find(|(i, _)| i == id)
            .map_or_else(|| id.to_owned(), |(_, t)| t.clone())
    };
    let correct = match kind {
        ExerciseKind::QuizSingle => block
            .get("correct")
            .and_then(Value::as_str)
            .map_or_default(text_of),
        ExerciseKind::QuizMulti => block
            .get("correct")
            .and_then(Value::as_array)
            .map_or_default(|c| {
                c.iter()
                    .filter_map(Value::as_str)
                    .map(text_of)
                    .collect::<Vec<_>>()
                    .join("; ")
            }),
        ExerciseKind::FillBlank => block
            .get("blanks")
            .and_then(Value::as_array)
            .map_or_default(|b| {
                b.iter()
                    .filter_map(|blank| {
                        let answers: Vec<&str> = blank
                            .get("answers")?
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_str)
                            .collect();
                        Some(format!(
                            "{} = {}",
                            blank.get("id")?.as_str()?,
                            answers.join(" / ")
                        ))
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            }),
        ExerciseKind::OrderSteps => step_texts
            .iter()
            .enumerate()
            .map(|(i, (_, t))| format!("{}) {t}", i + 1))
            .collect::<Vec<_>>()
            .join(" "),
        ExerciseKind::Flashcard => text("back").map_or_default(|b| plain(&b)),
        ExerciseKind::OpenAnswer => String::new(),
    };
    Some(Exercise {
        id,
        kind,
        prompt,
        code: block.get("code").and_then(|c| {
            Some((
                c.get("language")?.as_str()?.to_owned(),
                c.get("code")?.as_str()?.to_owned(),
            ))
        }),
        required: block
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        option_texts,
        step_texts,
        correct,
        option_ids: ids("options"),
        blank_ids: ids("blanks"),
        step_ids: ids("steps"),
        max_chars: block
            .get("max_chars")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(DEFAULT_MAX_CHARS),
        rubric: text("rubric"),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc() -> Value {
        json!({"blocks": [
            {"id": "q1", "type": "quiz_single", "question": "Q?", "options": [{"id": "a"}, {"id": "b"}], "correct": "a"},
            {"id": "qm", "type": "quiz_multi", "question": "Q?", "options": [{"id": "a"}, {"id": "b"}, {"id": "c"}]},
            {"id": "fb", "type": "fill_blank", "text": "x {{b1}} {{b2}}", "blanks": [{"id": "b1"}, {"id": "b2"}]},
            {"id": "os", "type": "order_steps", "prompt": "p", "steps": [{"id": "s1"}, {"id": "s2"}, {"id": "s3"}]},
            {"id": "fc", "type": "flashcard", "front": "f", "back": "b"},
            {"id": "oa", "type": "open_answer", "question": "Why?", "max_chars": 60, "rubric": "R"},
            {"id": "mc", "type": "missing_component", "fallback": [
                {"id": "in", "type": "quiz_single", "question": "N?", "options": [{"id": "x"}, {"id": "y"}]}
            ]},
            {"id": "p", "type": "paragraph", "text": "t"}
        ]})
    }

    #[test]
    fn finds_exercises_also_inside_a_fallback() {
        let ex = LessonExercises::from_document(&doc());
        assert_eq!(ex.len(), 7);
        assert_eq!(ex.get("in").unwrap().kind, ExerciseKind::QuizSingle);
        assert!(ex.get("p").is_none() && ex.get("mc").is_none());
        let oa = ex.get("oa").unwrap();
        assert_eq!((oa.max_chars, oa.rubric.as_deref()), (60, Some("R")));
        assert_eq!(ex.get("q1").unwrap().prompt, "Q?");
    }

    #[test]
    fn checks_the_shape_of_each_answer() {
        let ex = LessonExercises::from_document(&doc());
        let ok = |id: &str, v: Value| ex.get(id).unwrap().parse_answer(&v);
        assert_eq!(
            ok("q1", json!({"option": "b"})),
            Ok(Answer::Option("b".into()))
        );
        assert_eq!(ok("q1", json!({"option": "z"})), Err(IssueKind::Invalid));
        assert_eq!(
            ok("q1", json!({"option": "a", "extra": 1})),
            Err(IssueKind::Invalid)
        );
        assert!(ok("qm", json!({"options": ["a", "c"]})).is_ok());
        assert!(ok("qm", json!({"options": []})).is_err());
        assert!(ok("qm", json!({"options": ["a", "a"]})).is_err());
        assert!(ok("fb", json!({"blanks": {"b1": "x", "b2": "y"}})).is_ok());
        assert!(ok("fb", json!({"blanks": {"b1": "x"}})).is_err());
        assert_eq!(
            ok("fb", json!({"blanks": {"b1": "x".repeat(121), "b2": ""}})),
            Err(IssueKind::TooLong)
        );
        assert!(ok("os", json!({"steps": ["s3", "s1", "s2"]})).is_ok());
        assert!(ok("os", json!({"steps": ["s3", "s1"]})).is_err());
        assert_eq!(
            ok("fc", json!({"rating": "good"})),
            Ok(Answer::Rating(Rating::Good))
        );
        assert!(ok("fc", json!({"rating": "meh"})).is_err());
        assert_eq!(
            ok("oa", json!({"text": "  perché  "})),
            Ok(Answer::Text("perché".into()))
        );
        assert_eq!(ok("oa", json!({"text": "  "})), Err(IssueKind::Empty));
        assert_eq!(
            ok("oa", json!({"text": "x".repeat(61)})),
            Err(IssueKind::TooLong)
        );
        assert!(ok("oa", json!("text")).is_err());
    }
}
