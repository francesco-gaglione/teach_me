//! Stable instructions of the user about how to be taught, saved by the Tutor
//! (`docs/contracts/mission.md` §2).

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ids::{PreferenceId, TopicId, UserId};
use crate::validation::{ValidationIssue, normalize_text};

pub const PREFERENCE_TEXT_MAX_CHARS: usize = 300;

/// Validated preference text: trimmed, 1-300 code points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferenceText(String);

impl PreferenceText {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        normalize_text(raw, PREFERENCE_TEXT_MAX_CHARS, "text").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Identity ignoring case and runs of whitespace: saving the same
    /// instruction twice (a retried turn, a model repeating itself) must not
    /// duplicate it.
    #[must_use]
    pub fn unique_key(&self) -> String {
        self.0
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
}

/// Where a preference applies. A topic preference always names its topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceScope {
    Global,
    Topic(TopicId),
}

impl PreferenceScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Topic(_) => "topic",
        }
    }

    #[must_use]
    pub const fn topic_id(self) -> Option<TopicId> {
        match self {
            Self::Global => None,
            Self::Topic(id) => Some(id),
        }
    }
}

impl fmt::Display for PreferenceScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The two scope names, as the Tutor's tool and the API spell them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Topic,
}

impl FromStr for ScopeKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "global" => Ok(Self::Global),
            "topic" => Ok(Self::Topic),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preference {
    pub id: PreferenceId,
    pub user_id: UserId,
    pub scope: PreferenceScope,
    pub text: PreferenceText,
    pub created_at: OffsetDateTime,
}

impl Preference {
    /// `current_topic` is the topic of the conversation that produced it; it
    /// is only used for the `topic` scope.
    #[must_use]
    pub fn new(
        id: PreferenceId,
        user_id: UserId,
        kind: ScopeKind,
        current_topic: TopicId,
        text: PreferenceText,
        created_at: OffsetDateTime,
    ) -> Self {
        let scope = match kind {
            ScopeKind::Global => PreferenceScope::Global,
            ScopeKind::Topic => PreferenceScope::Topic(current_topic),
        };
        Self {
            id,
            user_id,
            scope,
            text,
            created_at,
        }
    }
}

/// Keyset position in the listing (`created_at`, then `id`, ascending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferenceCursor {
    pub created_at: OffsetDateTime,
    pub id: PreferenceId,
}

impl PreferenceCursor {
    #[must_use]
    pub fn after(p: &Preference) -> Self {
        Self {
            created_at: p.created_at,
            id: p.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::IssueKind;
    use time::macros::datetime;
    use uuid::Uuid;

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(PreferenceText::parse("  x ").unwrap().as_str(), "x");
        assert_eq!(
            PreferenceText::parse(" ").unwrap_err().issue,
            IssueKind::Empty
        );
        assert!(PreferenceText::parse(&"è".repeat(300)).is_ok());
        let err = PreferenceText::parse(&"è".repeat(301)).unwrap_err();
        assert_eq!((err.field, err.issue), ("text", IssueKind::TooLong));
    }

    #[test]
    fn key_ignores_case() {
        let a = PreferenceText::parse("Esempi in PYTHON").unwrap();
        let b = PreferenceText::parse("esempi in python").unwrap();
        assert_eq!(a.unique_key(), b.unique_key());
    }

    #[test]
    fn key_collapses_inner_whitespace() {
        let a = PreferenceText::parse("Esempi   in\tPython \n sempre").unwrap();
        let b = PreferenceText::parse("esempi in python sempre").unwrap();
        assert_eq!(a.unique_key(), b.unique_key());
        assert_ne!(
            a.unique_key(),
            PreferenceText::parse("esempi inpython sempre")
                .unwrap()
                .unique_key()
        );
    }

    #[test]
    fn topic_scope_takes_the_current_topic_and_global_none() {
        let topic = TopicId::new(Uuid::from_u128(7));
        let at = datetime!(2026-11-08 10:00 UTC);
        let text = PreferenceText::parse("x").unwrap();
        let id = PreferenceId::new(Uuid::from_u128(1));
        let p = Preference::new(id, UserId::OWNER, ScopeKind::Topic, topic, text.clone(), at);
        assert_eq!(p.scope, PreferenceScope::Topic(topic));
        assert_eq!(p.scope.topic_id(), Some(topic));
        let g = Preference::new(id, UserId::OWNER, ScopeKind::Global, topic, text, at);
        assert_eq!(
            (g.scope, g.scope.topic_id()),
            (PreferenceScope::Global, None)
        );
        assert_eq!("topic".parse::<ScopeKind>(), Ok(ScopeKind::Topic));
        assert!("both".parse::<ScopeKind>().is_err());
    }
}
