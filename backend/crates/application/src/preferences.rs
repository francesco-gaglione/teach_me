//! Preference use cases (`docs/contracts/mission.md` §4). The Tutor creates
//! them through its tool; the user can list and delete.

use std::sync::Arc;

use domain::{
    Clock, Page, PageRequest, Preference, PreferenceCursor, PreferenceId, PreferenceRepository,
    PreferenceScope, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId, UserId,
};

use crate::error::AppError;
use crate::paging::paginate;
use crate::time_util::now_millis;

pub struct PreferenceService {
    preferences: Arc<dyn PreferenceRepository>,
    events: Arc<dyn TopicEventPublisher>,
    clock: Arc<dyn Clock>,
}

impl PreferenceService {
    #[must_use]
    pub fn new(
        preferences: Arc<dyn PreferenceRepository>,
        events: Arc<dyn TopicEventPublisher>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            preferences,
            events,
            clock,
        }
    }

    /// Oldest first. With `topic`, the global preferences and that topic's; an
    /// unknown topic simply has no preferences of its own.
    pub async fn list(
        &self,
        user: UserId,
        topic: Option<TopicId>,
        request: PageRequest<PreferenceCursor>,
    ) -> Result<Page<Preference, PreferenceCursor>, AppError> {
        paginate(
            request,
            |after, limit| async move { self.preferences.list(user, topic, after, limit).await },
            PreferenceCursor::after,
        )
        .await
        .map_err(AppError::from)
    }

    /// Emits `preference_deleted` on the preference's topic, or, for a global
    /// one, on every topic with an open stream.
    pub async fn delete(&self, user: UserId, id: PreferenceId) -> Result<(), AppError> {
        let Some(preference) = self.preferences.get(user, id).await? else {
            return Err(AppError::NotFound);
        };
        if !self.preferences.delete(user, id).await? {
            // Deleted by someone else between the read and the delete.
            return Err(AppError::NotFound);
        }
        let at = now_millis(self.clock.as_ref());
        let payload = TopicEventPayload::PreferenceDeleted {
            id,
            scope: preference.scope,
        };
        match preference.scope {
            PreferenceScope::Topic(topic_id) => {
                self.events.publish(TopicEvent {
                    topic_id,
                    at,
                    payload,
                });
            }
            PreferenceScope::Global => self.events.publish_to_listeners(user, at, payload),
        }
        Ok(())
    }
}
