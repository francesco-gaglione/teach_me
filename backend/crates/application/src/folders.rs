use std::sync::Arc;

use domain::{
    Clock, FolderCursor, FolderId, FolderName, FolderRepository, FolderSummary, IdGenerator,
    IssueKind, NewFolder, Page, PageRequest, RepositoryError, TopicEventPublisher, TopicRepository,
    UserId, ValidationIssue,
};

use crate::error::AppError;
use crate::paging::paginate;
use crate::time_util::now_millis;

/// Folder use cases (create, list, get, rename, delete).
pub struct FolderService {
    folders: Arc<dyn FolderRepository>,
    topics: Arc<dyn TopicRepository>,
    events: Arc<dyn TopicEventPublisher>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl FolderService {
    #[must_use]
    pub fn new(
        folders: Arc<dyn FolderRepository>,
        topics: Arc<dyn TopicRepository>,
        events: Arc<dyn TopicEventPublisher>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            folders,
            topics,
            events,
            clock,
            ids,
        }
    }

    /// `name` is `None` when the client omitted it, which counts as empty.
    pub async fn create(
        &self,
        user: UserId,
        name: Option<&str>,
    ) -> Result<FolderSummary, AppError> {
        let name = FolderName::parse(name.unwrap_or_default()).map_err(AppError::validation)?;
        let new = NewFolder {
            id: FolderId::new(self.ids.new_id()),
            user_id: user,
            name,
            created_at: now_millis(self.clock.as_ref()),
        };
        self.folders.insert(new).await.map_err(map_duplicate)
    }

    pub async fn list(
        &self,
        user: UserId,
        request: PageRequest<FolderCursor>,
    ) -> Result<Page<FolderSummary, FolderCursor>, AppError> {
        paginate(
            request,
            |after, limit| async move { self.folders.list(user, after, limit).await },
            |s: &FolderSummary| FolderCursor::after(&s.folder),
        )
        .await
        .map_err(AppError::from)
    }

    pub async fn get(&self, user: UserId, id: FolderId) -> Result<FolderSummary, AppError> {
        self.folders.get(user, id).await?.ok_or(AppError::NotFound)
    }

    /// `name` of `None` leaves the folder untouched (PATCH semantics).
    pub async fn rename(
        &self,
        user: UserId,
        id: FolderId,
        name: Option<&str>,
    ) -> Result<FolderSummary, AppError> {
        let mut summary = self.get(user, id).await?;
        let Some(raw) = name else {
            return Ok(summary);
        };
        let name = FolderName::parse(raw).map_err(AppError::validation)?;
        let now = now_millis(self.clock.as_ref());
        if summary.folder.rename(name, now) {
            self.folders
                .update(&summary.folder)
                .await
                .map_err(map_duplicate)?;
        }
        Ok(summary)
    }

    pub async fn delete(&self, user: UserId, id: FolderId) -> Result<(), AppError> {
        // The cascade removes the topics; read their ids first so their event
        // streams can be closed afterwards.
        let topic_ids = self.topics.ids_in_folder(user, id).await?;
        if self.folders.delete(user, id).await? {
            for topic_id in topic_ids {
                self.events.topic_closed(topic_id);
            }
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }
}

fn map_duplicate(err: RepositoryError) -> AppError {
    match err {
        RepositoryError::Duplicate => {
            AppError::validation(ValidationIssue::new("name", IssueKind::Duplicate))
        }
        other => other.into(),
    }
}
