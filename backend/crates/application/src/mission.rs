//! Mission use cases (`docs/contracts/mission.md` §4). Drafts are created by
//! the Tutor's tools (see `tutor_tools`); the user's side is here.

use std::sync::Arc;

use domain::{
    Clock, IssueKind, JobKind, Mission, MissionCursor, MissionRepository, MissionRuleError, Page,
    PageRequest, RepositoryError, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId,
    TopicMission, TopicRepository, UserId, ValidationIssue,
};

use crate::error::{AppError, ConflictReason};
use crate::jobs::JobService;
use crate::paging::paginate;
use crate::time_util::now_millis;

pub struct MissionService {
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    events: Arc<dyn TopicEventPublisher>,
    clock: Arc<dyn Clock>,
    jobs: Arc<JobService>,
}

impl MissionService {
    #[must_use]
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        events: Arc<dyn TopicEventPublisher>,
        clock: Arc<dyn Clock>,
        jobs: Arc<JobService>,
    ) -> Self {
        Self {
            topics,
            missions,
            events,
            clock,
            jobs,
        }
    }

    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<domain::Topic, AppError> {
        self.topics.get(user, id).await?.ok_or(AppError::NotFound)
    }

    pub async fn get(&self, user: UserId, topic_id: TopicId) -> Result<TopicMission, AppError> {
        self.require_topic(user, topic_id).await?;
        self.snapshot(topic_id).await
    }

    async fn snapshot(&self, topic_id: TopicId) -> Result<TopicMission, AppError> {
        Ok(TopicMission {
            current: self.missions.current(topic_id).await?,
            draft: self.missions.open_draft(topic_id).await?,
        })
    }

    /// Newest version first.
    pub async fn versions(
        &self,
        user: UserId,
        topic_id: TopicId,
        request: PageRequest<MissionCursor>,
    ) -> Result<Page<Mission, MissionCursor>, AppError> {
        self.require_topic(user, topic_id).await?;
        paginate(
            request,
            |after, limit| async move { self.missions.list_versions(topic_id, after, limit).await },
            MissionCursor::after,
        )
        .await
        .map_err(AppError::from)
    }

    /// Confirms exactly `version`, which must be the open draft. `None` (the
    /// client sent no usable version) is a validation error.
    ///
    /// The first confirmation moves the topic to `researching` and queues its
    /// `research` job, all in the same transaction as the mission; confirming a
    /// change to an already confirmed mission leaves the state alone and starts
    /// no research (contract §5). No chat message is produced.
    pub async fn confirm(
        &self,
        user: UserId,
        topic_id: TopicId,
        version: Option<u32>,
    ) -> Result<TopicMission, AppError> {
        let mut topic = self.require_topic(user, topic_id).await?;
        let version = version.ok_or_else(|| {
            AppError::validation(ValidationIssue::new("version", IssueKind::Invalid))
        })?;

        let Some(mut draft) = self.missions.open_draft(topic_id).await? else {
            return Err(AppError::Conflict(ConflictReason::MissionNotOpen));
        };
        if draft.version != version {
            return Err(AppError::Conflict(ConflictReason::MissionNotOpen));
        }
        let previous = self.missions.current(topic_id).await?;

        let now = now_millis(self.clock.as_ref());
        let state_change = if previous.is_none() {
            // Checked before anything is stored, so a refusal leaves no trace.
            Some(
                topic
                    .confirm_mission(now)
                    .map_err(|_| AppError::Conflict(ConflictReason::MissionStateMismatch))?,
            )
        } else {
            None
        };
        // Only the first confirmation researches; a mission change leaves the
        // sources alone until the contract says otherwise.
        let follow_up = state_change.map(|_| self.jobs.new_job(topic_id, JobKind::Research));
        draft.confirm(now).map_err(|e| match e {
            MissionRuleError::NotDraft | MissionRuleError::AlreadySuperseded => {
                AppError::Conflict(ConflictReason::MissionNotOpen)
            }
        })?;
        // One transaction: the mission and the topic's new state are stored
        // together or not at all.
        self.missions
            .confirm(&draft, now, state_change, follow_up.as_ref())
            .await
            .map_err(|e| match e {
                RepositoryError::Conflict => AppError::Conflict(ConflictReason::MissionNotOpen),
                RepositoryError::Duplicate => AppError::Conflict(ConflictReason::JobActive),
                RepositoryError::NotFound | RepositoryError::MissingParent => AppError::NotFound,
                other @ (RepositoryError::Storage(_) | RepositoryError::RuleViolated) => {
                    other.into()
                }
            })?;
        self.publish(topic_id, now, TopicEventPayload::MissionConfirmed(draft));
        if state_change.is_some() {
            match self.topics.get(user, topic_id).await {
                Ok(Some(fresh)) => {
                    self.publish(topic_id, now, TopicEventPayload::TopicUpdated(fresh));
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "cannot reload topic after confirmation"),
            }
        }
        if let Some(job) = &follow_up {
            self.jobs.announce(job);
        }
        self.snapshot(topic_id).await
    }

    fn publish(&self, topic_id: TopicId, at: time::OffsetDateTime, payload: TopicEventPayload) {
        self.events.publish(TopicEvent {
            topic_id,
            at,
            payload,
        });
    }
}
