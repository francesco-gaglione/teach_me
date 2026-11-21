use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ids::{FolderId, TopicId, UserId};
use crate::language::ContentLanguage;
use crate::validation::{ValidationIssue, normalize_text};

pub const TOPIC_TITLE_MAX_CHARS: usize = 120;

/// Validated topic title: trimmed, 1-120 code points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicTitle(String);

impl TopicTitle {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        normalize_text(raw, TOPIC_TITLE_MAX_CHARS, "title").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Case-insensitive identity used for uniqueness within a folder.
    #[must_use]
    pub fn unique_key(&self) -> String {
        self.0.to_lowercase()
    }
}

/// Lifecycle of a topic (`docs/mvp.md` §5). `Researching` is entered on mission
/// confirmation; `Ready` is set once the sources are annotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicState {
    New,
    Interviewing,
    MissionProposed,
    Researching,
    Ready,
}

impl TopicState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Interviewing => "interviewing",
            Self::MissionProposed => "mission_proposed",
            Self::Researching => "researching",
            Self::Ready => "ready",
        }
    }
}

impl fmt::Display for TopicState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown topic state `{0}`")]
pub struct UnknownTopicState(pub String);

impl FromStr for TopicState {
    type Err = UnknownTopicState;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "new" => Self::New,
            "interviewing" => Self::Interviewing,
            "mission_proposed" => Self::MissionProposed,
            "researching" => Self::Researching,
            "ready" => Self::Ready,
            other => return Err(UnknownTopicState(other.to_owned())),
        })
    }
}

/// A transition that happened, so the caller can persist it conditionally
/// (`TopicRepository::transition_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateChange {
    pub from: TopicState,
    pub to: TopicState,
}

/// The requested transition is not allowed from the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TopicRuleError {
    #[error("not allowed while the topic is `{0}`")]
    NotAllowed(TopicState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub id: TopicId,
    /// Copy of the folder's owner: the topic is the root of almost everything,
    /// and this keeps the ownership check free of joins.
    pub user_id: UserId,
    pub folder_id: FolderId,
    pub title: TopicTitle,
    /// Chosen at creation and never changed (ADR 0019).
    pub language: ContentLanguage,
    pub state: TopicState,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Topic {
    #[must_use]
    pub fn new(
        id: TopicId,
        user_id: UserId,
        folder_id: FolderId,
        title: TopicTitle,
        created_at: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            user_id,
            folder_id,
            title,
            language: ContentLanguage::default(),
            state: TopicState::New,
            created_at,
            updated_at: created_at,
        }
    }

    /// The topic written in `language`; only meaningful before it is stored.
    #[must_use]
    pub fn with_language(mut self, language: ContentLanguage) -> Self {
        self.language = language;
        self
    }

    /// A user message arrived: the first one starts the interview (`new`), and
    /// writing while a proposal is waiting reopens it (`mission_proposed`).
    /// Any other state is left alone.
    pub fn on_user_message(&mut self, now: OffsetDateTime) -> Option<StateChange> {
        match self.state {
            TopicState::New | TopicState::MissionProposed => {
                Some(self.move_to(TopicState::Interviewing, now))
            }
            _ => None,
        }
    }

    /// The Tutor saved a mission draft. Allowed while the mission is still
    /// being defined; with a draft already waiting the state stays as it is.
    pub fn propose_mission(
        &mut self,
        now: OffsetDateTime,
    ) -> Result<Option<StateChange>, TopicRuleError> {
        match self.state {
            TopicState::New | TopicState::Interviewing => {
                Ok(Some(self.move_to(TopicState::MissionProposed, now)))
            }
            TopicState::MissionProposed => Ok(None),
            state => Err(TopicRuleError::NotAllowed(state)),
        }
    }

    /// The user confirmed the open draft. Still possible after they wrote a
    /// message (`interviewing`): the draft stays open until superseded.
    pub fn confirm_mission(&mut self, now: OffsetDateTime) -> Result<StateChange, TopicRuleError> {
        match self.state {
            TopicState::MissionProposed | TopicState::Interviewing => {
                Ok(self.move_to(TopicState::Researching, now))
            }
            state => Err(TopicRuleError::NotAllowed(state)),
        }
    }

    fn move_to(&mut self, to: TopicState, now: OffsetDateTime) -> StateChange {
        let change = StateChange {
            from: self.state,
            to,
        };
        self.state = to;
        self.updated_at = now;
        change
    }

    /// Applies a title change and/or a move. Returns whether anything changed,
    /// so a no-op leaves `updated_at` untouched.
    pub fn apply(
        &mut self,
        title: Option<TopicTitle>,
        folder_id: Option<FolderId>,
        now: OffsetDateTime,
    ) -> bool {
        let mut changed = false;
        if let Some(title) = title.filter(|t| *t != self.title) {
            self.title = title;
            changed = true;
        }
        if let Some(folder_id) = folder_id.filter(|f| *f != self.folder_id) {
            self.folder_id = folder_id;
            changed = true;
        }
        if changed {
            self.updated_at = now;
        }
        changed
    }
}

/// Keyset position in the topic ordering (`created_at`, then `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicCursor {
    pub created_at: OffsetDateTime,
    pub id: TopicId,
}

impl TopicCursor {
    #[must_use]
    pub fn after(topic: &Topic) -> Self {
        Self {
            created_at: topic.created_at,
            id: topic.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::IssueKind;

    fn issue(raw: &str) -> IssueKind {
        TopicTitle::parse(raw).unwrap_err().issue
    }

    #[test]
    fn trims_and_rejects_blank() {
        assert_eq!(TopicTitle::parse("  Rust ").unwrap().as_str(), "Rust");
        assert_eq!(issue("   "), IssueKind::Empty);
        assert_eq!(issue(""), IssueKind::Empty);
    }

    #[test]
    fn length_boundary_is_120() {
        assert!(TopicTitle::parse(&"a".repeat(120)).is_ok());
        assert_eq!(issue(&"a".repeat(121)), IssueKind::TooLong);
        assert!(TopicTitle::parse(&"日".repeat(120)).is_ok());
        assert_eq!(issue(&"日".repeat(121)), IssueKind::TooLong);
    }

    #[test]
    fn issue_field_is_title() {
        assert_eq!(TopicTitle::parse("").unwrap_err().field, "title");
    }

    #[test]
    fn state_round_trips_through_strings() {
        for s in [
            TopicState::New,
            TopicState::Interviewing,
            TopicState::MissionProposed,
            TopicState::Researching,
            TopicState::Ready,
        ] {
            assert_eq!(s.as_str().parse::<TopicState>().unwrap(), s);
        }
        assert!("bogus".parse::<TopicState>().is_err());
    }

    #[test]
    fn new_topic_starts_in_new_state() {
        let t = time::macros::datetime!(2026-11-08 10:00 UTC);
        let topic = Topic::new(
            TopicId::new(uuid::Uuid::nil()),
            UserId::OWNER,
            FolderId::new(uuid::Uuid::nil()),
            TopicTitle::parse("x").unwrap(),
            t,
        );
        assert_eq!(topic.state, TopicState::New);
        assert_eq!(topic.language, ContentLanguage::It);
        assert_eq!(topic.updated_at, topic.created_at);
    }

    fn topic_in(state: TopicState) -> Topic {
        let mut t = Topic::new(
            TopicId::new(uuid::Uuid::nil()),
            UserId::OWNER,
            FolderId::new(uuid::Uuid::nil()),
            TopicTitle::parse("x").unwrap(),
            time::macros::datetime!(2026-11-08 10:00 UTC),
        );
        t.state = state;
        t
    }

    const LATER: OffsetDateTime = time::macros::datetime!(2026-11-09 10:00 UTC);

    fn change(from: TopicState, to: TopicState) -> StateChange {
        StateChange { from, to }
    }

    #[test]
    fn a_user_message_starts_or_reopens_the_interview_and_nothing_else() {
        use TopicState::{Interviewing, MissionProposed, New, Ready, Researching};
        for (from, expected) in [
            (New, Some(change(New, Interviewing))),
            (MissionProposed, Some(change(MissionProposed, Interviewing))),
            (Interviewing, None),
            (Researching, None),
            (Ready, None),
        ] {
            let mut t = topic_in(from);
            let before = t.updated_at;
            assert_eq!(t.on_user_message(LATER), expected, "{from}");
            match expected {
                Some(c) => assert_eq!((t.state, t.updated_at), (c.to, LATER)),
                None => assert_eq!((t.state, t.updated_at), (from, before)),
            }
        }
    }

    #[test]
    fn proposing_a_mission_is_allowed_while_it_is_being_defined() {
        use TopicState::{Interviewing, MissionProposed, New, Ready, Researching};
        let mut t = topic_in(Interviewing);
        assert_eq!(
            t.propose_mission(LATER),
            Ok(Some(change(Interviewing, MissionProposed)))
        );
        assert_eq!(t.updated_at, LATER);
        // Another draft while one is waiting: no change.
        assert_eq!(t.propose_mission(LATER), Ok(None));
        assert_eq!(
            topic_in(New).propose_mission(LATER),
            Ok(Some(change(New, MissionProposed)))
        );
        for state in [Researching, Ready] {
            let mut t = topic_in(state);
            assert_eq!(
                t.propose_mission(LATER),
                Err(TopicRuleError::NotAllowed(state))
            );
            assert_eq!(t.state, state);
        }
    }

    #[test]
    fn confirming_moves_to_researching_from_proposed_or_interviewing() {
        use TopicState::{Interviewing, MissionProposed, New, Ready, Researching};
        for from in [MissionProposed, Interviewing] {
            let mut t = topic_in(from);
            assert_eq!(t.confirm_mission(LATER), Ok(change(from, Researching)));
            assert_eq!((t.state, t.updated_at), (Researching, LATER));
        }
        for state in [New, Researching, Ready] {
            let mut t = topic_in(state);
            assert_eq!(
                t.confirm_mission(LATER),
                Err(TopicRuleError::NotAllowed(state))
            );
        }
    }

    #[test]
    fn apply_without_changes_keeps_updated_at() {
        let t = time::macros::datetime!(2026-11-08 10:00 UTC);
        let later = time::macros::datetime!(2026-11-09 10:00 UTC);
        let f = FolderId::new(uuid::Uuid::nil());
        let mut topic = Topic::new(
            TopicId::new(uuid::Uuid::nil()),
            UserId::OWNER,
            f,
            TopicTitle::parse("x").unwrap(),
            t,
        );
        assert!(!topic.apply(Some(TopicTitle::parse("x").unwrap()), Some(f), later));
        assert_eq!(topic.updated_at, t);
        assert!(topic.apply(None, Some(FolderId::new(uuid::Uuid::from_u128(1))), later));
        assert_eq!(topic.updated_at, later);
    }
}
