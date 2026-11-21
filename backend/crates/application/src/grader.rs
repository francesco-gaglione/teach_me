//! The Grader: judges one `open_answer` against its rubric
//! (`docs/contracts/lessons.md` §5.1). Used by the request that records the
//! answer (with a time limit) and by the `lesson_completion` job.

use std::sync::Arc;

use domain::llm::{LlmMessage, LlmProvider, LlmRequest, LlmRole, MessageRole, SystemBlock};
use domain::{Exercise, FEEDBACK_MAX_CHARS, Grade, Lesson, MISCONCEPTION_MAX_CHARS, Verdict};

use crate::lesson_context::{plain_question, rubric_text};
use crate::prompts::{RolePrompts, TemplateError, render_template_in};
use crate::structured::{self, OutputSchema, Spent, check_schema, final_text, parse_object};
use domain::ContentLanguage;

const KICKOFF: &str = "Valuta la risposta.";

#[derive(Debug, thiserror::Error)]
pub enum GradeError {
    /// The call failed; the code was already logged by the call itself.
    #[error("the model call failed ({0})")]
    CallFailed(&'static str),
    #[error("the Grader's answer is not usable: {0}")]
    Unusable(String),
    #[error("the exercise has no rubric")]
    NoRubric,
    #[error("prompt: {0}")]
    Prompt(#[from] TemplateError),
}

pub struct Grader {
    llm: Arc<dyn LlmProvider>,
    prompts: RolePrompts,
    max_output_tokens: u32,
}

impl Grader {
    #[must_use]
    pub fn new(llm: Arc<dyn LlmProvider>, prompts: RolePrompts, max_output_tokens: u32) -> Self {
        Self {
            llm,
            prompts,
            max_output_tokens,
        }
    }

    /// One call, no repair: the learner is waiting (or the job will try
    /// again), and a failure only leaves the attempt `pending`.
    pub async fn grade(
        &self,
        lesson: &Lesson,
        exercise: &Exercise,
        answer: &str,
        language: ContentLanguage,
    ) -> Result<Grade, GradeError> {
        let rubric = exercise.rubric.as_deref().ok_or(GradeError::NoRubric)?;
        // The answer sits between two delimiters in the prompt; a text with the
        // delimiter characters could pretend to end early.
        let answer = answer.replace("<<<", "‹‹‹").replace(">>>", "›››");
        let context = render_template_in(
            self.prompts.context_template,
            language,
            &[
                ("lesson_title", &crate::context::one_line(&lesson.title)),
                (
                    "lesson_objective",
                    &crate::context::one_line(&lesson.objective),
                ),
                ("question", &plain_question(exercise)),
                ("rubric", &rubric_text(rubric)),
                ("max_chars", &exercise.max_chars.to_string()),
                ("answer", &answer),
            ],
        )?;
        let schema = self
            .prompts
            .output_schema
            .map(|raw| OutputSchema::parse(raw, "grade"))
            .transpose()
            .map_err(|e| GradeError::Unusable(format!("schema: {e}")))?;
        let mut request = LlmRequest::new(LlmRole::Grader, "grader.grade");
        request.system = vec![
            SystemBlock {
                text: self.prompts.system.to_owned(),
                cache_breakpoint: true,
            },
            SystemBlock {
                text: context,
                cache_breakpoint: false,
            },
        ];
        request.messages = vec![LlmMessage::text(MessageRole::User, KICKOFF)];
        if let Some(schema) = &schema {
            request.output = schema.format();
        }
        request.max_output_tokens = Some(self.max_output_tokens);

        let mut spent = Spent::default();
        let response = structured::call(
            self.llm.as_ref(),
            self.prompts.version,
            &request,
            &mut spent,
        )
        .await
        .map_err(|_| {
            // The code was already logged; the caller only needs to know
            // it failed. Keep the typed error for a direct provider failure.
            GradeError::Unusable("the call failed".to_owned())
        })?;
        parse_grade(&final_text(&response.message), schema.as_ref())
    }
}

/// The Grader's JSON as a [`Grade`]: feedback flattened to one line and held
/// to the contract's limits and inline subset.
pub fn parse_grade(text: &str, schema: Option<&OutputSchema>) -> Result<Grade, GradeError> {
    let unusable = |what: &str| GradeError::Unusable(what.to_owned());
    let value = parse_object(text).map_err(|e| GradeError::Unusable(e.join(" ")))?;
    if let Some(schema) = schema {
        let problems = check_schema(&value, &schema.schema);
        if !problems.is_empty() {
            return Err(GradeError::Unusable(problems.join(" ")));
        }
    }
    let verdict: Verdict = value["verdict"]
        .as_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| unusable("verdict"))?;
    let feedback = value["feedback"]
        .as_str()
        .map(|f| f.split_whitespace().collect::<Vec<_>>().join(" "))
        .ok_or_else(|| unusable("feedback"))?;
    let length = feedback.chars().count();
    if length == 0 || length > FEEDBACK_MAX_CHARS {
        return Err(unusable("feedback length"));
    }
    let scan = domain::lesson::scan_inline(&feedback);
    if !scan.problems.is_empty() || !scan.glossary_refs.is_empty() || !scan.citations.is_empty() {
        return Err(unusable("feedback markdown"));
    }
    let misconception = match value["misconception"].as_str() {
        None => None,
        Some(m) => {
            let m = m.split_whitespace().collect::<Vec<_>>().join(" ");
            match m.chars().count() {
                0 => None,
                n if n > MISCONCEPTION_MAX_CHARS => return Err(unusable("misconception length")),
                _ => Some(m),
            }
        }
    };
    Ok(Grade {
        verdict,
        feedback,
        misconception,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_good_grade_is_flattened_and_checked() {
        let g = parse_grade(
            r#"{"verdict":"partial","feedback":"Hai colto `b`.\nManca il perché.","misconception":"  "}"#,
            None,
        )
        .unwrap();
        assert_eq!(g.verdict, Verdict::Partial);
        assert_eq!(g.feedback, "Hai colto `b`. Manca il perché.");
        assert_eq!(g.misconception, None);
        let g = parse_grade(
            r#"{"verdict":"incorrect","feedback":"No.","misconception":"Pensa che `a` resti valida."}"#,
            None,
        )
        .unwrap();
        assert_eq!(
            g.misconception.as_deref(),
            Some("Pensa che `a` resti valida.")
        );
    }

    #[test]
    fn unusable_answers_are_rejected() {
        for text in [
            "non è json",
            r#"{"verdict":"great","feedback":"x","misconception":null}"#,
            r#"{"verdict":"correct","feedback":"","misconception":null}"#,
            r#"{"verdict":"correct","feedback":"[[term]] qui","misconception":null}"#,
            r#"{"verdict":"correct","feedback":"![img](https://x.y/z.png)","misconception":null}"#,
        ] {
            assert!(parse_grade(text, None).is_err(), "{text}");
        }
        let long = "x".repeat(801);
        assert!(
            parse_grade(
                &format!(r#"{{"verdict":"correct","feedback":"{long}","misconception":null}}"#),
                None
            )
            .is_err()
        );
    }
}
