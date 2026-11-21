//! Events emitted about a topic. Ids and wire format belong to the adapters.

use time::OffsetDateTime;

use crate::chat::ChatMessage;
use crate::ids::{MessageId, PreferenceId, TopicId};
use crate::job::Job;
use crate::mission::Mission;
use crate::preference::{Preference, PreferenceScope};
use crate::topic::Topic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicEventPayload {
    MessageCreated(ChatMessage),
    /// Text to append to the `part_index`-th part of the message.
    MessageDelta {
        message_id: MessageId,
        part_index: usize,
        text: String,
    },
    MessageCompleted(ChatMessage),
    MessageFailed(ChatMessage),
    TopicUpdated(Topic),
    /// A new draft; emitted right before the `message_completed` of the message
    /// that carries its card.
    MissionProposed(Mission),
    MissionConfirmed(Mission),
    /// A job was created, started, progressed, finished or retried: always the
    /// full resource, so applying it is an idempotent upsert.
    JobUpdated(Job),
    PreferenceSaved(Preference),
    PreferenceDeleted {
        id: PreferenceId,
        scope: PreferenceScope,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicEvent {
    pub topic_id: TopicId,
    pub at: OffsetDateTime,
    pub payload: TopicEventPayload,
}
