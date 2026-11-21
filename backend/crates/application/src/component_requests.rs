//! The ranking of components the catalog is missing
//! (`docs/contracts/component-requests.md`).

use std::sync::Arc;

use domain::{
    ComponentRequestCursor, ComponentRequestGroup, LessonRepository, Page, PageRequest, UserId,
};

use crate::error::AppError;
use crate::paging::paginate;

pub struct ComponentRequestService {
    lessons: Arc<dyn LessonRepository>,
}

impl ComponentRequestService {
    #[must_use]
    pub fn new(lessons: Arc<dyn LessonRepository>) -> Self {
        Self { lessons }
    }

    /// Most requested first; ties go to the most recent, then to the name.
    pub async fn ranking(
        &self,
        user: UserId,
        request: PageRequest<ComponentRequestCursor>,
    ) -> Result<Page<ComponentRequestGroup, ComponentRequestCursor>, AppError> {
        paginate(
            request,
            |after, limit| async move {
                self.lessons
                    .component_request_groups(user, after, limit)
                    .await
            },
            ComponentRequestCursor::after,
        )
        .await
        .map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        ComponentRequest, ComponentRequestId, FolderId, GeneratedLesson, JobId, LessonId,
        LessonMeta, LessonRepository, Topic, TopicId, TopicTitle,
    };
    use serde_json::json;
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;
    use crate::fakes::MemRepo;

    fn lesson(n: u128, wanted: &[&str]) -> GeneratedLesson {
        let at = datetime!(2026-11-09 10:00 UTC);
        let id = LessonId::new(Uuid::from_u128(n));
        let topic_id = TopicId::new(Uuid::from_u128(1));
        let document = json!({"catalog_version": 1, "title": "L", "objective": "O",
            "estimated_minutes": 5, "blocks": []});
        GeneratedLesson {
            quality_warnings: vec![],
            id,
            topic_id,
            job_id: JobId::new(Uuid::from_u128(0x200 + n)),
            meta: LessonMeta::from_document(&document).unwrap(),
            mission_version: 1,
            document,
            plan: json!({}),
            component_requests: wanted
                .iter()
                .enumerate()
                .map(|(i, w)| ComponentRequest {
                    id: ComponentRequestId::new(Uuid::from_u128(n * 100 + i as u128)),
                    topic_id,
                    lesson_id: id,
                    block_id: format!("m{i}"),
                    wanted: (*w).into(),
                    purpose: "p".into(),
                    fallback: json!([]),
                    created_at: at,
                })
                .collect(),
            created_at: at,
        }
    }

    #[tokio::test]
    async fn the_ranking_pages_without_repeating_or_skipping() {
        let repo = Arc::new(MemRepo::default());
        repo.0.lock().unwrap().topics.push(Topic::new(
            TopicId::new(Uuid::from_u128(1)),
            domain::UserId::OWNER,
            FolderId::new(Uuid::from_u128(9)),
            TopicTitle::parse("Rust").unwrap(),
            datetime!(2026-11-09 09:00 UTC),
        ));
        repo.insert_generated(&lesson(1, &["b", "a"]))
            .await
            .unwrap();
        repo.insert_generated(&lesson(2, &["b", "c"]))
            .await
            .unwrap();
        let service = ComponentRequestService::new(repo);

        let first = service
            .ranking(
                domain::UserId::OWNER,
                PageRequest {
                    after: None,
                    limit: 2,
                },
            )
            .await
            .unwrap();
        let names: Vec<_> = first.items.iter().map(|g| g.wanted.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
        assert_eq!(first.items[0].count, 2);
        let second = service
            .ranking(
                domain::UserId::OWNER,
                PageRequest {
                    after: first.next,
                    limit: 2,
                },
            )
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].wanted, "c");
        assert!(second.next.is_none());
    }
}
