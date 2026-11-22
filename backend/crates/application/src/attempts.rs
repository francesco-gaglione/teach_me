//! Recording what the learner answered (`docs/contracts/lessons.md` §5).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::{
    Answer, Attempt, AttemptId, AttemptRepository, Clock, ContentLanguage, Exercise, ExerciseKind,
    Grade, IdGenerator, IssueKind, Lesson, LessonExercises, LessonId, LessonRepository, Outcome,
    RepositoryError, Secret, TopicRepository, UserId, derive_flashcard_outcome,
};
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::{AppError, ConflictReason, FieldIssue};
use crate::grader::Grader;
use crate::llm_keys::{LlmKeys, with_key};
use crate::time_util::now_millis;

/// Most attempts in one request (`lessons.md` §5).
pub const MAX_ATTEMPTS_PER_REQUEST: usize = 50;

pub struct AttemptService {
    lessons: Arc<dyn LessonRepository>,
    topics: Arc<dyn TopicRepository>,
    attempts: Arc<dyn AttemptRepository>,
    grader: Arc<Grader>,
    keys: Arc<LlmKeys>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    grader_timeout: Duration,
}

/// One validated item of the request.
struct Item {
    exercise: Exercise,
    number: u8,
    answer: Answer,
    /// Reported by the app or derived from a flashcard rating; `None` for an
    /// `open_answer`.
    outcome: Option<Outcome>,
    answered_at: OffsetDateTime,
}

fn issue(i: usize, field: &str, kind: IssueKind) -> FieldIssue {
    FieldIssue::new(format!("attempts[{i}].{field}"), kind)
}

impl AttemptService {
    #[must_use]
    #[allow(clippy::too_many_arguments)] // composition-root constructor: one port each
    pub fn new(
        lessons: Arc<dyn LessonRepository>,
        topics: Arc<dyn TopicRepository>,
        attempts: Arc<dyn AttemptRepository>,
        grader: Arc<Grader>,
        keys: Arc<LlmKeys>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        grader_timeout: Duration,
    ) -> Self {
        Self {
            lessons,
            topics,
            attempts,
            grader,
            keys,
            clock,
            ids,
            grader_timeout,
        }
    }

    /// Records a batch (`body` is the request's JSON) and returns the attempts
    /// as stored, in the order of the request.
    #[allow(clippy::too_many_lines)] // load, validate, grade, store, in that order
    pub async fn record(
        &self,
        user: UserId,
        lesson_id: LessonId,
        body: &Value,
    ) -> Result<Vec<Attempt>, AppError> {
        let stored = self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let lesson = stored.lesson;
        // Open answers are graded in the language of the topic.
        let language = self
            .topics
            .get(user, lesson.topic_id)
            .await?
            .ok_or(AppError::NotFound)?
            .language;
        // A missing or unreadable key never fails the request: the open
        // answers stay `pending`, as for any other Grader failure
        // (`users.md` §3).
        // Over the daily token budget the grader is simply not called.
        let llm_key = if self.keys.check_budget(user).await.is_ok() {
            self.keys.for_user(user).await.ok()
        } else {
            None
        };
        if lesson.is_completed() {
            return Err(AppError::Conflict(ConflictReason::LessonCompleted));
        }
        let exercises = LessonExercises::from_document(&stored.document);
        let existing = self.attempts.list(lesson_id).await?;
        let items = parse_items(body, &exercises, &existing)?;

        let started = tokio::time::Instant::now();
        let now = now_millis(self.clock.as_ref());
        let key = |it: &Item| (it.exercise.id.clone(), it.number);
        let stored_by_key: HashMap<(String, u8), &Attempt> = existing
            .iter()
            .map(|a| ((a.block_id.clone(), a.attempt_number), a))
            .collect();

        // New attempts, with the open answer graded before it is stored.
        let mut fresh: Vec<Attempt> = Vec::new();
        let mut retry: Option<&Attempt> = None;
        for it in &items {
            let k = key(it);
            if let Some(old) = stored_by_key.get(&k) {
                if old.outcome == Outcome::Pending {
                    retry = Some(old);
                }
                continue;
            }
            if fresh
                .iter()
                .any(|a| (a.block_id.clone(), a.attempt_number) == k)
            {
                continue; // the same pair twice in one request: the first wins
            }
            let mut attempt = Attempt {
                id: AttemptId::new(self.ids.new_id()),
                lesson_id,
                block_id: it.exercise.id.clone(),
                kind: it.exercise.kind,
                attempt_number: it.number,
                answer: it.answer.clone(),
                outcome: it.outcome.unwrap_or(Outcome::Pending),
                grade: None,
                answered_at: it.answered_at,
                created_at: now,
            };
            if it.exercise.kind == ExerciseKind::OpenAnswer
                && let Some(grade) = self
                    .grade(
                        user,
                        llm_key.as_ref(),
                        &lesson,
                        &it.exercise,
                        &it.answer,
                        language,
                        started,
                    )
                    .await
            {
                attempt.outcome = grade.verdict.outcome();
                attempt.grade = Some(grade);
            }
            fresh.push(attempt);
        }

        let mut saved: HashMap<(String, u8), Attempt> = HashMap::new();
        if !fresh.is_empty() {
            let stored = self
                .attempts
                .record(lesson_id, &fresh)
                .await
                .map_err(|e| match e {
                    RepositoryError::Conflict => {
                        AppError::Conflict(ConflictReason::LessonCompleted)
                    }
                    // The first attempt changed while the request was being
                    // handled: the second one is not acceptable any more.
                    RepositoryError::RuleViolated => {
                        let at = items.iter().position(|it| it.number == 2).unwrap_or(0);
                        AppError::InvalidFields(vec![issue(
                            at,
                            "attempt_number",
                            IssueKind::Invalid,
                        )])
                    }
                    other => other.into(),
                })?;
            for a in stored {
                saved.insert((a.block_id.clone(), a.attempt_number), a);
            }
        }
        // A pending answer sent again is judged again (`lessons.md` §5.1).
        if let Some(old) = retry {
            let exercise = exercises.get(&old.block_id);
            if let (Some(exercise), Answer::Text(text)) = (exercise, &old.answer)
                && let Some(grade) = self
                    .grade(
                        user,
                        llm_key.as_ref(),
                        &lesson,
                        exercise,
                        &Answer::Text(text.clone()),
                        language,
                        started,
                    )
                    .await
            {
                let updated = self.attempts.set_grade(old.id, &grade).await?;
                saved.insert((updated.block_id.clone(), updated.attempt_number), updated);
            }
        }

        Ok(items
            .iter()
            .filter_map(|it| {
                let k = key(it);
                saved
                    .get(&k)
                    .or_else(|| stored_by_key.get(&k).copied())
                    .cloned()
            })
            .collect())
    }

    /// Grades within what is left of the request's time; any failure leaves
    /// the attempt `pending`.
    #[allow(clippy::too_many_arguments)] // one call site pair; a struct would only rename them
    async fn grade(
        &self,
        user: UserId,
        key: Option<&Secret>,
        lesson: &Lesson,
        exercise: &Exercise,
        answer: &Answer,
        language: ContentLanguage,
        started: tokio::time::Instant,
    ) -> Option<Grade> {
        let Answer::Text(text) = answer else {
            return None;
        };
        let key = key?.clone();
        let left = self.grader_timeout.saturating_sub(started.elapsed());
        match tokio::time::timeout(
            left,
            with_key(
                user,
                key,
                self.grader.grade(lesson, exercise, text, language),
            ),
        )
        .await
        {
            Ok(Ok(grade)) => Some(grade),
            Ok(Err(e)) => {
                tracing::warn!(lesson = %lesson.id, block = exercise.id, error = %grade_error_kind(&e), "grading failed; the answer stays pending");
                None
            }
            Err(_) => {
                tracing::warn!(lesson = %lesson.id, block = exercise.id, "grading timed out; the answer stays pending");
                None
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // a flat sequence of independent checks; helpers would only hide the order
fn parse_items(
    body: &Value,
    exercises: &LessonExercises,
    existing: &[Attempt],
) -> Result<Vec<Item>, AppError> {
    let invalid = |field: &str, kind| AppError::InvalidFields(vec![FieldIssue::new(field, kind)]);
    let list = body
        .get("attempts")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("attempts", IssueKind::Invalid))?;
    if list.is_empty() {
        return Err(invalid("attempts", IssueKind::Empty));
    }
    if list.len() > MAX_ATTEMPTS_PER_REQUEST {
        return Err(invalid("attempts", IssueKind::TooLong));
    }

    let mut problems = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut open_answers = 0;
    for (i, raw) in list.iter().enumerate() {
        let Some(exercise) = raw
            .get("block_id")
            .and_then(Value::as_str)
            .and_then(|id| exercises.get(id))
        else {
            problems.push(issue(i, "block_id", IssueKind::NotFound));
            continue;
        };
        let before = problems.len();

        let number = raw
            .get("attempt_number")
            .and_then(Value::as_u64)
            .and_then(|n| u8::try_from(n).ok());
        if number.is_none() {
            problems.push(issue(i, "attempt_number", IssueKind::Invalid));
        }
        let answer = match exercise.parse_answer(raw.get("answer").unwrap_or(&Value::Null)) {
            Ok(a) => Some(a),
            Err(kind) => {
                problems.push(issue(i, "answer", kind));
                None
            }
        };
        let reported = raw.get("outcome").filter(|v| !v.is_null());
        let outcome = match (exercise.kind, reported) {
            (ExerciseKind::Flashcard | ExerciseKind::OpenAnswer, Some(_)) => {
                problems.push(issue(i, "outcome", IssueKind::Invalid));
                None
            }
            (k, None) if k.corrected_by_app() => {
                problems.push(issue(i, "outcome", IssueKind::Empty));
                None
            }
            (k, Some(v)) if k.corrected_by_app() => {
                let parsed = v.as_str().and_then(|s| Outcome::parse_reported(k, s));
                if parsed.is_none() {
                    problems.push(issue(i, "outcome", IssueKind::Invalid));
                }
                parsed
            }
            (ExerciseKind::Flashcard, None) => match &answer {
                Some(Answer::Rating(r)) => Some(derive_flashcard_outcome(*r)),
                _ => None,
            },
            _ => None,
        };
        let answered_at = raw
            .get("answered_at")
            .and_then(Value::as_str)
            .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok());
        if answered_at.is_none() {
            problems.push(issue(i, "answered_at", IssueKind::Invalid));
        }
        if exercise.kind == ExerciseKind::OpenAnswer {
            open_answers += 1;
            if open_answers > 1 {
                problems.push(issue(i, "block_id", IssueKind::Invalid));
            }
        }

        if problems.len() == before
            && let (Some(number), Some(answer), Some(answered_at)) = (number, answer, answered_at)
        {
            // The rule of the second attempt looks at the first one, stored
            // earlier or sent earlier in this very request.
            let known = |n: u8| {
                existing
                    .iter()
                    .find(|a| a.block_id == exercise.id && a.attempt_number == n)
                    .map(|a| a.outcome)
                    .or_else(|| {
                        items
                            .iter()
                            .find(|it| it.exercise.id == exercise.id && it.number == n)
                            .and_then(|it| it.outcome)
                    })
            };
            let already_stored = existing
                .iter()
                .any(|a| a.block_id == exercise.id && a.attempt_number == number);
            if !already_stored && Attempt::check_number(exercise.kind, number, known(1)).is_err() {
                problems.push(issue(i, "attempt_number", IssueKind::Invalid));
                continue;
            }
            items.push(Item {
                exercise: exercise.clone(),
                number,
                answer,
                outcome,
                answered_at,
            });
        }
    }
    if problems.is_empty() {
        Ok(items)
    } else {
        Err(AppError::InvalidFields(problems))
    }
}

/// Which grading failure it was, without the text the model produced: that
/// can quote the student's answer.
fn grade_error_kind(error: &crate::grader::GradeError) -> &'static str {
    use crate::grader::GradeError;
    match error {
        GradeError::CallFailed(code) => code,
        GradeError::Unusable(_) => "unusable answer",
        GradeError::NoRubric => "no rubric",
        GradeError::Prompt(_) => "prompt template",
    }
}
