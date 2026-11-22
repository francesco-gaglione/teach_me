//! Shared fixtures for the lessons adapter tests.

#![allow(unused_imports)]

use async_trait::async_trait;
use domain::{
    Answer, Attempt, AttemptId, AttemptRepository, COMPONENT_REQUEST_EXAMPLES, CardOrigin,
    CardSchedule, CardStatus, CardTarget, CompletionSave, CompletionSummary, ComponentRequest,
    ComponentRequestCursor, ComponentRequestExample, ComponentRequestGroup, ComponentRequestId,
    ExerciseKind, GeneratedLesson, GlossaryTerm, GlossaryTermId, Grade, Highlight, HighlightColor,
    HighlightRepository, InProgressCursor, InProgressLesson, Job, JobId, LearningRecord,
    LearningRecordId, LearningRepository, Lesson, LessonCursor, LessonId, LessonOutputs,
    LessonRepository, LessonStatus, Outcome, Rating, RecordCursor, RecordKind, RecordStatus,
    RepositoryError, ReviewCard, ReviewCardId, StoredLesson, Stroke, StrokeColor, StrokeRepository,
    StrokeWidth, Suspension, SuspensionReason, TermCursor, TopicId, UserId, Verdict,
    is_to_consolidate, term_sort_key,
};
use serde_json::Value;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection, SqlitePool};
use time::OffsetDateTime;

use super::*;
pub(super) use super::*;
pub(super) use crate::jobs::SqliteJobRepository;
use crate::jobs::insert_job;
pub(super) use crate::repositories::{SqliteFolderRepository, SqliteTopicRepository};
use crate::repositories::{format_ts, parse_id, parse_ts, storage};
pub(super) use crate::sqlite::{connect, migrate};
pub(super) use domain::{
    FolderId, FolderName, FolderRepository, JobKind, JobRepository, JobResult, JobStatus,
    LessonMeta, NewCard, NewFolder, NewRecord, TermUpsert, Topic, TopicRepository, TopicTitle,
};
pub(super) use serde_json::json;
pub(super) use time::macros::datetime;
pub(super) use uuid::Uuid;

pub(super) const T0: OffsetDateTime = datetime!(2026-11-09 10:00:00.5 UTC);

pub(super) struct Env {
    lessons: SqliteLessonRepository,
    attempts: SqliteAttemptRepository,
    learning: SqliteLearningRepository,
    jobs: SqliteJobRepository,
    topic: TopicId,
    pool: SqlitePool,
}

pub(super) fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

pub(super) async fn env() -> Env {
    let pool = connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await.unwrap();
    let folders = SqliteFolderRepository::new(pool.clone());
    let topics = SqliteTopicRepository::new(pool.clone());
    let folder = FolderId::new(uid(1));
    folders
        .insert(NewFolder {
            user_id: domain::UserId::OWNER,
            id: folder,
            name: FolderName::parse("F").unwrap(),
            created_at: T0,
        })
        .await
        .unwrap();
    let topic = Topic::new(
        TopicId::new(uid(2)),
        domain::UserId::OWNER,
        folder,
        TopicTitle::parse("T").unwrap(),
        T0,
    );
    topics.insert(&topic).await.unwrap();
    Env {
        lessons: SqliteLessonRepository::new(pool.clone()),
        attempts: SqliteAttemptRepository::new(pool.clone()),
        learning: SqliteLearningRepository::new(pool.clone()),
        jobs: SqliteJobRepository::new(pool.clone()),
        topic: topic.id,
        pool,
    }
}

pub(super) fn document() -> Value {
    json!({"catalog_version": 1, "title": "Stop", "objective": "Saper compensare.",
    "estimated_minutes": 8, "blocks": [
        {"id": "q1", "type": "quiz_single", "question": "Q?", "options": [{"id": "a"}, {"id": "b"}], "correct": "a"},
        {"id": "oa", "type": "open_answer", "question": "Perché?", "rubric": "R"}
    ]})
}

pub(super) fn generated(env: &Env, n: u128) -> GeneratedLesson {
    let id = LessonId::new(uid(0x100 + n));
    GeneratedLesson {
        quality_warnings: vec![],
        id,
        topic_id: env.topic,
        job_id: JobId::new(uid(0x200 + n)),
        meta: LessonMeta::from_document(&document()).unwrap(),
        mission_version: 2,
        document: document(),
        plan: json!({"objective": "Saper compensare."}),
        component_requests: vec![ComponentRequest {
            id: ComponentRequestId::new(uid(0x300 + n)),
            topic_id: env.topic,
            lesson_id: id,
            block_id: "mc".into(),
            wanted: "slider".into(),
            purpose: "p".into(),
            fallback: json!([{"id": "f"}]),
            created_at: T0,
        }],
        created_at: T0 + time::Duration::seconds(i64::try_from(n).unwrap()),
    }
}

pub(super) async fn lesson(env: &Env, n: u128) -> Lesson {
    env.lessons
        .insert_generated(&generated(env, n))
        .await
        .unwrap()
}

/// A lesson of catalog `version` whose requests are `(wanted, purpose,
/// seconds after T0)`.
pub(super) async fn lesson_asking(env: &Env, n: u128, version: u32, asks: &[(&str, &str, i64)]) {
    let mut new = generated(env, n);
    new.meta.catalog_version = version;
    new.component_requests = asks
        .iter()
        .enumerate()
        .map(|(i, (wanted, purpose, secs))| ComponentRequest {
            id: ComponentRequestId::new(uid(0x500 + n * 10 + i as u128)),
            topic_id: env.topic,
            lesson_id: new.id,
            block_id: format!("m{i}"),
            wanted: (*wanted).into(),
            purpose: (*purpose).into(),
            fallback: json!([]),
            created_at: T0 + time::Duration::seconds(*secs),
        })
        .collect();
    env.lessons.insert_generated(&new).await.unwrap();
}

#[tokio::test]
pub(super) async fn in_progress_pages_one_by_one_through_equal_activity_times() {
    let env = env().await;
    let mut ids = Vec::new();
    for n in 1..=3 {
        ids.push(lesson(&env, n).await.id);
    }
    // Same activity time for all three (no attempts: the creation time).
    sqlx::query("UPDATE lessons SET status = 'in_progress', created_at = ?")
        .bind(format_ts(T0))
        .execute(&env.pool)
        .await
        .unwrap();
    let mut seen = Vec::new();
    let mut after = None;
    loop {
        let page = env
            .lessons
            .list_in_progress(domain::UserId::OWNER, after, 1)
            .await
            .unwrap();
        let Some(item) = page.first() else { break };
        assert_eq!(page.len(), 1);
        seen.push(item.lesson.id);
        after = Some(InProgressCursor {
            last_activity_at: item.last_activity_at,
            id: item.lesson.id,
        });
    }
    ids.sort_by_key(ToString::to_string);
    assert_eq!(seen, ids, "every lesson once, ties broken by id");
}

#[tokio::test]
pub(super) async fn quality_warnings_are_saved_read_back_and_default_to_none() {
    let env = env().await;
    let mut new = generated(&env, 1);
    new.quality_warnings = vec![domain::LessonQualityWarning {
        code: "too_many_callouts".into(),
        block_id: Some("c1".into()),
        message: "Troppi callout.".into(),
    }];
    let saved = env.lessons.insert_generated(&new).await.unwrap();
    assert_eq!(saved.quality_warnings, new.quality_warnings);
    let plain = lesson(&env, 2).await;
    assert_eq!(plain.quality_warnings.len(), 0);
    sqlx::query("UPDATE lessons SET quality_warnings = 'oops' WHERE id = ?")
        .bind(plain.id.to_string())
        .execute(&env.pool)
        .await
        .unwrap();
    let again = fetch_lesson(&mut env.pool.acquire().await.unwrap(), plain.id).await;
    assert!(
        matches!(again, Err(RepositoryError::Storage(e)) if e.to_string().contains("corrupt lesson quality warnings"))
    );
}

#[tokio::test]
pub(super) async fn component_requests_are_ranked_by_count_then_recency_then_name() {
    let env = env().await;
    let none = env
        .lessons
        .component_request_groups(domain::UserId::OWNER, None, 10)
        .await
        .unwrap();
    assert_eq!(none, []);

    lesson_asking(&env, 1, 1, &[("slider", "s1", 1), ("timeline", "t1", 2)]).await;
    lesson_asking(&env, 2, 2, &[("slider", "s2", 5), ("timeline", "t2", 3)]).await;
    lesson_asking(&env, 3, 1, &[("slider", "s3", 4), ("slider", "s4", 6)]).await;
    lesson_asking(&env, 4, 1, &[("map", "m1", 3), ("globe", "g1", 3)]).await;

    let groups = env
        .lessons
        .component_request_groups(domain::UserId::OWNER, None, 10)
        .await
        .unwrap();
    let names: Vec<_> = groups.iter().map(|g| g.wanted.as_str()).collect();
    // slider 4, timeline 2, then the two single asks tied on time: by name.
    assert_eq!(names, ["slider", "timeline", "globe", "map"]);
    let slider = &groups[0];
    assert_eq!(slider.count, 4);
    assert_eq!(slider.catalog_version, 2);
    assert_eq!(slider.first_requested_at, T0 + time::Duration::seconds(1));
    assert_eq!(slider.last_requested_at, T0 + time::Duration::seconds(6));
    // Three most recent, newest first.
    let purposes: Vec<_> = slider.examples.iter().map(|e| e.purpose.as_str()).collect();
    assert_eq!(purposes, ["s4", "s2", "s3"]);
    assert_eq!(slider.examples[0].topic_title, "T");
    assert_eq!(slider.examples[0].lesson_title, "Stop");
    assert_eq!(slider.examples[0].lesson_id, LessonId::new(uid(0x103)));
}

#[tokio::test]
pub(super) async fn component_request_groups_page_by_cursor() {
    let env = env().await;
    lesson_asking(&env, 1, 1, &[("a", "x", 1), ("b", "x", 1), ("c", "x", 1)]).await;
    lesson_asking(&env, 2, 1, &[("c", "x", 2)]).await;

    let first = env
        .lessons
        .component_request_groups(domain::UserId::OWNER, None, 2)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    let cursor = ComponentRequestCursor::after(&first[1]);
    let rest = env
        .lessons
        .component_request_groups(domain::UserId::OWNER, Some(cursor), 2)
        .await
        .unwrap();
    let all: Vec<_> = first
        .iter()
        .chain(&rest)
        .map(|g| g.wanted.as_str())
        .collect();
    assert_eq!(all, ["c", "a", "b"]);
}

#[tokio::test]
pub(super) async fn deleting_a_lesson_removes_its_requests_from_the_ranking() {
    let env = env().await;
    lesson_asking(&env, 1, 1, &[("slider", "x", 1)]).await;
    sqlx::query("DELETE FROM lessons")
        .execute(&env.pool)
        .await
        .unwrap();
    assert_eq!(
        env.lessons
            .component_request_groups(domain::UserId::OWNER, None, 10)
            .await
            .unwrap(),
        []
    );
}

pub(super) fn attempt(
    lesson: &Lesson,
    n: u128,
    block: &str,
    number: u8,
    outcome: Outcome,
) -> Attempt {
    Attempt {
        id: AttemptId::new(uid(0x400 + n)),
        lesson_id: lesson.id,
        block_id: block.into(),
        kind: if block == "oa" {
            ExerciseKind::OpenAnswer
        } else {
            ExerciseKind::QuizSingle
        },
        attempt_number: number,
        answer: if block == "oa" {
            Answer::Text("perché".into())
        } else {
            Answer::Option("a".into())
        },
        outcome,
        grade: None,
        answered_at: T0,
        created_at: T0 + time::Duration::milliseconds(i64::try_from(n).unwrap()),
    }
}

mod annotations;
mod attempts;
mod learning;
mod lessons;
