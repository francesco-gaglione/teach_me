use crate::event::{TopicEvent, TopicEventPayload};
use crate::ids::{TopicId, UserId};

/// Publishes events of a topic to whoever is listening (SSE streams).
///
/// Synchronous on purpose: publishing must never wait on a slow consumer, and
/// the implementations are in-memory.
pub trait TopicEventPublisher: Send + Sync {
    /// Returns the id the event was given, or `None` if it was dropped (the
    /// topic is closed).
    fn publish(&self, event: TopicEvent) -> Option<u64>;

    /// Id of the last event emitted for the topic in this run; the run's id base
    /// if there is none yet. Always a valid `Last-Event-ID`.
    fn latest_event_id(&self, topic_id: TopicId) -> u64;

    /// Publishes `payload` on every topic of `user` that has a stream open
    /// right now (a change that concerns all of the user's topics, such as
    /// deleting a global preference). Topics nobody is listening to are
    /// skipped: they reload on open. Streams of other users never see it.
    fn publish_to_listeners(
        &self,
        user: UserId,
        at: time::OffsetDateTime,
        payload: TopicEventPayload,
    );

    /// The topic no longer exists: drop what is buffered and end its streams.
    fn topic_closed(&self, topic_id: TopicId);
}
