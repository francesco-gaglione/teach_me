//! In-memory event hub: one broadcast channel and one replay buffer per topic.
//!
//! Event ids are consecutive integers per topic, assigned here. They start at
//! `start_ms * 1000 + 1`, where `start_ms` is the hub's creation instant, so ids
//! are monotonic across restarts (contract `chat.md` §4). Publishing and subscribing
//! take the same lock, so a subscriber sees every event exactly once: the
//! backlog it is handed plus whatever arrives on its live receiver afterwards.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use domain::{Clock, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId, UserId};
use time::OffsetDateTime;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy)]
pub struct EventHubConfig {
    /// Events always kept per topic (the contract's "last 500 events").
    pub min_events: usize,
    /// Events younger than this are kept even beyond `min_events` (the
    /// contract's "10 minutes").
    pub min_age: Duration,
    /// Memory safety net over the two rules above: a long, fast stream cannot
    /// grow the buffer without bound.
    pub hard_cap: usize,
    /// Capacity of the live channel; a subscriber that falls this far behind
    /// is disconnected and replays from the buffer when it reconnects.
    pub live_capacity: usize,
}

impl Default for EventHubConfig {
    fn default() -> Self {
        Self {
            min_events: 500,
            min_age: Duration::from_secs(600),
            hard_cap: 10_000,
            live_capacity: 2048,
        }
    }
}

/// An event with the id the hub gave it.
#[derive(Debug, PartialEq, Eq)]
pub struct StoredEvent {
    pub id: u64,
    pub event: TopicEvent,
}

/// What a new stream starts with.
pub struct Subscription {
    /// Events the client missed, oldest first.
    pub backlog: Vec<Arc<StoredEvent>>,
    /// Set when the client must reload state via REST. Holds the id of the
    /// latest event emitted so far (0 when none) so the client can adopt it as
    /// its new position.
    pub resync_at: Option<u64>,
    /// Events published from now on.
    pub live: broadcast::Receiver<Arc<StoredEvent>>,
}

struct Stored {
    event: Arc<StoredEvent>,
    at: OffsetDateTime,
}

struct Channel {
    /// Id of the next event, at least `id_base + 1`. Never reset while the process lives, even when
    /// the buffer is cleared, so ids are never reused within a run.
    next_id: u64,
    buffer: VecDeque<Stored>,
    tx: broadcast::Sender<Arc<StoredEvent>>,
    /// Who opened a stream on the topic (the handler checked they own it).
    /// Lets a change that concerns all of a user's topics reach only their
    /// streams. `None` until somebody subscribes.
    owner: Option<UserId>,
}

impl Channel {
    fn latest_id(&self) -> u64 {
        self.next_id - 1
    }
}

struct Hub {
    channels: HashMap<TopicId, Channel>,
    /// Topics that no longer exist: a late event of an in-flight generation
    /// must not bring their channel back.
    closed: HashSet<TopicId>,
    /// Set by `close_all`: nothing is delivered any more.
    shutdown: bool,
}

pub struct EventHub {
    clock: Arc<dyn Clock>,
    config: EventHubConfig,
    age_limit: time::Duration,
    /// Every id of this run is greater than this; every id of an earlier run
    /// (hub created earlier) is smaller than the first id of this one.
    id_base: u64,
    state: Mutex<Hub>,
}

impl EventHub {
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>, config: EventHubConfig) -> Self {
        let age_limit = time::Duration::try_from(config.min_age).unwrap_or(time::Duration::MAX);
        // Unix milliseconds * 1000 stays below 2^53 until the year 2255.
        let millis = clock.now().unix_timestamp_nanos() / 1_000_000;
        let id_base = u64::try_from(millis).unwrap_or(0) * 1000;
        Self {
            clock,
            config,
            age_limit,
            id_base,
            state: Mutex::new(Hub {
                channels: HashMap::new(),
                closed: HashSet::new(),
                shutdown: false,
            }),
        }
    }

    /// Id just below the first one this run emits.
    #[must_use]
    pub fn id_base(&self) -> u64 {
        self.id_base
    }

    /// Opens a stream on `topic_id` for `user`, who must own it (the caller
    /// checks). `last_event_id` is the client's `Last-Event-ID`; callers map an
    /// unparsable header to `u64::MAX`, which is always "ahead of us" and
    /// therefore a resync.
    #[must_use]
    pub fn subscribe(
        &self,
        user: UserId,
        topic_id: TopicId,
        last_event_id: Option<u64>,
    ) -> Subscription {
        let now = self.clock.now();
        let mut hub = self.lock();
        if hub.shutdown || hub.closed.contains(&topic_id) {
            // A receiver whose sender is already gone: the stream ends at once.
            return Subscription {
                backlog: Vec::new(),
                resync_at: None,
                live: broadcast::channel(1).1,
            };
        }
        let channel = self.channel(&mut hub.channels, topic_id);
        channel.owner = Some(user);
        self.prune(channel, now);

        let latest = channel.latest_id();
        let mut backlog = Vec::new();
        let mut resync_at = None;
        match last_event_id {
            None => {}
            Some(n) if n == latest => {}
            // Ahead of us: the process restarted (ids restart at 1) or the
            // client sent garbage. Either way it cannot be trusted.
            Some(n) if n > latest => resync_at = Some(latest),
            Some(n) => {
                // Replay is only valid if nothing between `n` and the buffer
                // start was dropped.
                if channel.buffer.front().is_some_and(|s| s.event.id <= n + 1) {
                    backlog = channel
                        .buffer
                        .iter()
                        .filter(|s| s.event.id > n)
                        .map(|s| Arc::clone(&s.event))
                        .collect();
                } else {
                    resync_at = Some(latest);
                }
            }
        }
        Subscription {
            backlog,
            resync_at,
            live: channel.tx.subscribe(),
        }
    }

    /// Ends every stream (graceful shutdown: open SSE connections would
    /// otherwise keep the server from stopping).
    pub fn close_all(&self) {
        let mut hub = self.lock();
        hub.shutdown = true;
        hub.channels.clear();
    }

    fn lock(&self) -> MutexGuard<'_, Hub> {
        // A poisoned lock only means another thread panicked mid-update of
        // plain data; the buffers stay structurally valid, so keep serving.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn channel<'a>(
        &self,
        channels: &'a mut HashMap<TopicId, Channel>,
        topic_id: TopicId,
    ) -> &'a mut Channel {
        channels.entry(topic_id).or_insert_with(|| Channel {
            next_id: self.id_base + 1,
            buffer: VecDeque::new(),
            tx: broadcast::channel(self.config.live_capacity).0,
            owner: None,
        })
    }

    /// Applies the retention rule: keep an event if it is among the last
    /// `min_events` or younger than `min_age`, but never more than `hard_cap`.
    /// An idle topic (nobody listening, newest event expired) is emptied.
    fn prune(&self, channel: &mut Channel, now: OffsetDateTime) {
        while channel.buffer.len() > self.config.hard_cap {
            channel.buffer.pop_front();
        }
        while channel.buffer.len() > self.config.min_events
            && channel
                .buffer
                .front()
                .is_some_and(|s| now - s.at >= self.age_limit)
        {
            channel.buffer.pop_front();
        }
        let idle_and_expired = channel.tx.receiver_count() == 0
            && channel
                .buffer
                .back()
                .is_some_and(|s| now - s.at >= self.age_limit);
        if idle_and_expired {
            channel.buffer.clear();
        }
    }
}

impl TopicEventPublisher for EventHub {
    fn publish(&self, event: TopicEvent) -> Option<u64> {
        let now = self.clock.now();
        let mut hub = self.lock();
        if hub.shutdown || hub.closed.contains(&event.topic_id) {
            return None;
        }
        let channel = self.channel(&mut hub.channels, event.topic_id);
        let stored = Arc::new(StoredEvent {
            id: channel.next_id,
            event,
        });
        channel.next_id += 1;
        channel.buffer.push_back(Stored {
            event: Arc::clone(&stored),
            at: now,
        });
        self.prune(channel, now);
        // No receivers is the normal case when nobody has the topic open.
        let id = stored.id;
        let _ = channel.tx.send(stored);
        Some(id)
    }

    fn latest_event_id(&self, topic_id: TopicId) -> u64 {
        // A topic with no events yet reports the run's base: a valid position
        // for `Last-Event-ID` (nothing to replay), unlike an id of an old run.
        self.lock()
            .channels
            .get(&topic_id)
            .map_or(self.id_base, Channel::latest_id)
    }

    fn publish_to_listeners(&self, user: UserId, at: OffsetDateTime, payload: TopicEventPayload) {
        let now = self.clock.now();
        let mut hub = self.lock();
        if hub.shutdown {
            return;
        }
        for (topic_id, channel) in &mut hub.channels {
            if channel.tx.receiver_count() == 0 || channel.owner != Some(user) {
                continue;
            }
            let stored = Arc::new(StoredEvent {
                id: channel.next_id,
                event: TopicEvent {
                    topic_id: *topic_id,
                    at,
                    payload: payload.clone(),
                },
            });
            channel.next_id += 1;
            channel.buffer.push_back(Stored {
                event: Arc::clone(&stored),
                at: now,
            });
            let _ = channel.tx.send(stored);
        }
    }

    fn topic_closed(&self, topic_id: TopicId) {
        let mut hub = self.lock();
        hub.channels.remove(&topic_id);
        hub.closed.insert(topic_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::TopicEventPayload;
    use domain::UserId;
    use time::macros::datetime;
    use uuid::Uuid;

    struct ManualClock(Mutex<OffsetDateTime>);
    impl Clock for ManualClock {
        fn now(&self) -> OffsetDateTime {
            *self.0.lock().unwrap()
        }
    }

    /// The epoch makes the id base 0, so the tests below can use small literal ids;
    /// the restart tests use a real date.
    const T0: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn hub(config: EventHubConfig) -> (EventHub, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock(Mutex::new(T0)));
        (EventHub::new(clock.clone(), config), clock)
    }

    fn small() -> EventHubConfig {
        EventHubConfig {
            min_events: 3,
            min_age: Duration::from_secs(600),
            hard_cap: 10,
            live_capacity: 16,
        }
    }

    fn topic(n: u128) -> TopicId {
        TopicId::new(Uuid::from_u128(n))
    }

    fn emit(hub: &EventHub, t: TopicId, n: usize) {
        for _ in 0..n {
            hub.publish(TopicEvent {
                topic_id: t,
                at: T0,
                payload: TopicEventPayload::MessageDelta {
                    message_id: domain::MessageId::new(Uuid::nil()),
                    part_index: 0,
                    text: "x".into(),
                },
            });
        }
    }

    fn ids(backlog: &[Arc<StoredEvent>]) -> Vec<u64> {
        backlog.iter().map(|e| e.id).collect()
    }

    #[test]
    fn ids_are_consecutive_per_topic() {
        let (hub, _) = hub(small());
        emit(&hub, topic(1), 2);
        emit(&hub, topic(2), 1);
        emit(&hub, topic(1), 1);
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(1), Some(0)).backlog),
            [1, 2, 3]
        );
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(2), Some(0)).backlog),
            [1]
        );
    }

    #[test]
    fn without_last_event_id_only_future_events_arrive() {
        let (hub, _) = hub(small());
        emit(&hub, topic(1), 2);
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), None);
        assert!(sub.backlog.is_empty() && sub.resync_at.is_none());
        assert!(sub.live.try_recv().is_err());
        emit(&hub, topic(1), 1);
        assert_eq!(sub.live.try_recv().unwrap().id, 3);
    }

    #[test]
    fn reconnect_replays_missed_events_then_continues_live() {
        let (hub, _) = hub(small());
        emit(&hub, topic(1), 3);
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), Some(1));
        assert_eq!(ids(&sub.backlog), [2, 3]);
        assert!(sub.resync_at.is_none());
        emit(&hub, topic(1), 1);
        assert_eq!(sub.live.try_recv().unwrap().id, 4);
        assert!(sub.live.try_recv().is_err());
    }

    #[test]
    fn up_to_date_client_gets_nothing_to_replay() {
        let (hub, _) = hub(small());
        emit(&hub, topic(1), 2);
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(2));
        assert!(sub.backlog.is_empty() && sub.resync_at.is_none());
    }

    #[test]
    fn id_ahead_of_the_hub_means_resync() {
        let (hub, _) = hub(small());
        // Fresh process, client remembers an id from before the restart.
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(7));
        assert_eq!(sub.resync_at, Some(0));
        emit(&hub, topic(1), 2);
        assert_eq!(
            hub.subscribe(UserId::OWNER, topic(1), Some(7)).resync_at,
            Some(2)
        );
        assert_eq!(
            hub.subscribe(UserId::OWNER, topic(1), Some(u64::MAX))
                .resync_at,
            Some(2)
        );
    }

    #[test]
    fn id_older_than_the_window_means_resync() {
        let (hub, clock) = hub(small());
        emit(&hub, topic(1), 4);
        // Within the age limit the buffer keeps everything; once old, only the
        // last `min_events` survive.
        *clock.0.lock().unwrap() = T0 + time::Duration::minutes(11);
        emit(&hub, topic(1), 1); // id 5; prune drops 1 and 2 (old, beyond 3)
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(1));
        assert_eq!(sub.resync_at, Some(5));
        assert_eq!(sub.backlog.len(), 0);
        // Id 2 is still contiguous with the buffer start (3).
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(1), Some(2)).backlog),
            [3, 4, 5]
        );
    }

    #[test]
    fn recent_events_survive_beyond_the_count_but_not_the_hard_cap() {
        let (hub, _) = hub(small());
        emit(&hub, topic(1), 8);
        // All younger than 10 minutes: kept although min_events is 3.
        assert_eq!(
            hub.subscribe(UserId::OWNER, topic(1), Some(0))
                .backlog
                .len(),
            8
        );
        emit(&hub, topic(1), 5);
        // 13 published, hard cap 10: the oldest three are gone.
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(0));
        assert_eq!(sub.resync_at, Some(13));
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(1), Some(3)).backlog),
            (4..=13).collect::<Vec<_>>()
        );
    }

    #[test]
    fn idle_expired_topic_is_cleared_but_ids_keep_growing() {
        let (hub, clock) = hub(small());
        emit(&hub, topic(1), 2);
        *clock.0.lock().unwrap() = T0 + time::Duration::minutes(20);
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(1));
        assert_eq!(sub.resync_at, Some(2));
        drop(sub);
        emit(&hub, topic(1), 1);
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(1), Some(2)).backlog),
            [3]
        );
    }

    #[test]
    fn a_broadcast_reaches_only_topics_with_an_open_stream() {
        let (hub, _) = hub(small());
        let (listening, idle) = (topic(1), topic(2));
        let mut open = hub.subscribe(UserId::OWNER, listening, None);
        emit(&hub, idle, 1); // the channel exists, but nobody listens to it
        let payload = TopicEventPayload::PreferenceDeleted {
            id: domain::PreferenceId::new(Uuid::from_u128(9)),
            scope: domain::PreferenceScope::Global,
        };
        hub.publish_to_listeners(UserId::OWNER, T0, payload.clone());

        let got = open.live.try_recv().unwrap();
        assert_eq!(
            got.event.topic_id, listening,
            "stamped with the topic it reaches"
        );
        assert_eq!(got.event.payload, payload);
        assert_eq!(got.id, 1);
        // The idle topic keeps its own numbering: no event, no gap.
        assert_eq!(hub.latest_event_id(idle), 1);
        emit(&hub, idle, 1);
        assert_eq!(hub.latest_event_id(idle), 2);
        // The broadcast is part of the replay buffer of the topic it reached.
        let replay = hub.subscribe(UserId::OWNER, listening, Some(0));
        assert_eq!(replay.backlog.len(), 1);
        hub.close_all();
        hub.publish_to_listeners(UserId::OWNER, T0, payload);
    }

    #[test]
    fn a_broadcast_never_reaches_the_streams_of_another_user() {
        let (hub, _) = hub(small());
        let other = UserId::new(Uuid::from_u128(77));
        let mut mine = hub.subscribe(UserId::OWNER, topic(1), None);
        let mut theirs = hub.subscribe(other, topic(2), None);
        let payload = TopicEventPayload::PreferenceDeleted {
            id: domain::PreferenceId::new(Uuid::from_u128(9)),
            scope: domain::PreferenceScope::Global,
        };
        hub.publish_to_listeners(UserId::OWNER, T0, payload);
        assert!(mine.live.try_recv().is_ok());
        assert!(matches!(
            theirs.live.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert_eq!(hub.latest_event_id(topic(2)), hub.id_base());
    }

    #[test]
    fn closing_a_topic_ends_its_streams() {
        let (hub, _) = hub(small());
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), None);
        emit(&hub, topic(1), 1);
        hub.topic_closed(topic(1));
        assert_eq!(sub.live.try_recv().unwrap().id, 1);
        assert!(matches!(
            sub.live.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
        hub.close_all();
    }

    #[test]
    fn a_late_event_does_not_resurrect_a_closed_topic() {
        let (hub, _) = hub(small());
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), None);
        hub.topic_closed(topic(1));
        emit(&hub, topic(1), 2); // an in-flight generation still publishing
        assert!(matches!(
            sub.live.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
        assert_eq!(hub.lock().channels.len(), 0, "no orphan channel");
        let mut later = hub.subscribe(UserId::OWNER, topic(1), None);
        assert!(matches!(
            later.live.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
    }

    #[test]
    fn subscribing_after_close_all_ends_at_once() {
        let (hub, _) = hub(small());
        hub.close_all();
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), None);
        assert!(matches!(
            sub.live.try_recv(),
            Err(broadcast::error::TryRecvError::Closed)
        ));
        emit(&hub, topic(1), 1);
        assert_eq!(hub.lock().channels.len(), 0);
    }

    #[test]
    fn slow_subscriber_lags_instead_of_blocking_publishers() {
        let (hub, _) = hub(EventHubConfig {
            live_capacity: 2,
            ..small()
        });
        let mut sub = hub.subscribe(UserId::OWNER, topic(1), None);
        emit(&hub, topic(1), 5);
        assert!(matches!(
            sub.live.try_recv(),
            Err(broadcast::error::TryRecvError::Lagged(_))
        ));
    }

    #[test]
    fn id_base_is_unix_millis_times_1000_and_ids_start_just_above_it() {
        let clock = Arc::new(ManualClock(Mutex::new(
            datetime!(2026-11-08 10:00:00.123 UTC),
        )));
        let hub = EventHub::new(clock, small());
        let base = 1_794_132_000_123_u64 * 1000;
        assert_eq!(hub.id_base(), base);
        assert!(base < (1 << 53));
        emit(&hub, topic(1), 2);
        assert_eq!(
            ids(&hub.subscribe(UserId::OWNER, topic(1), Some(base)).backlog),
            [base + 1, base + 2]
        );
    }

    #[test]
    fn restart_makes_every_old_id_a_resync_and_ids_stay_monotonic() {
        let first_run = Arc::new(ManualClock(Mutex::new(datetime!(2026-11-08 10:00:00 UTC))));
        let old = EventHub::new(first_run, small());
        emit(&old, topic(1), 5);
        let remembered = old
            .subscribe(UserId::OWNER, topic(1), Some(old.id_base()))
            .backlog
            .last()
            .unwrap()
            .id;

        // Restarted one second later, nothing emitted yet: the old id is below
        // the new run's ids, the buffer is empty, so the client must resync.
        let second_run = Arc::new(ManualClock(Mutex::new(datetime!(2026-11-08 10:00:01 UTC))));
        let new = EventHub::new(second_run, small());
        let sub = new.subscribe(UserId::OWNER, topic(1), Some(remembered));
        assert_eq!(sub.resync_at, Some(new.id_base()));
        assert_eq!(sub.backlog.len(), 0);

        // Even once the new run has events, an id from the old run is below the
        // buffer start and resyncs instead of replaying the wrong events.
        emit(&new, topic(1), 3);
        let sub = new.subscribe(UserId::OWNER, topic(1), Some(remembered));
        assert_eq!(sub.resync_at, Some(new.id_base() + 3));
        assert!(new.id_base() > remembered);
        // The first id of the new run is above everything the old one emitted.
        let first_new = new
            .subscribe(UserId::OWNER, topic(1), Some(new.id_base()))
            .backlog[0]
            .id;
        assert!(first_new > remembered);
    }

    #[test]
    fn ids_inside_the_window_replay_after_a_restart_too() {
        let clock = Arc::new(ManualClock(Mutex::new(datetime!(2026-11-08 10:00:01 UTC))));
        let hub = EventHub::new(clock, small());
        emit(&hub, topic(1), 3);
        let base = hub.id_base();
        let sub = hub.subscribe(UserId::OWNER, topic(1), Some(base + 1));
        assert!(sub.resync_at.is_none());
        assert_eq!(ids(&sub.backlog), [base + 2, base + 3]);
        // Ahead of the last emitted id: untrusted.
        assert_eq!(
            hub.subscribe(UserId::OWNER, topic(1), Some(base + 4))
                .resync_at,
            Some(base + 3)
        );
    }
}
