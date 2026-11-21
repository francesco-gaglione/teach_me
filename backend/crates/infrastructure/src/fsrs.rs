//! `ReviewScheduler` on the official `fsrs` crate (ADR 0014): the FSRS-7 memory
//! model with desired retention 0.9, no fuzz, a 36 500 day cap and fractional
//! elapsed days, plus the short learning steps of ts-fsrs v6 that the crate
//! does not have (`docs/contracts/reviews.md` §1.1).
//!
//! Every `fsrs` type stays in this file; the rest of the system sees only the
//! domain's `CardSchedule` and `SchedulerParameters`.

use domain::{
    CardSchedule, MemoryValues, Rating, RatingIntervals, ReviewEvent, ReviewScheduler, ReviewState,
    SchedulerError, SchedulerParameters,
};
use fsrs::{DEFAULT_PARAMETERS, FSRS, FSRSItem, FSRSReview, MemoryState, ModelVersion, NextStates};
use time::{Duration, OffsetDateTime};

/// Name stored next to the weights and on every card scheduled by this model.
pub(crate) const MODEL: &str = "fsrs7";
const DESIRED_RETENTION: f32 = 0.9;
const MAX_INTERVAL_DAYS: f64 = 36_500.0;
const MS_PER_DAY: f64 = 86_400_000.0;

/// Short steps (`reviews.md` §1.1). `new` and `learning` cards walk the first
/// list, `review` and `relearning` cards the second.
const LEARNING_STEPS: [Duration; 2] = [Duration::minutes(1), Duration::minutes(10)];
const RELEARNING_STEPS: [Duration; 1] = [Duration::minutes(10)];
/// The interval a card needs to leave the short steps ("graduate").
const GRADUATING_INTERVAL: Duration = Duration::days(1);

#[derive(Debug, Clone, Copy, Default)]
pub struct FsrsScheduler;

impl FsrsScheduler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// The four outcomes of a card at `now`, in the order again, hard, good,
    /// easy. `preview` and `schedule` both read from here, so the interval
    /// announced before a vote is the one the vote applies.
    fn outcomes(
        state: &CardSchedule,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<[CardSchedule; 4], SchedulerError> {
        let model = model(parameters)?;
        // Instants are kept at millisecond precision everywhere (storage, API):
        // a due date then survives a round trip.
        let now = millis(now);
        let (memory, elapsed) = if state.state == ReviewState::New {
            (None, 0.0)
        } else {
            let elapsed = state
                .last_reviewed_at
                .map_or(0.0, |last| elapsed_days(last, now));
            (Some(memory_of(state)), elapsed)
        };
        let next = model
            .next_states_with_elapsed_days(memory, DESIRED_RETENTION, elapsed)
            .map_err(|_| SchedulerError::InvalidState)?;
        let one = |rating, item: &fsrs::ItemState| -> Result<CardSchedule, SchedulerError> {
            let proposed = model_interval(item.interval)?;
            let step = apply_steps(state.state, state.learning_step, rating, proposed);
            Ok(CardSchedule {
                state: step.state,
                stability: f64::from(item.memory.stability),
                difficulty: f64::from(item.memory.difficulty),
                stability_fast: f64::from(item.memory.stability_fast),
                learning_step: step.index,
                due: now + step.interval,
                last_reviewed_at: Some(now),
                review_count: state.review_count.saturating_add(1),
                // A lapse is forgetting a card that had graduated.
                lapses: state.lapses
                    + u32::from(rating == Rating::Again && state.state == ReviewState::Review),
            })
        };
        let NextStates {
            again,
            hard,
            good,
            easy,
        } = &next;
        Ok([
            one(Rating::Again, again)?,
            one(Rating::Hard, hard)?,
            one(Rating::Good, good)?,
            one(Rating::Easy, easy)?,
        ])
    }
}

/// The `fsrs` model for `parameters`; anything but 34 FSRS-7 weights is refused
/// (the crate would otherwise fall back to defaults or to FSRS-6).
pub(crate) fn model(parameters: &SchedulerParameters) -> Result<FSRS, SchedulerError> {
    if parameters.model != MODEL
        || parameters.weights.len() != DEFAULT_PARAMETERS.len()
        || parameters.weights.iter().any(|w| !w.is_finite())
    {
        return Err(SchedulerError::InvalidParameters);
    }
    #[expect(clippy::cast_possible_truncation, reason = "the crate works in f32")]
    let weights: Vec<f32> = parameters.weights.iter().map(|w| *w as f32).collect();
    let model = FSRS::new(&weights).map_err(|_| SchedulerError::InvalidParameters)?;
    if model.version() == ModelVersion::Fsrs7 {
        Ok(model)
    } else {
        Err(SchedulerError::InvalidParameters)
    }
}

/// A card's votes as the crate reads them: the same millisecond instants and
/// the same elapsed days a vote reads from the card, so that replaying equals
/// having voted. The first vote is at elapsed time 0.
pub(crate) fn item_of(history: &[ReviewEvent]) -> FSRSItem {
    let mut previous: Option<OffsetDateTime> = None;
    let reviews = history
        .iter()
        .map(|event| {
            let at = millis(event.at);
            let delta_t = previous.map_or(0.0, |last| elapsed_days(last, at));
            previous = Some(at);
            FSRSReview {
                rating: rating_number(event.rating),
                delta_t,
            }
        })
        .collect();
    FSRSItem { reviews }
}

/// Truncates to the millisecond.
pub(crate) fn millis(at: OffsetDateTime) -> OffsetDateTime {
    let ms = at.unix_timestamp_nanos().div_euclid(1_000_000);
    OffsetDateTime::from_unix_timestamp_nanos(ms * 1_000_000).unwrap_or(at)
}

/// Days from `last` to `now`, fractional, never negative.
#[expect(clippy::cast_precision_loss, reason = "milliseconds fit an f64")]
#[expect(clippy::cast_possible_truncation, reason = "the crate works in f32")]
pub(crate) fn elapsed_days(last: OffsetDateTime, now: OffsetDateTime) -> f32 {
    let ms = (now - last).whole_milliseconds().max(0);
    (ms as f64 / MS_PER_DAY) as f32
}

/// A card with no fast trace yet gets it equal to the stability, as FSRS-6
/// does. The application never schedules a card still on the previous model
/// (it replays its votes first), so this is only a guard against a zero.
#[expect(clippy::cast_possible_truncation, reason = "the crate works in f32")]
fn memory_of(state: &CardSchedule) -> MemoryState {
    let fast = if state.stability_fast > 0.0 {
        state.stability_fast
    } else {
        state.stability
    };
    MemoryState {
        stability: state.stability as f32,
        difficulty: state.difficulty as f32,
        stability_fast: fast as f32,
    }
}

/// The rating as the crate numbers it: 1 again to 4 easy.
pub(crate) const fn rating_number(rating: Rating) -> u32 {
    match rating {
        Rating::Again => 1,
        Rating::Hard => 2,
        Rating::Good => 3,
        Rating::Easy => 4,
    }
}

/// The model's interval in days as a duration, capped at the maximum.
#[expect(clippy::cast_possible_truncation, reason = "bounded by the cap")]
fn model_interval(days: f32) -> Result<Duration, SchedulerError> {
    if !days.is_finite() {
        return Err(SchedulerError::InvalidState);
    }
    let ms = (f64::from(days).clamp(0.0, MAX_INTERVAL_DAYS) * MS_PER_DAY).round();
    Ok(Duration::milliseconds(ms as i64))
}

/// What the short steps decide for one vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StepOutcome {
    interval: Duration,
    state: ReviewState,
    /// Index of the next short step.
    index: u32,
}

/// The rule of `reviews.md` §1.1, taken from ts-fsrs v6 (`learning-steps`
/// middleware). `model` is the interval the memory model proposes; a short step
/// is used only when the model proposes less.
fn apply_steps(state: ReviewState, index: u32, rating: Rating, model: Duration) -> StepOutcome {
    let steps: &[Duration] = match state {
        ReviewState::New | ReviewState::Learning => &LEARNING_STEPS,
        ReviewState::Review | ReviewState::Relearning => &RELEARNING_STEPS,
    };
    // A graduated card steps only when it is forgotten.
    let in_steps = state != ReviewState::Review;
    let at = usize::try_from(index).unwrap_or(usize::MAX);
    let (step, next_index) = match rating {
        Rating::Again => (steps.first().copied(), 0),
        Rating::Hard if in_steps => (hard_step(steps), index),
        Rating::Good if in_steps => match steps.get(at.saturating_add(1)) {
            Some(next) => (Some(*next), index + 1),
            None => (None, index),
        },
        _ => (None, index),
    };
    let interval = match (rating, step) {
        (Rating::Again, Some(step)) => step.max(model.min(GRADUATING_INTERVAL)),
        (_, Some(step)) => step.max(model),
        (_, None) => model.max(GRADUATING_INTERVAL),
    };
    if interval < GRADUATING_INTERVAL || rating == Rating::Again {
        StepOutcome {
            interval,
            state: match state {
                ReviewState::New => ReviewState::Learning,
                ReviewState::Review => ReviewState::Relearning,
                other => other,
            },
            index: next_index,
        }
    } else {
        StepOutcome {
            interval,
            state: ReviewState::Review,
            index: 0,
        }
    }
}

/// "Hard": the mean of the first two steps, or 1.5 times the only one.
fn hard_step(steps: &[Duration]) -> Option<Duration> {
    match steps {
        [] => None,
        [only] => Some(*only * 3 / 2),
        [first, second, ..] => Some((*first + *second) / 2),
    }
}

impl ReviewScheduler for FsrsScheduler {
    fn default_parameters(&self) -> SchedulerParameters {
        SchedulerParameters {
            model: MODEL.to_owned(),
            weights: DEFAULT_PARAMETERS.iter().map(|w| f64::from(*w)).collect(),
        }
    }

    fn check_parameters(&self, parameters: &SchedulerParameters) -> Result<(), SchedulerError> {
        model(parameters).map(|_| ())
    }

    fn schedule(
        &self,
        state: &CardSchedule,
        rating: Rating,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<CardSchedule, SchedulerError> {
        let [again, hard, good, easy] = Self::outcomes(state, now, parameters)?;
        Ok(match rating {
            Rating::Again => again,
            Rating::Hard => hard,
            Rating::Good => good,
            Rating::Easy => easy,
        })
    }

    fn preview(
        &self,
        state: &CardSchedule,
        now: OffsetDateTime,
        parameters: &SchedulerParameters,
    ) -> Result<RatingIntervals, SchedulerError> {
        let now = millis(now);
        let [again, hard, good, easy] = Self::outcomes(state, now, parameters)?;
        Ok(RatingIntervals {
            again: again.due - now,
            hard: hard.due - now,
            good: good.due - now,
            easy: easy.due - now,
        })
    }

    fn replay(
        &self,
        parameters: &SchedulerParameters,
        history: &[ReviewEvent],
    ) -> Result<Option<MemoryValues>, SchedulerError> {
        if history.is_empty() {
            return Ok(None);
        }
        let model = model(parameters)?;
        let memory = model
            .memory_state(item_of(history), None)
            .map_err(|_| SchedulerError::InvalidState)?;
        let finite = [memory.stability, memory.difficulty, memory.stability_fast]
            .iter()
            .all(|v| v.is_finite());
        if !finite {
            return Err(SchedulerError::InvalidState);
        }
        Ok(Some(MemoryValues {
            stability: f64::from(memory.stability),
            difficulty: f64::from(memory.difficulty),
            stability_fast: f64::from(memory.stability_fast),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const T0: OffsetDateTime = datetime!(2022-11-29 12:30:00 UTC);

    fn new_card() -> CardSchedule {
        CardSchedule::initial(T0)
    }

    fn params() -> SchedulerParameters {
        FsrsScheduler::new().default_parameters()
    }

    fn schedule(card: &CardSchedule, rating: Rating, now: OffsetDateTime) -> CardSchedule {
        FsrsScheduler::new()
            .schedule(card, rating, now, &params())
            .unwrap()
    }

    fn preview(card: &CardSchedule, now: OffsetDateTime) -> RatingIntervals {
        FsrsScheduler::new().preview(card, now, &params()).unwrap()
    }

    fn days(d: f64) -> Duration {
        Duration::seconds_f64(d * 86_400.0)
    }

    /// The doctest of `FSRS::next_states` in fsrs 7.0.0: new card, default
    /// weights, retention 0.9. Guards the crate itself against a regression.
    #[test]
    fn the_crate_reproduces_its_reference_values_for_a_new_card() {
        let fsrs = FSRS::new(&DEFAULT_PARAMETERS).unwrap();
        let next = fsrs.next_states(None, 0.9, 0).unwrap();
        let expected = [
            (&next.again, 0.1104, 6.1686, 0.08832, 3.827_562_6e-5),
            (&next.hard, 2.2395, 5.261_278, 1.791_600_1, 0.597_459_8),
            (&next.good, 3.9221, 3.530_724_3, 3.13768, 4.777_724_7),
            (&next.easy, 11.7841, 1.0, 9.427_279, 53.869_392),
        ];
        for (got, s, d, sf, interval) in expected {
            assert_eq!(got.memory.stability, s);
            assert_eq!(got.memory.difficulty, d);
            assert_eq!(got.memory.stability_fast, sf);
            assert_eq!(got.interval, interval);
        }
    }

    #[test]
    fn a_new_card_previews_one_minute_then_hours_then_days() {
        let p = preview(&new_card(), T0);
        assert_eq!(p.again, Duration::minutes(1));
        // The model's own intervals, in days.
        assert!((p.hard.as_seconds_f64() / 86_400.0 - 0.597_459_8).abs() < 1e-6);
        assert!((p.good.as_seconds_f64() / 86_400.0 - 4.777_724_7).abs() < 1e-6);
        assert!((p.easy.as_seconds_f64() / 86_400.0 - 53.869_392).abs() < 1e-6);

        let states: Vec<_> = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
            .into_iter()
            .map(|r| schedule(&new_card(), r, T0).state)
            .collect();
        assert_eq!(
            states,
            [
                ReviewState::Learning,
                ReviewState::Learning,
                ReviewState::Review,
                ReviewState::Review
            ]
        );
    }

    #[test]
    fn a_vote_stores_the_memory_of_the_model_and_the_vote_time() {
        let good = schedule(&new_card(), Rating::Good, T0);
        assert_eq!(good.stability, f64::from(3.9221_f32));
        assert_eq!(good.stability_fast, f64::from(3.13768_f32));
        assert_eq!((good.review_count, good.lapses), (1, 0));
        assert_eq!(good.last_reviewed_at, Some(T0));
        assert_eq!(good.learning_step, 0);
    }

    /// Regression: three "good" votes, each when the card falls due, with
    /// ts-fsrs v6 as the source of the expected days (4.78, 26.17, 92.44).
    #[test]
    fn three_good_votes_follow_the_reference_intervals() {
        let mut card = new_card();
        let mut now = T0;
        let mut got = Vec::new();
        for _ in 0..3 {
            card = schedule(&card, Rating::Good, now);
            got.push((card.due - now).as_seconds_f64() / 86_400.0);
            now = card.due;
        }
        for (g, e) in got.iter().zip([4.78, 26.17, 92.44]) {
            assert!((g - e).abs() < 0.01, "{got:?}");
        }
    }

    #[test]
    fn preview_is_exactly_what_a_vote_applies() {
        let mut card = new_card();
        let mut now = T0;
        // Learning, review and relearning states, early and late reviews.
        for (r, wait) in [
            (Rating::Good, Duration::minutes(11)),
            (Rating::Hard, Duration::minutes(7)),
            (Rating::Good, Duration::days(5)),
            (Rating::Hard, Duration::days(1)),
            (Rating::Again, Duration::days(30)),
            (Rating::Good, Duration::minutes(12)),
            (Rating::Easy, Duration::hours(7)),
        ] {
            now += wait;
            let p = preview(&card, now);
            for vote in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
                let applied = schedule(&card, vote, now);
                assert_eq!(applied.due - now, p.get(vote), "{vote:?} {card:?}");
            }
            card = schedule(&card, r, now);
        }
    }

    #[test]
    fn the_replayed_history_equals_the_state_voted_one_by_one() {
        use fsrs::{FSRSItem, FSRSReview};
        let votes = [
            (Rating::Good, Duration::ZERO),
            (Rating::Hard, Duration::minutes(10)),
            (Rating::Good, Duration::hours(20)),
            (Rating::Again, Duration::days(6)),
            (Rating::Good, Duration::minutes(10)),
            (Rating::Easy, Duration::days(2)),
        ];
        let mut card = new_card();
        let mut now = T0;
        let mut history = Vec::new();
        let mut last = None;
        for (rating, wait) in votes {
            now += wait;
            card = schedule(&card, rating, now);
            history.push(FSRSReview {
                rating: match rating {
                    Rating::Again => 1,
                    Rating::Hard => 2,
                    Rating::Good => 3,
                    Rating::Easy => 4,
                },
                delta_t: last.map_or(0.0, |l| elapsed_days(l, now)),
            });
            last = Some(now);
        }
        let replayed = model(&params())
            .unwrap()
            .memory_state(FSRSItem { reviews: history }, None)
            .unwrap();
        assert_eq!(card.stability, f64::from(replayed.stability));
        assert_eq!(card.difficulty, f64::from(replayed.difficulty));
        assert_eq!(card.stability_fast, f64::from(replayed.stability_fast));
    }

    #[test]
    fn replay_gives_the_memory_of_voting_one_by_one_and_none_for_no_votes() {
        let votes = [
            (Rating::Good, Duration::ZERO),
            (Rating::Hard, Duration::minutes(10)),
            (Rating::Good, Duration::hours(20)),
            (Rating::Again, Duration::days(6)),
            (Rating::Good, Duration::minutes(10)),
            (Rating::Easy, Duration::days(2) + Duration::milliseconds(7)),
        ];
        let mut card = new_card();
        let mut now = T0;
        let mut history = Vec::new();
        for (rating, wait) in votes {
            now += wait;
            card = schedule(&card, rating, now);
            history.push(ReviewEvent { rating, at: now });
        }
        let s = FsrsScheduler::new();
        let got = s.replay(&params(), &history).unwrap().unwrap();
        assert_eq!(
            got,
            MemoryValues {
                stability: card.stability,
                difficulty: card.difficulty,
                stability_fast: card.stability_fast
            }
        );
        assert_eq!(s.replay(&params(), &[]), Ok(None));
        let mut bad = params();
        bad.weights.pop();
        assert_eq!(
            s.replay(&bad, &history),
            Err(SchedulerError::InvalidParameters)
        );
    }

    #[test]
    fn the_same_input_gives_the_same_output_and_sub_millisecond_time_is_ignored() {
        let a = schedule(&new_card(), Rating::Good, T0);
        let b = schedule(&new_card(), Rating::Good, T0);
        let c = schedule(&new_card(), Rating::Good, T0 + Duration::microseconds(400));
        assert_eq!(a, b);
        assert_eq!(a.due, c.due);
        // Not rounded to the day: the due date is the model's instant.
        assert_ne!((a.due - T0).whole_seconds() % 86_400, 0);
    }

    #[test]
    fn again_on_a_reviewed_card_lapses_and_comes_back_after_the_step() {
        let c = schedule(&new_card(), Rating::Easy, T0);
        let lapsed = schedule(&c, Rating::Again, c.due);
        assert_eq!(lapsed.state, ReviewState::Relearning);
        assert_eq!(lapsed.lapses, 1);
        assert_eq!(lapsed.learning_step, 0);
        assert!(lapsed.stability < c.stability);
        // The model proposes a few hours at most: the 10 minute step is the floor.
        assert!(lapsed.due - c.due >= Duration::minutes(10));
        assert!(lapsed.due - c.due <= GRADUATING_INTERVAL);
    }

    #[test]
    fn a_card_at_the_second_step_leaves_the_steps_on_good() {
        let c = schedule(&new_card(), Rating::Again, T0);
        assert_eq!((c.state, c.learning_step), (ReviewState::Learning, 0));
        // "Hard" keeps the step; "good" moves to the second one only while the
        // model proposes less than a day, which a card this young does.
        let hard = schedule(&c, Rating::Hard, c.due);
        assert_eq!(hard.learning_step, 0);
        let at_step_1 = CardSchedule {
            learning_step: 1,
            ..c
        };
        let good = schedule(&at_step_1, Rating::Good, c.due);
        assert_eq!((good.state, good.learning_step), (ReviewState::Review, 0));
        assert!(good.due - c.due >= GRADUATING_INTERVAL);
    }

    #[test]
    fn a_relearning_card_walks_the_relearning_step() {
        let reviewed = schedule(&new_card(), Rating::Easy, T0);
        let re = schedule(&reviewed, Rating::Again, reviewed.due);
        assert_eq!(re.state, ReviewState::Relearning);
        let again = schedule(&re, Rating::Again, re.due);
        assert_eq!(
            (again.state, again.learning_step, again.lapses),
            (ReviewState::Relearning, 0, 1)
        );
        assert!(again.due - re.due >= Duration::minutes(10));
        let good = schedule(&re, Rating::Good, re.due);
        assert_eq!((good.state, good.learning_step), (ReviewState::Review, 0));
        assert!(good.due - re.due >= GRADUATING_INTERVAL);
    }

    #[test]
    fn a_card_from_the_previous_model_is_still_schedulable() {
        let old = CardSchedule {
            state: ReviewState::Review,
            stability: 12.0,
            difficulty: 5.0,
            stability_fast: 0.0,
            learning_step: 0,
            due: T0,
            last_reviewed_at: Some(T0 - Duration::days(12)),
            review_count: 4,
            lapses: 0,
        };
        let next = schedule(&old, Rating::Good, T0);
        assert!(next.stability > 0.0 && next.stability_fast > 0.0);
        assert!(next.due > T0);
    }

    #[test]
    fn unusable_parameters_are_an_error_not_a_panic() {
        let s = FsrsScheduler::new();
        let mut bad = params();
        bad.weights.pop();
        assert_eq!(
            s.preview(&new_card(), T0, &bad),
            Err(SchedulerError::InvalidParameters)
        );
        let mut other = params();
        other.model = "fsrs5".into();
        assert_eq!(
            s.schedule(&new_card(), Rating::Good, T0, &other),
            Err(SchedulerError::InvalidParameters)
        );
        let mut nan = params();
        nan.weights[0] = f64::NAN;
        assert!(s.preview(&new_card(), T0, &nan).is_err());
        let mut empty = params();
        empty.weights.clear();
        assert!(s.preview(&new_card(), T0, &empty).is_err());
    }

    #[test]
    fn the_interval_is_capped_at_the_maximum() {
        assert_eq!(model_interval(1.0e9).unwrap(), Duration::days(36_500));
        assert!(model_interval(f32::NAN).is_err());
    }

    // The short steps, one case per row of the table in `reviews.md` §1.1.
    // `m` is what the memory model proposes.
    fn steps(
        state: ReviewState,
        index: u32,
        rating: Rating,
        m: Duration,
    ) -> (Duration, ReviewState, u32) {
        let o = apply_steps(state, index, rating, m);
        (o.interval, o.state, o.index)
    }

    #[test]
    fn again_uses_the_first_step_capped_at_a_day() {
        use ReviewState::{Learning, Relearning};
        let (sec, min) = (Duration::seconds(3), Duration::minutes(1));
        // New and learning: step 1 minute; the model decides when it asks for more.
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Again, sec),
            (min, Learning, 0)
        );
        assert_eq!(steps(Learning, 1, Rating::Again, sec), (min, Learning, 0));
        assert_eq!(
            steps(Learning, 1, Rating::Again, Duration::hours(3)),
            (Duration::hours(3), Learning, 0)
        );
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Again, Duration::days(5)),
            (Duration::days(1), Learning, 0)
        );
        // A graduated card forgotten: the relearning step (10 minutes).
        assert_eq!(
            steps(ReviewState::Review, 0, Rating::Again, sec),
            (Duration::minutes(10), Relearning, 0)
        );
        assert_eq!(
            steps(ReviewState::Review, 0, Rating::Again, Duration::days(9)),
            (Duration::days(1), Relearning, 0)
        );
        assert_eq!(
            steps(Relearning, 0, Rating::Again, sec),
            (Duration::minutes(10), Relearning, 0)
        );
    }

    #[test]
    fn hard_uses_the_mean_of_two_steps_or_one_and_a_half_times_the_only_one() {
        use ReviewState::{Learning, Relearning, Review};
        let m = Duration::minutes(1);
        // 5.5 minutes with two steps; the index stays.
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Hard, m),
            (Duration::seconds(330), Learning, 0)
        );
        assert_eq!(
            steps(Learning, 1, Rating::Hard, m),
            (Duration::seconds(330), Learning, 1)
        );
        // The model wins when it proposes more, and graduates at a day.
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Hard, Duration::hours(14)),
            (Duration::hours(14), Learning, 0)
        );
        assert_eq!(
            steps(Learning, 1, Rating::Hard, Duration::days(2)),
            (Duration::days(2), Review, 0)
        );
        // 15 minutes with the single relearning step.
        assert_eq!(
            steps(Relearning, 0, Rating::Hard, m),
            (Duration::minutes(15), Relearning, 0)
        );
        assert_eq!(
            steps(Relearning, 0, Rating::Hard, Duration::hours(1)),
            (Duration::hours(1), Relearning, 0)
        );
        // A graduated card has no step for "hard": at least a day.
        assert_eq!(
            steps(Review, 0, Rating::Hard, m),
            (Duration::days(1), Review, 0)
        );
        assert_eq!(
            steps(Review, 0, Rating::Hard, Duration::days(3)),
            (Duration::days(3), Review, 0)
        );
    }

    #[test]
    fn good_advances_to_the_next_step_and_graduates_after_the_last() {
        use ReviewState::{Learning, Relearning, Review};
        let m = Duration::minutes(2);
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Good, m),
            (Duration::minutes(10), Learning, 1)
        );
        assert_eq!(
            steps(Learning, 0, Rating::Good, m),
            (Duration::minutes(10), Learning, 1)
        );
        // The model proposes days: the card graduates at once.
        assert_eq!(
            steps(ReviewState::New, 0, Rating::Good, Duration::days(5)),
            (Duration::days(5), Review, 0)
        );
        // No step after the last one: a day at least, back to `review`.
        assert_eq!(
            steps(Learning, 1, Rating::Good, m),
            (Duration::days(1), Review, 0)
        );
        assert_eq!(
            steps(Relearning, 0, Rating::Good, m),
            (Duration::days(1), Review, 0)
        );
        assert_eq!(
            steps(Review, 0, Rating::Good, Duration::days(8)),
            (Duration::days(8), Review, 0)
        );
        assert_eq!(
            steps(Review, 0, Rating::Good, m),
            (Duration::days(1), Review, 0)
        );
    }

    #[test]
    fn easy_never_uses_a_step() {
        use ReviewState::{Learning, Relearning, Review};
        for state in [ReviewState::New, Learning, Review, Relearning] {
            assert_eq!(
                steps(state, 1, Rating::Easy, Duration::seconds(5)),
                (Duration::days(1), Review, 0),
                "{state:?}"
            );
            assert_eq!(
                steps(state, 0, Rating::Easy, days(54.0)),
                (days(54.0), Review, 0),
                "{state:?}"
            );
        }
    }
}
