use std::sync::Arc;

use domain::{FolderCursor, FolderId, IssueKind, PageRequest, TopicCursor, TopicId, TopicState};
use time::macros::datetime;

use crate::error::AppError;
use crate::fakes::{MemRepo, RecordingEvents, SeqIds, TestClock};
use crate::{FolderService, TopicService, UpdateTopic};

struct Fixture {
    folders: FolderService,
    topics: TopicService,
    clock: Arc<TestClock>,
}

fn fixture() -> Fixture {
    let repo = Arc::new(MemRepo::default());
    let clock = Arc::new(TestClock::new(datetime!(2026-11-08 10:00:00.123_456 UTC)));
    let ids = Arc::new(SeqIds::default());
    Fixture {
        folders: FolderService::new(
            repo.clone(),
            repo.clone(),
            Arc::new(RecordingEvents::default()),
            clock.clone(),
            ids.clone(),
        ),
        topics: TopicService::new(
            repo.clone(),
            repo,
            clock.clone(),
            ids,
            Arc::new(RecordingEvents::default()),
        ),
        clock,
    }
}

fn issues(err: AppError) -> Vec<(&'static str, IssueKind)> {
    match err {
        AppError::Validation(v) => v.into_iter().map(|i| (i.field, i.issue)).collect(),
        other => panic!("expected validation error, got {other:?}"),
    }
}

fn page<C>(limit: u32, after: Option<C>) -> PageRequest<C> {
    PageRequest { after, limit }
}

#[tokio::test]
async fn create_folder_assigns_position_and_millisecond_timestamps() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("  Lavoro "))
        .await
        .unwrap();
    let b = f
        .folders
        .create(domain::UserId::OWNER, Some("Studio"))
        .await
        .unwrap();
    assert_eq!(a.folder.name.as_str(), "Lavoro");
    assert_eq!((a.folder.position, b.folder.position), (0, 1));
    assert_eq!(a.topic_count, 0);
    assert_eq!(a.folder.created_at, datetime!(2026-11-08 10:00:00.123 UTC));
    assert_eq!(a.folder.updated_at, a.folder.created_at);
}

#[tokio::test]
async fn create_folder_validation_errors() {
    let f = fixture();
    for (input, kind) in [(None, IssueKind::Empty), (Some("   "), IssueKind::Empty)] {
        assert_eq!(
            issues(
                f.folders
                    .create(domain::UserId::OWNER, input)
                    .await
                    .unwrap_err()
            ),
            vec![("name", kind)]
        );
    }
    let long = "a".repeat(81);
    assert_eq!(
        issues(
            f.folders
                .create(domain::UserId::OWNER, Some(&long))
                .await
                .unwrap_err()
        ),
        vec![("name", IssueKind::TooLong)]
    );
}

#[tokio::test]
async fn folder_names_are_unique_case_insensitively() {
    let f = fixture();
    f.folders
        .create(domain::UserId::OWNER, Some("Lavoro"))
        .await
        .unwrap();
    let err = f
        .folders
        .create(domain::UserId::OWNER, Some("  LAVORO "))
        .await
        .unwrap_err();
    assert_eq!(issues(err), vec![("name", IssueKind::Duplicate)]);
}

#[tokio::test]
async fn rename_folder_to_own_name_in_other_case_is_allowed() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("lavoro"))
        .await
        .unwrap();
    f.clock.set(datetime!(2026-11-09 08:00:00 UTC));
    let r = f
        .folders
        .rename(domain::UserId::OWNER, a.folder.id, Some("Lavoro"))
        .await
        .unwrap();
    assert_eq!(r.folder.name.as_str(), "Lavoro");
    assert_eq!(r.folder.updated_at, datetime!(2026-11-09 08:00:00 UTC));
    assert_eq!(r.folder.created_at, a.folder.created_at);
}

#[tokio::test]
async fn rename_folder_to_other_existing_name_is_duplicate() {
    let f = fixture();
    f.folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap();
    let b = f
        .folders
        .create(domain::UserId::OWNER, Some("B"))
        .await
        .unwrap();
    let err = f
        .folders
        .rename(domain::UserId::OWNER, b.folder.id, Some("a"))
        .await
        .unwrap_err();
    assert_eq!(issues(err), vec![("name", IssueKind::Duplicate)]);
}

#[tokio::test]
async fn empty_folder_patch_and_same_name_do_not_touch_updated_at() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap();
    f.clock.set(datetime!(2026-11-09 08:00:00 UTC));
    let r = f
        .folders
        .rename(domain::UserId::OWNER, a.folder.id, None)
        .await
        .unwrap();
    assert_eq!(r.folder.updated_at, a.folder.updated_at);
    let r = f
        .folders
        .rename(domain::UserId::OWNER, a.folder.id, Some(" A "))
        .await
        .unwrap();
    assert_eq!(r.folder.updated_at, a.folder.updated_at);
}

#[tokio::test]
async fn rename_missing_folder_is_not_found_even_with_invalid_name() {
    let f = fixture();
    let err = f
        .folders
        .rename(
            domain::UserId::OWNER,
            FolderId::new(uuid::Uuid::from_u128(999)),
            Some(""),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound));
}

#[tokio::test]
async fn folder_pagination_is_stable_across_creations_and_deletions() {
    let f = fixture();
    let mut ids = Vec::new();
    for n in 0..5 {
        ids.push(
            f.folders
                .create(domain::UserId::OWNER, Some(&format!("F{n}")))
                .await
                .unwrap()
                .folder
                .id,
        );
    }
    let p1 = f
        .folders
        .list(domain::UserId::OWNER, page(2, None))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    let cursor = p1.next.expect("more pages");
    assert_eq!(cursor, FolderCursor::after(&p1.items[1].folder));

    // The last item of page 1 vanishes and a new folder is appended.
    f.folders
        .delete(domain::UserId::OWNER, p1.items[1].folder.id)
        .await
        .unwrap();
    f.folders
        .create(domain::UserId::OWNER, Some("F5"))
        .await
        .unwrap();

    let p2 = f
        .folders
        .list(domain::UserId::OWNER, page(2, Some(cursor)))
        .await
        .unwrap();
    let names: Vec<_> = p2.items.iter().map(|s| s.folder.name.as_str()).collect();
    assert_eq!(names, ["F2", "F3"]);
    let p3 = f
        .folders
        .list(domain::UserId::OWNER, page(2, p2.next))
        .await
        .unwrap();
    let names: Vec<_> = p3.items.iter().map(|s| s.folder.name.as_str()).collect();
    assert_eq!(names, ["F4", "F5"]);
    assert!(p3.next.is_none());
}

#[tokio::test]
async fn exact_page_boundary_has_no_next_cursor() {
    let f = fixture();
    for n in 0..4 {
        f.folders
            .create(domain::UserId::OWNER, Some(&format!("F{n}")))
            .await
            .unwrap();
    }
    let p = f
        .folders
        .list(domain::UserId::OWNER, page(4, None))
        .await
        .unwrap();
    assert_eq!(p.items.len(), 4);
    assert!(p.next.is_none());
}

#[tokio::test]
async fn topic_count_and_cascade_on_folder_delete() {
    let f = fixture();
    let folder = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let topic = f
        .topics
        .create(domain::UserId::OWNER, folder, Some("T1"))
        .await
        .unwrap();
    f.topics
        .create(domain::UserId::OWNER, folder, Some("T2"))
        .await
        .unwrap();
    assert_eq!(
        f.folders
            .get(domain::UserId::OWNER, folder)
            .await
            .unwrap()
            .topic_count,
        2
    );
    f.folders
        .delete(domain::UserId::OWNER, folder)
        .await
        .unwrap();
    assert!(matches!(
        f.topics.get(domain::UserId::OWNER, topic.id).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        f.folders.delete(domain::UserId::OWNER, folder).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn create_topic_in_new_state_and_does_not_touch_folder() {
    let f = fixture();
    let folder = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap();
    f.clock.set(datetime!(2026-11-09 08:00:00 UTC));
    let t = f
        .topics
        .create(domain::UserId::OWNER, folder.folder.id, Some(" Rust "))
        .await
        .unwrap();
    assert_eq!(t.title.as_str(), "Rust");
    assert_eq!(t.state, TopicState::New);
    let after = f
        .folders
        .get(domain::UserId::OWNER, folder.folder.id)
        .await
        .unwrap();
    assert_eq!(after.folder.updated_at, folder.folder.updated_at);
    assert_eq!(after.topic_count, 1);
}

#[tokio::test]
async fn create_topic_errors() {
    let f = fixture();
    let missing = FolderId::new(uuid::Uuid::from_u128(999));
    assert!(matches!(
        f.topics
            .create(domain::UserId::OWNER, missing, Some(""))
            .await,
        Err(AppError::NotFound)
    ));
    let folder = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    assert_eq!(
        issues(
            f.topics
                .create(domain::UserId::OWNER, folder, None)
                .await
                .unwrap_err()
        ),
        vec![("title", IssueKind::Empty)]
    );
    assert_eq!(
        issues(
            f.topics
                .create(domain::UserId::OWNER, folder, Some(&"x".repeat(121)))
                .await
                .unwrap_err()
        ),
        vec![("title", IssueKind::TooLong)]
    );
    f.topics
        .create(domain::UserId::OWNER, folder, Some("Rust"))
        .await
        .unwrap();
    assert_eq!(
        issues(
            f.topics
                .create(domain::UserId::OWNER, folder, Some("rUST"))
                .await
                .unwrap_err()
        ),
        vec![("title", IssueKind::Duplicate)]
    );
}

#[tokio::test]
async fn same_title_allowed_in_different_folders() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let b = f
        .folders
        .create(domain::UserId::OWNER, Some("B"))
        .await
        .unwrap()
        .folder
        .id;
    f.topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap();
    f.topics
        .create(domain::UserId::OWNER, b, Some("Rust"))
        .await
        .unwrap();
}

#[tokio::test]
async fn list_topics_requires_folder_and_paginates() {
    let f = fixture();
    assert!(matches!(
        f.topics
            .list(
                domain::UserId::OWNER,
                FolderId::new(uuid::Uuid::from_u128(999)),
                page(10, None)
            )
            .await,
        Err(AppError::NotFound)
    ));
    let folder = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    for n in 0..3 {
        f.clock
            .set(datetime!(2026-11-08 10:00 UTC) + time::Duration::seconds(n));
        f.topics
            .create(domain::UserId::OWNER, folder, Some(&format!("T{n}")))
            .await
            .unwrap();
    }
    let p1 = f
        .topics
        .list(domain::UserId::OWNER, folder, page(2, None))
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    assert_eq!(p1.next, Some(TopicCursor::after(&p1.items[1])));
    let p2 = f
        .topics
        .list(domain::UserId::OWNER, folder, page(2, p1.next))
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 1);
    assert_eq!(p2.items[0].title.as_str(), "T2");
    assert!(p2.next.is_none());
}

#[tokio::test]
async fn move_topic_checks_destination_and_title_uniqueness_there() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let b = f
        .folders
        .create(domain::UserId::OWNER, Some("B"))
        .await
        .unwrap()
        .folder
        .id;
    let t = f
        .topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap();
    f.topics
        .create(domain::UserId::OWNER, b, Some("rust"))
        .await
        .unwrap();

    let patch = |title: Option<&str>, folder: Option<String>| UpdateTopic {
        title: title.map(str::to_owned),
        folder_id: folder,
    };

    let err = f
        .topics
        .update(
            domain::UserId::OWNER,
            t.id,
            patch(None, Some(b.to_string())),
        )
        .await
        .unwrap_err();
    assert_eq!(issues(err), vec![("title", IssueKind::Duplicate)]);

    f.clock.set(datetime!(2026-11-09 08:00 UTC));
    let moved = f
        .topics
        .update(
            domain::UserId::OWNER,
            t.id,
            patch(Some("Rust avanzato"), Some(b.to_string())),
        )
        .await
        .unwrap();
    assert_eq!(moved.folder_id, b);
    assert_eq!(moved.title.as_str(), "Rust avanzato");
    assert_eq!(moved.updated_at, datetime!(2026-11-09 08:00 UTC));
    assert_eq!(
        f.folders
            .get(domain::UserId::OWNER, a)
            .await
            .unwrap()
            .topic_count,
        0
    );
    assert_eq!(
        f.folders
            .get(domain::UserId::OWNER, b)
            .await
            .unwrap()
            .topic_count,
        2
    );
}

#[tokio::test]
async fn update_topic_collects_all_problems() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let t = f
        .topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap();
    let missing = uuid::Uuid::from_u128(999).to_string();

    let err = f
        .topics
        .update(
            domain::UserId::OWNER,
            t.id,
            UpdateTopic {
                title: Some("  ".into()),
                folder_id: Some(missing),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        issues(err),
        vec![
            ("title", IssueKind::Empty),
            ("folder_id", IssueKind::NotFound)
        ]
    );

    for bad in ["", "not-a-uuid", "0193-3b4e"] {
        let err = f
            .topics
            .update(
                domain::UserId::OWNER,
                t.id,
                UpdateTopic {
                    title: None,
                    folder_id: Some(bad.into()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(issues(err), vec![("folder_id", IssueKind::Invalid)]);
    }
}

#[tokio::test]
async fn update_topic_noop_keeps_updated_at() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let t = f
        .topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap();
    f.clock.set(datetime!(2026-11-09 08:00 UTC));
    let same = f
        .topics
        .update(
            domain::UserId::OWNER,
            t.id,
            UpdateTopic {
                title: Some("Rust".into()),
                folder_id: Some(a.to_string()),
            },
        )
        .await
        .unwrap();
    assert_eq!(same.updated_at, t.updated_at);
    let empty = f
        .topics
        .update(domain::UserId::OWNER, t.id, UpdateTopic::default())
        .await
        .unwrap();
    assert_eq!(empty, t);
}

#[tokio::test]
async fn rename_topic_case_only_is_not_a_duplicate_of_itself() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let t = f
        .topics
        .create(domain::UserId::OWNER, a, Some("rust"))
        .await
        .unwrap();
    let r = f
        .topics
        .update(
            domain::UserId::OWNER,
            t.id,
            UpdateTopic {
                title: Some("Rust".into()),
                folder_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(r.title.as_str(), "Rust");
}

#[tokio::test]
async fn update_and_delete_missing_topic_are_not_found() {
    let f = fixture();
    let id = TopicId::new(uuid::Uuid::from_u128(999));
    assert!(matches!(
        f.topics
            .update(domain::UserId::OWNER, id, UpdateTopic::default())
            .await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        f.topics.delete(domain::UserId::OWNER, id).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn delete_topic_removes_it() {
    let f = fixture();
    let a = f
        .folders
        .create(domain::UserId::OWNER, Some("A"))
        .await
        .unwrap()
        .folder
        .id;
    let t = f
        .topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap();
    f.topics.delete(domain::UserId::OWNER, t.id).await.unwrap();
    assert!(matches!(
        f.topics.get(domain::UserId::OWNER, t.id).await,
        Err(AppError::NotFound)
    ));
    f.topics
        .create(domain::UserId::OWNER, a, Some("Rust"))
        .await
        .unwrap(); // title is free again
}
