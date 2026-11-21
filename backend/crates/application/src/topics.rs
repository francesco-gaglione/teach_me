use std::sync::Arc;

use domain::{
    Clock, ContentLanguage, FolderId, FolderRepository, IdGenerator, IssueKind, Page, PageRequest,
    RepositoryError, Topic, TopicCursor, TopicEvent, TopicEventPayload, TopicEventPublisher,
    TopicId, TopicRepository, TopicTitle, UserId, ValidationIssue, parse_uuid,
};

use crate::error::AppError;
use crate::paging::paginate;
use crate::time_util::now_millis;

/// Topic use cases (create, list, get, update/move, delete).
pub struct TopicService {
    topics: Arc<dyn TopicRepository>,
    folders: Arc<dyn FolderRepository>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    events: Arc<dyn TopicEventPublisher>,
}

/// PATCH input. `None` means "leave unchanged". The HTTP layer maps an
/// explicit JSON `null` to `Some("")`, which fails validation the same way
/// the contract wants (`empty` for the title, `invalid` for `folder_id`).
#[derive(Debug, Default, Clone)]
pub struct UpdateTopic {
    pub title: Option<String>,
    pub folder_id: Option<String>,
}

impl TopicService {
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        folders: Arc<dyn FolderRepository>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
        events: Arc<dyn TopicEventPublisher>,
    ) -> Self {
        Self {
            topics,
            folders,
            clock,
            ids,
            events,
        }
    }

    /// A topic in the default language (Italian).
    pub async fn create(
        &self,
        user: UserId,
        folder_id: FolderId,
        title: Option<&str>,
    ) -> Result<Topic, AppError> {
        self.create_with_language(user, folder_id, title, None)
            .await
    }

    /// `language` is the raw code from the request: `None` means the default,
    /// anything but `it`/`en` is a `language: invalid` validation error.
    pub async fn create_with_language(
        &self,
        user: UserId,
        folder_id: FolderId,
        title: Option<&str>,
        language: Option<&str>,
    ) -> Result<Topic, AppError> {
        // Unknown folder wins over validation errors: the resource itself is missing.
        self.require_folder(user, folder_id).await?;
        // Every problem at once, like the update does.
        let mut issues = Vec::new();
        let title = match TopicTitle::parse(title.unwrap_or_default()) {
            Ok(t) => Some(t),
            Err(issue) => {
                issues.push(issue);
                None
            }
        };
        let language = match language.map(str::parse::<ContentLanguage>) {
            None => Some(ContentLanguage::default()),
            Some(Ok(l)) => Some(l),
            Some(Err(_)) => {
                issues.push(ValidationIssue::new("language", IssueKind::Invalid));
                None
            }
        };
        let (Some(title), Some(language)) = (title, language) else {
            return Err(AppError::Validation(issues));
        };
        let topic = Topic::new(
            TopicId::new(self.ids.new_id()),
            user,
            folder_id,
            title,
            now_millis(self.clock.as_ref()),
        )
        .with_language(language);
        self.topics.insert(&topic).await.map_err(|e| match e {
            RepositoryError::Duplicate => duplicate_title(),
            RepositoryError::MissingParent => AppError::NotFound,
            other => other.into(),
        })?;
        Ok(topic)
    }

    pub async fn list(
        &self,
        user: UserId,
        folder_id: FolderId,
        request: PageRequest<TopicCursor>,
    ) -> Result<Page<Topic, TopicCursor>, AppError> {
        self.require_folder(user, folder_id).await?;
        paginate(
            request,
            |after, limit| async move {
                self.topics
                    .list_by_folder(user, folder_id, after, limit)
                    .await
            },
            TopicCursor::after,
        )
        .await
        .map_err(AppError::from)
    }

    pub async fn get(&self, user: UserId, id: TopicId) -> Result<Topic, AppError> {
        self.topics.get(user, id).await?.ok_or(AppError::NotFound)
    }

    pub async fn update(
        &self,
        user: UserId,
        id: TopicId,
        patch: UpdateTopic,
    ) -> Result<Topic, AppError> {
        let mut topic = self.get(user, id).await?;

        // Collect every problem before failing, so the client sees them all.
        let mut issues = Vec::new();
        let title = match patch.title.as_deref().map(TopicTitle::parse) {
            None => None,
            Some(Ok(t)) => Some(t),
            Some(Err(issue)) => {
                issues.push(issue);
                None
            }
        };
        let destination = match patch.folder_id.as_deref() {
            None => None,
            Some(raw) => match parse_folder_id(raw) {
                None => {
                    issues.push(ValidationIssue::new("folder_id", IssueKind::Invalid));
                    None
                }
                Some(folder_id) => {
                    if folder_id != topic.folder_id
                        && self.folders.get(user, folder_id).await?.is_none()
                    {
                        issues.push(ValidationIssue::new("folder_id", IssueKind::NotFound));
                        None
                    } else {
                        Some(folder_id)
                    }
                }
            },
        };
        if !issues.is_empty() {
            return Err(AppError::Validation(issues));
        }

        let now = now_millis(self.clock.as_ref());
        if topic.apply(title, destination, now) {
            self.topics.update(&topic).await.map_err(|e| match e {
                RepositoryError::Duplicate => duplicate_title(),
                RepositoryError::MissingParent => {
                    // The destination vanished between the check and the write.
                    AppError::validation(ValidationIssue::new("folder_id", IssueKind::NotFound))
                }
                other => other.into(),
            })?;
            self.events.publish(TopicEvent {
                topic_id: topic.id,
                at: now,
                payload: TopicEventPayload::TopicUpdated(topic.clone()),
            });
        }
        Ok(topic)
    }

    pub async fn delete(&self, user: UserId, id: TopicId) -> Result<(), AppError> {
        if self.topics.delete(user, id).await? {
            self.events.topic_closed(id);
            Ok(())
        } else {
            Err(AppError::NotFound)
        }
    }

    async fn require_folder(&self, user: UserId, id: FolderId) -> Result<(), AppError> {
        self.folders
            .get(user, id)
            .await?
            .map(|_| ())
            .ok_or(AppError::NotFound)
    }
}

fn duplicate_title() -> AppError {
    AppError::validation(ValidationIssue::new("title", IssueKind::Duplicate))
}

fn parse_folder_id(raw: &str) -> Option<FolderId> {
    parse_uuid(raw).map(FolderId::new)
}
